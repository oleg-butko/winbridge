//! Regression foundation for FSM/Store migration. No processes are launched.
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;
use winbridge_protocol::{CommandSource, OutputStream, TaskRecord, TaskStatus, TaskSubmission};
use winbridge_worker::store::Store;

const JOURNAL: &str = include_str!("../../../tests/fixtures/legacy-v1/journal-v1.sql");
const RECORD: &str = include_str!("../../../tests/fixtures/legacy-v1/task-record.json");
const FINGERPRINT: &str = include_str!("../../../tests/fixtures/legacy-v1/task-fingerprint.txt");

fn record() -> TaskRecord {
    serde_json::from_str(RECORD).unwrap()
}

fn historical_journal(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(JOURNAL)
        .unwrap();
}

fn saved_row(path: &Path) -> (String, String) {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT payload,record FROM tasks WHERE key='legacy-task-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn accept(store: &mut Store, key: &str) -> TaskRecord {
    let legacy = record();
    let mut request = legacy.submission;
    request.idempotency_key = key.into();
    store
        .accept(request, legacy.resolved_command, None)
        .unwrap()
}

#[test]
fn historical_schema_migration_keeps_exact_payload_and_record_and_consistent_backup() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    historical_journal(&path);
    let before = saved_row(&path);
    assert_eq!(before.0, FINGERPRINT.trim_end());
    assert_eq!(
        serde_json::from_str::<Value>(&before.1).unwrap(),
        serde_json::from_str::<Value>(RECORD).unwrap()
    );
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.get(&record().task_id).unwrap().status,
        TaskStatus::Running
    );
    assert_eq!(
        saved_row(&path),
        before,
        "migration must not reserialize old dedup payloads"
    );
    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 6);
    drop(connection);
    drop(store);
    let backups: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v1-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(saved_row(&backups[0]), before);
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(backup);
    drop(Store::open(&path).unwrap());
    assert_eq!(saved_row(&path), before);
}

#[test]
fn acceptance_is_committed_before_return_and_lost_ack_retry_keeps_the_original_command() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let accepted = accept(&mut store, "legacy-task-1");
    let (payload, persisted) = saved_row(&path); // An independent reader sees the commit.
    assert_eq!(payload, FINGERPRINT.trim_end());
    let persisted: TaskRecord = serde_json::from_str(&persisted).unwrap();
    assert_eq!(persisted.task_id, accepted.task_id);
    assert_eq!(persisted.status, TaskStatus::Queued);
    assert_eq!(persisted.events.len(), 1);
    drop(store); // Treat the acceptance response as lost.
    let mut reopened = Store::open(&path).unwrap();
    let mut retry = accepted.submission.clone();
    retry.request_id = "retry-after-reconnect".into();
    let mut changed_resolution = accepted.resolved_command.clone();
    changed_resolution.executable = "must-not-replace-the-accepted-command.exe".into();
    let replay = reopened.accept(retry, changed_resolution, None).unwrap();
    assert_eq!(replay.task_id, accepted.task_id);
    assert_eq!(replay.resolved_command, accepted.resolved_command);
    assert_eq!(replay.submission.request_id, accepted.submission.request_id);
    assert_eq!(reopened.all().unwrap().len(), 1);
}

#[test]
fn frozen_legacy_key_survives_migration_and_conflicting_payloads_fail() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    historical_journal(&path);
    let mut store = Store::open(&path).unwrap();
    let legacy = record();
    let mut retry = legacy.submission.clone();
    retry.request_id = "new-correlation".into();
    let accepted = store
        .accept(retry, legacy.resolved_command.clone(), None)
        .unwrap();
    assert_eq!(accepted.task_id, legacy.task_id);
    assert_eq!(accepted.status, TaskStatus::Running);
    let mut changed_timeout = legacy.submission.clone();
    changed_timeout.timeout_ms += 1;
    let mut changed_arguments = legacy.submission.clone();
    let CommandSource::Direct { arguments, .. } = &mut changed_arguments.command else {
        panic!()
    };
    arguments.push("different-effect".into());
    let mut changed_snapshot = legacy.submission.clone();
    changed_snapshot.snapshot_id = Some("different-snapshot".into());
    for request in [changed_timeout, changed_arguments, changed_snapshot] {
        assert!(store
            .accept(request, legacy.resolved_command.clone(), None)
            .unwrap_err()
            .contains("different payload"));
    }
    assert_eq!(store.all().unwrap().len(), 1);
    assert_eq!(saved_row(&path).0, FINGERPRINT.trim_end());
}

#[test]
fn idempotency_keys_are_scoped_to_the_project() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let first = accept(&mut store, "shared-key");
    let mut other_project: TaskSubmission = first.submission.clone();
    other_project.project_id = "other-project".into();
    let second = store
        .accept(other_project, first.resolved_command.clone(), None)
        .unwrap();
    assert_ne!(first.task_id, second.task_id);
    assert_eq!(
        store.existing(&first.submission).unwrap().unwrap().task_id,
        first.task_id
    );
    assert_eq!(
        store.existing(&second.submission).unwrap().unwrap().task_id,
        second.task_id
    );
}

