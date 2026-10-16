//! Recovery is inspection of evidence, never authorization to replay a restored DB.
use rusqlite::{types::Value, Connection};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker, RECOVERY_MARKER};

fn setup() -> (TempDir, WorkerConfig, Store) {
    let root = TempDir::new().unwrap();
    let config: WorkerConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":format!("recovery-{}",uuid::Uuid::new_v4()),
        "nodeId":"test","stateDirectory":root.path().join("state"),
        "worker":{"keepRunningOnUiExit":true},"outputRetention":{"automatic":true,"olderThanDays":1},
        "projects":[{"id":"fixture","workspaceRoot":root.path(),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap();
    drop(Worker::open(config.clone()).unwrap());
    let store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    (root, config, store)
}
fn request(key: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        idempotency_key: key.into(),
        project_id: "fixture".into(),
        snapshot_id: None,
        timeout_ms: 60000,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec!["success".into()],
            working_directory: ".".into(),
        },
    }
}
fn accept(store: &mut Store, cfg: &WorkerConfig, key: &str) -> TaskRecord {
    let submission = request(key);
    let command = winbridge_runner::resolve(cfg, &submission).unwrap();
    store.accept(submission, command, None).unwrap()
}
fn db(cfg: &WorkerConfig) -> Connection {
    Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap()
}
fn fingerprint(cfg: &WorkerConfig) -> String {
    let connection = db(cfg);
    let mut names = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables = names
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let mut result = String::new();
    for table in tables {
        let mut rows = connection
            .prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                table.replace('"', "\"\"")
            ))
            .unwrap();
        let count = rows.column_count();
        let values = rows
            .query_map([], |row| {
                (0..count)
                    .map(|i| row.get::<_, Value>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        result.push_str(&format!("{table}: {values:?}\n"));
    }
    result
}
fn info(worker: &Worker) -> WorkerInfo {
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("info unavailable")
    };
    *info
}
fn denied(worker: &Worker, request: LocalRequest) {
    let LocalResponse::Error { message } = worker.handle(request) else {
        panic!("mutation accepted")
    };
    assert!(message.contains("только для диагностики"), "{message}");
}

