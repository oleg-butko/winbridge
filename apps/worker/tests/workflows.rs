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

struct WorkerOwner(Arc<Worker>);
impl std::ops::Deref for WorkerOwner {
    type Target = Arc<Worker>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for WorkerOwner {
    fn drop(&mut self) {
        self.0.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.0.is_idle() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
        .status
        .success());
}
fn setup() -> (TempDir, WorkerConfig, WorkflowSubmission) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    for dir in ["source/src", "project", "shared"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let source = root.join("source");
    fs::write(
        source.join("Cargo.toml"),
        include_str!("../../../fixtures/snapshot-project/Cargo.toml"),
    )
    .unwrap();
    fs::write(
        source.join("Cargo.lock"),
        include_str!("../../../fixtures/snapshot-project/Cargo.lock"),
    )
    .unwrap();
    fs::write(
        source.join("src/main.rs"),
        include_str!("../../../fixtures/snapshot-project/src/main.rs"),
    )
    .unwrap();
    fs::write(
        source.join("Привет.txt"),
        "Проверенный commit → Windows build ✓\n",
    )
    .unwrap();
    git(&source, &["init", "-q"]);
    git(&source, &["add", "."]);
    git(
        &source,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "workflow fixture",
        ],
    );
    let delivery = DeliverySettings {
        adapter: "shared-folder".into(),
        root: root.join("shared"),
        namespace: "WinBridge".into(),
        peer_root: None,
    };
    let ready = winbridge_workspace_sync::publish(
        &delivery,
        &source,
        &root.join("capture"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    let project = root.join("project");
    let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("wf-{}",uuid::Uuid::new_v4()),"nodeId":"fixture-worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"delivery":{"adapter":"shared-folder","root":delivery.root,"namespace":"WinBridge"},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    let request = WorkflowSubmission {
        protocol_version: 1,
        request_id: "request".into(),
        idempotency_key: "startup".into(),
        name: "fixture startup".into(),
        project_id: "fixture".into(),
        snapshot_id: ready.snapshot_id,
        manifest_sha256: ready.manifest_sha256,
        timeout_ms: 30000,
        build: CommandSource::Direct {
            executable: "cargo".into(),
            arguments: vec![
                "build".into(),
                "--locked".into(),
                "--offline".into(),
                "--target-dir".into(),
                project.join("build-cache").to_string_lossy().into(),
            ],
            working_directory: ".".into(),
        },
        start: CommandSource::Direct {
            executable: project
                .join(format!(
                    "build-cache/debug/snapshot-fixture{}",
                    std::env::consts::EXE_SUFFIX
                ))
                .to_string_lossy()
                .into(),
            arguments: vec![
                "serve".into(),
                project.join("logs/app.log").to_string_lossy().into(),
                "ready".into(),
            ],
            working_directory: ".".into(),
        },
        readiness: ReadinessProbe {
            stdout_contains: "READY Привет ✓".into(),
            timeout_ms: 5000,
            netwatch: None,
        },
        log_path: "logs/app.log".into(),
        artifacts: vec![],
    };
    (temp, config, request)
}
fn flow(worker: &Worker, id: &str) -> WorkflowRecord {
    let LocalResponse::Workflow { workflow } = worker.handle(LocalRequest::GetWorkflow {
        workflow_id: id.into(),
    }) else {
        panic!("workflow missing")
    };
    *workflow
}
fn accept(worker: &Worker, request: WorkflowSubmission) -> WorkflowRecord {
    let response = worker.handle(LocalRequest::SubmitWorkflow {
        request: Box::new(request),
    });
    let LocalResponse::Workflow { workflow } = response else {
        panic!("{response:?}")
    };
    *workflow
}

#[test]
fn artifacts_link_real_bytes_to_the_build_and_missing_file_prevents_start() {
    use sha2::{Digest, Sha256};
    let (_temp, config, mut request) = setup();
    request.artifacts = vec![ArtifactSpec {
        name: "fixture executable".into(),
        path: format!(
            "build-cache/debug/snapshot-fixture{}",
            std::env::consts::EXE_SUFFIX
        ),
    }];
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let done = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    assert_eq!(done.status, TaskStatus::Succeeded, "{done:?}");
    let artifact = &done.artifacts[0];
    let bytes = fs::read(&artifact.path).unwrap();
    assert_eq!(artifact.bytes, bytes.len() as u64);
    assert_eq!(artifact.sha256, format!("{:x}", Sha256::digest(bytes)));
    assert_eq!(
        artifact.build_task_id,
        done.steps[1].operation_id.clone().unwrap()
    );
    assert_eq!(artifact.manifest_sha256, request.manifest_sha256);
    stop(&worker, done.application_id.as_ref().unwrap());
    request.idempotency_key = "missing-artifact".into();
    request.artifacts[0].path = "build-cache/debug/missing.exe".into();
    let failed = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed.application_id.is_none());
    assert!(failed.steps[2].operation_id.is_none());
    assert!(failed
        .reason
        .unwrap()
        .contains("artifact verification failed"));
    request.idempotency_key = "unsafe-artifact".into();
    request.artifacts[0].path = "../outside.exe".into();
    assert!(matches!(
        worker.handle(LocalRequest::SubmitWorkflow {
            request: Box::new(request)
        }),
        LocalResponse::Error { .. }
    ));
}
fn app(worker: &Worker, id: &str) -> ApplicationRecord {
    let LocalResponse::Application { application } = worker.handle(LocalRequest::GetApplication {
        application_id: id.into(),
    }) else {
        panic!()
    };
    application
}
fn until(worker: &Arc<Worker>, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        worker.schedule().unwrap();
        if check() {
            return;
        }
        assert!(Instant::now() < deadline, "fixture deadline exceeded");
        thread::sleep(Duration::from_millis(5));
    }
}
fn complete(worker: &Arc<Worker>, id: &str) -> WorkflowRecord {
    until(worker, || flow(worker, id).status.is_terminal());
    flow(worker, id)
}
fn stop(worker: &Arc<Worker>, id: &str) {
    worker.handle(LocalRequest::StopApplication {
        application_id: id.into(),
    });
    until(worker, || {
        app(worker, id).status.is_terminal() && worker.is_idle()
    });
}

