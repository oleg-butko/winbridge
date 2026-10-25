//! Durable scheduling decisions use real SQLite. Synthetic commands are never spawned.
use rusqlite::Connection;
use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{
    store::{CleanupProof, LeaseState, OperationKind, Store},
    Worker,
};

fn task(store: &mut Store, project: &str, key: &str) -> TaskRecord {
    store
        .accept(
            TaskSubmission {
                protocol_version: 1,
                request_id: key.into(),
                project_id: project.into(),
                idempotency_key: key.into(),
                timeout_ms: 5000,
                snapshot_id: None,
                command: command(),
            },
            ResolvedCommand {
                executable: "not-dispatched".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 5000,
            },
            None,
        )
        .unwrap()
}
fn command() -> CommandSource {
    CommandSource::Direct {
        executable: "not-dispatched".into(),
        arguments: vec![],
        working_directory: ".".into(),
    }
}
fn import(store: &mut Store, project: &str, key: &str) -> ImportRecord {
    store
        .accept_import(
            ImportSubmission {
                protocol_version: 1,
                request_id: key.into(),
                project_id: project.into(),
                idempotency_key: key.into(),
                snapshot_id: "snapshot".into(),
                manifest_sha256: "a".repeat(64),
                timeout_ms: 5000,
            },
            "shared".into(),
            "workspace".into(),
        )
        .unwrap()
}
fn flow(store: &mut Store, key: &str) -> WorkflowRecord {
    store
        .accept_workflow(WorkflowSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            name: key.into(),
            project_id: "first".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
            build: command(),
            start: command(),
            readiness: ReadinessProbe {
                stdout_contains: "READY".into(),
                timeout_ms: 1000,
                netwatch: None,
            },
            log_path: "logs/app.log".into(),
            artifacts: vec![],
        })
        .unwrap()
}
fn setup() -> (TempDir, Store) {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    store.begin_authority().unwrap();
    (root, store)
}
fn sql(root: &TempDir, script: &str) {
    Connection::open(root.path().join("journal.sqlite"))
        .unwrap()
        .execute_batch(script)
        .unwrap();
}
fn finish(
    store: &mut Store,
    token: &winbridge_worker::store::ExecutionToken,
    proof: CleanupProof,
) -> Result<TaskRecord, String> {
    store.with_execution(token, |s| {
        s.finish_task_execution(token, TaskStatus::Succeeded, Some(0), "done", proof)
    })
}

#[test]
fn concurrent_task_and_import_claims_have_one_project_owner_and_independent_projects_work() {
    let (root, mut store) = setup();
    let first = task(&mut store, "first", "task");
    let second = import(&mut store, "first", "import");
    let barrier = Arc::new(Barrier::new(2));
    let left = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let right = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let b = barrier.clone();
    let id = first.task_id.clone();
    let left = thread::spawn(move || {
        let mut s = left;
        b.wait();
        s.claim_task(&id).map(|(_, t)| t)
    });
    let right = thread::spawn(move || {
        let mut s = right;
        barrier.wait();
        s.claim_import(&second.import_id).map(|(_, t)| t)
    });
    let results = [left.join().unwrap(), right.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .find_map(|r| r.as_ref().err())
        .unwrap()
        .starts_with("resource busy:"));
    assert_eq!(store.resource_leases(true).unwrap().len(), 1);
    assert_eq!(attempt_count(&root), 1);
    let independent = task(&mut store, "second", "independent");
    let (_, token) = store.claim_task(&independent.task_id).unwrap();
    finish(&mut store, &token, CleanupProof::NoProcessCreated).unwrap();
    assert_eq!(store.resource_leases(true).unwrap().len(), 1);
}

#[test]
fn ignored_lease_insert_rolls_back_intent_attempt_and_entire_claim() {
    let (root, mut store) = setup();
    let accepted = task(&mut store, "first", "task");
    let before = store.task_snapshot(&accepted.task_id).unwrap();
    sql(
        &root,
        "CREATE TRIGGER fault BEFORE INSERT ON resource_leases BEGIN SELECT RAISE(IGNORE); END;",
    );
    assert!(store.claim_task(&accepted.task_id).is_err());
    assert_eq!(
        serde_json::to_value(store.get(&accepted.task_id).unwrap()).unwrap(),
        serde_json::to_value(&accepted).unwrap()
    );
    assert_eq!(
        store.task_snapshot(&accepted.task_id).unwrap().revision(),
        before.revision()
    );
    assert!(store.resource_leases(false).unwrap().is_empty());
    assert!(attempt_count(&root) == 0);
    sql(&root, "DROP TRIGGER fault;");
    store.claim_task(&accepted.task_id).unwrap();
}

