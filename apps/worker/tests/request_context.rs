use std::{
    fs,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker};

fn config(root: &Path, instance: &str, state: &str) -> WorkerConfig {
    fs::create_dir_all(root.join("project")).unwrap();
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"worker","stateDirectory":root.join(state),"maxOutputBytesPerTask":10000,"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root.join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}
fn prepare(config: &WorkerConfig) {
    fs::create_dir_all(&config.state_directory).unwrap();
    #[cfg(unix)]
    fs::set_permissions(
        &config.state_directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    store.require_fenced_requests().unwrap();
}
fn submit(key: &str) -> LocalRequest {
    LocalRequest::Submit {
        request: TaskSubmission {
            protocol_version: 1,
            request_id: "request".into(),
            project_id: "fixture".into(),
            idempotency_key: key.into(),
            timeout_ms: 5000,
            snapshot_id: None,
            command: CommandSource::Direct {
                executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                arguments: vec!["success".into()],
                working_directory: ".".into(),
            },
        },
    }
}
fn call(worker: &Worker, context: RequestContext, request: LocalRequest) -> FencedResponse {
    worker.handle_fenced(FencedRequest {
        request_contract_version: 2,
        context,
        request,
    })
}
fn accepted(response: LocalResponse) -> TaskRecord {
    if let LocalResponse::Task { task } = response {
        *task
    } else {
        panic!("{response:?}")
    }
}
fn wait(worker: &Arc<Worker>, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let task = accepted(worker.handle(LocalRequest::GetTask { task_id: id.into() }));
        if task.status.is_terminal() && worker.is_idle() {
            return task;
        }
        assert!(Instant::now() < deadline, "{task:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn fenced_request_and_key_survive_worker_restart_without_rebinding_or_duplicate_process() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), "pair", "state");
    prepare(&cfg);
    let worker = Worker::open(cfg.clone()).unwrap();
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("worker info")
    };
    let CatalogObservation::Known(catalog) = interpret_catalog(&info) else {
        panic!("worker catalog")
    };
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 2));
    assert!(
        !info
            .fenced_requests
            .as_ref()
            .unwrap()
            .legacy_mutations_allowed
    );
    let context = worker.request_context().unwrap();
    let first = call(&worker, context.clone(), submit("same-key"));
    assert_eq!(first.outcome, FencedReplyOutcome::Responded);
    let task = accepted(first.response);
    assert_eq!(wait(&worker, &task.task_id).status, TaskStatus::Succeeded);
    drop(worker);
    let worker = Worker::open(cfg).unwrap();
    assert_eq!(worker.request_context().unwrap(), context);
    let repeated = accepted(call(&worker, context, submit("same-key")).response);
    assert_eq!(repeated.task_id, task.task_id);
    worker.schedule().unwrap();
    assert!(worker.is_idle());
    assert_eq!(
        fs::read_to_string(root.path().join("project/runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn new_pair_rejects_old_logical_request_and_v1_after_config_reload_before_any_effect() {
    let root = TempDir::new().unwrap();
    let old_cfg = config(root.path(), "old-pair", "old");
    let old = Worker::open(old_cfg.clone()).unwrap();
    let context = old.request_context().unwrap();
    let task = accepted(call(&old, context.clone(), submit("lost-ack")).response);
    wait(&old, &task.task_id);
    drop(old);
    let cfg = config(root.path(), "new-pair", "new");
    prepare(&cfg);
    let worker = Worker::open(cfg.clone()).unwrap();
    let reply = call(&worker, context, submit("lost-ack"));
    assert_eq!(reply.outcome, FencedReplyOutcome::RejectedBeforeDispatch);
    assert!(matches!(reply.response, LocalResponse::Error { .. }));
    assert!(matches!(
        worker.handle(submit("lost-ack")),
        LocalResponse::Error { .. }
    ));
    assert!(
        matches!(worker.handle(LocalRequest::ListTasks),LocalResponse::Tasks{tasks} if tasks.is_empty())
    );
    assert_eq!(
        fs::read_to_string(root.path().join("project/runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let task = accepted(
        call(
            &worker,
            worker.request_context().unwrap(),
            submit("explicit-new-operation"),
        )
        .response,
    );
    wait(&worker, &task.task_id);
    assert_eq!(
        fs::read_to_string(root.path().join("project/runs.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(old_cfg.state_directory.join("journal.sqlite").is_file());
}

#[test]
fn every_legacy_mutation_and_stale_version_are_rejected_before_handler_validation() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), "pair", "state");
    prepare(&cfg);
    let worker = Worker::open(cfg).unwrap();
    let import = ImportSubmission {
        protocol_version: 1,
        request_id: "r".into(),
        project_id: "fixture".into(),
        snapshot_id: "missing".into(),
        manifest_sha256: "0".repeat(64),
        idempotency_key: "import".into(),
        timeout_ms: 1000,
    };
    let recipe = CommandSource::Recipe {
        recipe_id: "missing".into(),
        arguments: vec![],
    };
    let requests = vec![
        submit("key"),
        LocalRequest::Cancel {
            task_id: "missing".into(),
        },
        LocalRequest::CancelImport {
            import_id: "missing".into(),
        },
        LocalRequest::CancelWorkflow {
            workflow_id: "missing".into(),
        },
        LocalRequest::StopApplication {
            application_id: "missing".into(),
        },
        LocalRequest::AcknowledgeAttentionEvent {
            attention_id: "missing".into(),
        },
        LocalRequest::AcknowledgeAttention {
            attention_id: "missing".into(),
        },
        LocalRequest::ImportSnapshot {
            request: import.clone(),
        },
        LocalRequest::ImportDeltaSnapshot { request: import },
        LocalRequest::RequestAttention {
            request: AttentionSubmission {
                request_id: "r".into(),
                idempotency_key: "attention".into(),
                task_id: "missing".into(),
                severity: AttentionSeverity::Warning,
                message: "blocked".into(),
            },
        },
        LocalRequest::SubmitWorkflow {
            request: Box::new(WorkflowSubmission {
                protocol_version: 1,
                request_id: "r".into(),
                idempotency_key: "workflow".into(),
                name: "blocked".into(),
                project_id: "fixture".into(),
                snapshot_id: "missing".into(),
                manifest_sha256: "0".repeat(64),
                timeout_ms: 1000,
                build: recipe.clone(),
                start: recipe,
                readiness: ReadinessProbe {
                    stdout_contains: "ready".into(),
                    timeout_ms: 1000,
                    netwatch: None,
                },
                log_path: "log".into(),
                artifacts: vec![],
            }),
        },
        LocalRequest::SaveRuntimeSettings {
            request: SaveRuntimeSettings {
                generation: "old".into(),
                request_id: "r".into(),
                expected_sha256: "0".repeat(64),
                json: "{}".into(),
            },
        },
        LocalRequest::ApplyOutputCleanup {
            request: ApplyOutputCleanup {
                idempotency_key: "cleanup".into(),
                plan: OutputCleanupPlan {
                    before_ms: 0,
                    project_id: None,
                    task_ids: vec![],
                    sha256: "0".repeat(64),
                },
            },
        },
        LocalRequest::PrepareTaskDiagnostic {
            request: DiagnosticRequest {
                task_id: "missing".into(),
                include_command: false,
                include_output: false,
                redact_values: vec![],
            },
        },
    ];
    for request in requests {
        assert!(!request.is_observation());
        assert!(
            matches!(worker.handle(request.clone()),LocalResponse::Error{message} if message.contains("contract 2"))
        );
        let reply = worker.handle_fenced(FencedRequest {
            request_contract_version: 3,
            context: worker.request_context().unwrap(),
            request,
        });
        assert_eq!(reply.outcome, FencedReplyOutcome::RejectedBeforeDispatch);
    }
    assert!(!root.path().join("project/runs.log").exists());
}

#[test]
fn admission_floor_cannot_be_downgraded_deleted_or_added_to_existing_history() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), "pair", "state");
    prepare(&cfg);
    let db = rusqlite::Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap();
    for sql in [
        "UPDATE journal_admission SET require_fenced=0",
        "DELETE FROM journal_admission",
        "INSERT OR REPLACE INTO journal_admission VALUES(1,'foreign',0)",
    ] {
        assert!(db.execute_batch(sql).is_err());
    }
    drop(db);
    let worker = Worker::open(cfg.clone()).unwrap();
    assert!(
        matches!(worker.handle(LocalRequest::Info),LocalResponse::Info{info} if !info.fenced_requests.as_ref().unwrap().legacy_mutations_allowed)
    );
    drop(worker);
    let cfg = config(root.path(), "legacy", "legacy");
    let worker = Worker::open(cfg.clone()).unwrap();
    accepted(worker.handle(submit("accepted")));
    drop(worker);
    let mut store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert!(store.require_fenced_requests().is_err());
    assert!(!store.requires_fenced_requests().unwrap());
    drop(store);
    let cfg = config(root.path(), "empty-live", "empty-live");
    let worker = Worker::open(cfg.clone()).unwrap();
    let mut store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert!(store
        .require_fenced_requests()
        .unwrap_err()
        .contains("before any worker authority"));
    assert!(
        matches!(worker.handle(LocalRequest::Info),LocalResponse::Info{info} if info.fenced_requests.as_ref().unwrap().legacy_mutations_allowed)
    );
}

#[test]
fn schema_eleven_migration_preserves_authority_and_history_with_a_consistent_backup() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), "legacy", "state");
    let worker = Worker::open(cfg.clone()).unwrap();
    let context = worker.request_context().unwrap();
    let task = accepted(worker.handle(submit("migrate")));
    drop(worker);
    let path = cfg.state_directory.join("journal.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE journal_admission; PRAGMA user_version=11; PRAGMA wal_checkpoint(TRUNCATE);",
    )
    .unwrap();
    let raw: String = db
        .query_row(
            "SELECT record FROM tasks WHERE id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(db);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.journal_authority().incarnation,
        context.journal_incarnation
    );
    assert!(!store.requires_fenced_requests().unwrap());
    let migrated = store.get(&task.task_id).unwrap();
    assert_eq!(
        serde_json::to_value(&migrated).unwrap(),
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()
    );
    drop(store);
    let backup = fs::read_dir(&cfg.state_directory)
        .unwrap()
        .filter_map(|p| p.ok())
        .map(|p| p.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v11-")
        })
        .unwrap();
    let backup =
        rusqlite::Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let version: u32 = backup
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 11);
    let original: String = backup
        .query_row(
            "SELECT record FROM tasks WHERE id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, raw);
}