#[test]
fn actual_pipeline_dedup_readiness_log_and_owner_leases() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let accepted = accept(&worker, request.clone());
    let mut duplicate = request.clone();
    duplicate.request_id = "lost-ack".into();
    assert_eq!(accept(&worker, duplicate).workflow_id, accepted.workflow_id);
    let mut conflict = request.clone();
    conflict.name = "different parameters".into();
    assert!(matches!(
        worker.handle(LocalRequest::SubmitWorkflow {
            request: Box::new(conflict)
        }),
        LocalResponse::Error { .. }
    ));
    let done = complete(&worker, &accepted.workflow_id);
    assert_eq!(done.status, TaskStatus::Succeeded, "{done:?}");
    assert!(done
        .steps
        .iter()
        .all(|s| s.status == WorkflowStepStatus::Succeeded));
    assert!(done.steps[..3].iter().all(|s| s.operation_id.is_some()));
    let id = done.application_id.unwrap();
    let ready = app(&worker, &id);
    assert_eq!(ready.status, ApplicationStatus::Ready);
    assert!(ready.process_id.is_some());
    assert_eq!(
        accept(&worker, request).workflow_id,
        accepted.workflow_id,
        "dedup after ready was rejected by active application policy"
    );
    let LocalResponse::Log { page } = worker.handle(LocalRequest::ReadApplicationLog {
        application_id: id.clone(),
        cursor: None,
        max_bytes: 65536,
    }) else {
        panic!()
    };
    assert!(String::from_utf8(page.bytes).unwrap().contains("Привет ✓"));
    let submission = |key: &str, mode: &str| TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "fixture".into(),
        idempotency_key: key.into(),
        timeout_ms: 10000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task: long } = worker.handle(LocalRequest::Submit {
        request: submission("long", "long"),
    }) else {
        panic!()
    };
    until(
        &worker,
        || matches!(worker.handle(LocalRequest::GetTask {task_id:long.task_id.clone()}),LocalResponse::Task {task} if task.process_id.is_some()),
    );
    worker.handle(LocalRequest::StopApplication {
        application_id: id.clone(),
    });
    let LocalResponse::Task { task: queued } = worker.handle(LocalRequest::Submit {
        request: submission("after-app-stop", "success"),
    }) else {
        panic!()
    };
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Stopped
    });
    for _ in 0..10 {
        worker.schedule().unwrap();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(worker.handle(LocalRequest::GetTask {task_id:queued.task_id.clone()}),LocalResponse::Task {task} if task.status==TaskStatus::Queued),
        "old application cleanup released a newer task's lease"
    );
    worker.handle(LocalRequest::Cancel {
        task_id: long.task_id.clone(),
    });
    until(&worker, || {
        matches!(worker.handle(LocalRequest::GetTask {task_id:queued.task_id.clone()}),LocalResponse::Task {task} if task.status==TaskStatus::Succeeded)
            && worker.is_idle()
    });
}

#[test]
fn ignored_readiness_resource_handoff_keeps_mutation_reserved_and_reports_recovery_required() {
    let (_temp, config, request) = setup();
    let path = config.state_directory.join("journal.sqlite");
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let accepted = accept(&worker, request);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER ignore_handoff BEFORE UPDATE OF scope ON resource_leases BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match worker.schedule() {
            Ok(()) => {}
            Err(error) => {
                assert!(error.contains("application resource handoff"), "{error}");
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "handoff failure was not surfaced"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(
        worker.handle(LocalRequest::GetWorkflow {
            workflow_id: accepted.workflow_id.clone()
        }),
        LocalResponse::Error { .. }
    ));
    let store = winbridge_worker::store::Store::open(&path).unwrap();
    let record = store.workflow(&accepted.workflow_id).unwrap();
    assert_ne!(record.status, TaskStatus::Succeeded);
    let app = store
        .application(record.application_id.as_ref().unwrap())
        .unwrap();
    let leases = store.resource_leases(true).unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].operation_id, app.task_id);
    assert_eq!(
        leases[0].scope,
        winbridge_worker::store::LeaseScope::ProjectMutation
    );
    assert_eq!(leases[0].cleanup_proof, None);
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!()
    };
    assert!(info.journal_recovery_required.is_some());
}