#[test]
fn terminal_result_without_cleanup_remains_uncertain_and_wait_reason_is_not_spammed() {
    let (root, mut store) = setup();
    let first = task(&mut store, "first", "first");
    let next = task(&mut store, "first", "next");
    let (_, token) = store.claim_task(&first.task_id).unwrap();
    store
        .with_execution(&token, |s| {
            s.finish(
                &first.task_id,
                TaskStatus::Indeterminate,
                None,
                "tree stop unconfirmed",
            )
        })
        .unwrap();
    assert_eq!(
        store.resource_leases(true).unwrap()[0].state,
        LeaseState::Uncertain
    );
    let error = store.claim_task(&next.task_id).unwrap_err();
    assert!(error.contains(&first.task_id));
    store
        .note_resource_wait(OperationKind::Task, &next.task_id, &error)
        .unwrap();
    let before = store.task_snapshot(&next.task_id).unwrap();
    store
        .note_resource_wait(OperationKind::Task, &next.task_id, &error)
        .unwrap();
    let after = store.task_snapshot(&next.task_id).unwrap();
    assert_eq!(
        serde_json::to_value(&after.record).unwrap(),
        serde_json::to_value(&before.record).unwrap()
    );
    assert_eq!(after.revision(), before.revision());
    assert_eq!(after.record.status, TaskStatus::Queued);
    assert!(after.record.process_id.is_none());
    assert_eq!(attempt_count(&root), 1);
    assert!(store
        .with_execution(&token, |s| s
            .record_execution_cleanup(&token, CleanupProof::ProcessTreeEmpty))
        .is_err());
    assert_eq!(
        store.cancel(&next.task_id).unwrap().status,
        TaskStatus::Cancelled
    );
}

#[test]
fn cleanup_and_outcome_release_are_atomic_even_if_lease_release_is_ignored() {
    let (root, mut store) = setup();
    let accepted = task(&mut store, "first", "task");
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let before = store.get(&accepted.task_id).unwrap();
    sql(&root, "CREATE TRIGGER fault BEFORE UPDATE ON resource_leases WHEN NEW.state='released' BEGIN SELECT RAISE(IGNORE); END;");
    assert!(finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).is_err());
    assert_eq!(
        serde_json::to_value(store.get(&accepted.task_id).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    let lease = &store.resource_leases(true).unwrap()[0];
    assert_eq!(lease.state, LeaseState::Held);
    assert_eq!(lease.cleanup_proof, None);
    assert!(store
        .execution_attempt(OperationKind::Task, &accepted.task_id)
        .unwrap()
        .unwrap()
        .completed_at_ms
        .is_none());
    sql(&root, "DROP TRIGGER fault;");
    finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).unwrap();
    assert!(store.resource_leases(true).unwrap().is_empty());
    assert!(store
        .execution_attempt(OperationKind::Task, &accepted.task_id)
        .unwrap()
        .unwrap()
        .completed_at_ms
        .is_some());
    let next = task(&mut store, "first", "next");
    store.claim_task(&next.task_id).unwrap();
}

#[test]
fn durable_cleanup_receipt_survives_failed_outcome_and_restart_without_fabricating_success() {
    let (root, mut store) = setup();
    let first = task(&mut store, "first", "first");
    let next = task(&mut store, "first", "next");
    let (_, token) = store.claim_task(&first.task_id).unwrap();
    store
        .with_execution(&token, |s| {
            s.record_execution_cleanup(&token, CleanupProof::ProcessTreeEmpty)
        })
        .unwrap();
    sql(&root, "CREATE TRIGGER fault BEFORE UPDATE ON tasks WHEN NEW.status='succeeded' BEGIN SELECT RAISE(IGNORE); END;");
    assert!(finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).is_err());
    assert!(store.claim_task(&next.task_id).is_err());
    sql(&root, "DROP TRIGGER fault;");
    let mut new = Store::open(&root.path().join("journal.sqlite")).unwrap();
    new.begin_authority().unwrap();
    new.recover_resource_leases().unwrap();
    new.recover().unwrap();
    assert_eq!(
        new.get(&first.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        new.resource_leases(false).unwrap()[0].state,
        LeaseState::Released
    );
    assert!(new.claim_task(&next.task_id).is_ok());
    assert!(finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).is_err());
}

