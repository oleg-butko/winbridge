use serde_json::json;
use std::{
    fs,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{
    store::{now_ms, Store},
    Worker,
};

fn setup() -> (tempfile::TempDir, WorkerConfig, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("project")).unwrap();
    let config: WorkerConfig = serde_json::from_value(json!({"schemaVersion":1,"instanceId":"maintenance-test","nodeId":"owned","stateDirectory":temp.path().join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":temp.path().join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    let path = temp.path().join("worker.local.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    (temp, config, path)
}
fn request(key: &str, mode: &str) -> TaskSubmission {
    serde_json::from_value(json!({"protocolVersion":1,"requestId":key,"projectId":"fixture","idempotencyKey":key,"timeoutMs":30000,"command":{"kind":"direct","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":[mode],"workingDirectory":"."}})).unwrap()
}
fn old_task(store: &mut Store, config: &WorkerConfig, key: &str) -> TaskRecord {
    let r = request(key, "success");
    let task = store
        .accept(
            r.clone(),
            winbridge_runner::resolve(config, &r).unwrap(),
            None,
        )
        .unwrap();
    store
        .append_output(
            &task.task_id,
            OutputStream::Stdout,
            0,
            "Привет ✓".as_bytes(),
            4096,
        )
        .unwrap();
    store
        .finish(
            &task.task_id,
            TaskStatus::Succeeded,
            Some(0),
            "test completed",
        )
        .unwrap();
    let mut task = store.get(&task.task_id).unwrap();
    task.finished_at_ms = Some(now_ms() - 40 * 86_400_000);
    let mut observed = store.task_snapshot(&task.task_id).unwrap();
    observed.record = task.clone();
    store.save(&observed).unwrap();
    task
}
fn preview(store: &Store) -> OutputCleanupPreview {
    store
        .preview_output_cleanup(&OutputCleanupQuery {
            older_than_days: 30,
            project_id: None,
        })
        .unwrap()
}
fn edit(worker: &Worker, value: serde_json::Value) -> SaveRuntimeSettings {
    let old = worker.runtime_settings().unwrap();
    SaveRuntimeSettings {
        generation: old.generation,
        request_id: uuid::Uuid::new_v4().to_string(),
        expected_sha256: old.saved_sha256,
        json: serde_json::to_string_pretty(&value).unwrap(),
    }
}
struct Owner(Arc<Worker>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.0.is_idle() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn settings_save_is_local_pinned_and_does_not_change_a_running_process() {
    let (_temp, config, path) = setup();
    let owner = Owner(Worker::open_file(&path).unwrap());
    let worker = &owner.0;
    assert!(!worker.runtime_settings().unwrap().restart_required);
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
        request: request("live", "long"),
    }) else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid = loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) else {
            panic!()
        };
        if let Some(pid) = current.process_id {
            break pid;
        };
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    };
    let before = fs::read(&path).unwrap();
    let loaded_policy = match worker.handle(LocalRequest::Info) {
        LocalResponse::Info { info } => info.project_policies,
        other => panic!("{other:?}"),
    };
    let mut next = serde_json::to_value(&config).unwrap();
    next["maxOutputBytesPerTask"] = json!(4096);
    next["worker"]["controllerUrl"] = json!("ws://127.0.0.1:1143");
    let save = edit(worker, next);
    let saved = worker.save_runtime_settings(&save).unwrap();
    assert!(saved.settings.restart_required);
    assert_eq!(
        fs::read(saved.backup_path.as_ref().unwrap()).unwrap(),
        before
    );
    assert_eq!(
        worker.config.max_output_bytes_per_task,
        config.max_output_bytes_per_task
    );
    assert!(worker.config.worker.controller_url.is_none());
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("info")
    };
    assert_eq!(
        info.project_policies, loaded_policy,
        "Pending saved settings cannot change loaded policy"
    );
    let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
        task_id: task.task_id.clone(),
    }) else {
        panic!()
    };
    assert_eq!(current.process_id, Some(pid));
    assert_eq!(current.status, TaskStatus::Running);
    assert_eq!(
        worker
            .save_runtime_settings(&save)
            .unwrap()
            .settings
            .saved_sha256,
        saved.settings.saved_sha256
    );
    worker.handle(LocalRequest::Cancel {
        task_id: task.task_id,
    });
    drop(owner);
    let worker = Worker::open_file(&path).unwrap();
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("info")
    };
    assert_ne!(
        info.project_policies, loaded_policy,
        "Restart must advertise newly loaded policy"
    );
    assert_eq!(worker.config.max_output_bytes_per_task, 4096);
    assert!(!worker.runtime_settings().unwrap().restart_required);
}
#[test]
fn invalid_settings_identity_and_external_edit_are_preserved() {
    let (_temp, config, path) = setup();
    let worker = Worker::open_file(&path).unwrap();
    let before = fs::read(&path).unwrap();
    for (key, value) in [
        (
            "stateDirectory",
            json!(path.parent().unwrap().join("other-state")),
        ),
        ("instanceId", json!("other")),
        ("nodeId", json!("other")),
        ("maxOutputBytesPerTask", json!(1)),
    ] {
        let mut next = serde_json::to_value(&config).unwrap();
        next[key] = value;
        assert!(worker.save_runtime_settings(&edit(&worker, next)).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let mut next = serde_json::to_value(&config).unwrap();
    next["worker"]["controllerUrl"] = json!("wss://remote.invalid");
    assert!(worker.save_runtime_settings(&edit(&worker, next)).is_err());
    let mut next = serde_json::to_value(&config).unwrap();
    next["maxOutputBytesPerTask"] = json!(4096);
    let save = edit(&worker, next);
    fs::write(&path, b"{ external editing }").unwrap();
    assert!(worker
        .save_runtime_settings(&save)
        .unwrap_err()
        .contains("changed"));
    assert_eq!(fs::read(&path).unwrap(), b"{ external editing }");
}
#[test]
fn cleanup_preserves_metadata_keys_and_results_across_restart() {
    let (_temp, config, _path) = setup();
    let path = config.state_directory.with_file_name("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = old_task(&mut store, &config, "old");
    let request = ApplyOutputCleanup {
        idempotency_key: "reviewed-cleanup".into(),
        plan: preview(&store).plan,
    };
    let result = store.apply_output_cleanup(&request).unwrap();
    assert_eq!(result.removed_tasks, 1);
    assert_eq!(result.removed_bytes, "Привет ✓".len() as u64);
    assert!(store.output(&task.task_id, 0).unwrap().is_empty());
    let record = store.get(&task.task_id).unwrap();
    assert_eq!(record.exit_code, Some(0));
    assert_eq!(record.status, TaskStatus::Succeeded);
    assert!(record.output_removed_at_ms.is_some());
    assert_eq!(record.output_bytes, task.output_bytes);
    assert_eq!(
        store.existing(&task.submission).unwrap().unwrap().task_id,
        task.task_id
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        serde_json::to_value(store.apply_output_cleanup(&request).unwrap()).unwrap(),
        serde_json::to_value(result).unwrap()
    );
    let preview = store
        .diagnostic(&DiagnosticRequest {
            task_id: task.task_id,
            include_command: false,
            include_output: true,
            redact_values: vec![],
        })
        .unwrap();
    assert!(preview.output_truncated);
    assert!(preview.json.contains("outputRemovedAtMs"));
    let mut changed = request;
    changed.plan.sha256 = "0".repeat(64);
    assert!(store.apply_output_cleanup(&changed).is_err());
}
#[test]
fn new_attention_between_preview_and_apply_protects_output() {
    let (_temp, config, _) = setup();
    let mut store = Store::open(&config.state_directory.with_file_name("journal.sqlite")).unwrap();
    let task = old_task(&mut store, &config, "attention");
    let plan = preview(&store).plan;
    let attention = store
        .request_attention(&AttentionSubmission {
            request_id: "attention".into(),
            idempotency_key: "attention".into(),
            task_id: task.task_id.clone(),
            severity: AttentionSeverity::Warning,
            message: "Review result".into(),
        })
        .unwrap();
    assert!(preview(&store).entries.is_empty());
    assert!(store
        .apply_output_cleanup(&ApplyOutputCleanup {
            idempotency_key: "changed".into(),
            plan
        })
        .is_err());
    assert!(!store.output(&task.task_id, 0).unwrap().is_empty());
    store
        .acknowledge_attention(&attention.attention_id)
        .unwrap();
    assert_eq!(preview(&store).entries.len(), 1);
}
#[test]
fn cleanup_transaction_rolls_back_on_journal_error() {
    let (_temp, config, _) = setup();
    let path = config.state_directory.with_file_name("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = old_task(&mut store, &config, "rollback");
    let request = ApplyOutputCleanup {
        idempotency_key: "rollback".into(),
        plan: preview(&store).plan,
    };
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_cleanup BEFORE INSERT ON output_cleanup BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    assert!(store.apply_output_cleanup(&request).is_err());
    assert!(!store.output(&task.task_id, 0).unwrap().is_empty());
    assert!(store
        .get(&task.task_id)
        .unwrap()
        .output_removed_at_ms
        .is_none());
    connection
        .execute_batch("DROP TRIGGER fail_cleanup;")
        .unwrap();
    assert_eq!(
        store.apply_output_cleanup(&request).unwrap().removed_tasks,
        1
    );
}
#[test]
fn retention_excludes_running_indeterminate_workflows_and_applications() {
    let (_temp, config, _) = setup();
    let path = config.state_directory.with_file_name("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut task = old_task(&mut store, &config, "protected");
    for status in [
        TaskStatus::Queued,
        TaskStatus::Running,
        TaskStatus::Indeterminate,
    ] {
        task.status = status;
        let mut observed = store.task_snapshot(&task.task_id).unwrap();
        observed.record = task.clone();
        store.save(&observed).unwrap();
        assert!(preview(&store).entries.is_empty());
    }
    task.status = TaskStatus::Succeeded;
    let mut observed = store.task_snapshot(&task.task_id).unwrap();
    observed.record = task.clone();
    store.save(&observed).unwrap();
    // SQL fixtures exercise the exact relationship predicates, independent of orchestration.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let record = json!({"steps":[{"operationId":task.task_id}]}).to_string();
    connection.execute("INSERT INTO workflows(id,project,key,payload,status,record) VALUES ('wf','fixture','wf','{}','running',?1)",[&record]).unwrap();
    assert!(preview(&store).entries.is_empty());
    connection
        .execute(
            "UPDATE workflows SET status='succeeded',revision=revision+1",
            [],
        )
        .unwrap();
    assert_eq!(preview(&store).entries.len(), 1);
    connection.execute("INSERT INTO applications(id,workflow,task,status,record) VALUES ('app','wf',?1,'ready','{}')",[&task.task_id]).unwrap();
    assert!(preview(&store).entries.is_empty());
    connection
        .execute(
            "UPDATE applications SET status='stopped',revision=revision+1",
            [],
        )
        .unwrap();
    assert_eq!(preview(&store).entries.len(), 1);
    let attention = store
        .request_attention(&AttentionSubmission {
            request_id: "pending-wf".into(),
            idempotency_key: "pending-wf".into(),
            task_id: task.task_id,
            severity: AttentionSeverity::Warning,
            message: "Review".into(),
        })
        .unwrap();
    connection
        .execute_batch("DROP TRIGGER attention_revision_guard;")
        .unwrap();
    connection
        .execute(
            "UPDATE attention SET task=NULL,target=?2 WHERE id=?1",
            rusqlite::params![
                attention.attention_id,
                json!({"kind":"application","applicationId":"app","workflowId":"wf"}).to_string()
            ],
        )
        .unwrap();
    assert!(preview(&store).entries.is_empty());
}
#[test]
fn automatic_cleanup_is_opt_in_and_preserves_history() {
    let (_temp, config, path) = setup();
    let owner = Owner(Worker::open_file(&path).unwrap());
    let db = config.state_directory.join("journal.sqlite");
    let mut store = Store::open(&db).unwrap();
    let task = old_task(&mut store, &config, "automatic");
    owner.0.automatic_output_cleanup().unwrap();
    thread::sleep(Duration::from_millis(30));
    assert!(!store.output(&task.task_id, 0).unwrap().is_empty());
    drop(store);
    drop(owner);
    let mut enabled = config;
    enabled.output_retention.automatic = true;
    fs::write(&path, serde_json::to_vec(&enabled).unwrap()).unwrap();
    let owner = Owner(Worker::open_file(&path).unwrap());
    owner.0.automatic_output_cleanup().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner.0.is_idle() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let store = Store::open(&db).unwrap();
    assert!(store.output(&task.task_id, 0).unwrap().is_empty());
    assert_eq!(
        store.get(&task.task_id).unwrap().status,
        TaskStatus::Succeeded
    );
}
#[test]
fn schema_five_upgrade_has_a_consistent_backup_and_preserves_output() {
    let (temp, config, _) = setup();
    let path = temp.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = old_task(&mut store, &config, "migration");
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; DROP TABLE execution_attempts; DROP TABLE journal_authority; DROP TRIGGER imports_revision_guard; ALTER TABLE imports DROP COLUMN revision; DROP TRIGGER workflows_revision_guard; ALTER TABLE workflows DROP COLUMN revision; DROP TRIGGER applications_revision_guard; ALTER TABLE applications DROP COLUMN revision; DROP TRIGGER attention_revision_guard; ALTER TABLE attention DROP COLUMN revision; DROP TRIGGER tasks_revision_guard; ALTER TABLE tasks DROP COLUMN revision; DROP TABLE output_cleanup; PRAGMA user_version=5;")
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert!(!store.output(&task.task_id, 0).unwrap().is_empty());
    let backups: Vec<_> = fs::read_dir(temp.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v5-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u16>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM output", [], |r| r.get::<_, u16>(0))
            .unwrap(),
        1
    );
}
#[cfg(unix)]
#[test]
fn linked_config_is_refused_without_modifying_its_target() {
    let (temp, _config, path) = setup();
    let before = fs::read(&path).unwrap();
    let link = temp.path().join("link.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(Worker::open_file(&link).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn cleanup_batches_and_invalid_queries_keep_recent_output() {
    let (_temp, config, _) = setup();
    let mut store = Store::open(&config.state_directory.with_file_name("journal.sqlite")).unwrap();
    for i in 0..55 {
        old_task(&mut store, &config, &format!("batch-{i}"));
    }
    let mut recent = old_task(&mut store, &config, "recent");
    recent.finished_at_ms = Some(now_ms());
    let mut observed = store.task_snapshot(&recent.task_id).unwrap();
    observed.record = recent.clone();
    store.save(&observed).unwrap();
    for query in [
        OutputCleanupQuery {
            older_than_days: 0,
            project_id: None,
        },
        OutputCleanupQuery {
            older_than_days: 30,
            project_id: Some("../outside".into()),
        },
    ] {
        assert!(store.preview_output_cleanup(&query).is_err());
    }
    let first = preview(&store);
    assert_eq!(first.entries.len(), 50);
    assert!(first.has_more);
    store
        .apply_output_cleanup(&ApplyOutputCleanup {
            idempotency_key: "batch-one".into(),
            plan: first.plan,
        })
        .unwrap();
    let next = preview(&store);
    assert_eq!(next.entries.len(), 5);
    assert!(!next.has_more);
    store
        .apply_output_cleanup(&ApplyOutputCleanup {
            idempotency_key: "batch-two".into(),
            plan: next.plan,
        })
        .unwrap();
    assert!(preview(&store).entries.is_empty());
    assert!(!store.output(&recent.task_id, 0).unwrap().is_empty());
    assert_eq!(store.storage_usage().unwrap().task_count, 56);
}