#[test]
fn build_failure_never_launches_a_stale_existing_executable() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let first = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    stop(&worker, first.application_id.as_ref().unwrap());
    let before = fs::read(
        worker.config.projects[0]
            .workspace_root
            .join("logs/app.log"),
    )
    .unwrap();
    let mut bad = request;
    bad.idempotency_key = "bad-build".into();
    bad.build = CommandSource::Direct {
        executable: "cargo".into(),
        arguments: vec!["build".into(), "--invalid-workflow-fixture-option".into()],
        working_directory: ".".into(),
    };
    let failed = complete(&worker, &accept(&worker, bad).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert_eq!(failed.steps[1].status, WorkflowStepStatus::Failed);
    assert_eq!(failed.steps[2].status, WorkflowStepStatus::Skipped);
    assert!(failed.application_id.is_none());
    assert_eq!(
        fs::read(
            worker.config.projects[0]
                .workspace_root
                .join("logs/app.log")
        )
        .unwrap(),
        before
    );
}

#[test]
fn readiness_timeout_stops_the_application_without_claiming_ready() {
    let (_temp, config, mut request) = setup();
    request.readiness.timeout_ms = 300;
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "silent".into();
    }
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let failed = complete(&worker, &accept(&worker, request).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed, "{failed:?}");
    let app = app(&worker, failed.application_id.as_ref().unwrap());
    assert_eq!(app.status, ApplicationStatus::Failed);
    assert!(app.ready_at_ms.is_none());
    assert!(app.reason.unwrap().contains("deadline exceeded"));
    until(&worker, || worker.is_idle());
}

#[test]
fn stopping_during_startup_does_not_confirm_readiness() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let id = accept(&worker, request).workflow_id;
    until(&worker, || {
        flow(&worker, &id).steps[2].status == WorkflowStepStatus::Succeeded
    });
    let starting = flow(&worker, &id);
    let app_id = starting.application_id.unwrap();
    assert!(app(&worker, &app_id).ready_at_ms.is_none());
    worker.handle(LocalRequest::StopApplication {
        application_id: app_id.clone(),
    });
    let ended = complete(&worker, &id);
    assert_eq!(ended.status, TaskStatus::Cancelled);
    assert_eq!(app(&worker, &app_id).status, ApplicationStatus::Stopped);
    assert!(app(&worker, &app_id).ready_at_ms.is_none());
}

#[test]
fn overall_startup_timeout_is_an_application_failure() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let first = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    stop(&worker, first.application_id.as_ref().unwrap());
    let mut timed = request;
    timed.idempotency_key = "overall-timeout".into();
    timed.timeout_ms = 5000;
    timed.readiness.timeout_ms = 10000;
    if let CommandSource::Direct { arguments, .. } = &mut timed.start {
        arguments[2] = "silent".into();
    }
    let failed = complete(&worker, &accept(&worker, timed).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    let application = app(&worker, failed.application_id.as_ref().unwrap());
    assert_eq!(application.status, ApplicationStatus::Failed);
    assert!(application.ready_at_ms.is_none());
    assert!(application
        .reason
        .unwrap()
        .contains("Workflow startup timeout"));
    until(&worker, || worker.is_idle());
}

#[test]
fn runtime_crash_is_separate_from_successful_build_and_startup() {
    let (_temp, config, mut request) = setup();
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "crash-late".into();
    }
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    assert_eq!(done.status, TaskStatus::Succeeded);
    let id = done.application_id.unwrap();
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Failed && worker.is_idle()
    });
    assert_eq!(
        flow(&worker, &done.workflow_id).status,
        TaskStatus::Succeeded,
        "historical startup success was overwritten by application lifecycle"
    );
}

#[test]
fn queued_cancel_has_no_child_effects_and_v2_migration_preserves_state() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let queued = accept(&worker, request);
    worker.handle(LocalRequest::CancelWorkflow {
        workflow_id: queued.workflow_id.clone(),
    });
    let cancelled = complete(&worker, &queued.workflow_id);
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert!(cancelled.steps.iter().all(|s| s.operation_id.is_none()));
    let LocalResponse::Task { task: queued_task } = worker.handle(LocalRequest::Submit {
        request: TaskSubmission {
            protocol_version: 1,
            request_id: "migration".into(),
            idempotency_key: "migration".into(),
            project_id: "fixture".into(),
            snapshot_id: None,
            timeout_ms: 10000,
            command: CommandSource::Direct {
                executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                arguments: vec!["success".into()],
                working_directory: ".".into(),
            },
        },
    }) else {
        panic!()
    };
    drop(worker);
    let db = config.state_directory.join("journal.sqlite");
    let mut store = Store::open(&db).unwrap();
    store
        .append_output(
            &queued_task.task_id,
            OutputStream::Stdout,
            0,
            b"migration evidence",
            10000,
        )
        .unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("DROP TABLE resource_leases; DROP TABLE execution_attempts; DROP TABLE journal_authority; DROP TRIGGER imports_revision_guard; ALTER TABLE imports DROP COLUMN revision; DROP TRIGGER workflows_revision_guard; ALTER TABLE workflows DROP COLUMN revision; DROP TRIGGER applications_revision_guard; ALTER TABLE applications DROP COLUMN revision; DROP TRIGGER attention_revision_guard; ALTER TABLE attention DROP COLUMN revision; DROP TRIGGER tasks_revision_guard; ALTER TABLE tasks DROP COLUMN revision; DROP TABLE output_cleanup; DROP TABLE attention; DELETE FROM workflows; DROP TABLE applications; DROP TABLE workflows; PRAGMA user_version=2;").unwrap();
    drop(connection);
    // A v2 task/output survives the consistent snapshot and v3 table migration.
    let store = Store::open(&db).unwrap();
    assert!(store.workflows(true).unwrap().is_empty());
    assert_eq!(
        store.get(&queued_task.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert_eq!(
        store.output(&queued_task.task_id, 0).unwrap()[0].bytes,
        b"migration evidence"
    );
    drop(store);
    let backups: Vec<_> = fs::read_dir(&config.state_directory)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("journal-v2-"))
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(backups[0].path()).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    drop(backup);
    drop(Store::open(&db).unwrap());
    assert_eq!(
        fs::read_dir(&config.state_directory)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("journal-v2-"))
            .count(),
        1
    );
}

