//! SQLite decisions only: fake fixture commands/PIDs are never dispatched.
use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;
use winbridge_core::task::TaskEvent;
use winbridge_protocol::*;
use winbridge_worker::store::{JournalSnapshot, Store, JOURNAL_SCHEMA_VERSION};

fn command() -> CommandSource {
    CommandSource::Direct {
        executable: "not-dispatched-fixture".into(),
        arguments: vec![],
        working_directory: ".".into(),
    }
}
fn workflow(key: &str) -> WorkflowSubmission {
    WorkflowSubmission {
        protocol_version: 1,
        request_id: key.into(),
        idempotency_key: key.into(),
        name: "journal test".into(),
        project_id: "fixture".into(),
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
    }
}
fn import(key: &str) -> ImportSubmission {
    ImportSubmission {
        protocol_version: 1,
        request_id: key.into(),
        idempotency_key: key.into(),
        project_id: "fixture".into(),
        snapshot_id: "snapshot".into(),
        manifest_sha256: "a".repeat(64),
        timeout_ms: 5000,
    }
}
fn child(flow: &WorkflowRecord, index: usize) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: flow.steps[index].step_id.clone(),
        idempotency_key: format!(
            "workflow:{}:{}",
            flow.workflow_id, flow.steps[index].step_id
        ),
        project_id: "fixture".into(),
        snapshot_id: Some("snapshot".into()),
        timeout_ms: 5000,
        command: command(),
    }
}
fn admit_child(
    store: &mut Store,
    flow: &mut JournalSnapshot<WorkflowRecord>,
    index: usize,
) -> Result<(), String> {
    let request = child(flow, index);
    store.accept_workflow_task(
        flow,
        index,
        request,
        ResolvedCommand {
            executable: "not-dispatched-fixture".into(),
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
}
fn seed_flow(store: &mut Store) -> JournalSnapshot<WorkflowRecord> {
    let flow = store.accept_workflow(workflow("flow")).unwrap();
    let mut observed = store.workflow_snapshot(&flow.workflow_id).unwrap();
    observed.status = TaskStatus::Running;
    store.save_workflow(&mut observed).unwrap();
    observed
}
fn seed_app(
    store: &mut Store,
) -> (
    JournalSnapshot<WorkflowRecord>,
    JournalSnapshot<ApplicationRecord>,
) {
    let mut flow = seed_flow(store);
    admit_child(store, &mut flow, 2).unwrap();
    let app = store
        .application_snapshot(flow.application_id.as_ref().unwrap())
        .unwrap();
    store
        .transition(&app.task_id, TaskEvent::LaunchIntent)
        .unwrap();
    (flow, app)
}
fn attention_request(store: &mut Store, task: &str, key: &str) -> AttentionRecord {
    store
        .request_attention(&AttentionSubmission {
            request_id: key.into(),
            idempotency_key: key.into(),
            task_id: task.into(),
            severity: AttentionSeverity::Warning,
            message: "Review".into(),
        })
        .unwrap()
}
fn value<T: serde::Serialize>(record: &T) -> Value {
    serde_json::to_value(record).unwrap()
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn stale_import_progress_cannot_erase_cancel_or_terminal_outcome() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut other = Store::open(&path).unwrap();
    let accepted = store
        .accept_import(import("import"), "shared".into(), "workspace".into())
        .unwrap();
    let mut running = store.import_snapshot(&accepted.import_id).unwrap();
    running.status = TaskStatus::Running;
    store.save_import(&mut running).unwrap();
    let mut stale = other.import_snapshot(&accepted.import_id).unwrap();
    store.cancel_import(&accepted.import_id).unwrap();
    stale.completed_bytes = 100;
    assert!(other.save_import(&mut stale).unwrap_err().contains("stale"));
    assert!(
        store
            .get_import(&accepted.import_id)
            .unwrap()
            .cancel_requested
    );
    let mut pending = other.import_snapshot(&accepted.import_id).unwrap();
    let terminal = store
        .finish_import(
            &accepted.import_id,
            TaskStatus::Succeeded,
            "actually verified",
        )
        .unwrap();
    pending.phase = "copy".into();
    assert!(other.save_import(&mut pending).is_err());
    assert_eq!(
        value(&store.get_import(&accepted.import_id).unwrap()),
        value(&terminal)
    );
    let mut terminal = store.import_snapshot(&accepted.import_id).unwrap();
    terminal.status = TaskStatus::Running;
    assert!(store
        .save_import(&mut terminal)
        .unwrap_err()
        .contains("terminal"));
}

#[test]
fn stale_workflow_steps_cannot_erase_cancel_and_aba_is_rejected() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut flow = seed_flow(&mut store);
    let mut other = Store::open(&path).unwrap();
    let mut stale = other.workflow_snapshot(&flow.workflow_id).unwrap();
    store.cancel_workflow(&flow.workflow_id).unwrap();
    stale.steps[0].status = WorkflowStepStatus::Succeeded;
    assert!(other
        .save_workflow(&mut stale)
        .unwrap_err()
        .contains("stale"));
    assert!(store.workflow(&flow.workflow_id).unwrap().cancel_requested);
    flow = store.workflow_snapshot(&flow.workflow_id).unwrap();
    let mut aba = flow.clone();
    let original = flow.record.clone();
    flow.reason = Some("temporary".into());
    store.save_workflow(&mut flow).unwrap();
    flow.record = original;
    store.save_workflow(&mut flow).unwrap();
    assert!(other.save_workflow(&mut aba).unwrap_err().contains("stale"));
}

#[test]
fn stale_application_readiness_cannot_erase_stop_and_terminal_is_immutable() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (_, mut stale) = seed_app(&mut store);
    let mut other = Store::open(&path).unwrap();
    other.stop_application(&stale.application_id).unwrap();
    stale.status = ApplicationStatus::Ready;
    assert!(store
        .save_application(&mut stale)
        .unwrap_err()
        .contains("stale"));
    assert_eq!(
        store.application(&stale.application_id).unwrap().status,
        ApplicationStatus::Stopping
    );
    assert!(store.get(&stale.task_id).unwrap().cancel_requested);
    let mut current = store.application_snapshot(&stale.application_id).unwrap();
    current.status = ApplicationStatus::Stopped;
    store.save_application(&mut current).unwrap();
    current.status = ApplicationStatus::Ready;
    assert!(store
        .save_application(&mut current)
        .unwrap_err()
        .contains("terminal"));
}

