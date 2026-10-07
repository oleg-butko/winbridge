use std::{
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{
    AttentionRecord, AttentionSeverity, AttentionSubmission, CommandSource, LocalRequest,
    LocalResponse, ResolvedCommand, TaskRecord, TaskStatus, TaskSubmission,
};
use winbridge_worker::{store::Store, Worker};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"attention-test","nodeId":"test-worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}
fn submission(key: &str, mode: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "fixture".into(),
        idempotency_key: key.into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
fn task(response: LocalResponse) -> TaskRecord {
    match response {
        LocalResponse::Task { task } => *task,
        other => panic!("Expected task: {other:?}"),
    }
}
fn attention(response: LocalResponse) -> AttentionRecord {
    match response {
        LocalResponse::Attention { attention } => attention,
        other => panic!("Expected attention: {other:?}"),
    }
}
fn request(id: &str, key: &str) -> AttentionSubmission {
    AttentionSubmission {
        request_id: "initial".into(),
        idempotency_key: key.into(),
        task_id: id.into(),
        severity: AttentionSeverity::Warning,
        message: "Проверьте результат ✓".into(),
    }
}
fn accepted(store: &mut Store) -> TaskRecord {
    store
        .accept(
            submission("fixture", "success"),
            ResolvedCommand {
                executable: "fixture".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 5000,
            },
            None,
        )
        .unwrap()
}

#[test]
fn active_count_includes_tasks_older_than_recent_page() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path())).unwrap();
    for i in 0..13 {
        task(worker.handle(LocalRequest::Submit {
            request: submission(&format!("count-{i}"), "success"),
        }));
    }
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!()
    };
    assert_eq!(info.active_task_count, Some(13));
}
fn wait(worker: &Arc<Worker>, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let record = task(worker.handle(LocalRequest::GetTask { task_id: id.into() }));
        if record.status.is_terminal() && worker.is_idle() {
            return record;
        }
        assert!(Instant::now() < deadline, "Task deadline: {record:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn uncommitted_process_result_is_not_reported_fresh_and_new_launches_wait_for_recovery() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let failed = task(worker.handle(LocalRequest::Submit {
        request: submission("uncommitted", "failure"),
    }));
    let queued = task(worker.handle(LocalRequest::Submit {
        request: submission("waiting", "success"),
    }));
    let connection =
        rusqlite::Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_outcome BEFORE INSERT ON attention WHEN NEW.origin='task_outcome' BEGIN SELECT RAISE(ABORT,'expected outcome persistence failure'); END;").unwrap();
    worker.schedule().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !worker.is_idle() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!()
    };
    assert_eq!(info.active_task_count, None);
    assert!(info
        .journal_recovery_required
        .unwrap()
        .contains("uncommitted"));
    assert!(
        matches!(worker.handle(LocalRequest::GetTask {task_id:failed.task_id.clone()}),LocalResponse::Error {message} if message.contains("status is stale"))
    );
    assert!(
        matches!(worker.handle(LocalRequest::Submit {request:submission("blocked","success")}),LocalResponse::Error {message} if message.contains("suspended"))
    );
    worker.schedule().unwrap();
    assert_eq!(
        task(worker.handle(LocalRequest::GetTask {
            task_id: queued.task_id.clone()
        }))
        .status,
        TaskStatus::Queued
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    connection
        .execute_batch("DROP TRIGGER fail_outcome;")
        .unwrap();
    drop(connection);
    drop(worker);
    let recovered = Worker::open(cfg).unwrap();
    assert_eq!(
        task(recovered.handle(LocalRequest::GetTask {
            task_id: failed.task_id
        }))
        .status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        task(recovered.handle(LocalRequest::GetTask {
            task_id: queued.task_id
        }))
        .status,
        TaskStatus::Queued
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn lost_ack_dedup_conflict_and_acknowledgement_survive_restart() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let record = task(worker.handle(LocalRequest::Submit {
        request: submission("queued", "success"),
    }));
    let mut frozen = request(&record.task_id, "one");
    let first = attention(worker.handle(LocalRequest::RequestAttention {
        request: frozen.clone(),
    }));
    frozen.request_id = "reconnected".into();
    assert_eq!(
        first,
        attention(worker.handle(LocalRequest::RequestAttention {
            request: frozen.clone()
        }))
    );
    frozen.message = "Another reason".into();
    assert!(matches!(
        worker.handle(LocalRequest::RequestAttention { request: frozen }),
        LocalResponse::Error { .. }
    ));
    let ack = attention(worker.handle(LocalRequest::AcknowledgeAttention {
        attention_id: first.attention_id.clone(),
    }));
    assert!(ack.acknowledged_at_ms.is_some());
    drop(worker);
    let worker = Worker::open(cfg).unwrap();
    assert_eq!(
        ack,
        attention(worker.handle(LocalRequest::AcknowledgeAttention {
            attention_id: first.attention_id.clone()
        }))
    );
    assert_eq!(
        ack,
        attention(worker.handle(LocalRequest::RequestAttention {
            request: request(&record.task_id, "one")
        }))
    );
    let LocalResponse::AttentionList { page } = worker.handle(LocalRequest::ListAttention {
        after_sequence: 0,
        pending_only: true,
    }) else {
        panic!()
    };
    assert!(page.records.is_empty());
    assert_eq!(page.pending_count, 0);
    let LocalResponse::AttentionList { page } = worker.handle(LocalRequest::ListAttention {
        after_sequence: 0,
        pending_only: false,
    }) else {
        panic!()
    };
    assert_eq!(page.records, vec![ack]);
    assert_eq!(
        task(worker.handle(LocalRequest::GetTask {
            task_id: record.task_id
        }))
        .status,
        TaskStatus::Queued,
        "Opening attention must not execute a task"
    );
}

#[test]
fn actual_failed_process_creates_one_attention_success_and_cancel_stay_quiet() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let failed = task(worker.handle(LocalRequest::Submit {
        request: submission("failed", "failure"),
    }));
    assert_eq!(wait(&worker, &failed.task_id).status, TaskStatus::Failed);
    let success = task(worker.handle(LocalRequest::Submit {
        request: submission("success", "success"),
    }));
    assert_eq!(
        wait(&worker, &success.task_id).status,
        TaskStatus::Succeeded
    );
    let cancelled = task(worker.handle(LocalRequest::Submit {
        request: submission("cancelled", "long"),
    }));
    assert_eq!(
        task(worker.handle(LocalRequest::Cancel {
            task_id: cancelled.task_id
        }))
        .status,
        TaskStatus::Cancelled
    );
    drop(worker);
    let worker = Worker::open(cfg).unwrap();
    let LocalResponse::AttentionList { page } = worker.handle(LocalRequest::ListAttention {
        after_sequence: 0,
        pending_only: true,
    }) else {
        panic!()
    };
    assert_eq!(page.pending_count, 1);
    assert_eq!(page.records[0].task_id, failed.task_id);
    assert_eq!(page.records[0].severity, AttentionSeverity::Error);
    assert_eq!(page.records[0].origin, "task_outcome");
}