#[tokio::test]
async fn owned_worker_crash_recovers_application_without_repeating_steps() {
    let (_temp, config, request) = setup();
    let path = config
        .state_directory
        .parent()
        .unwrap()
        .join("worker.local.json");
    // Serialize the concrete fixture config; WorkerConfig itself intentionally has no writer.
    fs::write(&path,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"instanceId":config.instance_id,"nodeId":config.node_id,"stateDirectory":config.state_directory,"worker":{"keepRunningOnUiExit":true},"delivery":{"adapter":"shared-folder","root":config.delivery.as_ref().unwrap().root,"namespace":"WinBridge"},"projects":[{"id":"fixture","workspaceRoot":config.projects[0].workspace_root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
    struct ProcessOwner(std::process::Child, Option<u32>);
    impl Drop for ProcessOwner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            #[cfg(unix)]
            if let Some(pid) = self.1 {
                unsafe extern "C" {
                    fn kill(pid: i32, signal: i32) -> i32;
                }
                unsafe {
                    kill(-(pid as i32), 9);
                }
            }
        }
    }
    let spawn = || {
        ProcessOwner(
            Command::new(env!("CARGO_BIN_EXE_winbridge-worker"))
                .args(["--config", path.to_str().unwrap(), "serve"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
            None,
        )
    };
    let endpoint =
        winbridge_transport::endpoint(&config.state_directory, &config.instance_id).unwrap();
    let mut process = spawn();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if winbridge_transport::call(&endpoint, &LocalRequest::Info)
            .await
            .is_ok()
        {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let LocalResponse::Workflow { workflow } = winbridge_transport::call(
        &endpoint,
        &LocalRequest::SubmitWorkflow {
            request: Box::new(request.clone()),
        },
    )
    .await
    .unwrap() else {
        panic!()
    };
    let id = workflow.workflow_id;
    let before = loop {
        let LocalResponse::Workflow { workflow } = winbridge_transport::call(
            &endpoint,
            &LocalRequest::GetWorkflow {
                workflow_id: id.clone(),
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        if workflow.status.is_terminal() {
            break *workflow;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(before.status, TaskStatus::Succeeded);
    let app_id = before.application_id.clone().unwrap();
    let LocalResponse::Application { application } = winbridge_transport::call(
        &endpoint,
        &LocalRequest::GetApplication {
            application_id: app_id.clone(),
        },
    )
    .await
    .unwrap() else {
        panic!()
    };
    process.1 = application.process_id;
    println!(
        "owned crash proof: platform={} workflow={} application={} appPid={:?}",
        std::env::consts::OS,
        id,
        app_id,
        process.1
    );
    process.0.kill().unwrap();
    process.0.wait().unwrap();
    #[cfg(windows)]
    {
        let pid = application.process_id.unwrap();
        let script = format!(
            "if(Get-Process -Id {pid} -ErrorAction SilentlyContinue){{exit 1}}else{{exit 0}}"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Command::new("powershell.exe")
                .args(["-NoProfile", "-Command", &script])
                .status()
                .unwrap()
                .success()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned Job Object did not terminate app after worker crash"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    drop(process); // Unix adapter explicitly cleans its owned process group; Windows relies on Job close.
    let mut restarted = spawn();
    let deadline = Instant::now() + Duration::from_secs(10);
    let recovered = loop {
        if let Ok(LocalResponse::Application { application }) = winbridge_transport::call(
            &endpoint,
            &LocalRequest::GetApplication {
                application_id: app_id.clone(),
            },
        )
        .await
        {
            if application.status == ApplicationStatus::Indeterminate {
                break application;
            }
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(recovered.application_id, app_id);
    let LocalResponse::Workflow { workflow: after } = winbridge_transport::call(
        &endpoint,
        &LocalRequest::SubmitWorkflow {
            request: Box::new(request),
        },
    )
    .await
    .unwrap() else {
        panic!()
    };
    assert_eq!(after.workflow_id, id);
    assert_eq!(
        after
            .steps
            .iter()
            .map(|s| &s.operation_id)
            .collect::<Vec<_>>(),
        before
            .steps
            .iter()
            .map(|s| &s.operation_id)
            .collect::<Vec<_>>()
    );
    assert!(winbridge_transport::call(&endpoint, &LocalRequest::Info)
        .await
        .is_ok());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let LocalResponse::Application { application } = winbridge_transport::call(
        &endpoint,
        &LocalRequest::GetApplication {
            application_id: app_id,
        },
    )
    .await
    .unwrap() else {
        panic!()
    };
    assert_eq!(
        application.status,
        ApplicationStatus::Indeterminate,
        "automatic application restart occurred"
    );
    restarted.0.kill().unwrap();
    restarted.0.wait().unwrap();
}

fn attention_events(worker: &Worker) -> Vec<AttentionEvent> {
    let LocalResponse::AttentionEvents { page } =
        worker.handle(LocalRequest::ListAttentionEvents {
            after_sequence: 0,
            pending_only: false,
        })
    else {
        panic!("attention feed missing")
    };
    page.records
}

#[test]
fn history_mixed_pipeline_links_import_tasks_workflow_and_application() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    let LocalResponse::History { page } = worker.handle(LocalRequest::History {
        query: HistoryQuery {
            project_id: Some("fixture".into()),
            kind: None,
            status: None,
            search: String::new(),
            before: None,
        },
    }) else {
        panic!()
    };
    assert_eq!(page.records.len(), 5);
    for kind in [
        HistoryKind::Task,
        HistoryKind::Import,
        HistoryKind::Workflow,
        HistoryKind::Application,
    ] {
        assert!(page.records.iter().any(|r| r.kind == kind));
    }
    let app = page
        .records
        .iter()
        .find(|r| r.kind == HistoryKind::Application)
        .unwrap();
    assert_eq!(app.workflow_id.as_deref(), Some(done.workflow_id.as_str()));
    assert_eq!(Some(&app.operation_id), done.application_id.as_ref());
    stop(&worker, done.application_id.as_ref().unwrap());
}
#[test]
fn attention_import_failure_without_task_has_workflow_target_and_ack_survives_restart() {
    let (_temp, config, mut request) = setup();
    request.snapshot_id = "absent-published-package".into();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let failed = complete(&worker, &accept(&worker, request).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed.steps[1].operation_id.is_none());
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].target,
        AttentionTarget::Workflow {
            workflow_id: failed.workflow_id.clone()
        }
    );
    assert_eq!(events[0].severity, AttentionSeverity::Error);
    assert!(events[0].message.contains("import"));
    assert!(matches!(
        worker.handle(LocalRequest::AcknowledgeAttention {
            attention_id: events[0].attention_id.clone()
        }),
        LocalResponse::Error { .. }
    ));
    assert!(attention_events(&worker)[0].acknowledged_at_ms.is_none());
    let LocalResponse::AttentionEvent { event: ack } =
        worker.handle(LocalRequest::AcknowledgeAttentionEvent {
            attention_id: events[0].attention_id.clone(),
        })
    else {
        panic!()
    };
    drop(worker);
    let recovered = WorkerOwner(Worker::open(config).unwrap());
    for _ in 0..4 {
        recovered.schedule().unwrap();
    }
    assert_eq!(attention_events(&recovered), vec![ack]);
}
#[test]
fn attention_build_failure_is_one_parent_event_and_cancel_stays_quiet() {
    let (_temp, config, mut request) = setup();
    request.build = CommandSource::Direct {
        executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
        arguments: vec!["failure".into()],
        working_directory: ".".into(),
    };
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let failed = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].origin, "workflow_outcome");
    assert!(events[0].message.contains("build"));
    let LocalResponse::AttentionList { page } = worker.handle(LocalRequest::ListAttention {
        after_sequence: 0,
        pending_only: true,
    }) else {
        panic!()
    };
    assert!(page.records.is_empty());
    assert_eq!(page.pending_count, 0);
    request.idempotency_key = "cancel-before-child".into();
    let cancelled = accept(&worker, request);
    worker.handle(LocalRequest::CancelWorkflow {
        workflow_id: cancelled.workflow_id.clone(),
    });
    assert_eq!(
        complete(&worker, &cancelled.workflow_id).status,
        TaskStatus::Cancelled
    );
    assert_eq!(attention_events(&worker), events);
}
#[test]
fn attention_readiness_failure_stops_tree_and_has_no_duplicate_application_event() {
    let (_temp, config, mut request) = setup();
    request.readiness.timeout_ms = 300;
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "silent".into();
    }
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let failed = complete(&worker, &accept(&worker, request).workflow_id);
    until(&worker, || worker.is_idle());
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].target,
        AttentionTarget::Workflow {
            workflow_id: failed.workflow_id
        }
    );
    assert_eq!(events[0].severity, AttentionSeverity::Error);
    assert_eq!(
        app(&worker, failed.application_id.as_ref().unwrap()).status,
        ApplicationStatus::Failed
    );
}
#[test]
fn attention_runtime_crash_keeps_startup_success_and_explicit_stop_is_quiet() {
    let (_temp, config, mut request) = setup();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let healthy = complete(&worker, &accept(&worker, request.clone()).workflow_id);
    stop(&worker, healthy.application_id.as_ref().unwrap());
    assert!(attention_events(&worker).is_empty());
    request.idempotency_key = "runtime-crash-attention".into();
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "crash-late".into();
    }
    let startup = complete(&worker, &accept(&worker, request).workflow_id);
    let id = startup.application_id.unwrap();
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Failed && worker.is_idle()
    });
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].target,
        AttentionTarget::Application {
            application_id: id,
            workflow_id: startup.workflow_id.clone()
        }
    );
    assert_eq!(events[0].severity, AttentionSeverity::Error);
    assert_eq!(
        flow(&worker, &startup.workflow_id).status,
        TaskStatus::Succeeded
    );
    drop(worker);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    for _ in 0..3 {
        worker.schedule().unwrap();
    }
    assert_eq!(attention_events(&worker), events);
}
#[test]
fn attention_workflow_write_failure_rolls_back_and_blocks_stale_reads_until_recovery() {
    let (_temp, config, mut request) = setup();
    request.snapshot_id = "absent-published-package".into();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let accepted = accept(&worker, request.clone());
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_workflow_event BEFORE INSERT ON attention WHEN NEW.origin='workflow_outcome' BEGIN SELECT RAISE(ABORT,'expected workflow journal failure'); END;").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if worker.schedule().is_err() {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!()
    };
    assert!(info.journal_recovery_required.is_some());
    assert!(
        matches!(worker.handle(LocalRequest::GetWorkflow {workflow_id:accepted.workflow_id.clone()}),LocalResponse::Error {message} if message.contains("stale"))
    );
    request.idempotency_key = "must-not-launch".into();
    assert!(matches!(
        worker.handle(LocalRequest::SubmitWorkflow {
            request: Box::new(request)
        }),
        LocalResponse::Error { .. }
    ));
    assert!(attention_events(&worker).is_empty());
    let record: String = db
        .query_row(
            "SELECT record FROM workflows WHERE id=?1",
            [&accepted.workflow_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!serde_json::from_str::<WorkflowRecord>(&record)
        .unwrap()
        .status
        .is_terminal());
    db.execute_batch("DROP TRIGGER reject_workflow_event;")
        .unwrap();
    drop(db);
    drop(worker);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let recovered = complete(&worker, &accepted.workflow_id);
    assert!(recovered.status.is_terminal());
    assert_eq!(attention_events(&worker).len(), 1);
}
#[test]
fn attention_application_write_failure_is_not_reported_as_fresh_ready() {
    let (_temp, config, mut request) = setup();
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "crash-late".into();
    }
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    let id = done.application_id.unwrap();
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_app_event BEFORE INSERT ON attention WHEN NEW.origin='application_outcome' BEGIN SELECT RAISE(ABORT,'expected application journal failure'); END;").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if worker.schedule().is_err() {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(worker.handle(LocalRequest::GetApplication {application_id:id.clone()}),LocalResponse::Error {message} if message.contains("stale"))
    );
    assert_eq!(
        flow(&worker, &done.workflow_id).status,
        TaskStatus::Succeeded
    );
    assert!(attention_events(&worker).is_empty());
    db.execute_batch("DROP TRIGGER reject_app_event;").unwrap();
    drop(db);
    drop(worker);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Failed
    });
    assert_eq!(attention_events(&worker).len(), 1);
}

