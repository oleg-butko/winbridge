//! Durable process bindings and conservative recovery against real SQLite/Windows.
use rusqlite::Connection;
use std::{fs, path::Path};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_platform::process_identity::ProcessIdentity;
use winbridge_protocol::*;
use winbridge_worker::{
    store::{CleanupProof, ExecutionToken, LeaseState, Store, JOURNAL_SCHEMA_VERSION},
    Worker,
};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"reconciliation-tests","nodeId":"test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"first","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}
fn setup() -> (TempDir, WorkerConfig, Store) {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    drop(Worker::open(cfg.clone()).unwrap());
    let store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    (root, cfg, store)
}
fn task(store: &mut Store, root: &Path, project: &str, key: &str, mode: &str) -> TaskRecord {
    let request = TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: project.into(),
        idempotency_key: key.into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    };
    // Use exactly the runtime policy's resolved Windows paths. Handwritten
    // TempDir paths omit its canonical prefix and look like changed commands.
    let mut cfg = config(root);
    cfg.projects[0].id = project.into();
    let command = winbridge_runner::resolve(&cfg, &request).unwrap();
    store.accept(request, command, None).unwrap()
}

fn identity(token: &ExecutionToken) -> ProcessIdentity {
    ProcessIdentity {
        job_name: Store::execution_job_name(token).unwrap(),
        host_id: "a".repeat(64),
        session_id: 1,
        process_id: 1234,
        creation_time: 42,
    }
}
fn sql(cfg: &WorkerConfig, script: &str) {
    Connection::open(cfg.state_directory.join("journal.sqlite"))
        .unwrap()
        .execute_batch(script)
        .unwrap();
}

#[test]
fn ignored_physical_binding_rolls_back_pid_event_and_lease_together() {
    let (root, cfg, mut store) = setup();
    let accepted = task(&mut store, root.path(), "first", "atomic", "success");
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let before = store.task_snapshot(&accepted.task_id).unwrap();
    sql(&cfg, "CREATE TRIGGER fault BEFORE UPDATE OF physical ON resource_leases BEGIN SELECT RAISE(IGNORE); END;");
    assert!(store
        .bind_task_process(&token, 1234, Some(&identity(&token)))
        .unwrap_err()
        .contains("ignored"));
    let after = store.task_snapshot(&accepted.task_id).unwrap();
    assert_eq!(before.revision(), after.revision());
    assert!(after.record.process_id.is_none());
    assert!(store.resource_leases(true).unwrap()[0].physical.is_none());
    sql(&cfg, "DROP TRIGGER fault;");
    store
        .bind_task_process(&token, 1234, Some(&identity(&token)))
        .unwrap();
    assert_eq!(store.get(&accepted.task_id).unwrap().process_id, Some(1234));
}

#[test]
fn process_binding_rejects_foreign_identity_rebinding_stale_owners_and_false_no_process_proof() {
    let (root, cfg, mut store) = setup();
    let accepted = task(&mut store, root.path(), "first", "identity", "success");
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let mut wrong = identity(&token);
    wrong.job_name.push_str("-foreign");
    assert!(store.bind_task_process(&token, 1234, Some(&wrong)).is_err());
    assert!(store
        .bind_task_process(&token, 5678, Some(&identity(&token)))
        .is_err());
    store
        .bind_task_process(&token, 1234, Some(&identity(&token)))
        .unwrap();
    assert!(store
        .bind_task_process(&token, 1234, Some(&identity(&token)))
        .is_err());
    assert!(store
        .record_execution_cleanup(&token, CleanupProof::NoProcessCreated)
        .is_err());
    let db = Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap();
    assert!(db
        .execute(
            "UPDATE resource_leases SET physical=NULL,revision=revision+1",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "UPDATE resource_leases SET physical='{}',revision=revision+1",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "UPDATE resource_leases SET proof='no_process_created',revision=revision+1",
            []
        )
        .is_err());
    let mut next = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    next.begin_authority().unwrap();
    assert!(store.bind_task_process(&token, 1234, None).is_err());
    assert!(next.bind_task_process(&token, 1234, None).is_err());
}

