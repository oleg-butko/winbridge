use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::{config::WorkerConfig, update::SOURCE_MANIFEST_NAME};
use winbridge_protocol::{update::*, *};
use winbridge_worker::{Worker, UPDATE_STARTUP_GUARD};

const SOURCE: &str = "[package]\nname='coordination-fixture'\nversion='0.1.0'\n";
struct Owner(Arc<Worker>);
impl Drop for Owner {
    fn drop(&mut self) {
        if let LocalResponse::Tasks { tasks } = self.0.handle(LocalRequest::ListTasks) {
            for task in tasks {
                if !task.status.is_terminal() {
                    self.0.handle(LocalRequest::Cancel {
                        task_id: task.task_id,
                    });
                }
            }
        }
        self.0.stop();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.0.is_idle() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn config(root: &Path) -> (WorkerConfig, String) {
    for id in ["source", "other", "third"] {
        fs::create_dir(root.join(id)).unwrap();
    }
    // Fixture logs are build output, outside the manifest-owned source set.
    fs::create_dir(root.join("source/target")).unwrap();
    let manifest = SourceManifest {
        schema_version: 1,
        source_commit: "a".repeat(40),
        dirty: false,
        created_at: "2026-10-17T00:00:00Z".into(),
        includes_built_ui: false,
        files: vec![SourceFile {
            path: "Cargo.toml".into(),
            bytes: SOURCE.len() as u64,
            sha256: hash(SOURCE.as_bytes()),
        }],
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    fs::write(root.join("source/Cargo.toml"), SOURCE).unwrap();
    fs::write(root.join("source").join(SOURCE_MANIFEST_NAME), &bytes).unwrap();
    (serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("update-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"workScheduling":{"maxConcurrentWork":3},"projects":(["source","other","third"].map(|id|serde_json::json!({"id":id,"workspaceRoot":root.join(id),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]})))})).unwrap(), hash(&bytes))
}
fn source_spec(w: &Worker, root: &Path, baseline: &str) -> WorkerUpdateSpec {
    w.update_source_spec(
        &uuid::Uuid::new_v4().to_string(),
        &uuid::Uuid::new_v4().to_string(),
        1,
        &root.join("source"),
        (baseline, baseline),
    )
    .unwrap()
}
fn command(id: &str, revision: u64, action: WorkerUpdateAction) -> WorkerUpdateCommand {
    WorkerUpdateCommand {
        command_id: id.into(),
        expected_revision: revision,
        action,
        source_manifest_sha256: None,
        reason: format!("Verified {id}"),
    }
}
fn submit(w: &Worker, project: &str, key: &str, mode: &str) -> LocalResponse {
    w.handle(LocalRequest::Submit {
        request: TaskSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            project_id: project.into(),
            timeout_ms: 120_000,
            snapshot_id: None,
            command: CommandSource::Direct {
                executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                arguments: vec![mode.into()],
                working_directory: if project == "source" {
                    "target".into()
                } else {
                    ".".into()
                },
            },
        },
    })
}
fn accepted(response: LocalResponse) -> TaskRecord {
    let LocalResponse::Task { task } = response else {
        panic!("Task not accepted: {response:?}")
    };
    *task
}
fn task(w: &Worker, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } = w.handle(LocalRequest::GetTask { task_id: id.into() })
    else {
        panic!("task")
    };
    *task
}
async fn until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(Instant::now() < deadline, "Update coordination deadline");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn queue(w: &Worker, command: QueueControlCommand) -> QueueControlResponse {
    w.handle_queue_control(QueueControlRequest {
        queue_control_version: 1,
        context: w.request_context().unwrap(),
        command,
    })
}
fn queue_view(w: &Worker) -> QueueControlObservation {
    let QueueControlResult::Observed { observation, .. } =
        queue(w, QueueControlCommand::Observe { command_id: None }).result
    else {
        panic!("queue view")
    };
    observation
}
async fn observation(endpoint: &str) -> serde_json::Value {
    let LocalResponse::Info { info } = winbridge_transport::call(endpoint, &LocalRequest::Info)
        .await
        .unwrap()
    else {
        panic!("native Info")
    };
    assert!(info.journal_recovery_required.is_none());
    assert!(info.maintenance_error.is_none());
    serde_json::json!({"instanceId":info.instance_id,"generation":info.generation,"build":info.build,"fencedRequests":info.fenced_requests,"updateCoordination":info.update_coordination,"journalRecoveryRequired":info.journal_recovery_required,"maintenanceError":info.maintenance_error})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn real_source_lease_drain_physical_quiescence_and_verified_release() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, baseline) = config(dir.path());
    let endpoint = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let worker = Worker::open(cfg.clone()).unwrap();
    let owner = Owner(worker.clone());
    let a = accepted(submit(&worker, "source", "source", "long"));
    let b = accepted(submit(&worker, "other", "other", "long"));
    let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
    until(|| {
        [a.task_id.as_str(), b.task_id.as_str()]
            .iter()
            .all(|id| task(&worker, id).process_id.is_some())
    })
    .await;
    let b_pid = task(&worker, &b.task_id).process_id.unwrap();
    let spec = source_spec(&worker, dir.path(), &baseline);
    assert_eq!(spec.projects, vec!["source"]);
    assert_eq!(
        worker.begin_source_update(spec.clone()).unwrap().phase,
        WorkerUpdatePhase::SourcePending
    );
    let mut views = vec![observation(&endpoint).await];
    assert_eq!(
        accepted(submit(&worker, "source", "source", "long")).task_id,
        a.task_id
    );
    assert!(matches!(
        submit(&worker, "source", "blocked", "success"),
        LocalResponse::Error { .. }
    ));
    assert!(worker
        .advance_source_update(
            &spec,
            &command("source-grant", 0, WorkerUpdateAction::AcquireSource)
        )
        .is_err());
    let c = accepted(submit(&worker, "third", "third", "success"));
    until(|| task(&worker, &c.task_id).status == TaskStatus::Succeeded).await;
    worker.handle(LocalRequest::Cancel {
        task_id: a.task_id.clone(),
    });
    until(|| task(&worker, &a.task_id).status == TaskStatus::Cancelled).await;
    // The terminal task record can precede thread teardown; poll the grant,
    // which must also observe the in-memory owner registry.
    let acquire = command("source-grant", 0, WorkerUpdateAction::AcquireSource);
    until(|| worker.advance_source_update(&spec, &acquire).is_ok()).await;
    views.push(observation(&endpoint).await);
    assert_eq!(task(&worker, &b.task_id).process_id, Some(b_pid));
    assert_eq!(task(&worker, &b.task_id).status, TaskStatus::Running);
    assert!(matches!(
        submit(&worker, "source", "held", "success"),
        LocalResponse::Error { .. }
    ));
    assert_eq!(
        worker
            .advance_source_update(&spec, &command("drain", 1, WorkerUpdateAction::BeginDrain))
            .unwrap()
            .phase,
        WorkerUpdatePhase::Draining
    );
    views.push(observation(&endpoint).await);
    assert!(matches!(
        submit(&worker, "third", "draining", "success"),
        LocalResponse::Error { .. }
    ));
    let close = command("close", 2, WorkerUpdateAction::CloseDispatch);
    assert!(worker.advance_source_update(&spec, &close).is_err());
    assert_eq!(task(&worker, &b.task_id).process_id, Some(b_pid));
    worker.handle(LocalRequest::Cancel {
        task_id: b.task_id.clone(),
    });
    until(|| task(&worker, &b.task_id).status == TaskStatus::Cancelled && worker.is_idle()).await;
    let pause = QueueControlCommand::Set {
        command_id: "user-pause".into(),
        expected_revision: queue_view(&worker).revision,
        mode: QueueMode::DispatchPaused,
    };
    assert!(matches!(
        queue(&worker, pause.clone()).result,
        QueueControlResult::Applied { .. }
    ));
    assert_eq!(
        worker.advance_source_update(&spec, &close).unwrap().phase,
        WorkerUpdatePhase::Quiescing
    );
    views.push(observation(&endpoint).await);
    // Lost ACKs remain readable after normal mutations are frozen.
    assert!(matches!(
        queue(&worker, pause).result,
        QueueControlResult::Applied { .. }
    ));
    assert!(matches!(
        queue(
            &worker,
            QueueControlCommand::Set {
                command_id: "cannot-thaw-update".into(),
                expected_revision: queue_view(&worker).revision,
                mode: QueueMode::Running
            }
        )
        .result,
        QueueControlResult::Rejected { .. }
    ));
    assert!(matches!(
        worker.handle(LocalRequest::ReadOutput {
            task_id: c.task_id.clone(),
            after_sequence: 0
        }),
        LocalResponse::Output { .. }
    ));
    assert_eq!(
        worker
            .advance_source_update(
                &spec,
                &command("physical", 3, WorkerUpdateAction::ConfirmQuiesced)
            )
            .unwrap()
            .phase,
        WorkerUpdatePhase::Quiesced
    );
    // Cover periodic reconciliation after dispatch and journal writes close.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    views.push(observation(&endpoint).await);
    let mut release = command("release", 4, WorkerUpdateAction::Release);
    release.source_manifest_sha256 = Some(baseline.clone());
    fs::write(
        dir.path().join("source/Cargo.toml"),
        "LOCAL EDIT MUST NOT BE OVERWRITTEN",
    )
    .unwrap();
    assert!(worker
        .advance_source_update(&spec, &release)
        .unwrap_err()
        .contains("Source verification failed"));
    assert!(cfg.state_directory.join(UPDATE_STARTUP_GUARD).exists());
    assert_eq!(queue_view(&worker).mode, QueueMode::DispatchPaused);
    fs::write(dir.path().join("source/Cargo.toml"), SOURCE).unwrap();
    assert_eq!(
        worker.advance_source_update(&spec, &release).unwrap().phase,
        WorkerUpdatePhase::Released
    );
    assert!(!cfg.state_directory.join(UPDATE_STARTUP_GUARD).exists());
    views.push(observation(&endpoint).await);
    assert_eq!(queue_view(&worker).mode, QueueMode::DispatchPaused);
    let d = accepted(submit(&worker, "source", "after-release", "success"));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(task(&worker, &d.task_id).status, TaskStatus::Queued);
    assert!(matches!(
        queue(
            &worker,
            QueueControlCommand::Set {
                command_id: "user-resume".into(),
                expected_revision: queue_view(&worker).revision,
                mode: QueueMode::Running
            }
        )
        .result,
        QueueControlResult::Applied { .. }
    ));
    until(|| task(&worker, &d.task_id).status == TaskStatus::Succeeded && worker.is_idle()).await;
    // A receipt from an earlier update cannot erase a later startup barrier.
    let next = source_spec(&worker, dir.path(), &baseline);
    worker.begin_source_update(next.clone()).unwrap();
    worker.advance_source_update(&spec, &release).unwrap();
    assert!(cfg.state_directory.join(UPDATE_STARTUP_GUARD).exists());
    worker
        .advance_source_update(
            &next,
            &command("abort-before-grant", 0, WorkerUpdateAction::Release),
        )
        .unwrap();
    assert!(!cfg.state_directory.join(UPDATE_STARTUP_GUARD).exists());
    if let Some(path) = std::env::var_os("WINBRIDGE_UPDATE_COORDINATION_REPORT") {
        let proof = serde_json::json!({"schemaVersion":1,"passed":true,"platform":std::env::consts::OS,"observations":views,"unrelatedTaskId":b.task_id,"unrelatedPidPreserved":b_pid,"sourceTaskCancelledExplicitly":true,"userPausePreserved":true,"modifiedSourceReleaseRejected":true,"staleReleasePreservedNewGuard":true,"runtimeApplied":false,"windowsOpened":false});
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
    worker.stop();
    server.await.unwrap().unwrap();
    drop(owner);
    drop(worker);
}

#[test]
fn config_changes_wait_and_pending_saved_config_blocks_new_update() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, baseline) = config(dir.path());
    let path = dir.path().join("worker.local.json");
    fs::write(&path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    let worker = Worker::open_file(&path).unwrap();
    let owner = Owner(worker.clone());
    let spec = source_spec(&worker, dir.path(), &baseline);
    worker.begin_source_update(spec.clone()).unwrap();
    let settings = worker.runtime_settings().unwrap();
    let mut next = serde_json::to_value(&cfg).unwrap();
    next["maxOutputBytesPerTask"] = serde_json::json!(8192);
    let save = SaveRuntimeSettings {
        generation: settings.generation,
        request_id: uuid::Uuid::new_v4().to_string(),
        expected_sha256: settings.saved_sha256,
        json: serde_json::to_string(&next).unwrap(),
    };
    let before = fs::read(&path).unwrap();
    assert!(worker
        .save_runtime_settings(&save)
        .unwrap_err()
        .contains("waits until update"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!fs::read_dir(dir.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".before-")));
    worker
        .advance_source_update(&spec, &command("abort", 0, WorkerUpdateAction::Release))
        .unwrap();
    worker.save_runtime_settings(&save).unwrap();
    let next = source_spec(&worker, dir.path(), &baseline);
    assert!(worker
        .begin_source_update(next)
        .unwrap_err()
        .contains("ConfigRestartPending"));
    assert!(!cfg.state_directory.join(UPDATE_STARTUP_GUARD).exists());
    drop(owner);
    drop(worker);
}

#[test]
fn startup_guard_blocks_migration_and_lost_guard_does_not_release_database_lease() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, baseline) = config(dir.path());
    fs::create_dir(&cfg.state_directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cfg.state_directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = cfg.state_directory.join("journal.sqlite");
    drop(winbridge_worker::store::Store::open(&path).unwrap());
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; PRAGMA user_version=14;").unwrap();
    drop(c);
    let before = fs::read(&path).unwrap();
    let guard = cfg.state_directory.join(UPDATE_STARTUP_GUARD);
    fs::write(&guard, b"incomplete startup barrier").unwrap();
    assert!(Worker::open(cfg.clone())
        .err()
        .unwrap()
        .contains("startup guard"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!fs::read_dir(&cfg.state_directory).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("journal-v14-")));
    // This is only a deliberately malformed marker in an owned test directory.
    fs::remove_file(&guard).unwrap();
    let w = Worker::open(cfg.clone()).unwrap();
    assert!(fs::read_dir(&cfg.state_directory).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("journal-v14-")));
    let spec = source_spec(&w, dir.path(), &baseline);
    w.begin_source_update(spec.clone()).unwrap();
    w.stop();
    drop(w);
    fs::remove_file(&guard).unwrap();
    assert!(Worker::open(cfg.clone())
        .err()
        .unwrap()
        .contains("unfinished"));
    let readonly = Worker::open_recovery(cfg.clone()).unwrap();
    assert!(readonly
        .advance_source_update(&spec, &command("release", 0, WorkerUpdateAction::Release))
        .is_err());
    let LocalResponse::Info { info } = readonly.handle(LocalRequest::Info) else {
        panic!("readonly")
    };
    assert_eq!(info.update_coordination.unwrap()["readOnly"], true);
    readonly.stop();
    drop(readonly);
    let destination = dir.path().join("forbidden-recovery");
    assert!(winbridge_worker::prepare_recovery(&cfg, &destination)
        .unwrap_err()
        .contains("unfinished"));
    assert!(!destination.exists());
}

#[test]
#[ignore = "native gate provides a copied schema-14 reference binary"]
fn previous_worker_refuses_schema_fifteen_without_mutating_journal() {
    let executable = std::env::var_os("WINBRIDGE_UPDATE_REFERENCE_WORKER")
        .expect("Explicit schema-14 reference worker required");
    let dir = tempfile::tempdir().unwrap();
    let (cfg, _) = config(dir.path());
    fs::create_dir(&cfg.state_directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cfg.state_directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = cfg.state_directory.join("journal.sqlite");
    drop(winbridge_worker::store::Store::open(&path).unwrap());
    let before = fs::read(&path).unwrap();
    let config_path = dir.path().join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    let mut cmd = std::process::Command::new(executable);
    cmd.args(["--config", config_path.to_str().unwrap(), "serve"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Owned reference worker failed to refuse schema 15");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("journal schema 15 is unsupported"));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn fixture_build_identity_query_never_runs_fixture_or_writes_into_cwd() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_winbridge-fixture"))
        .arg("--build-info")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let identity: winbridge_protocol::BuildIdentity =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
    assert!(!identity.build_id.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}