#[test]
fn attention_startup_application_write_failure_blocks_both_related_records() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let id = accept(&worker, request).workflow_id;
    until(&worker, || flow(&worker, &id).application_id.is_some());
    let app_id = flow(&worker, &id).application_id.unwrap();
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_ready BEFORE UPDATE ON applications WHEN NEW.status='ready' BEGIN SELECT RAISE(ABORT,'expected startup application journal failure'); END;").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while worker.schedule().is_ok() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    for request in [
        LocalRequest::GetWorkflow {
            workflow_id: id.clone(),
        },
        LocalRequest::GetApplication {
            application_id: app_id,
        },
    ] {
        assert!(
            matches!(worker.handle(request), LocalResponse::Error {message} if message.contains("stale"))
        );
    }
    assert!(attention_events(&worker).is_empty());
    db.execute_batch("DROP TRIGGER reject_ready;").unwrap();
    drop(db);
    drop(worker);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    assert!(complete(&worker, &id).status.is_terminal());
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].target,
        AttentionTarget::Workflow { workflow_id: id }
    );
}

#[test]
fn attention_uncommitted_child_result_blocks_application_but_keeps_startup_history() {
    let (_temp, config, mut request) = setup();
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "crash-late".into();
    }
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    let id = done.application_id.unwrap();
    let task_id = app(&worker, &id).task_id;
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_child_result BEFORE UPDATE ON tasks WHEN json_extract(NEW.record,'$.status') IN ('failed','indeterminate') BEGIN SELECT RAISE(ABORT,'expected child result failure'); END;").unwrap();
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
        LocalRequest::GetApplication {
            application_id: id.clone(),
        },
        LocalRequest::StopApplication {
            application_id: id.clone(),
        },
    ] {
        assert!(
            matches!(worker.handle(request), LocalResponse::Error {message} if message.contains("stale") && message.contains(&task_id))
        );
    }
    assert_eq!(
        flow(&worker, &done.workflow_id).status,
        TaskStatus::Succeeded
    );
    db.execute_batch("DROP TRIGGER reject_child_result;")
        .unwrap();
    drop(db);
    drop(worker);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Indeterminate
    });
    assert_eq!(attention_events(&worker).len(), 1);
}

