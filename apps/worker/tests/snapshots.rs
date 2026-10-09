use std::{
    fs,
    path::Path,
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::{DeliverySettings, WorkerConfig};
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker};
use winbridge_workspace_sync as sync;
fn setup() -> (TempDir, WorkerConfig, ImportSubmission) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    for dir in ["source", "project", "shared"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    let repo = root.join("source");
    fs::create_dir(repo.join("src")).unwrap();
    fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"snapshot-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("Cargo.lock"),
        "version = 3\n[[package]]\nname = \"snapshot-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/main.rs"),
        "fn main() { println!(\"{}\", include_str!(\"../Привет.txt\")); }\n",
    )
    .unwrap();
    fs::write(repo.join("Привет.txt"), "snapshot build ✓").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let delivery = DeliverySettings {
        adapter: "shared-folder".into(),
        root: root.join("shared"),
        namespace: "WinBridge".into(),
        peer_root: None,
    };
    let ready = sync::publish(
        &delivery,
        &repo,
        &root.join("capture-state"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    let request = ImportSubmission {
        protocol_version: 1,
        request_id: "request".into(),
        project_id: "fixture".into(),
        snapshot_id: ready.snapshot_id,
        manifest_sha256: ready.manifest_sha256,
        idempotency_key: "import-key".into(),
        timeout_ms: 10000,
    };
    let config=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"snapshot-test","nodeId":"test-worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"delivery":{"adapter":"shared-folder","root":delivery.root,"namespace":"WinBridge"},"projects":[{"id":"fixture","workspaceRoot":root.join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    (temp, config, request)
}

#[test]
fn history_import_journal_failure_blocks_stale_reads_and_admission_until_restart() {
    let (_temp, config, mut request) = setup();
    request.snapshot_id = "missing-owned-package".into();
    let worker = Worker::open(config.clone()).unwrap();
    let id = accept(&worker, request).import_id;
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_import_result BEFORE UPDATE ON imports WHEN json_extract(NEW.record,'$.status') IN ('failed','indeterminate') BEGIN SELECT RAISE(ABORT,'expected import result failure'); END;").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) {
            if info.journal_recovery_required.is_some() {
                break;
            }
        }
        let _ = worker.schedule();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    for request in [
        LocalRequest::GetImport {
            import_id: id.clone(),
        },
        LocalRequest::CancelImport {
            import_id: id.clone(),
        },
        LocalRequest::ListImports,
        LocalRequest::History {
            query: HistoryQuery {
                project_id: None,
                kind: None,
                status: None,
                search: String::new(),
                before: None,
            },
        },
    ] {
        assert!(matches!(
            worker.handle(request),
            LocalResponse::Error { .. }
        ));
    }
    db.execute_batch("DROP TRIGGER reject_import_result;")
        .unwrap();
    drop(db);
    worker.stop();
    drop(worker);
    let worker = Worker::open(config).unwrap();
    assert_eq!(get(&worker, &id).status, TaskStatus::Interrupted);
    worker.stop();
}
fn git(root: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success());
}
fn accept(worker: &Worker, request: ImportSubmission) -> ImportRecord {
    let LocalResponse::Import { import } = worker.handle(LocalRequest::ImportSnapshot { request })
    else {
        panic!("import rejected")
    };
    *import
}
fn get(worker: &Worker, id: &str) -> ImportRecord {
    let LocalResponse::Import { import } = worker.handle(LocalRequest::GetImport {
        import_id: id.into(),
    }) else {
        panic!()
    };
    *import
}
fn wait(worker: &Arc<Worker>, id: &str) -> ImportRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let record = get(worker, id);
        if record.status.is_terminal() && worker.is_idle() {
            return record;
        }
        assert!(Instant::now() < deadline, "{record:?}");
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn discovery_requires_registration_and_missing_import_explains_admission_failure() {
    let (_temp, config, request) = setup();
    let worker = Worker::open(config).unwrap();
    assert!(
        matches!(worker.handle(LocalRequest::ListSnapshots {project_id:"unregistered".into(),offset:0}), LocalResponse::Error {message} if message.contains("not registered"))
    );
    assert!(
        matches!(worker.handle(LocalRequest::ListSnapshots {project_id:request.project_id.clone(),offset:0}), LocalResponse::Snapshots {page} if page.snapshots.len() == 1)
    );
    let command = TaskSubmission {
        protocol_version: 1,
        request_id: "missing".into(),
        project_id: request.project_id,
        snapshot_id: Some(request.snapshot_id),
        idempotency_key: "missing".into(),
        timeout_ms: 10000,
        command: CommandSource::Direct {
            executable: "unused".into(),
            arguments: vec![],
            working_directory: ".".into(),
        },
    };
    assert!(
        matches!(worker.handle(LocalRequest::Submit {request:command}), LocalResponse::Error {message} if message.contains("Сначала успешно импортируйте") && message.contains("процесс не запущен"))
    );
    assert!(
        matches!(worker.handle(LocalRequest::ListTasks), LocalResponse::Tasks {tasks} if tasks.is_empty())
    );
}

