use std::{
    fs, thread,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker};

fn setup() -> (tempfile::TempDir, WorkerConfig) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("project")).unwrap();
    let config=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"history-test","nodeId":"owned","stateDirectory":temp.path().join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":temp.path().join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    (temp, config)
}
fn request(key: &str) -> TaskSubmission {
    serde_json::from_value(serde_json::json!({"protocolVersion":1,"requestId":key,"projectId":"fixture","idempotencyKey":key,"timeoutMs":5000,"command":{"kind":"direct","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":["success"],"workingDirectory":"."}})).unwrap()
}
fn query() -> HistoryQuery {
    HistoryQuery {
        project_id: None,
        kind: None,
        status: None,
        search: String::new(),
        before: None,
    }
}

#[test]
fn cursor_ties_filters_literal_wildcards_and_restart_cover_the_entire_history() {
    let (temp, config) = setup();
    let path = temp.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut ids = Vec::new();
    for n in 0..65 {
        let r = request(&format!("history-{n}"));
        let mut task = store
            .accept(
                r.clone(),
                winbridge_runner::resolve(&config, &r).unwrap(),
                None,
            )
            .unwrap();
        task.accepted_at_ms = 100;
        task.resolved_command.executable = if n == 0 {
            "literal_%_Привет".into()
        } else {
            "tool".into()
        };
        store.save(&task).unwrap();
        ids.push(task.task_id);
    }
    let first = store.history(&query()).unwrap();
    assert_eq!(first.records.len(), 50);
    assert!(first.has_more);
    let mut q = query();
    q.before = first.next_cursor.clone();
    drop(store);
    let store = Store::open(&path).unwrap();
    let last = store.history(&q).unwrap();
    assert_eq!(last.records.len(), 15);
    assert!(!last.has_more);
    let mut actual = first
        .records
        .iter()
        .chain(&last.records)
        .map(|r| r.operation_id.clone())
        .collect::<Vec<_>>();
    actual.sort();
    ids.sort();
    assert_eq!(actual, ids);
    q = query();
    q.search = "_%_Привет".into();
    assert_eq!(store.history(&q).unwrap().records.len(), 1);
    q.search.clear();
    q.kind = Some(HistoryKind::Workflow);
    assert!(store.history(&q).unwrap().records.is_empty());
    q.kind = Some(HistoryKind::Task);
    q.status = Some("failed".into());
    assert!(store.history(&q).unwrap().records.is_empty());
    q.status = Some("invalid".into());
    assert!(store.history(&q).is_err());
    q = query();
    q.search = "x".repeat(201);
    assert!(store.history(&q).is_err());
}
#[test]
fn diagnostic_defaults_exclude_command_and_output_and_do_not_mutate_task() {
    let (_temp, config) = setup();
    fs::create_dir(&config.state_directory).unwrap();
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let mut r = request("key-private");
    if let CommandSource::Direct { arguments, .. } = &mut r.command {
        *arguments = vec![
            "--token".into(),
            "not-for-report".into(),
            "-EncodedCommand".into(),
            "UEFZTE9BRA==".into(),
        ];
    }
    let task = store
        .accept(
            r.clone(),
            winbridge_runner::resolve(&config, &r).unwrap(),
            None,
        )
        .unwrap();
    store
        .append_output(
            &task.task_id,
            OutputStream::Stdout,
            0,
            b"private output",
            1024,
        )
        .unwrap();
    let before = serde_json::to_value(store.get(&task.task_id).unwrap()).unwrap();
    let request = DiagnosticRequest {
        task_id: task.task_id.clone(),
        include_command: false,
        include_output: false,
        redact_values: vec![],
    };
    let preview = store.diagnostic(&request).unwrap();
    assert!(!preview.json.contains("not-for-report"));
    assert!(!preview.json.contains("private output"));
    assert!(!preview.json.contains("key-private"));
    let preview = store
        .diagnostic(&DiagnosticRequest {
            include_command: true,
            include_output: true,
            redact_values: vec!["private output".into()],
            ..request
        })
        .unwrap();
    assert!(!preview.json.contains("not-for-report"));
    assert!(!preview.json.contains("UEFZTE9BRA=="));
    assert!(!preview.json.contains("private output"));
    assert!(preview.json.contains("[REDACTED]"));
    assert_eq!(
        serde_json::to_value(store.get(&task.task_id).unwrap()).unwrap(),
        before
    );
}
#[test]
fn diagnostic_preserves_split_unicode_and_marks_output_and_quota_limits() {
    let (_temp, config) = setup();
    fs::create_dir(&config.state_directory).unwrap();
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let r = request("unicode");
    let task = store
        .accept(
            r.clone(),
            winbridge_runner::resolve(&config, &r).unwrap(),
            None,
        )
        .unwrap();
    let unicode = "Привет ✓".as_bytes();
    for (index, byte) in unicode.iter().enumerate() {
        store
            .append_output(
                &task.task_id,
                OutputStream::Stdout,
                index as u64,
                &[*byte],
                1024,
            )
            .unwrap();
    }
    let request = DiagnosticRequest {
        task_id: task.task_id.clone(),
        include_command: false,
        include_output: true,
        redact_values: vec![],
    };
    let preview = store.diagnostic(&request).unwrap();
    let json: serde_json::Value = serde_json::from_str(&preview.json).unwrap();
    assert_eq!(json["output"]["stdout"], "Привет ✓");
    assert!(!preview.output_truncated);
    store
        .append_output(
            &task.task_id,
            OutputStream::Stderr,
            0,
            &vec![b'a'; 70000],
            80000,
        )
        .unwrap();
    let preview = store.diagnostic(&request).unwrap();
    assert!(preview.output_truncated);
    let json: serde_json::Value = serde_json::from_str(&preview.json).unwrap();
    assert_eq!(json["capturedOutputBytes"], 65536);
    store
        .append_output(&task.task_id, OutputStream::Stderr, 70000, b"over quota", 1)
        .unwrap();
    assert!(store.diagnostic(&request).unwrap().output_truncated);
}
#[test]
fn actual_failed_process_has_history_diagnostic_and_truthful_storage_usage() {
    let (_temp, config) = setup();
    let worker = Worker::open(config.clone()).unwrap();
    let mut r = request("actual");
    if let CommandSource::Direct { arguments, .. } = &mut r.command {
        *arguments = vec!["failure".into()];
    }
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit { request: r }) else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        if let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) {
            if task.status.is_terminal() {
                assert_eq!(task.status, TaskStatus::Failed);
                break;
            }
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let LocalResponse::History { page } = worker.handle(LocalRequest::History { query: query() })
    else {
        panic!()
    };
    assert_eq!(page.records[0].operation_id, task.task_id);
    assert_eq!(page.records[0].status, "failed");
    let LocalResponse::StorageUsage { usage } = worker.handle(LocalRequest::StorageUsage) else {
        panic!()
    };
    assert_eq!(usage.task_count, 1);
    assert_eq!(usage.attention_count, 1);
    assert!(usage.stored_output_bytes > 0);
    assert!(usage.journal_bytes > 0);
    let LocalResponse::Diagnostic { preview } =
        worker.handle(LocalRequest::PrepareTaskDiagnostic {
            request: DiagnosticRequest {
                task_id: task.task_id.clone(),
                include_command: false,
                include_output: true,
                redact_values: vec![],
            },
        })
    else {
        panic!()
    };
    assert!(preview.json.contains("\"failed\""));
    assert!(preview.json.contains("stderr"));
    worker.stop();
}
#[test]
fn unknown_ids_bad_redactions_and_bad_cursors_do_not_create_tasks() {
    let (_temp, config) = setup();
    let worker = Worker::open(config).unwrap();
    for redactions in [vec![], vec!["".into()], vec!["x".into(); 33]] {
        assert!(matches!(
            worker.handle(LocalRequest::PrepareTaskDiagnostic {
                request: DiagnosticRequest {
                    task_id: "absent".into(),
                    include_command: true,
                    include_output: true,
                    redact_values: redactions
                }
            }),
            LocalResponse::Error { .. }
        ));
    }
    let mut q = query();
    q.before = Some(HistoryCursor {
        created_at_ms: u64::MAX,
        kind: HistoryKind::Task,
        operation_id: "a".into(),
    });
    assert!(matches!(
        worker.handle(LocalRequest::History { query: q }),
        LocalResponse::Error { .. }
    ));
    let LocalResponse::Tasks { tasks } = worker.handle(LocalRequest::ListTasks) else {
        panic!()
    };
    assert!(tasks.is_empty());
    worker.stop();
}