fn downgrade_to_ten(cfg: &WorkerConfig) {
    sql(cfg, "DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TRIGGER resource_physical_guard; ALTER TABLE resource_leases DROP COLUMN physical; PRAGMA user_version=10;");
}
#[test]
fn schema_ten_upgrade_preserves_raw_records_owners_keys_and_consistent_backup() {
    let (root, cfg, mut store) = setup();
    let accepted = task(&mut store, root.path(), "first", "old-key", "success");
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let db_path = cfg.state_directory.join("journal.sqlite");
    let raw = |db: &Connection| -> (String, String, String) {
        db.query_row("SELECT payload,record,key FROM tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
    };
    let original = raw(&Connection::open(&db_path).unwrap());
    drop(store);
    downgrade_to_ten(&cfg);
    let mut migrated = Store::open(&db_path).unwrap();
    assert_eq!(raw(&Connection::open(&db_path).unwrap()), original);
    assert_eq!(
        migrated
            .execution_attempt(
                winbridge_worker::store::OperationKind::Task,
                &accepted.task_id
            )
            .unwrap()
            .unwrap()
            .token,
        token
    );
    assert!(migrated.resource_leases(true).unwrap()[0]
        .physical
        .is_none());
    assert_eq!(
        task(&mut migrated, root.path(), "first", "old-key", "success").task_id,
        accepted.task_id
    );
    let backups: Vec<_> = fs::read_dir(&cfg.state_directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v10-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        10
    );
    assert_eq!(raw(&backup), original);
    assert_eq!(
        Connection::open(&db_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        JOURNAL_SCHEMA_VERSION
    );
}

#[test]
fn failed_physical_schema_upgrade_rolls_back_column_and_version() {
    let (_root, cfg, store) = setup();
    drop(store);
    downgrade_to_ten(&cfg);
    sql(&cfg, "CREATE TRIGGER resource_physical_guard BEFORE UPDATE ON resource_leases BEGIN SELECT RAISE(ABORT,'canary'); END;");
    assert!(Store::open(&cfg.state_directory.join("journal.sqlite")).is_err());
    let db = Connection::open(cfg.state_directory.join("journal.sqlite")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        10
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('resource_leases') WHERE name='physical'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn startup_never_releases_unbound_attempts_and_info_bounds_the_recovery_inventory() {
    let (root, cfg, mut store) = setup();
    assert!(store.resource_recovery_summary().unwrap().is_none());
    for index in 0..52 {
        let accepted = task(
            &mut store,
            root.path(),
            &format!("project-{index}"),
            &format!("old-{index}"),
            "success",
        );
        store.claim_task(&accepted.task_id).unwrap();
    }
    drop(store);
    let worker = Worker::open(cfg.clone()).unwrap();
    let info = match worker.handle(LocalRequest::Info) {
        LocalResponse::Info { info } => info,
        other => panic!("{other:?}"),
    };
    let recovery = info.resource_recovery.unwrap();
    assert_eq!(recovery.unresolved_count, 52);
    assert_eq!(recovery.resources.len(), 50);
    assert!(recovery.truncated);
    assert!(recovery.resources.iter().all(|r| !r.physical_binding));
    assert!(info.journal_recovery_required.is_none());
    let store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert!(store
        .resource_leases(true)
        .unwrap()
        .iter()
        .all(|l| l.state == LeaseState::Uncertain));
    assert!(!root.path().join("runs.log").exists());
}

#[cfg(windows)]
#[test]
fn windows_process_remains_suspended_until_durable_binding_and_never_runs_after_failed_binding() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use winbridge_platform::process_identity::{observe_process, ProcessObservation};
    use winbridge_platform::ManagedChild;
    let (root, cfg, mut store) = setup();
    let accepted = task(&mut store, root.path(), "first", "suspended", "success");
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let (child, _out, _err) = ManagedChild::prepare(
        &accepted.resolved_command,
        Some(&Store::execution_job_name(&token).unwrap()),
    )
    .unwrap();
    let collision = ManagedChild::prepare(
        &accepted.resolved_command,
        Some(&Store::execution_job_name(&token).unwrap()),
    )
    .err()
    .unwrap();
    assert!(collision.no_process_created);
    thread::sleep(Duration::from_millis(100));
    assert!(!root.path().join("runs.log").exists());
    sql(&cfg, "CREATE TRIGGER fault BEFORE UPDATE OF physical ON resource_leases BEGIN SELECT RAISE(IGNORE); END;");
    assert!(store
        .bind_task_process(&token, child.id(), child.identity())
        .is_err());
    let physical = child.identity().unwrap().clone();
    drop(child);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(observe_process(&physical), ProcessObservation::Empty(_)) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!root.path().join("runs.log").exists());
    sql(&cfg, "DROP TRIGGER fault;");
    // No automatic recreation of this operation: release only after cleanup.
    store
        .record_execution_cleanup(&token, CleanupProof::ProcessTreeEmpty)
        .unwrap();
    store
        .finish_task_execution(
            &token,
            TaskStatus::Indeterminate,
            None,
            "binding failed; tree confirmed stopped",
            CleanupProof::ProcessTreeEmpty,
        )
        .unwrap();
    let next = task(&mut store, root.path(), "first", "explicit-next", "success");
    let (_, token) = store.claim_task(&next.task_id).unwrap();
    let (mut child, _out, _err) = ManagedChild::prepare(
        &next.resolved_command,
        Some(&Store::execution_job_name(&token).unwrap()),
    )
    .unwrap();
    assert!(!root.path().join("runs.log").exists());
    store
        .bind_task_process(&token, child.id(), child.identity())
        .unwrap();
    child.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    drop(child);
    assert_eq!(
        fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[cfg(windows)]
#[test]
fn windows_live_tree_blocks_recovery_then_verified_empty_job_frees_only_new_requests() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use winbridge_platform::{
        process_identity::{observe_process, ProcessObservation},
        ManagedChild,
    };
    let (root, cfg, mut store) = setup();
    let accepted = task(
        &mut store,
        root.path(),
        "first",
        "tree",
        "tree-parent-exits",
    );
    let (_, token) = store.claim_task(&accepted.task_id).unwrap();
    let (mut child, _out, _err) = ManagedChild::prepare(
        &accepted.resolved_command,
        Some(&Store::execution_job_name(&token).unwrap()),
    )
    .unwrap();
    let identity = child.identity().unwrap().clone();
    store
        .bind_task_process(&token, child.id(), Some(&identity))
        .unwrap();
    child.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(root.path().join("runs.log"))
        .unwrap_or_default()
        .lines()
        .count()
        < 2
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    assert!(matches!(
        observe_process(&identity),
        ProcessObservation::Occupied(_)
    ));
    // The root can exit while its descendant still occupies the project.
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(
        observe_process(&identity),
        ProcessObservation::Occupied(_)
    ));
    let mut wrong_host = identity.clone();
    wrong_host.host_id = "0".repeat(64);
    assert!(matches!(
        observe_process(&wrong_host),
        ProcessObservation::Unknown(_)
    ));
    let mut wrong_session = identity.clone();
    wrong_session.session_id = u32::MAX;
    assert!(matches!(
        observe_process(&wrong_session),
        ProcessObservation::Unknown(_)
    ));
    drop(store);
    let worker = Worker::open(cfg.clone()).unwrap();
    let mut store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    assert_eq!(
        store.get(&accepted.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        store.resource_leases(true).unwrap()[0].state,
        LeaseState::Uncertain
    );
    assert!(
        store
            .resource_recovery_summary()
            .unwrap()
            .unwrap()
            .resources[0]
            .physical_binding
    );
    let next = task(&mut store, root.path(), "first", "explicit-next", "success");
    assert!(store
        .claim_task(&next.task_id)
        .unwrap_err()
        .contains("resource busy"));
    drop(child); // Only this test's own tree, never a process opened by PID.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(observe_process(&identity), ProcessObservation::Empty(_)) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    sql(&cfg, "CREATE TRIGGER fault BEFORE UPDATE OF state ON resource_leases WHEN NEW.state='released' BEGIN SELECT RAISE(IGNORE); END;");
    thread::sleep(Duration::from_millis(1100));
    worker.schedule().unwrap();
    let info = match worker.handle(LocalRequest::Info) {
        LocalResponse::Info { info } => info,
        other => panic!("{other:?}"),
    };
    assert!(info.journal_recovery_required.is_some());
    assert_eq!(store.get(&next.task_id).unwrap().status, TaskStatus::Queued);
    assert_eq!(
        store.resource_leases(true).unwrap()[0].state,
        LeaseState::Uncertain
    );
    sql(&cfg, "DROP TRIGGER fault;");
    drop(worker);
    drop(store);
    let worker = Worker::open(cfg.clone()).unwrap();
    let mut store = Store::open(&cfg.state_directory.join("journal.sqlite")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        if store.get(&next.task_id).unwrap().status.is_terminal() && worker.is_idle() {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    let outcome = store.get(&next.task_id).unwrap();
    assert_eq!(outcome.status, TaskStatus::Succeeded, "{outcome:?}");
    assert_eq!(
        store.get(&accepted.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        task(
            &mut store,
            root.path(),
            "first",
            "tree",
            "tree-parent-exits"
        )
        .task_id,
        accepted.task_id
    );
    assert!(store.resource_recovery_summary().unwrap().is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}