#[test]
fn child_import_link_failure_rolls_back_admission_and_preserves_observation() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut flow = seed_flow(&mut store);
    let old = value(&flow.record);
    let rev = flow.revision();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_link BEFORE UPDATE ON workflows BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .accept_workflow_import(
            &mut flow,
            import("child"),
            "shared".into(),
            "workspace".into()
        )
        .is_err());
    assert_eq!(count(&db, "imports"), 0);
    assert_eq!(value(&flow.record), old);
    assert_eq!(flow.revision(), rev);
    db.execute_batch("DROP TRIGGER fail_link").unwrap();
    store
        .accept_workflow_import(
            &mut flow,
            import("child"),
            "shared".into(),
            "workspace".into(),
        )
        .unwrap();
    assert_eq!(count(&db, "imports"), 1);
    assert!(flow.steps[0].operation_id.is_some());
}

#[test]
fn child_start_link_or_registry_failure_leaves_no_task_or_application() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut flow = seed_flow(&mut store);
    let old = value(&flow.record);
    let db = Connection::open(&path).unwrap();
    for fault in [
        "CREATE TRIGGER fail_write BEFORE UPDATE ON workflows BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail_write BEFORE INSERT ON applications BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        db.execute_batch(fault).unwrap();
        assert!(admit_child(&mut store, &mut flow, 2).is_err());
        assert_eq!(count(&db, "tasks"), 0);
        assert_eq!(count(&db, "applications"), 0);
        assert_eq!(value(&flow.record), old);
        db.execute_batch("DROP TRIGGER fail_write").unwrap();
    }
    admit_child(&mut store, &mut flow, 2).unwrap();
    assert_eq!(count(&db, "tasks"), 1);
    assert_eq!(count(&db, "applications"), 1);
    let committed = value(&flow.record);
    assert!(admit_child(&mut store, &mut flow, 2).is_err());
    assert_eq!(value(&flow.record), committed);
}