#[test]
fn attention_clean_service_exit_is_warning_and_completed_startup_stays_successful() {
    let (_temp, config, mut request) = setup();
    if let CommandSource::Direct { arguments, .. } = &mut request.start {
        arguments[2] = "exit-late".into();
    }
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    let id = done.application_id.unwrap();
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Exited && worker.is_idle()
    });
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].severity, AttentionSeverity::Warning);
    assert_eq!(
        events[0].target,
        AttentionTarget::Application {
            application_id: id,
            workflow_id: done.workflow_id.clone()
        }
    );
    assert_eq!(
        flow(&worker, &done.workflow_id).status,
        TaskStatus::Succeeded
    );
}
#[test]
fn attention_recovered_launch_intent_is_warning_without_another_external_start() {
    let (_temp, config, request) = setup();
    let worker = WorkerOwner(Worker::open(config.clone()).unwrap());
    let done = complete(&worker, &accept(&worker, request).workflow_id);
    let id = done.application_id.unwrap();
    let ready_app = app(&worker, &id);
    let LocalResponse::Task { task: running } = worker.handle(LocalRequest::GetTask {
        task_id: ready_app.task_id.clone(),
    }) else {
        panic!()
    };
    stop(&worker, &id);
    drop(worker);
    let log = config.projects[0].workspace_root.join("logs/app.log");
    let before = fs::read(&log).unwrap();
    let mut store =
        winbridge_worker::store::Store::open(&config.state_directory.join("journal.sqlite"))
            .unwrap();
    let mut observed = store.task_snapshot(&running.task_id).unwrap();
    observed.record = *running;
    store.save(&observed).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(config.state_directory.join("journal.sqlite")).unwrap();
    db.execute(
        "UPDATE applications SET status='ready',record=?2,revision=revision+1 WHERE id=?1",
        rusqlite::params![id, serde_json::to_string(&ready_app).unwrap()],
    )
    .unwrap();
    drop(db);
    let worker = WorkerOwner(Worker::open(config).unwrap());
    until(&worker, || {
        app(&worker, &id).status == ApplicationStatus::Indeterminate
    });
    let events = attention_events(&worker);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].severity, AttentionSeverity::Warning);
    assert_eq!(
        events[0].target,
        AttentionTarget::Application {
            application_id: id,
            workflow_id: done.workflow_id
        }
    );
    assert_eq!(fs::read(&log).unwrap(), before);
}