#[test]
fn failed_attention_write_rolls_back_task_finish_and_retry_is_atomic() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let queued = accepted(&mut store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_attention BEFORE INSERT ON attention BEGIN SELECT RAISE(ABORT,'expected attention write failure'); END;").unwrap();
    assert!(store
        .finish(
            &queued.task_id,
            TaskStatus::Failed,
            Some(1),
            "fixture failure"
        )
        .unwrap_err()
        .contains("expected attention write failure"));
    assert_eq!(
        store.get(&queued.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert!(store.list_attention(0, false).unwrap().records.is_empty());
    connection
        .execute_batch("DROP TRIGGER fail_attention;")
        .unwrap();
    let finished = store
        .finish(
            &queued.task_id,
            TaskStatus::Failed,
            Some(1),
            "fixture failure",
        )
        .unwrap();
    assert_eq!(finished.status, TaskStatus::Failed);
    store
        .finish(
            &queued.task_id,
            TaskStatus::Failed,
            Some(2),
            "retry must not change result",
        )
        .unwrap();
    assert_eq!(store.list_attention(0, false).unwrap().records.len(), 1);
}

#[test]
fn crash_recovery_adds_attention_without_replaying_command() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut running = accepted(&mut store);
    running.status = TaskStatus::Running;
    running.process_id = Some(12345);
    store.save(&running).unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    store.recover().unwrap();
    store.recover().unwrap();
    assert_eq!(
        store.get(&running.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    let page = store.list_attention(0, true).unwrap();
    assert_eq!(page.pending_count, 1);
    assert_eq!(page.records[0].severity, AttentionSeverity::Warning);
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn bounded_pages_and_validation_preserve_pending_events() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let queued = accepted(&mut store);
    for key in 0..56 {
        store
            .request_attention(&request(&queued.task_id, &key.to_string()))
            .unwrap();
    }
    let first = store.list_attention(0, true).unwrap();
    assert_eq!(first.records.len(), 50);
    assert!(first.has_more);
    assert_eq!(first.pending_count, 56);
    store
        .acknowledge_attention(&first.records[0].attention_id)
        .unwrap();
    let second = store.list_attention(first.next_sequence, true).unwrap();
    assert_eq!(second.records.len(), 6);
    assert!(!second.has_more);
    assert_eq!(second.pending_count, 55);
    assert!(second.records[0].sequence > first.next_sequence);
    assert!(store.list_attention(u64::MAX, true).is_err());
    assert!(store.acknowledge_attention("unknown").is_err());
    assert!(store
        .request_attention(&request("unknown", "unknown"))
        .is_err());
    for message in ["".to_string(), "Привет".repeat(100), "line\ncontrol".into()] {
        let mut invalid = request(&queued.task_id, "invalid");
        invalid.message = message;
        assert!(store.request_attention(&invalid).is_err());
    }
    assert_eq!(store.list_attention(0, false).unwrap().pending_count, 55);
}

#[test]
fn explicit_queue_limit_does_not_drop_task_outcomes_or_reject_original_retry() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let queued = accepted(&mut store);
    let first = store
        .request_attention(&request(&queued.task_id, "0"))
        .unwrap();
    for key in 1..1000 {
        store
            .request_attention(&request(&queued.task_id, &key.to_string()))
            .unwrap();
    }
    assert!(store
        .request_attention(&request(&queued.task_id, "overflow"))
        .unwrap_err()
        .contains("limit"));
    assert_eq!(
        store
            .request_attention(&request(&queued.task_id, "0"))
            .unwrap(),
        first
    );
    store
        .finish(
            &queued.task_id,
            TaskStatus::Failed,
            Some(1),
            "fixture failure",
        )
        .unwrap();
    assert_eq!(store.list_attention(0, true).unwrap().pending_count, 1001);
    store.acknowledge_attention(&first.attention_id).unwrap();
    store
        .request_attention(&request(&queued.task_id, "overflow"))
        .unwrap();
}

#[test]
fn v3_migration_has_consistent_backup_and_preserves_old_task() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let queued = accepted(&mut store);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE attention; PRAGMA user_version=3;")
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.get(&queued.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert!(store.list_attention(0, false).unwrap().records.is_empty());
    drop(store);
    let backups: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v3-")
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
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(Store::open(&path).unwrap());
    assert_eq!(
        std::fs::read_dir(root.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("journal-v3-"))
            .count(),
        1
    );
}