#[test]
fn stale_parent_rolls_back_new_child_without_losing_cancel() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut flow = seed_flow(&mut store);
    let mut other = Store::open(&path).unwrap();
    other.cancel_workflow(&flow.workflow_id).unwrap();
    assert!(admit_child(&mut store, &mut flow, 1).is_err());
    assert!(store.all().unwrap().is_empty());
    assert!(store.workflow(&flow.workflow_id).unwrap().cancel_requested);
}

#[test]
fn stop_failure_rolls_back_task_cancel_and_success_survives_reopen() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (_, app) = seed_app(&mut store);
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_stop BEFORE UPDATE ON applications BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store.stop_application(&app.application_id).is_err());
    assert!(!store.get(&app.task_id).unwrap().cancel_requested);
    assert_eq!(
        store
            .application_snapshot(&app.application_id)
            .unwrap()
            .revision(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_stop").unwrap();
    store.stop_application(&app.application_id).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.get(&app.task_id).unwrap().cancel_requested);
    assert_eq!(
        store
            .application_snapshot(&app.application_id)
            .unwrap()
            .revision(),
        1
    );
}

#[test]
fn readiness_pair_and_attention_outcome_are_atomic() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (mut flow, mut app) = seed_app(&mut store);
    let db = Connection::open(&path).unwrap();
    app.status = ApplicationStatus::Ready;
    app.ready_at_ms = Some(1);
    flow.steps[3].status = WorkflowStepStatus::Succeeded;
    db.execute_batch(
        "CREATE TRIGGER fail_pair BEFORE UPDATE ON workflows BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .save_workflow_application(&mut flow, &mut app, false)
        .is_err());
    assert_eq!(
        store.application(&app.application_id).unwrap().status,
        ApplicationStatus::Starting
    );
    assert_eq!(
        store.workflow(&flow.workflow_id).unwrap().steps[3].status,
        WorkflowStepStatus::Pending
    );
    db.execute_batch("DROP TRIGGER fail_pair").unwrap();
    store
        .save_workflow_application(&mut flow, &mut app, false)
        .unwrap();
    flow.status = TaskStatus::Failed;
    flow.reason = Some("confirmed startup failure".into());
    db.execute_batch(
        "CREATE TRIGGER fail_attention BEFORE INSERT ON attention BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store.save_workflow(&mut flow).is_err());
    assert_eq!(
        store.workflow(&flow.workflow_id).unwrap().status,
        TaskStatus::Running
    );
    db.execute_batch("DROP TRIGGER fail_attention").unwrap();
    store.save_workflow(&mut flow).unwrap();
    assert_eq!(count(&db, "attention"), 1);
}

#[test]
fn explicit_attention_ack_is_checked_monotone_and_idempotent() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (_, app) = seed_app(&mut store);
    let event = attention_request(&mut store, &app.task_id, "review");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_ack BEFORE UPDATE ON attention BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .acknowledge_attention_event(&event.attention_id)
        .is_err());
    assert!(store
        .attention_event(&event.attention_id)
        .unwrap()
        .acknowledged_at_ms
        .is_none());
    db.execute_batch("DROP TRIGGER fail_ack").unwrap();
    let ack = store
        .acknowledge_attention_event_at(&event.attention_id, 0)
        .unwrap();
    assert!(store
        .acknowledge_attention_event_at(&event.attention_id, 0)
        .unwrap_err()
        .contains("stale"));
    assert_eq!(
        value(
            &store
                .acknowledge_attention_event(&event.attention_id)
                .unwrap()
        ),
        value(&ack)
    );
    let rev: i64 = db
        .query_row(
            "SELECT revision FROM attention WHERE id=?1",
            [&event.attention_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rev, 1);
    assert_eq!(
        attention_request(&mut store, &app.task_id, "review").attention_id,
        event.attention_id
    );
}