#[test]
fn queued_workflow_pins_recipe_before_ack_and_refuses_changed_profile_before_import() {
    let (_temp, mut config, mut request) = setup();
    config.recipe_profiles=serde_json::from_value(serde_json::json!([{"id":"rust-local","label":"Original profile","description":"Frozen recipe check","recipes":[{"id":"build","executable":"cargo","arguments":["build","--locked","--offline","--target-dir","${workspaceRoot}/build-cache"],"workingDirectory":".","timeoutMs":30000}]}])).unwrap();
    config.projects[0].recipe_profile = Some("rust-local".into());
    request.build = CommandSource::Recipe {
        recipe_id: "build".into(),
        arguments: vec![],
    };
    let worker = Worker::open(config.clone()).unwrap();
    let accepted = accept(&worker, request.clone());
    let pin = &accepted.recipe_pins.as_ref().unwrap()[0];
    assert_eq!(
        pin.origin.profile_label.as_deref(),
        Some("Original profile")
    );
    assert!(pin
        .command
        .arguments
        .last()
        .unwrap()
        .ends_with("/build-cache"));
    worker.stop();
    drop(worker);
    config.recipe_profiles[0].recipes[0]
        .arguments
        .push("--release".into());
    config.recipe_profiles[0].label = "Changed profile".into();
    let worker = WorkerOwner(Worker::open(config).unwrap());
    assert_eq!(accept(&worker, request).workflow_id, accepted.workflow_id);
    let failed = complete(&worker, &accepted.workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed
        .reason
        .as_ref()
        .unwrap()
        .contains("recipe changed since acceptance"));
    assert!(failed.steps.iter().all(|step| step.operation_id.is_none()));
    let LocalResponse::Imports { imports } = worker.handle(LocalRequest::ListImports) else {
        panic!("imports")
    };
    assert!(imports.is_empty());
    let LocalResponse::Tasks { tasks } = worker.handle(LocalRequest::ListTasks) else {
        panic!("tasks")
    };
    assert!(tasks.is_empty());
    assert_eq!(
        failed.recipe_pins.unwrap()[0]
            .origin
            .profile_label
            .as_deref(),
        Some("Original profile")
    );
}