#[test]
fn new_epoch_recovery_never_frees_a_project_without_a_physical_receipt() {
    let (root, mut store) = setup();
    let first = task(&mut store, "first", "first");
    let next = task(&mut store, "first", "next");
    store.claim_task(&first.task_id).unwrap();
    for _ in 0..2 {
        let mut new = Store::open(&root.path().join("journal.sqlite")).unwrap();
        new.begin_authority().unwrap();
        new.recover_resource_leases().unwrap();
        new.recover().unwrap();
        assert!(new.claim_task(&next.task_id).is_err());
        let leases = new.resource_leases(true).unwrap();
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].state, LeaseState::Uncertain);
        assert_eq!(leases[0].cleanup_proof, None);
    }
    assert_eq!(attempt_count(&root), 1);
}

#[test]
fn proof_kind_owner_and_final_receipts_cannot_be_replaced() {
    let (root, mut store) = setup();
    let first = task(&mut store, "first", "first");
    let (_, token) = store.claim_task(&first.task_id).unwrap();
    assert!(store
        .with_execution(&token, |s| s
            .record_execution_cleanup(&token, CleanupProof::ImportThreadStopped))
        .is_err());
    let mut forged = token.clone();
    forged.operation.project_id = "second".into();
    assert!(finish(&mut store, &forged, CleanupProof::ProcessTreeEmpty).is_err());
    store
        .with_execution(&token, |s| {
            s.record_execution_cleanup(&token, CleanupProof::NoProcessCreated)
        })
        .unwrap();
    assert!(finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).is_err());
    finish(&mut store, &token, CleanupProof::NoProcessCreated).unwrap();
    let connection = Connection::open(root.path().join("journal.sqlite")).unwrap();
    assert!(connection
        .execute(
            "UPDATE resource_leases SET state='held',revision=revision+1",
            []
        )
        .is_err());
    assert!(connection
        .execute("DELETE FROM resource_leases", [])
        .is_err());
    assert_eq!(
        store.resource_leases(false).unwrap()[0].cleanup_proof,
        Some(CleanupProof::NoProcessCreated)
    );
}

#[test]
fn import_completion_releases_only_its_project_and_rejects_task_proofs() {
    let (_root, mut store) = setup();
    let first = import(&mut store, "first", "import");
    let next = task(&mut store, "first", "next");
    let (_, token) = store.claim_import(&first.import_id).unwrap();
    assert!(store.claim_task(&next.task_id).is_err());
    assert!(store
        .with_execution(&token, |s| s
            .record_execution_cleanup(&token, CleanupProof::ProcessTreeEmpty))
        .is_err());
    assert!(store
        .with_execution(&token, |s| s.finish_import_execution(
            &token,
            TaskStatus::Running,
            "not terminal"
        ))
        .is_err());
    assert_eq!(store.resource_leases(true).unwrap()[0].cleanup_proof, None);
    let completed = store
        .with_execution(&token, |s| {
            s.finish_import_execution(&token, TaskStatus::Failed, "copy stopped")
        })
        .unwrap();
    assert!(completed.finished_at_ms.is_some());
    assert_eq!(
        store.resource_leases(false).unwrap()[0].cleanup_proof,
        Some(CleanupProof::ImportThreadStopped)
    );
    store.claim_task(&next.task_id).unwrap();
    let ready = import(&mut store, "second", "verified-import");
    let (_, ready_token) = store.claim_import(&ready.import_id).unwrap();
    store
        .with_execution(&ready_token, |s| {
            let mut record = s.import_snapshot(&ready.import_id)?;
            record.phase = "verified".into();
            record.commit = Some("verified-commit".into());
            s.save_import(&mut record)
        })
        .unwrap();
    let ready = store
        .with_execution(&ready_token, |s| {
            s.finish_import_execution(&ready_token, TaskStatus::Succeeded, "ready for build")
        })
        .unwrap();
    assert_eq!(ready.phase, "verified");
    assert_eq!(ready.commit.as_deref(), Some("verified-commit"));
    assert!(ready.finished_at_ms.is_some());
}

#[test]
fn root_reservation_and_task_claim_race_has_one_winner() {
    let (root, mut store) = setup();
    let accepted = task(&mut store, "first", "task");
    let workflow = flow(&mut store, "flow");
    let mut first = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let mut second = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let mut observed = second.workflow_snapshot(&workflow.workflow_id).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let left = thread::spawn(move || {
        b.wait();
        first.claim_task(&accepted.task_id).map(|_| ())
    });
    let right = thread::spawn(move || {
        barrier.wait();
        observed.status = TaskStatus::Running;
        second.save_workflow(&mut observed)
    });
    let results = [left.join().unwrap(), right.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .find_map(|r| r.as_ref().err())
        .unwrap()
        .starts_with("resource busy:"));
}