#[test]
fn ignored_domain_acceptance_is_never_acknowledged() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    for table in ["imports", "workflows"] {
        db.execute_batch(&format!(
            "CREATE TRIGGER fail_accept BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END;"
        ))
        .unwrap();
        assert!(if table == "imports" {
            store
                .accept_import(import("lost"), "shared".into(), "workspace".into())
                .map(|_| ())
        } else {
            store.accept_workflow(workflow("lost")).map(|_| ())
        }
        .is_err());
        assert_eq!(count(&db, table), 0);
        db.execute_batch("DROP TRIGGER fail_accept").unwrap();
    }
    let (_, app) = seed_app(&mut store);
    db.execute_batch(
        "CREATE TRIGGER fail_accept BEFORE INSERT ON attention BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    let request = AttentionSubmission {
        request_id: "lost".into(),
        idempotency_key: "lost".into(),
        task_id: app.task_id.clone(),
        severity: AttentionSeverity::Warning,
        message: "Review".into(),
    };
    assert!(store
        .request_attention(&request)
        .unwrap_err()
        .contains("not stored"));
    assert_eq!(count(&db, "attention"), 0);
}

#[test]
fn identities_and_sql_guards_reject_unversioned_writers() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (mut flow, mut app) = seed_app(&mut store);
    flow.steps[0].step_id = "different".into();
    assert!(store
        .save_workflow(&mut flow)
        .unwrap_err()
        .contains("identity"));
    app.task_id = "different".into();
    assert!(store
        .save_application(&mut app)
        .unwrap_err()
        .contains("identity"));
    let imported = store
        .accept_import(import("immutable"), "shared".into(), "workspace".into())
        .unwrap();
    let mut imported = store.import_snapshot(&imported.import_id).unwrap();
    imported.workspace_path = "elsewhere".into();
    assert!(store
        .save_import(&mut imported)
        .unwrap_err()
        .contains("identity"));
    let db = Connection::open(&path).unwrap();
    for table in ["imports", "workflows", "applications"] {
        assert!(db
            .execute(&format!("UPDATE {table} SET record=record"), [])
            .is_err());
    }
}