#[test]
fn workflow_profile_recipe_uses_shared_runner_and_commits_build_provenance() {
    let (_temp, mut config, mut request) = setup();
    let CommandSource::Direct {
        executable,
        arguments,
        working_directory,
    } = &request.build
    else {
        panic!("fixture build")
    };
    config.recipe_profiles=serde_json::from_value(serde_json::json!([{"id":"rust-local","label":"Rust build profile","description":"Shared workflow runner","recipes":[{"id":"build","executable":executable,"arguments":arguments,"workingDirectory":working_directory,"timeoutMs":30000}]}])).unwrap();
    config.projects[0].recipe_profile = Some("rust-local".into());
    request.build = CommandSource::Recipe {
        recipe_id: "build".into(),
        arguments: vec![],
    };
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let accepted = accept(&worker, request);
    let done = complete(&worker, &accepted.workflow_id);
    assert_eq!(done.status, TaskStatus::Succeeded);
    let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask {
        task_id: done.steps[1].operation_id.clone().unwrap(),
    }) else {
        panic!("build")
    };
    assert_eq!(task.status, TaskStatus::Succeeded);
    assert_eq!(
        task.recipe_origin.as_ref().unwrap().profile_id.as_deref(),
        Some("rust-local")
    );
    assert_eq!(
        task.recipe_origin.as_ref().unwrap(),
        &accepted.recipe_pins.as_ref().unwrap()[0].origin
    );
}

#[test]
fn legacy_queued_recipe_is_readable_but_requires_explicit_new_workflow() {
    let (_temp, mut config, mut request) = setup();
    let CommandSource::Direct {
        executable,
        arguments,
        working_directory,
    } = &request.build
    else {
        panic!("build")
    };
    config.projects[0].recipes.push(serde_json::from_value(serde_json::json!({"id":"build","executable":executable,"arguments":arguments,"workingDirectory":working_directory,"timeoutMs":30000})).unwrap());
    request.build = CommandSource::Recipe {
        recipe_id: "build".into(),
        arguments: vec![],
    };
    let worker = Worker::open(config.clone()).unwrap();
    worker.stop();
    drop(worker);
    let accepted = {
        let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
        store.accept_workflow(request.clone()).unwrap()
    };
    assert!(accepted.recipe_pins.is_none());
    let worker = WorkerOwner(Worker::open(config).unwrap());
    assert_eq!(accept(&worker, request).workflow_id, accepted.workflow_id);
    let failed = complete(&worker, &accepted.workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed
        .reason
        .as_ref()
        .unwrap()
        .contains("no frozen recipe definitions"));
    assert!(failed.steps.iter().all(|step| step.operation_id.is_none()));
}

#[test]
fn recovered_running_workflow_reconciles_import_but_does_not_expand_changed_build_recipe() {
    let (_temp, mut config, mut request) = setup();
    let CommandSource::Direct {
        executable,
        arguments,
        working_directory,
    } = &request.build
    else {
        panic!("build")
    };
    config.projects[0].recipes.push(serde_json::from_value(serde_json::json!({"id":"build","executable":executable,"arguments":arguments,"workingDirectory":working_directory,"timeoutMs":30000})).unwrap());
    request.build = CommandSource::Recipe {
        recipe_id: "build".into(),
        arguments: vec![],
    };
    let worker = Worker::open(config.clone()).unwrap();
    let mut accepted = accept(&worker, request.clone());
    worker.stop();
    drop(worker);
    let import_request = ImportSubmission {
        protocol_version: 1,
        request_id: accepted.steps[0].step_id.clone(),
        project_id: request.project_id.clone(),
        snapshot_id: request.snapshot_id.clone(),
        manifest_sha256: request.manifest_sha256.clone(),
        idempotency_key: format!(
            "workflow:{}:{}",
            accepted.workflow_id, accepted.steps[0].step_id
        ),
        timeout_ms: 30000,
    };
    let delivery = config.delivery.as_ref().unwrap();
    let project = &config.projects[0];
    winbridge_workspace_sync::import(
        delivery,
        &project.workspace_root,
        &import_request,
        "recovered-import",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let database = config.state_directory.join("journal.sqlite");
    {
        let mut store = Store::open(&database).unwrap();
        let imported = store
            .accept_import(
                import_request,
                "shared".into(),
                "verified generation".into(),
            )
            .unwrap();
        store
            .finish_import(
                &imported.import_id,
                TaskStatus::Succeeded,
                "Actual immutable import completed",
            )
            .unwrap();
        accepted.steps[0].operation_id = Some(imported.import_id);
        accepted.steps[0].status = WorkflowStepStatus::Succeeded;
    }
    // Model the exact persisted crash window: import committed, no build accepted.
    accepted.status = TaskStatus::Running;
    {
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute(
                "UPDATE workflows SET record=?2,revision=revision+1 WHERE id=?1",
                rusqlite::params![
                    accepted.workflow_id,
                    serde_json::to_string(&accepted).unwrap()
                ],
            )
            .unwrap();
    }
    config.projects[0].recipes[0]
        .arguments
        .push("--release".into());
    let worker = WorkerOwner(Worker::open(config).unwrap());
    let failed = complete(&worker, &accepted.workflow_id);
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed
        .reason
        .as_ref()
        .unwrap()
        .contains("recipe changed since acceptance"));
    assert!(failed.steps[1].operation_id.is_none());
    let LocalResponse::Tasks { tasks } = worker.handle(LocalRequest::ListTasks) else {
        panic!("tasks")
    };
    assert!(tasks.is_empty());
    let LocalResponse::Imports { imports } = worker.handle(LocalRequest::ListImports) else {
        panic!("imports")
    };
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].status, TaskStatus::Succeeded);
}