#[test]
fn workflow_child_borrows_its_root_reservation_and_keeps_project_after_root_finishes() {
    let (_root, mut store) = setup();
    let workflow = flow(&mut store, "flow");
    let mut root = store.workflow_snapshot(&workflow.workflow_id).unwrap();
    root.status = TaskStatus::Running;
    store.save_workflow(&mut root).unwrap();
    let other_root = flow(&mut store, "other");
    let mut other = store.workflow_snapshot(&other_root.workflow_id).unwrap();
    other.status = TaskStatus::Running;
    assert!(store.save_workflow(&mut other).is_err());
    let unrelated = task(&mut store, "first", "unrelated");
    assert!(store.claim_task(&unrelated.task_id).is_err());
    let request = TaskSubmission {
        protocol_version: 1,
        request_id: "child".into(),
        project_id: "first".into(),
        idempotency_key: "child".into(),
        timeout_ms: 5000,
        snapshot_id: Some("snapshot".into()),
        command: command(),
    };
    store
        .accept_workflow_task(
            &mut root,
            1,
            request,
            ResolvedCommand {
                executable: "not-dispatched".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 5000,
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
    let id = root.steps[1].operation_id.as_ref().unwrap().clone();
    let (_, token) = store.claim_task(&id).unwrap();
    root.status = TaskStatus::Succeeded;
    store.save_workflow(&mut root).unwrap();
    assert!(store.claim_task(&unrelated.task_id).is_err());
    finish(&mut store, &token, CleanupProof::ProcessTreeEmpty).unwrap();
    assert!(store.claim_task(&unrelated.task_id).is_ok());
}

#[test]
fn schema_nine_upgrade_retains_raw_bytes_keys_and_consistent_backup_without_inventing_owners() {
    let (root, mut store) = setup();
    let accepted = task(&mut store, "first", "task");
    let path = root.path().join("journal.sqlite");
    drop(store);
    sql(
        &root,
        "DROP TABLE worker_journal_activations; DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; PRAGMA user_version=9;",
    );
    let raw = |path: &Path| {
        Connection::open(path)
            .unwrap()
            .query_row("SELECT record FROM tasks", [], |r| r.get::<_, String>(0))
            .unwrap()
    };
    let before = raw(&path);
    let mut upgraded = Store::open(&path).unwrap();
    assert!(upgraded.resource_leases(false).unwrap().is_empty());
    assert_eq!(raw(&path), before);
    assert_eq!(
        serde_json::to_value(upgraded.existing(&accepted.submission).unwrap().unwrap()).unwrap(),
        serde_json::to_value(accepted).unwrap()
    );
    upgraded.begin_authority().unwrap();
    let backups: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v9-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(raw(&backups[0]), before);
    let backup = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        9
    );
    Store::open(&path).unwrap();
    assert_eq!(
        fs::read_dir(root.path())
            .unwrap()
            .filter(|r| r
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("journal-v9-"))
            .count(),
        1
    );
}

#[test]
fn failed_schema_ten_upgrade_is_atomic() {
    let (root, store) = setup();
    drop(store);
    sql(&root, "DROP TABLE worker_journal_activations; DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; PRAGMA user_version=9; CREATE TABLE resource_leases(canary TEXT); INSERT INTO resource_leases VALUES('unchanged');");
    assert!(Store::open(&root.path().join("journal.sqlite")).is_err());
    let db = Connection::open(root.path().join("journal.sqlite")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row("SELECT canary FROM resource_leases", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "unchanged"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='resource_project_owner'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
}

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"lease-tests","nodeId":"test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"first","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}

#[test]
fn runtime_recovery_retains_all_legacy_process_identities_without_manufactured_attempts() {
    let root = TempDir::new().unwrap();
    let config = config(root.path());
    fs::create_dir_all(&config.state_directory).unwrap();
    #[cfg(unix)]
    fs::set_permissions(
        &config.state_directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let mut store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let a = task(&mut store, "first", "a");
    let b = task(&mut store, "first", "b");
    let c = task(&mut store, "first", "already-indeterminate");
    store
        .transition(&c.task_id, winbridge_core::task::TaskEvent::LaunchIntent)
        .unwrap();
    store
        .finish(
            &c.task_id,
            TaskStatus::Indeterminate,
            None,
            "Old worker crashed before recording PID",
        )
        .unwrap();
    store
        .transition(&a.task_id, winbridge_core::task::TaskEvent::LaunchIntent)
        .unwrap();
    store
        .transition(
            &a.task_id,
            winbridge_core::task::TaskEvent::ProcessStarted { process_id: 1234 },
        )
        .unwrap();
    store
        .transition(&b.task_id, winbridge_core::task::TaskEvent::LaunchIntent)
        .unwrap();
    store
        .transition(
            &b.task_id,
            winbridge_core::task::TaskEvent::ProcessStarted { process_id: 5678 },
        )
        .unwrap();
    drop(store);
    let worker = Worker::open(config.clone()).unwrap();
    drop(worker);
    let store = Store::open(&config.state_directory.join("journal.sqlite")).unwrap();
    let leases = store.resource_leases(true).unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].state, LeaseState::Uncertain);
    assert!(leases[0].attempt_id.is_none());
    let members: Vec<serde_json::Value> = serde_json::from_str(&leases[0].legacy_members).unwrap();
    assert_eq!(members.len(), 3);
    assert!(members
        .iter()
        .any(|m| m["id"] == c.task_id && m["processId"].is_null()));
    assert!(members
        .iter()
        .any(|m| m["id"] == a.task_id && m["processId"] == 1234));
    assert!(members
        .iter()
        .any(|m| m["id"] == b.task_id && m["processId"] == 5678));
    assert!(store
        .execution_attempt(OperationKind::Task, &a.task_id)
        .unwrap()
        .is_none());
    assert_eq!(
        store.get(&a.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    drop(store);
    drop(Worker::open(config).unwrap());
}

fn attempt_count(root: &TempDir) -> i64 {
    Connection::open(root.path().join("journal.sqlite"))
        .unwrap()
        .query_row("SELECT count(*) FROM execution_attempts", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn two_real_tasks_use_sequential_leases_and_duplicate_requests_do_not_spawn_again() {
    use std::time::{Duration, Instant};
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let request = |key: &str| TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "first".into(),
        idempotency_key: key.into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec!["success".into()],
            working_directory: ".".into(),
        },
    };
    let submit = |key: &str| match worker.handle(LocalRequest::Submit {
        request: request(key),
    }) {
        LocalResponse::Task { task } => *task,
        other => panic!("{other:?}"),
    };
    let first = submit("first");
    let second = submit("second");
    assert_eq!(submit("first").task_id, first.task_id);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
        let a = store.get(&first.task_id).unwrap();
        let b = store.get(&second.task_id).unwrap();
        if a.status.is_terminal() && b.status.is_terminal() && worker.is_idle() {
            assert_eq!(a.status, TaskStatus::Succeeded);
            assert_eq!(b.status, TaskStatus::Succeeded);
            assert_eq!(a.exit_code, Some(0));
            assert_eq!(b.exit_code, Some(0));
            let leases = store.resource_leases(false).unwrap();
            assert_eq!(leases.len(), 2);
            assert!(leases.iter().all(|l| l.state == LeaseState::Released
                && l.cleanup_proof == Some(CleanupProof::ProcessTreeEmpty)));
            assert_ne!(leases[0].attempt_id, leases[1].attempt_id);
            let a = store
                .execution_attempt(OperationKind::Task, &a.task_id)
                .unwrap()
                .unwrap();
            let b = store
                .execution_attempt(OperationKind::Task, &b.task_id)
                .unwrap()
                .unwrap();
            assert!(a.completed_at_ms.unwrap() <= b.started_at_ms);
            break;
        }
        assert!(Instant::now() < deadline, "sequential execution timed out");
        thread::sleep(Duration::from_millis(10));
    }
    worker.stop();
}

#[test]
fn restart_proves_copy_threads_stopped_but_never_uses_that_proof_for_processes() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    // Let Worker create its private state directory and instance lock normally.
    drop(Worker::open(cfg.clone()).unwrap());
    let mut store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    let imported = import(&mut store, "first", "unfinished-copy");
    store.claim_import(&imported.import_id).unwrap();
    drop(store);
    drop(Worker::open(cfg.clone()).unwrap());
    let store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert_eq!(
        store.get_import(&imported.import_id).unwrap().status,
        TaskStatus::Interrupted
    );
    let leases = store.resource_leases(false).unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].state, LeaseState::Released);
    assert_eq!(
        leases[0].cleanup_proof,
        Some(CleanupProof::ImportThreadStopped)
    );
}
