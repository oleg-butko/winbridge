//! Authority/attempt races through real SQLite; runtime faults use isolated workers.
use rusqlite::Connection;
use std::{
    fs,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::{config::WorkerConfig, task::TaskEvent};
use winbridge_protocol::*;
use winbridge_worker::{
    store::{ExecutionToken, OperationKind, Store, JOURNAL_SCHEMA_VERSION},
    Worker,
};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"ownership-tests","nodeId":"test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}
fn request(key: &str, mode: &str) -> TaskSubmission {
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
fn accept(store: &mut Store, root: &Path, key: &str) -> TaskRecord {
    let request = request(key, "success");
    let command = winbridge_runner::resolve(&config(root), &request).unwrap();
    store.accept(request, command, None).unwrap()
}
fn imported(store: &mut Store) -> ImportRecord {
    imported_in(store, "fixture")
}
fn imported_in(store: &mut Store, project: &str) -> ImportRecord {
    store
        .accept_import(
            ImportSubmission {
                protocol_version: 1,
                request_id: "import".into(),
                project_id: project.into(),
                snapshot_id: "snapshot".into(),
                manifest_sha256: "a".repeat(64),
                idempotency_key: "import".into(),
                timeout_ms: 5000,
            },
            "shared".into(),
            "workspace".into(),
        )
        .unwrap()
}
fn attention(store: &mut Store, id: &str) -> AttentionRecord {
    store
        .request_attention(&AttentionSubmission {
            request_id: "review".into(),
            idempotency_key: "review".into(),
            task_id: id.into(),
            severity: AttentionSeverity::Warning,
            message: "Review".into(),
        })
        .unwrap()
}
fn task(response: LocalResponse) -> TaskRecord {
    match response {
        LocalResponse::Task { task } => *task,
        other => panic!("{other:?}"),
    }
}
fn wait_idle(worker: &Arc<Worker>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !worker.is_idle() {
        assert!(
            Instant::now() < deadline,
            "worker did not release its owned job"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

struct RunningWorker(Arc<Worker>);
impl Drop for RunningWorker {
    fn drop(&mut self) {
        self.0.stop();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.0.is_idle() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn is_executing(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &format!("if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}")])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW, including the test probe.
            .status().expect("process liveness probe").success()
    }
    #[cfg(unix)]
    {
        fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|stat| !stat.split_once(") ").unwrap().1.starts_with('Z'))
    }
}

#[test]
fn simultaneous_claims_authorize_exactly_one_durable_attempt() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let accepted = accept(&mut store, root.path(), "race");
    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let threads: Vec<_> = [first, second]
        .into_iter()
        .map(|mut store| {
            let barrier = barrier.clone();
            let id = accepted.task_id.clone();
            thread::spawn(move || {
                barrier.wait();
                store.claim_task(&id)
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let token = &results.iter().find_map(|r| r.as_ref().ok()).unwrap().1;
    assert_eq!(
        &store
            .execution_attempt(OperationKind::Task, &accepted.task_id)
            .unwrap()
            .unwrap()
            .token,
        token
    );
    assert_eq!(
        store.task_snapshot(&accepted.task_id).unwrap().revision(),
        1
    );
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn losing_journal_authority_terminates_owned_tree_without_publishing_a_false_outcome() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    let owner = RunningWorker(Worker::open(cfg.clone()).unwrap());
    let mut submission = request("tree", "tree");
    submission.timeout_ms = 60000;
    let accepted = task(owner.0.handle(LocalRequest::Submit {
        request: submission,
    }));
    owner.0.schedule().unwrap();
    let path = cfg.state_directory.join("journal.sqlite");
    let mut observer = Store::open(&path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (parent, child) = loop {
        let record = observer.get(&accepted.task_id).unwrap();
        let bytes: Vec<u8> = observer
            .output(&accepted.task_id, 0)
            .unwrap()
            .into_iter()
            .flat_map(|c| c.bytes)
            .collect();
        let text = String::from_utf8_lossy(&bytes);
        if let (Some(pid), Some(child)) = (
            record.process_id,
            text.lines().find_map(|line| {
                line.strip_prefix("child pid=")
                    .and_then(|s| s.parse::<u32>().ok())
            }),
        ) {
            break (pid, child);
        }
        assert!(
            Instant::now() < deadline,
            "fixture tree did not start: {record:?}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert!(is_executing(parent) && is_executing(child));
    let attempt = observer
        .execution_attempt(OperationKind::Task, &accepted.task_id)
        .unwrap()
        .unwrap();
    // Deliberate isolated fault injection; production takeover still needs the OS lock.
    observer.begin_authority().unwrap();
    wait_idle(&owner.0);
    assert!(!is_executing(parent) && !is_executing(child));
    assert!(matches!(
        owner.0.handle(LocalRequest::GetTask {
            task_id: accepted.task_id.clone()
        }),
        LocalResponse::Error { .. }
    ));
    assert_eq!(
        observer.get(&accepted.task_id).unwrap().status,
        TaskStatus::Running
    );
    assert!(observer
        .execution_attempt(OperationKind::Task, &accepted.task_id)
        .unwrap()
        .unwrap()
        .completed_at_ms
        .is_none());
    assert!(output(&mut observer, &attempt.token).is_err());
    observer.recover().unwrap();
    assert_eq!(
        observer.get(&accepted.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        observer
            .execution_attempt(OperationKind::Task, &accepted.task_id)
            .unwrap()
            .unwrap()
            .outcome,
        Some(TaskStatus::Indeterminate)
    );
}
fn output(store: &mut Store, token: &ExecutionToken) -> Result<(), String> {
    store.with_execution(token, |s| {
        s.append_output(
            &token.operation.operation_id,
            OutputStream::Stdout,
            0,
            b"actual output",
            4096,
        )
    })
}

fn flow_request() -> WorkflowSubmission {
    let command = request("unused", "long").command;
    WorkflowSubmission {
        protocol_version: 1,
        request_id: "flow".into(),
        idempotency_key: "flow".into(),
        name: "owner test".into(),
        project_id: "fixture".into(),
        snapshot_id: "snapshot".into(),
        manifest_sha256: "a".repeat(64),
        timeout_ms: 5000,
        build: command.clone(),
        start: command,
        readiness: ReadinessProbe {
            stdout_contains: "READY".into(),
            timeout_ms: 1000,
            netwatch: None,
        },
        log_path: "logs/app.log".into(),
        artifacts: vec![],
    }
}

#[test]
fn activation_preserves_incarnation_advances_epoch_and_rejects_old_connection_and_snapshot() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut old = Store::open(&path).unwrap();
    let task = accept(&mut old, root.path(), "snapshot");
    let before = old.journal_authority().clone();
    let stale = old.task_snapshot(&task.task_id).unwrap();
    let mut next = Store::open(&path).unwrap();
    let first = next.begin_authority().unwrap();
    assert_eq!(first.incarnation, before.incarnation);
    assert_eq!(first.epoch, before.epoch + 1);
    assert_ne!(first.authority_id, before.authority_id);
    assert!(old.save(&stale).unwrap_err().contains("stale"));
    assert!(next.save(&stale).unwrap_err().contains("stale"));
    assert!(old.begin_authority().unwrap_err().contains("stale"));
    assert_eq!(next.task_snapshot(&task.task_id).unwrap().revision(), 0);
    drop(next);
    let next = Store::open(&path).unwrap();
    assert_eq!(next.journal_authority(), &first);
}

#[test]
fn every_existing_write_family_rejects_a_stale_authority() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut old = Store::open(&path).unwrap();
    let task = accept(&mut old, root.path(), "task");
    let import = imported(&mut old);
    let attention = attention(&mut old, &task.task_id);
    let mut import_observation = old.import_snapshot(&import.import_id).unwrap();
    let flow = old.accept_workflow(flow_request()).unwrap();
    let mut flow_observation = old.workflow_snapshot(&flow.workflow_id).unwrap();
    let mut child_request = request("application", "long");
    child_request.snapshot_id = Some("snapshot".into());
    let resolved =
        winbridge_runner::resolve(&config(root.path()), &request("application", "long")).unwrap();
    old.accept_workflow_task(
        &mut flow_observation,
        2,
        child_request,
        resolved,
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
    let mut app_observation = old
        .application_snapshot(flow_observation.application_id.as_ref().unwrap())
        .unwrap();
    let plan = old
        .preview_output_cleanup(&OutputCleanupQuery {
            project_id: None,
            older_than_days: 30,
        })
        .unwrap()
        .plan;
    let mut next = Store::open(&path).unwrap();
    next.begin_authority().unwrap();
    assert!(old
        .accept_workflow(flow_request())
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .save_workflow(&mut flow_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .cancel_workflow(&flow.workflow_id)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .save_application(&mut app_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .stop_application(&app_observation.application_id)
        .unwrap_err()
        .contains("stale"));
    assert!(next
        .save_workflow(&mut flow_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(next
        .save_application(&mut app_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(next
        .save_import(&mut import_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(old.cancel(&task.task_id).unwrap_err().contains("stale"));
    assert!(old
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"late", 100)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .save_import(&mut import_observation)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .cancel_import(&import.import_id)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .accept_import(
            import.submission.clone(),
            "shared".into(),
            "workspace".into()
        )
        .unwrap_err()
        .contains("stale"));
    let command = task.resolved_command.clone();
    assert!(old
        .accept(task.submission.clone(), command, None)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .acknowledge_attention_event(&attention.attention_id)
        .unwrap_err()
        .contains("stale"));
    assert!(old
        .apply_output_cleanup(&ApplyOutputCleanup {
            idempotency_key: "cleanup".into(),
            plan
        })
        .unwrap_err()
        .contains("stale"));
    assert_eq!(next.get(&task.task_id).unwrap().output_bytes, 0);
    assert!(next
        .attention(&attention.attention_id)
        .unwrap()
        .acknowledged_at_ms
        .is_none());
}

#[test]
fn active_attempt_survives_output_and_cancel_revisions_then_commits_actual_exit_once() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let task = accept(&mut store, root.path(), "actual");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    let mut other = Store::open(&path).unwrap();
    other.cancel(&task.task_id).unwrap();
    output(&mut store, &token).unwrap();
    store
        .with_execution(&token, |s| {
            s.transition(
                &task.task_id,
                TaskEvent::ProcessStarted { process_id: 4242 },
            )
        })
        .unwrap();
    let completed = store
        .with_execution(&token, |s| {
            s.finish(&task.task_id, TaskStatus::Succeeded, Some(0), "actual exit")
        })
        .unwrap();
    assert!(completed.cancel_requested);
    assert_eq!(completed.output_bytes, 13);
    let attempt = store
        .execution_attempt(OperationKind::Task, &task.task_id)
        .unwrap()
        .unwrap();
    assert_eq!(attempt.token, token);
    assert_eq!(attempt.outcome, Some(TaskStatus::Succeeded));
    assert!(attempt.completed_at_ms.is_some());
    assert!(output(&mut store, &token).is_err());
    assert!(store
        .with_execution(&token, |s| s.finish(
            &task.task_id,
            TaskStatus::Failed,
            None,
            "late"
        ))
        .is_err());
    assert_eq!(
        store.get(&task.task_id).unwrap().status,
        TaskStatus::Succeeded
    );
}

#[test]
fn old_attempt_cannot_write_pid_output_finish_or_import_progress_after_authority_change() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut old = Store::open(&path).unwrap();
    old.begin_authority().unwrap();
    let task = accept(&mut old, root.path(), "old");
    let (_, task_token) = old.claim_task(&task.task_id).unwrap();
    let import = imported_in(&mut old, "other");
    let (_, import_token) = old.claim_import(&import.import_id).unwrap();
    let mut next = Store::open(&path).unwrap();
    next.begin_authority().unwrap();
    assert!(next
        .with_execution(&task_token, |s| s
            .transition(&task.task_id, TaskEvent::ProcessStarted { process_id: 99 }))
        .is_err());
    assert!(output(&mut old, &task_token).is_err());
    assert!(next
        .with_execution(&task_token, |s| s.finish(
            &task.task_id,
            TaskStatus::Succeeded,
            Some(0),
            "late"
        ))
        .is_err());
    assert!(next
        .with_execution(&import_token, |s| {
            let mut row = s.import_snapshot(&import.import_id)?;
            row.completed_bytes = 123;
            s.save_import(&mut row)
        })
        .is_err());
    assert!(next
        .with_execution(&import_token, |s| s.finish_import(
            &import.import_id,
            TaskStatus::Succeeded,
            "late"
        ))
        .is_err());
    assert_eq!(next.get(&task.task_id).unwrap().process_id, None);
    assert_eq!(next.get(&task.task_id).unwrap().output_bytes, 0);
    assert_eq!(
        next.get_import(&import.import_id).unwrap().completed_bytes,
        0
    );
    next.recover().unwrap();
    assert_eq!(
        next.get(&task.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        next.execution_attempt(OperationKind::Task, &task.task_id)
            .unwrap()
            .unwrap()
            .outcome,
        Some(TaskStatus::Indeterminate)
    );
}

#[test]
fn callback_cannot_retarget_or_nest_another_operation_or_change_administrative_state() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let first = accept(&mut store, root.path(), "first");
    // Separate projects provide independent owners; this test never dispatches.
    let mut other = request("second", "success");
    other.project_id = "other".into();
    let second = store
        .accept(other, first.resolved_command.clone(), None)
        .unwrap();
    let (_, one) = store.claim_task(&first.task_id).unwrap();
    let (_, two) = store.claim_task(&second.task_id).unwrap();
    assert!(store
        .with_execution(&one, |s| s.finish(
            &second.task_id,
            TaskStatus::Failed,
            None,
            "wrong operation"
        ))
        .unwrap_err()
        .contains("another operation"));
    assert!(store
        .with_execution(&one, |s| s
            .with_execution(&two, |s| s.cancel(&second.task_id)))
        .unwrap_err()
        .contains("cannot switch"));
    assert!(store.with_execution(&one, |s| s.begin_authority()).is_err());
    assert!(store
        .with_execution(&one, |s| s.request_attention(&AttentionSubmission {
            request_id: "wrong".into(),
            idempotency_key: "wrong".into(),
            task_id: first.task_id.clone(),
            severity: AttentionSeverity::Warning,
            message: "Review".into()
        }))
        .is_err());
    let mut forged = one.clone();
    forged.operation.project_id = "other".into();
    assert!(output(&mut store, &forged).is_err());
    forged = one.clone();
    forged.attempt_id = two.attempt_id;
    assert!(output(&mut store, &forged).is_err());
    output(&mut store, &one).unwrap();
    assert_eq!(
        store.get(&second.task_id).unwrap().status,
        TaskStatus::Running
    );
}

#[test]
fn execution_scope_is_restored_after_error_and_panic() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    store.begin_authority().unwrap();
    let task = accept(&mut store, root.path(), "scope");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    let failed: Result<(), String> =
        store.with_execution(&token, |_| Err("deliberate callback error".into()));
    assert!(failed.is_err());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), String> =
            store.with_execution(&token, |_| panic!("deliberate callback panic"));
    }));
    assert!(panic.is_err());
    store.cancel(&task.task_id).unwrap();
    attention(&mut store, &task.task_id);
}

#[test]
fn dispatch_and_attempt_failure_roll_back_both_records_before_real_spawn() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path())).unwrap();
    let accepted = task(worker.handle(LocalRequest::Submit {
        request: request("claim-fault", "success"),
    }));
    let db = Connection::open(root.path().join("state/journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_attempt BEFORE INSERT ON execution_attempts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(worker.schedule().unwrap_err().contains("not stored"));
    assert!(worker.is_idle());
    assert!(!root.path().join("runs.log").exists());
    let store = Store::open(&root.path().join("state/journal.sqlite")).unwrap();
    assert_eq!(
        store.get(&accepted.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert!(store
        .execution_attempt(OperationKind::Task, &accepted.task_id)
        .unwrap()
        .is_none());
    db.execute_batch("DROP TRIGGER fail_attempt").unwrap();
    worker.schedule().unwrap();
    wait_idle(&worker);
    assert_eq!(
        task(worker.handle(LocalRequest::GetTask {
            task_id: accepted.task_id.clone()
        }))
        .status,
        TaskStatus::Succeeded
    );
    assert_eq!(
        fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let store = Store::open(&root.path().join("state/journal.sqlite")).unwrap();
    assert_eq!(
        store
            .execution_attempt(OperationKind::Task, &accepted.task_id)
            .unwrap()
            .unwrap()
            .outcome,
        Some(TaskStatus::Succeeded)
    );
}

#[test]
fn failed_import_claim_and_failed_attempt_completion_are_atomic() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let import = imported(&mut store);
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_attempt BEFORE INSERT ON execution_attempts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(store.claim_import(&import.import_id).is_err());
    assert_eq!(
        store.get_import(&import.import_id).unwrap().status,
        TaskStatus::Queued
    );
    db.execute_batch("DROP TRIGGER fail_attempt").unwrap();
    let (_, token) = store.claim_import(&import.import_id).unwrap();
    db.execute_batch("CREATE TRIGGER fail_completion BEFORE UPDATE ON execution_attempts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(store
        .with_execution(&token, |s| s.finish_import(
            &import.import_id,
            TaskStatus::Succeeded,
            "verified"
        ))
        .is_err());
    assert_eq!(
        store.get_import(&import.import_id).unwrap().status,
        TaskStatus::Running
    );
    db.execute_batch("DROP TRIGGER fail_completion").unwrap();
    store
        .with_execution(&token, |s| {
            s.finish_import(&import.import_id, TaskStatus::Succeeded, "verified")
        })
        .unwrap();
    assert!(store
        .with_execution(&token, |s| s.finish_import(
            &import.import_id,
            TaskStatus::Failed,
            "late"
        ))
        .is_err());
}

#[test]
fn task_outcome_attention_and_attempt_completion_share_one_transaction() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let task = accept(&mut store, root.path(), "completion");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_completion BEFORE UPDATE ON execution_attempts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(store
        .with_execution(&token, |s| s.finish(
            &task.task_id,
            TaskStatus::Failed,
            Some(1),
            "actual failure"
        ))
        .is_err());
    assert_eq!(
        store.get(&task.task_id).unwrap().status,
        TaskStatus::Running
    );
    assert_eq!(
        store.list_attention_events(0, false).unwrap().records.len(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_completion").unwrap();
    store
        .with_execution(&token, |s| {
            s.finish(&task.task_id, TaskStatus::Failed, Some(1), "actual failure")
        })
        .unwrap();
    assert_eq!(
        store.list_attention_events(0, false).unwrap().records.len(),
        1
    );
    assert_eq!(
        store
            .execution_attempt(OperationKind::Task, &task.task_id)
            .unwrap()
            .unwrap()
            .outcome,
        Some(TaskStatus::Failed)
    );
}

#[test]
fn attempts_are_retained_and_an_incorrectly_requeued_record_cannot_claim_again() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let task = accept(&mut store, root.path(), "retained");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    store
        .with_execution(&token, |s| {
            s.finish(&task.task_id, TaskStatus::Succeeded, Some(0), "actual exit")
        })
        .unwrap();
    let db = Connection::open(&path).unwrap();
    assert!(db.execute("DELETE FROM execution_attempts", []).is_err());
    assert!(db
        .execute("UPDATE execution_attempts SET attempt='replacement'", [])
        .is_err());
    assert!(db
        .execute(
            "UPDATE execution_attempts SET completed=NULL,outcome=NULL",
            []
        )
        .is_err());
    let mut observed = store.task_snapshot(&task.task_id).unwrap();
    observed.record.status = TaskStatus::Queued;
    observed.record.started_at_ms = None;
    store.save(&observed).unwrap();
    assert!(store.claim_task(&task.task_id).is_err());
    assert_eq!(store.get(&task.task_id).unwrap().status, TaskStatus::Queued);
}

#[test]
fn owner_epoch_overflow_and_ignored_activation_do_not_grant_an_authority() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let initial = store.journal_authority().clone();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_activation BEFORE UPDATE ON journal_authority BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(store.begin_authority().is_err());
    assert_eq!(store.journal_authority(), &initial);
    db.execute_batch("DROP TRIGGER fail_activation; DROP TRIGGER authority_epoch_guard; UPDATE journal_authority SET epoch=9223372036854775807").unwrap();
    let mut current = Store::open(&path).unwrap();
    assert!(current.begin_authority().unwrap_err().contains("exhausted"));
}

#[test]
fn incarnation_change_rejects_equal_revision_observations_and_tokens() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    let task = accept(&mut store, root.path(), "incarnation");
    let (_, token) = store.claim_task(&task.task_id).unwrap();
    let observed = store.task_snapshot(&task.task_id).unwrap();
    let db = Connection::open(&path).unwrap();
    // Controlled fixture only: production restore policy is the remaining A2 package.
    let replacement = uuid::Uuid::new_v4().to_string();
    let guard: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='admission_update_guard'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("BEGIN IMMEDIATE; DROP TRIGGER admission_update_guard;")
        .unwrap();
    db.execute(
        "UPDATE journal_authority SET incarnation=?1,epoch=epoch+1,authority=?2",
        rusqlite::params![replacement, uuid::Uuid::new_v4().to_string()],
    )
    .unwrap();
    // Simulate a coherent foreign header, not a corrupt admission row. The test
    // must still exercise stale observation/attempt guards with equal revisions.
    db.execute(
        "UPDATE journal_admission SET incarnation=?1",
        [&replacement],
    )
    .unwrap();
    db.execute_batch(&guard).unwrap();
    db.execute_batch("COMMIT;").unwrap();
    let mut next = Store::open(&path).unwrap();
    assert_eq!(
        next.task_snapshot(&task.task_id).unwrap().revision(),
        observed.revision()
    );
    assert!(next.save(&observed).unwrap_err().contains("stale"));
    assert!(output(&mut next, &token).is_err());
}

#[test]
fn schema_eight_migration_keeps_legacy_bytes_backup_and_keys_without_fabricating_attempts() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "legacy");
    store
        .transition(&task.task_id, TaskEvent::LaunchIntent)
        .unwrap();
    attention(&mut store, &task.task_id);
    drop(store);
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; DROP TABLE execution_attempts; DROP TABLE journal_authority; PRAGMA user_version=8;",
    )
    .unwrap();
    let raw: (String, String) = db
        .query_row("SELECT record,payload FROM tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    drop(db);
    let mut store = Store::open(&path).unwrap();
    assert!(store
        .execution_attempt(OperationKind::Task, &task.task_id)
        .unwrap()
        .is_none());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT record,payload FROM tasks", [], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?
        )))
        .unwrap(),
        raw
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        JOURNAL_SCHEMA_VERSION
    );
    let backups: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v8-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        backup
            .query_row("SELECT record,payload FROM tasks", [], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            )))
            .unwrap(),
        raw
    );
    store.begin_authority().unwrap();
    store.recover().unwrap();
    assert_eq!(
        accept(&mut store, root.path(), "legacy").task_id,
        task.task_id
    );
    assert_eq!(
        store.get(&task.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    drop(store);
    Store::open(&path).unwrap();
    assert_eq!(
        fs::read_dir(root.path())
            .unwrap()
            .filter(|p| p
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("journal-v8-"))
            .count(),
        1
    );
}

#[test]
fn failed_metadata_migration_rolls_back_tables_and_schema() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    drop(Store::open(&path).unwrap());
    let db = Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; DROP TABLE execution_attempts; DROP TABLE journal_authority; CREATE TABLE execution_attempts(dummy); PRAGMA user_version=8;").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        8
    );
    assert!(db.prepare("SELECT * FROM journal_authority").is_err());
    db.execute_batch("DROP TABLE execution_attempts").unwrap();
    Store::open(&path).unwrap();
}