#[test]
fn recovery_keeps_queued_and_terminal_records_and_does_not_repeat_unknown_intents() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    historical_journal(&path); // A frozen running intent without a PID.
    let mut store = Store::open(&path).unwrap();
    let mut with_pid = accept(&mut store, "running-with-pid");
    with_pid.status = TaskStatus::Running;
    with_pid.process_id = Some(4242); // Synthetic ID; never used to signal a process.
    Store::event(&mut with_pid, "Synthetic process observation");
    store.save(&with_pid).unwrap();
    let queued = accept(&mut store, "still-queued");
    let terminal = accept(&mut store, "already-finished");
    let terminal = store
        .finish(&terminal.task_id, TaskStatus::Succeeded, Some(0), "Done")
        .unwrap();
    let expected_terminal = serde_json::to_value(&terminal).unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    store.recover().unwrap();
    for id in [&record().task_id, &with_pid.task_id] {
        let recovered = store.get(id).unwrap();
        assert_eq!(recovered.status, TaskStatus::Indeterminate);
        assert!(recovered
            .reason
            .as_ref()
            .unwrap()
            .contains("Automatic retry is disabled"));
        assert_eq!(
            recovered
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
    assert_eq!(
        store.get(&queued.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert_eq!(
        serde_json::to_value(store.get(&terminal.task_id).unwrap()).unwrap(),
        expected_terminal
    );
    let before: Vec<Value> = store
        .all()
        .unwrap()
        .into_iter()
        .map(|task| serde_json::to_value(task).unwrap())
        .collect();
    assert_eq!(store.list_attention(0, false).unwrap().records.len(), 2);
    store.recover().unwrap();
    assert_eq!(
        store
            .all()
            .unwrap()
            .into_iter()
            .map(|task| serde_json::to_value(task).unwrap())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(store.list_attention(0, false).unwrap().records.len(), 2);
    assert_eq!(
        store
            .existing(&record().submission)
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Indeterminate
    );
    assert_eq!(store.all().unwrap().len(), 4);
}

#[test]
fn cancellation_is_an_intent_until_completion_and_terminal_result_is_stable() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    historical_journal(&path);
    let mut store = Store::open(&path).unwrap();
    let queued = accept(&mut store, "cancel-before-launch");
    assert_eq!(
        store.cancel(&queued.task_id).unwrap().status,
        TaskStatus::Cancelled
    );
    let id = record().task_id;
    let intent = store.cancel(&id).unwrap();
    assert_eq!(intent.status, TaskStatus::Running);
    assert!(intent.cancel_requested);
    assert!(intent.finished_at_ms.is_none());
    assert_eq!(intent.events.len(), 3);
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        store.cancel(&id).unwrap().events.len(),
        3,
        "duplicate cancel adds no event"
    );
    let finished = store
        .finish(&id, TaskStatus::Cancelled, None, "Managed tree stopped")
        .unwrap();
    let expected = serde_json::to_value(finished).unwrap();
    assert_eq!(
        serde_json::to_value(
            store
                .finish(&id, TaskStatus::Succeeded, Some(0), "Late exit")
                .unwrap()
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        serde_json::to_value(store.cancel(&id).unwrap()).unwrap(),
        expected
    );
}

#[test]
fn output_sequence_is_per_task_across_streams_and_cursor_survives_reopen() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, "output-cursor");
    store
        .append_output(
            &task.task_id,
            OutputStream::Stdout,
            0,
            "Привет".as_bytes(),
            1024,
        )
        .unwrap();
    store
        .append_output(&task.task_id, OutputStream::Stderr, 0, b"diagnostic", 1024)
        .unwrap();
    let other = accept(&mut store, "other-output");
    store
        .append_output(&other.task_id, OutputStream::Stdout, 0, b"other", 1024)
        .unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let page = store.output(&task.task_id, 0).unwrap();
    assert_eq!(
        page.iter().map(|chunk| chunk.sequence).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(matches!(page[0].stream, OutputStream::Stdout));
    assert!(matches!(page[1].stream, OutputStream::Stderr));
    assert_eq!(page[0].bytes, "Привет".as_bytes());
    assert_eq!(store.output(&other.task_id, 0).unwrap()[0].sequence, 1);
    store
        .append_output(
            &task.task_id,
            OutputStream::Stdout,
            "Привет".len() as u64,
            b" tail",
            1024,
        )
        .unwrap();
    let resumed = store.output(&task.task_id, 2).unwrap();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].sequence, 3);
    assert_eq!(resumed[0].offset, "Привет".len() as u64);
    assert_eq!(resumed[0].bytes, b" tail");
    assert_eq!(
        store.get(&task.task_id).unwrap().events.len(),
        1,
        "output and task-event sequences have different scopes"
    );
}

#[test]
fn incompatible_or_unversioned_journal_is_refused_without_changing_legacy_rows() {
    for version in [0, 7] {
        let root = TempDir::new().unwrap();
        let path = root.path().join("journal.sqlite");
        historical_journal(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        drop(connection);
        let before = saved_row(&path);
        let error = Store::open(&path)
            .err()
            .expect("must refuse an incompatible journal");
        assert!(error.contains(if version == 0 {
            "unversioned existing journal"
        } else {
            "unsupported"
        }));
        assert_eq!(saved_row(&path), before);
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            version
        );
        assert_eq!(connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get::<_, u32>(0)).unwrap(), 2);
    }
}