#[test]
fn read_only_store_preserves_records_authority_and_keys_and_rejects_writers() {
    let (_root, cfg, mut store) = setup();
    let task = accept(&mut store, &cfg, "stored-key");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    store
        .with_execution(&token, |s| {
            s.append_output(
                &task.task_id,
                OutputStream::Stdout,
                0,
                b"retained output",
                4096,
            )
        })
        .unwrap();
    let authority = store.journal_authority().clone();
    let before = fingerprint(&cfg);
    drop(store);
    let mut inspection =
        Store::open_read_only(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert_eq!(inspection.journal_authority(), &authority);
    assert_eq!(
        inspection
            .existing(&request("stored-key"))
            .unwrap()
            .unwrap()
            .task_id,
        task.task_id
    );
    assert_eq!(
        inspection.output(&task.task_id, 0).unwrap()[0].bytes,
        b"retained output"
    );
    assert!(inspection
        .begin_authority()
        .unwrap_err()
        .contains("read-only"));
    assert!(inspection.recover().is_err());
    assert!(inspection.claim_task(&task.task_id).is_err());
    assert!(inspection.cancel(&task.task_id).is_err());
    assert!(inspection
        .with_execution(&token, |s| s.append_output(
            &task.task_id,
            OutputStream::Stdout,
            15,
            b"bad",
            4096
        ))
        .is_err());
    drop(inspection);
    assert_eq!(before, fingerprint(&cfg));
}

#[test]
fn inspection_exposes_legacy_overlap_without_recovering_or_dispatching() {
    let (root, cfg, mut store) = setup();
    let owned = accept(&mut store, &cfg, "held");
    store.claim_task(&owned.task_id).unwrap();
    let legacy = accept(&mut store, &cfg, "legacy-overlap");
    db(&cfg).execute("UPDATE tasks SET record=json_set(record,'$.status','running','$.processId',1234),revision=revision+1 WHERE id=?1",[&legacy.task_id]).unwrap();
    drop(store);
    assert!(Worker::open(cfg.clone())
        .err()
        .unwrap()
        .contains("serve-recovery"));
    let before = fingerprint(&cfg);
    let worker = Worker::open_recovery(cfg.clone()).unwrap();
    let status = info(&worker);
    assert!(status.journal_inspection.is_some());
    assert!(status.journal_recovery_required.is_some());
    assert!(status.active_task_count.is_none());
    assert!(!status.maintenance_available);
    let inventory = status.resource_recovery.unwrap();
    assert_eq!(inventory.unresolved_count, 2);
    assert!(inventory
        .resources
        .iter()
        .any(|r| r.operation_id == legacy.task_id && !r.physical_binding));
    for request in [
        LocalRequest::ListTasks,
        LocalRequest::ListImports,
        LocalRequest::ListWorkflows,
        LocalRequest::ListApplications,
        LocalRequest::History {
            query: HistoryQuery::default(),
        },
        LocalRequest::StorageUsage,
        LocalRequest::ListAttentionEvents {
            after_sequence: 0,
            pending_only: false,
        },
    ] {
        assert!(!matches!(
            worker.handle(request),
            LocalResponse::Error { .. }
        ));
    }
    let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask {
        task_id: legacy.task_id.clone(),
    }) else {
        panic!()
    };
    assert_eq!(
        task.status,
        TaskStatus::Running,
        "historical record was rewritten"
    );
    for _ in 0..3 {
        worker.schedule().unwrap();
        worker.automatic_output_cleanup().unwrap();
    }
    for request in [
        LocalRequest::Submit {
            request: request("new"),
        },
        LocalRequest::Cancel {
            task_id: owned.task_id,
        },
        LocalRequest::CancelImport {
            import_id: "unknown".into(),
        },
        LocalRequest::CancelWorkflow {
            workflow_id: "unknown".into(),
        },
        LocalRequest::StopApplication {
            application_id: "unknown".into(),
        },
        LocalRequest::AcknowledgeAttention {
            attention_id: "unknown".into(),
        },
        LocalRequest::AcknowledgeAttentionEvent {
            attention_id: "unknown".into(),
        },
        LocalRequest::SaveRuntimeSettings {
            request: SaveRuntimeSettings {
                generation: worker.generation.clone(),
                request_id: "save".into(),
                expected_sha256: "a".repeat(64),
                json: "{}".into(),
            },
        },
    ] {
        denied(&worker, request);
    }
    drop(worker);
    assert_eq!(before, fingerprint(&cfg));
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn database_only_restore_with_lost_keys_remains_blocked_across_restarts() {
    let (root, cfg, mut store) = setup();
    let old = accept(&mut store, &cfg, "old");
    let backup = root.path().join("consistent-old.sqlite");
    db(&cfg)
        .execute("VACUUM INTO ?1", [backup.to_str().unwrap()])
        .unwrap();
    let newer = accept(&mut store, &cfg, "lost-key");
    drop(store);
    let inspection = Worker::open_recovery(cfg.clone()).unwrap();
    assert!(matches!(
        inspection.handle(LocalRequest::GetTask {
            task_id: newer.task_id.clone()
        }),
        LocalResponse::Task { .. }
    ));
    drop(inspection);
    // A read-only WAL connection may leave sidecars. Checkpoint/close the test
    // database before restoring its consistent main-file backup.
    db(&cfg)
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    assert!(!cfg.state_directory.join("journal.sqlite-wal").exists());
    fs::copy(&backup, cfg.state_directory.join("journal.sqlite")).unwrap(); // All test connections closed.
    let bytes = fs::read(cfg.state_directory.join("journal.sqlite")).unwrap();
    let marker = fs::read(cfg.state_directory.join(RECOVERY_MARKER)).unwrap();
    assert!(Worker::open(cfg.clone())
        .err()
        .unwrap()
        .contains("Recovery marker"));
    let again = Worker::open_recovery(cfg.clone()).unwrap();
    denied(
        &again,
        LocalRequest::Submit {
            request: request("lost-key"),
        },
    );
    assert!(matches!(
        again.handle(LocalRequest::GetTask {
            task_id: newer.task_id
        }),
        LocalResponse::Error { .. }
    ));
    assert!(matches!(
        again.handle(LocalRequest::GetTask {
            task_id: old.task_id
        }),
        LocalResponse::Task { .. }
    ));
    again.schedule().unwrap();
    drop(again);
    assert_eq!(
        bytes,
        fs::read(cfg.state_directory.join("journal.sqlite")).unwrap()
    );
    assert_eq!(
        marker,
        fs::read(cfg.state_directory.join(RECOVERY_MARKER)).unwrap()
    );
    assert!(Worker::open(cfg).is_err());
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn incompatible_inspection_does_not_migrate_but_keeps_normal_start_latched() {
    for version in [
        6,
        10,
        11,
        12,
        winbridge_worker::store::JOURNAL_SCHEMA_VERSION + 1,
    ] {
        let (_root, cfg, store) = setup();
        drop(store);
        db(&cfg)
            .pragma_update(None, "user_version", version)
            .unwrap();
        let before = fingerprint(&cfg);
        assert!(Worker::open_recovery(cfg.clone())
            .err()
            .unwrap()
            .contains("no migration performed"));
        assert!(cfg.state_directory.join(RECOVERY_MARKER).is_file());
        assert!(Worker::open(cfg.clone())
            .err()
            .unwrap()
            .contains("Recovery marker"));
        assert_eq!(before, fingerprint(&cfg));
        assert_eq!(
            db(&cfg)
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            version
        );
    }
}

#[test]
fn recovery_requires_exclusive_lock_existing_db_and_retains_incomplete_marker() {
    let (_root, cfg, store) = setup();
    drop(store);
    let owner = Worker::open(cfg.clone()).unwrap();
    assert!(Worker::open_recovery(cfg.clone())
        .err()
        .unwrap()
        .contains("another worker"));
    assert!(!cfg.state_directory.join(RECOVERY_MARKER).exists());
    drop(owner);
    fs::write(cfg.state_directory.join(RECOVERY_MARKER), b"").unwrap();
    assert!(Worker::open(cfg.clone())
        .err()
        .unwrap()
        .contains("Recovery marker"));
    drop(Worker::open_recovery(cfg.clone()).unwrap());
    assert_eq!(
        fs::read(cfg.state_directory.join(RECOVERY_MARKER)).unwrap(),
        b""
    );
    fs::remove_file(cfg.state_directory.join("journal.sqlite")).unwrap();
    assert!(Worker::open_recovery(cfg.clone())
        .err()
        .unwrap()
        .contains("existing journal"));
    assert!(!cfg.state_directory.join("journal.sqlite").exists());
}

#[test]
fn legacy_inspection_inventory_is_bounded_without_losing_total_count() {
    let (_root, cfg, mut store) = setup();
    for n in 0..52 {
        accept(&mut store, &cfg, &format!("legacy-{n:03}"));
    }
    db(&cfg)
        .execute_batch(
            "UPDATE tasks SET record=json_set(record,'$.status','running'),revision=revision+1;",
        )
        .unwrap();
    drop(store);
    let worker = Worker::open_recovery(cfg.clone()).unwrap();
    let inventory = info(&worker).resource_recovery.unwrap();
    assert_eq!(inventory.unresolved_count, 52);
    assert_eq!(inventory.resources.len(), 50);
    assert!(inventory.truncated);
    assert!(inventory.resources.iter().all(|r| !r.physical_binding));
    assert_eq!(
        db(&cfg)
            .query_row("SELECT count(*) FROM resource_leases", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

struct Server(Child, Vec<u32>);

fn legacy_ready_application(store: &mut Store) -> String {
    let source = request("app").command;
    let flow = store
        .accept_workflow(WorkflowSubmission {
            protocol_version: 1,
            request_id: "flow".into(),
            idempotency_key: "flow".into(),
            name: "legacy application".into(),
            project_id: "fixture".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 60000,
            build: source.clone(),
            start: source.clone(),
            readiness: ReadinessProbe {
                stdout_contains: "READY".into(),
                timeout_ms: 1000,
                netwatch: None,
            },
            log_path: "app.log".into(),
            artifacts: vec![],
        })
        .unwrap();
    let mut flow = store.workflow_snapshot(&flow.workflow_id).unwrap();
    flow.status = TaskStatus::Running;
    store.save_workflow(&mut flow).unwrap();
    let mut child = request("child");
    child.snapshot_id = Some("snapshot".into());
    store
        .accept_workflow_task(
            &mut flow,
            2,
            child,
            ResolvedCommand {
                executable: "not-executed".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 60000,
            },
            Some(VerifiedSnapshotRef {
                snapshot_id: "snapshot".into(),
                manifest_sha256: "a".repeat(64),
                source: SnapshotSource::GitCommit {
                    commit: "b".repeat(40),
                },
            }),
            None,
        )
        .unwrap();
    let id = flow.application_id.clone().unwrap();
    let mut app = store.application_snapshot(&id).unwrap();
    app.status = ApplicationStatus::Ready;
    store.save_application(&mut app).unwrap();
    id
}

#[test]
fn first_application_read_after_startup_is_reconciled_without_scheduling() {
    let (root, cfg, mut store) = setup();
    let id = legacy_ready_application(&mut store);
    db(&cfg)
        .execute_batch(
            "UPDATE tasks SET record=json_set(record,'$.status','running'),revision=revision+1;",
        )
        .unwrap();
    drop(store);
    let worker = Worker::open(cfg).unwrap();
    let LocalResponse::Application { application } =
        worker.handle(LocalRequest::GetApplication { application_id: id })
    else {
        panic!()
    };
    assert_eq!(application.status, ApplicationStatus::Indeterminate);
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn startup_application_write_fault_keeps_inspection_available_and_rejects_stale_result() {
    let (root, cfg, mut store) = setup();
    let id = legacy_ready_application(&mut store);
    db(&cfg).execute_batch("UPDATE tasks SET record=json_set(record,'$.status','running'),revision=revision+1; CREATE TRIGGER app_fault BEFORE UPDATE ON applications BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    drop(store);
    let worker = Worker::open(cfg).unwrap();
    assert!(info(&worker).journal_recovery_required.is_some());
    assert!(matches!(
        worker.handle(LocalRequest::GetApplication { application_id: id }),
        LocalResponse::Error { .. }
    ));
    worker.schedule().unwrap();
    assert!(!root.path().join("runs.log").exists());
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        // Only this test's recorded groups, if the Unix adapter was abruptly
        // killed before its own process owner completed cleanup.
        #[cfg(unix)]
        for pid in &self.1 {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .status();
        }
    }
}
async fn start(cfg: &WorkerConfig, path: &Path, mode: &str) -> (Server, String) {
    let endpoint = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_winbridge-worker"));
    command
        .args(["--config", path.to_str().unwrap(), mode])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let server = Server(command.spawn().unwrap(), vec![]);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if matches!(
            winbridge_transport::call(&endpoint, &LocalRequest::Info).await,
            Ok(LocalResponse::Info { .. })
        ) {
            return (server, endpoint);
        }
        assert!(Instant::now() < deadline, "worker IPC startup timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn real_cli_inspection_serves_history_and_refuses_mutation_after_restart() {
    let (root, cfg, mut store) = setup();
    let task = accept(&mut store, &cfg, "queued");
    drop(store);
    let path = root.path().join("worker.local.json");
    fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    let before = fingerprint(&cfg);
    for _ in 0..2 {
        let (server, endpoint) = start(&cfg, &path, "serve-recovery").await;
        let LocalResponse::Info { info } =
            winbridge_transport::call(&endpoint, &LocalRequest::Info)
                .await
                .unwrap()
        else {
            panic!()
        };
        assert!(info.journal_inspection.is_some());
        assert!(matches!(
            winbridge_transport::call(
                &endpoint,
                &LocalRequest::GetTask {
                    task_id: task.task_id.clone()
                }
            )
            .await
            .unwrap(),
            LocalResponse::Task { .. }
        ));
        assert!(matches!(
            winbridge_transport::call(
                &endpoint,
                &LocalRequest::Submit {
                    request: request("queued")
                }
            )
            .await
            .unwrap(),
            LocalResponse::Error { .. }
        ));
        drop(server);
    }
    assert_eq!(before, fingerprint(&cfg));
    assert!(!root.path().join("runs.log").exists());
}

#[tokio::test]
async fn real_server_survives_scheduler_write_failure_and_services_accepted_tasks() {
    let (root, mut cfg, store) = setup();
    drop(store);
    let first_root = root.path().join("first");
    let second_root = root.path().join("second");
    fs::create_dir(&first_root).unwrap();
    fs::create_dir(&second_root).unwrap();
    cfg.projects[0].workspace_root = first_root.clone();
    let mut second = cfg.projects[0].clone();
    second.id = "second".into();
    second.workspace_root = second_root.clone();
    cfg.projects.push(second);
    let path = root.path().join("worker.local.json");
    fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    let (mut server, endpoint) = start(&cfg, &path, "serve").await;
    let mut running = request("already-running");
    if let CommandSource::Direct { arguments, .. } = &mut running.command {
        *arguments = vec!["long".into()];
    }
    let LocalResponse::Task { task: active } =
        winbridge_transport::call(&endpoint, &LocalRequest::Submit { request: running })
            .await
            .unwrap()
    else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let LocalResponse::Task { task } = winbridge_transport::call(
            &endpoint,
            &LocalRequest::GetTask {
                task_id: active.task_id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        if let Some(pid) = task.process_id {
            server.1.push(pid);
            break;
        }
        assert!(Instant::now() < deadline, "first process did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    db(&cfg).execute_batch("CREATE TRIGGER scheduler_fault BEFORE UPDATE ON tasks WHEN NEW.project='second' AND json_extract(NEW.record,'$.status')='running' BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    let mut queued = request("claim-fails");
    queued.project_id = "second".into();
    let LocalResponse::Task { task: queued } =
        winbridge_transport::call(&endpoint, &LocalRequest::Submit { request: queued })
            .await
            .unwrap()
    else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let LocalResponse::Info { info } =
            winbridge_transport::call(&endpoint, &LocalRequest::Info)
                .await
                .unwrap()
        else {
            panic!()
        };
        if info.journal_recovery_required.is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "scheduler did not report fault");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        server.0.try_wait().unwrap().is_none(),
        "scheduler fault killed the server"
    );
    let LocalResponse::Task { task } = winbridge_transport::call(
        &endpoint,
        &LocalRequest::GetTask {
            task_id: active.task_id.clone(),
        },
    )
    .await
    .unwrap() else {
        panic!()
    };
    assert_eq!(task.status, TaskStatus::Running);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let LocalResponse::Output { chunks } = winbridge_transport::call(
            &endpoint,
            &LocalRequest::ReadOutput {
                task_id: active.task_id.clone(),
                after_sequence: 0,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        if !chunks.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "accepted owner stopped draining output after scheduler fault"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for id in [&queued.task_id, &active.task_id] {
        assert!(matches!(
            winbridge_transport::call(
                &endpoint,
                &LocalRequest::Cancel {
                    task_id: id.clone()
                }
            )
            .await
            .unwrap(),
            LocalResponse::Task { .. }
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let LocalResponse::Task { task } = winbridge_transport::call(
            &endpoint,
            &LocalRequest::GetTask {
                task_id: active.task_id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        if task.status.is_terminal() {
            assert_eq!(task.status, TaskStatus::Cancelled);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "accepted owner did not complete cancellation"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    server.1.clear(); // Normal OS-owner cleanup succeeded.
    assert!(!second_root.join("runs.log").exists());
    assert_eq!(
        fs::read_to_string(first_root.join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[cfg(windows)]
#[test]
fn inspection_neither_resumes_nor_terminates_a_bound_suspended_windows_process() {
    use winbridge_platform::{
        process_identity::{observe_process, ProcessObservation},
        ManagedChild,
    };
    let (root, cfg, mut store) = setup();
    let task = accept(&mut store, &cfg, "suspended-inspection");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    let (child, _stdout, _stderr) = ManagedChild::prepare(
        &task.resolved_command,
        Some(&Store::execution_job_name(&token).unwrap()),
    )
    .unwrap();
    let identity = child.identity().unwrap().clone();
    store
        .bind_task_process(&token, child.id(), Some(&identity))
        .unwrap();
    drop(store);
    let before = fingerprint(&cfg);
    let worker = Worker::open_recovery(cfg.clone()).unwrap();
    assert!(info(&worker).resource_recovery.unwrap().resources[0].physical_binding);
    worker.schedule().unwrap();
    drop(worker);
    assert!(matches!(
        observe_process(&identity),
        ProcessObservation::Occupied(_)
    ));
    assert!(!root.path().join("runs.log").exists());
    assert_eq!(before, fingerprint(&cfg));
    drop(child); // Only this test's OS owner cleans its tree.
}