#[test]
fn import_acceptance_dedup_cancel_and_restart_are_durable() {
    let (_temp, config, request) = setup();
    let worker = Worker::open(config.clone()).unwrap();
    let first = accept(&worker, request.clone());
    let mut duplicate = request.clone();
    duplicate.request_id = "reconnect".into();
    assert_eq!(first.import_id, accept(&worker, duplicate).import_id);
    let mut conflict = request.clone();
    conflict.manifest_sha256 = "a".repeat(64);
    assert!(matches!(
        worker.handle(LocalRequest::ImportSnapshot { request: conflict }),
        LocalResponse::Error { .. }
    ));
    worker.handle(LocalRequest::CancelImport {
        import_id: first.import_id.clone(),
    });
    assert_eq!(
        wait(&worker, &first.import_id).status,
        TaskStatus::Cancelled
    );
    drop(worker);
    let worker = Worker::open(config).unwrap();
    assert_eq!(accept(&worker, request.clone()).import_id, first.import_id);
    let mut retry = request;
    retry.idempotency_key = "explicit-retry".into();
    let imported = accept(&worker, retry);
    assert_eq!(
        wait(&worker, &imported.import_id).status,
        TaskStatus::Succeeded
    );
}
#[test]
fn recovery_reconciles_verified_generation_and_refuses_incomplete_staging() {
    let (_temp, config, request) = setup();
    let worker = Worker::open(config.clone()).unwrap();
    let accepted = accept(&worker, request.clone());
    drop(worker);
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let mut import = store.get_import(&accepted.import_id).unwrap();
    import.status = TaskStatus::Running;
    store.save_import(&import).unwrap();
    drop(store);
    let worker = Worker::open(config.clone()).unwrap();
    assert_eq!(
        get(&worker, &accepted.import_id).status,
        TaskStatus::Interrupted
    );
    drop(worker);
    let worker = Worker::open(config.clone()).unwrap();
    let mut retry = request.clone();
    retry.idempotency_key = "published-before-crash".into();
    let accepted = accept(&worker, retry.clone());
    drop(worker);
    sync::import(
        config.delivery.as_ref().unwrap(),
        &config.projects[0].workspace_root,
        &retry,
        &accepted.import_id,
        |_, _, _| Ok(()),
    )
    .unwrap();
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let mut import = store.get_import(&accepted.import_id).unwrap();
    import.status = TaskStatus::Running;
    store.save_import(&import).unwrap();
    drop(store);
    let worker = Worker::open(config).unwrap();
    assert_eq!(
        get(&worker, &accepted.import_id).status,
        TaskStatus::Succeeded
    );
    assert!(worker.is_idle());
}
#[cfg(unix)]
#[test]
fn real_build_from_verified_generation_and_tamper_blocks_process_launch() {
    let (temp, config, request) = setup();
    let worker = Worker::open(config.clone()).unwrap();
    let imported = accept(&worker, request.clone());
    assert_eq!(
        wait(&worker, &imported.import_id).status,
        TaskStatus::Succeeded
    );
    let submission = TaskSubmission {
        protocol_version: 1,
        request_id: "build".into(),
        project_id: "fixture".into(),
        idempotency_key: "build".into(),
        timeout_ms: 30000,
        snapshot_id: Some(request.snapshot_id.clone()),
        command: CommandSource::Direct {
            executable: "/home/user1/.local/bin/cargo".into(),
            arguments: vec![
                "build".into(),
                "--locked".into(),
                "--offline".into(),
                "--target-dir".into(),
                temp.path()
                    .join("project/build-cache")
                    .to_string_lossy()
                    .into(),
            ],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
        request: submission.clone(),
    }) else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) else {
            panic!()
        };
        if current.status.is_terminal() {
            assert_eq!(current.exit_code, Some(0), "{current:?}");
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    while !worker.is_idle() {
        thread::sleep(Duration::from_millis(5));
    }
    let output = Command::new(
        temp.path()
            .join("project/build-cache/debug/snapshot-fixture"),
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("snapshot build ✓"));
    assert!(task
        .snapshot
        .as_ref()
        .is_some_and(|s| s.manifest_sha256 == request.manifest_sha256));
    let mut unbound = submission.clone();
    unbound.snapshot_id = None;
    unbound.idempotency_key = "unbound-source-cwd".into();
    if let CommandSource::Direct {
        working_directory, ..
    } = &mut unbound.command
    {
        *working_directory = format!("snapshots/{}", request.snapshot_id);
    }
    assert!(matches!(
        worker.handle(LocalRequest::Submit { request: unbound }),
        LocalResponse::Error { .. }
    ));
    let mut second = submission.clone();
    second.idempotency_key = "tampered-build".into();
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit { request: second })
    else {
        panic!()
    };
    fs::write(
        sync::generation_path(&config.projects[0].workspace_root, &request.snapshot_id)
            .unwrap()
            .join("src/main.rs"),
        "tampered",
    )
    .unwrap();
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) else {
            panic!()
        };
        if current.status.is_terminal() {
            assert_eq!(current.status, TaskStatus::Failed);
            assert!(current.process_id.is_none());
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    while !worker.is_idle() {
        thread::sleep(Duration::from_millis(5));
    }
    let mut third = submission;
    third.idempotency_key = "replaced-manifest-build".into();
    let LocalResponse::Task { task: bound } =
        worker.handle(LocalRequest::Submit { request: third })
    else {
        panic!()
    };
    let generation =
        sync::generation_path(&config.projects[0].workspace_root, &request.snapshot_id).unwrap();
    let manifest_path = generation.join(".winbridge/manifest.json");
    let marker_path = generation.join(".winbridge/verified.json");
    let mut manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let file = manifest
        .files
        .iter_mut()
        .find(|f| f.path == "src/main.rs")
        .unwrap();
    manifest.total_bytes = manifest.total_bytes - file.bytes + 8;
    file.bytes = 8;
    file.sha256 = sync::sha256(b"tampered");
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let mut ready: SnapshotReady =
        serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    ready.manifest_sha256 = sync::sha256(&bytes);
    fs::write(manifest_path, &bytes).unwrap();
    fs::write(marker_path, serde_json::to_vec(&ready).unwrap()).unwrap();
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
            task_id: bound.task_id.clone(),
        }) else {
            panic!()
        };
        if current.status.is_terminal() {
            assert_eq!(current.status, TaskStatus::Failed);
            assert!(current.process_id.is_none());
            assert!(current.reason.unwrap().contains("manifest hash mismatch"));
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn project_lease_keeps_import_queued_until_command_has_terminated() {
    let (_temp, config, request) = setup();
    let worker = Worker::open(config).unwrap();
    let command = TaskSubmission {
        protocol_version: 1,
        request_id: "command".into(),
        project_id: "fixture".into(),
        idempotency_key: "command".into(),
        timeout_ms: 10000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec!["long".into()],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit { request: command })
    else {
        panic!()
    };
    worker.schedule().unwrap();
    let imported = accept(&worker, request);
    for _ in 0..10 {
        worker.schedule().unwrap();
        assert_eq!(get(&worker, &imported.import_id).status, TaskStatus::Queued);
        thread::sleep(Duration::from_millis(10));
    }
    worker.handle(LocalRequest::Cancel {
        task_id: task.task_id,
    });
    assert_eq!(
        wait(&worker, &imported.import_id).status,
        TaskStatus::Succeeded
    );
}

#[test]
fn schema_migration_takes_consistent_backup_and_preserves_accepted_commands() {
    let (_temp, config, _request) = setup();
    let worker = Worker::open(config.clone()).unwrap();
    let request = TaskSubmission {
        protocol_version: 1,
        request_id: "old".into(),
        project_id: "fixture".into(),
        idempotency_key: "old".into(),
        timeout_ms: 10000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec!["success".into()],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit { request }) else {
        panic!()
    };
    drop(worker);
    let path = config.state_directory.join("journal.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE output_cleanup; DROP TABLE attention; DROP TABLE applications; DROP TABLE workflows; DROP TABLE imports; PRAGMA user_version=1;")
        .unwrap();
    drop(connection);
    let worker = Worker::open(config.clone()).unwrap();
    let LocalResponse::Task { task: recovered } = worker.handle(LocalRequest::GetTask {
        task_id: task.task_id.clone(),
    }) else {
        panic!()
    };
    assert_eq!(recovered.status, TaskStatus::Queued);
    let backups: Vec<_> = fs::read_dir(&config.state_directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v1-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open_with_flags(
        &backups[0],
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    drop(worker);
    let _worker = Worker::open(config).unwrap();
}