#[test]
fn matching_v2_context_cannot_bypass_read_only_inspection_or_bad_durable_boundary() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), "pair", "state");
    prepare(&cfg);
    let worker = Worker::open_recovery(cfg.clone()).unwrap();
    let reply = call(
        &worker,
        worker.request_context().unwrap(),
        submit("readonly"),
    );
    assert!(
        matches!(reply.response,LocalResponse::Error{message} if message.contains("диагностики"))
    );
    assert!(!root.path().join("project/runs.log").exists());
    drop(worker);
    let cfg = config(root.path(), "corrupt", "corrupt");
    prepare(&cfg);
    let worker = Worker::open(cfg.clone()).unwrap();
    let context = worker.request_context().unwrap();
    let db = rusqlite::Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap();
    db.execute_batch(
        "DROP TRIGGER admission_update_guard; UPDATE journal_admission SET incarnation='foreign';",
    )
    .unwrap();
    assert!(
        matches!(call(&worker,context,submit("corrupt")).response,LocalResponse::Error{message} if message.contains("boundary"))
    );
    assert!(!root.path().join("project/runs.log").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_ipc_and_cli_keep_the_explicit_contract_without_legacy_fallback() {
    let root = TempDir::new().unwrap();
    let cfg = config(
        root.path(),
        &format!("pair-{}", uuid::Uuid::new_v4()),
        "state",
    );
    prepare(&cfg);
    let worker = Worker::open(cfg.clone()).unwrap();
    let context = worker.request_context().unwrap();
    let server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
    let endpoint = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if winbridge_transport::call(&endpoint, &LocalRequest::Info)
            .await
            .is_ok()
        {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let request = FencedRequest {
        request_contract_version: 2,
        context: context.clone(),
        request: submit("cli"),
    };
    let config_path = root.path().join("worker.json");
    let request_path = root.path().join("request.json");
    fs::write(&config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let run = |operation: &str, path: &Path| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_winbridge-worker"));
        command
            .arg("--config")
            .arg(&config_path)
            .arg(operation)
            .arg(path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command.output().unwrap()
    };
    let result = run("invoke-v2", &request_path);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: FencedResponse = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response.context.as_ref(), Some(&context));
    let first = accepted(response.response);
    assert_eq!(wait(&worker, &first.task_id).status, TaskStatus::Succeeded);
    let again = winbridge_transport::call_fenced(&endpoint, &request)
        .await
        .unwrap();
    assert_eq!(accepted(again.response).task_id, first.task_id);
    let legacy_path = root.path().join("legacy.json");
    let LocalRequest::Submit { request } = submit("v1") else {
        unreachable!()
    };
    fs::write(&legacy_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let denied = run("submit", &legacy_path);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("contract 2"));
    worker.stop();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("project/runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