#[test]
fn schema_seven_migration_preserves_all_records_and_creates_exact_backup() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (flow, app) = seed_app(&mut store);
    let event = attention_request(&mut store, &app.task_id, "migration");
    store.acknowledge_attention(&event.attention_id).unwrap();
    let imported = store
        .accept_import(import("migration"), "shared".into(), "workspace".into())
        .unwrap();
    drop(store);
    let db = Connection::open(&path).unwrap();
    for table in ["imports", "workflows", "applications", "attention"] {
        db.execute_batch(&format!(
            "DROP TRIGGER {table}_revision_guard; ALTER TABLE {table} DROP COLUMN revision;"
        ))
        .unwrap();
    }
    db.pragma_update(None, "user_version", 7).unwrap();
    db.execute(
        "UPDATE sqlite_sequence SET seq=5000 WHERE name='attention'",
        [],
    )
    .unwrap();
    let payloads: Vec<_> = ["imports", "workflows", "attention"]
        .into_iter()
        .map(|table| {
            (
                table,
                db.query_row(&format!("SELECT payload FROM {table}"), [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            )
        })
        .collect();
    let raw: Vec<(String, String)> = ["imports", "workflows", "applications"]
        .into_iter()
        .map(|table| {
            (
                table.into(),
                db.query_row(&format!("SELECT record FROM {table}"), [], |r| r.get(0))
                    .unwrap(),
            )
        })
        .collect();
    drop(db);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        value(&store.workflow(&flow.workflow_id).unwrap()),
        value(&flow.record)
    );
    assert_eq!(
        value(&store.application(&app.application_id).unwrap()),
        value(&app.record)
    );
    assert_eq!(
        value(&store.get_import(&imported.import_id).unwrap()),
        value(&imported)
    );
    assert!(store
        .attention(&event.attention_id)
        .unwrap()
        .acknowledged_at_ms
        .is_some());
    assert_eq!(
        store
            .workflow_snapshot(&flow.workflow_id)
            .unwrap()
            .revision(),
        0
    );
    assert_eq!(
        store
            .application_snapshot(&app.application_id)
            .unwrap()
            .revision(),
        0
    );
    let db = Connection::open(&path).unwrap();
    let version: u32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, JOURNAL_SCHEMA_VERSION);
    let backups: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v7-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
    for (table, original) in payloads {
        assert_eq!(
            db.query_row(&format!("SELECT payload FROM {table}"), [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            original
        );
        assert_eq!(
            backup
                .query_row(&format!("SELECT payload FROM {table}"), [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            original
        );
    }
    let new_event = attention_request(&mut store, &app.task_id, "after-migration");
    assert!(new_event.sequence > 5000);
    for (table, original) in raw {
        assert_eq!(
            db.query_row(&format!("SELECT record FROM {table}"), [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            original
        );
        assert_eq!(
            backup
                .query_row(&format!("SELECT record FROM {table}"), [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            original
        );
    }
    assert_eq!(
        attention_request(&mut store, &app.task_id, "migration").attention_id,
        event.attention_id
    );
    drop(store);
    Store::open(&path).unwrap();
    assert_eq!(
        std::fs::read_dir(root.path())
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("journal-v7-"))
            .count(),
        1
    );
}

#[test]
fn domain_migration_failure_rolls_back_every_added_column_and_schema() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    drop(Store::open(&path).unwrap());
    let db = Connection::open(&path).unwrap();
    for table in ["imports", "workflows", "applications", "attention"] {
        db.execute_batch(&format!(
            "DROP TRIGGER {table}_revision_guard; ALTER TABLE {table} DROP COLUMN revision;"
        ))
        .unwrap();
    }
    db.pragma_update(None, "user_version", 7).unwrap();
    db.execute_batch(
        "CREATE TRIGGER imports_revision_guard BEFORE UPDATE ON imports BEGIN SELECT 1; END;",
    )
    .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
    for table in ["imports", "workflows", "applications", "attention"] {
        assert!(db
            .prepare(&format!("SELECT revision FROM {table}"))
            .is_err());
    }
    db.execute_batch("DROP TRIGGER imports_revision_guard")
        .unwrap();
    Store::open(&path).unwrap();
}

#[test]
fn simultaneous_import_workflow_and_attention_retries_commit_one_identity_each() {
    use std::sync::{Arc, Barrier};
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (_, app) = seed_app(&mut store);
    for domain in ["imports", "workflows", "attention"] {
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for index in 0..3 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            let task_id = app.task_id.clone();
            threads.push(std::thread::spawn(move || {
                let mut store = Store::open(&path).unwrap();
                barrier.wait();
                match domain {
                    "imports" => {
                        let mut request = import("simultaneous");
                        request.request_id = index.to_string();
                        store
                            .accept_import(request, "shared".into(), "workspace".into())
                            .unwrap()
                            .import_id
                    }
                    "workflows" => {
                        let mut request = workflow("simultaneous");
                        request.request_id = index.to_string();
                        store.accept_workflow(request).unwrap().workflow_id
                    }
                    _ => {
                        store
                            .request_attention(&AttentionSubmission {
                                request_id: index.to_string(),
                                idempotency_key: "simultaneous".into(),
                                task_id,
                                severity: AttentionSeverity::Warning,
                                message: "Review".into(),
                            })
                            .unwrap()
                            .attention_id
                    }
                }
            }));
        }
        let ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(ids.iter().all(|id| id == &ids[0]));
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row(
                &format!("SELECT count(*) FROM {domain} WHERE key='simultaneous'"),
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    let mut different_import = import("simultaneous");
    different_import.snapshot_id = "different".into();
    assert!(store
        .accept_import(different_import, "shared".into(), "workspace".into())
        .unwrap_err()
        .contains("different payload"));
    let mut different_workflow = workflow("simultaneous");
    different_workflow.name = "different".into();
    assert!(store
        .accept_workflow(different_workflow)
        .unwrap_err()
        .contains("different payload"));
    let different_attention = AttentionSubmission {
        request_id: "changed".into(),
        idempotency_key: "simultaneous".into(),
        task_id: app.task_id.clone(),
        severity: AttentionSeverity::Error,
        message: "Review".into(),
    };
    assert!(store
        .request_attention(&different_attention)
        .unwrap_err()
        .contains("different payload"));
}

#[test]
fn exhausted_domain_revisions_cannot_mutate_records_or_acknowledge_attention() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (flow, app) = seed_app(&mut store);
    let imported = store
        .accept_import(import("overflow"), "shared".into(), "workspace".into())
        .unwrap();
    let attention = attention_request(&mut store, &app.task_id, "overflow");
    let db = Connection::open(&path).unwrap();
    for table in ["imports", "workflows", "applications", "attention"] {
        db.execute_batch(&format!(
            "DROP TRIGGER {table}_revision_guard; UPDATE {table} SET revision=9223372036854775807;"
        ))
        .unwrap();
    }
    let mut observed_import = store.import_snapshot(&imported.import_id).unwrap();
    observed_import.reason = Some("overflow".into());
    assert!(store
        .save_import(&mut observed_import)
        .unwrap_err()
        .contains("exhausted"));
    let mut observed_flow = store.workflow_snapshot(&flow.workflow_id).unwrap();
    observed_flow.reason = Some("overflow".into());
    assert!(store
        .save_workflow(&mut observed_flow)
        .unwrap_err()
        .contains("exhausted"));
    let mut observed_app = store.application_snapshot(&app.application_id).unwrap();
    observed_app.reason = Some("overflow".into());
    assert!(store
        .save_application(&mut observed_app)
        .unwrap_err()
        .contains("exhausted"));
    assert!(store
        .acknowledge_attention_event(&attention.attention_id)
        .unwrap_err()
        .contains("exhausted"));
    assert!(store
        .attention(&attention.attention_id)
        .unwrap()
        .acknowledged_at_ms
        .is_none());
    assert_eq!(
        value(&store.get_import(&imported.import_id).unwrap()),
        value(&imported)
    );
    assert_eq!(
        value(&store.workflow(&flow.workflow_id).unwrap()),
        value(&flow.record)
    );
    assert_eq!(
        value(&store.application(&app.application_id).unwrap()),
        value(&app.record)
    );
}

#[test]
fn legacy_orphan_application_is_relinked_without_resetting_recovered_outcome() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let (mut flow, mut app) = seed_app(&mut store);
    store.recover().unwrap();
    app.status = ApplicationStatus::Indeterminate;
    app.reason = Some("legacy crash; no replay".into());
    store.save_application(&mut app).unwrap();
    // Old schema could commit the task/registry before linking the parent.
    flow.steps[2].operation_id = None;
    flow.application_id = None;
    store.save_workflow(&mut flow).unwrap();
    admit_child(&mut store, &mut flow, 2).unwrap();
    assert_eq!(
        flow.application_id.as_deref(),
        Some(app.application_id.as_str())
    );
    assert_eq!(
        value(&store.application(&app.application_id).unwrap()),
        value(&app.record)
    );
    assert_eq!(
        store.get(&app.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(store.all().unwrap().len(), 1);
}
