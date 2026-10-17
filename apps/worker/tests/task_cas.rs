//! Durable task revisions: real SQLite connections, faults and legacy migration.
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::{fs, path::Path};
use tempfile::TempDir;
use winbridge_core::{config::WorkerConfig, task::TaskEvent};
use winbridge_protocol::{OutputStream, TaskRecord, TaskStatus, TaskSubmission};
use winbridge_worker::store::{now_ms, Store, JOURNAL_SCHEMA_VERSION};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(json!({
        "schemaVersion":1,"instanceId":"task-cas-tests","nodeId":"test",
        "stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap()
}
fn accept(store: &mut Store, root: &Path, key: &str) -> TaskRecord {
    let request: TaskSubmission = serde_json::from_value(json!({
        "protocolVersion":1,"requestId":key,"projectId":"fixture","idempotencyKey":key,
        "timeoutMs":5000,"command":{"kind":"direct","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":["success"],"workingDirectory":"."}
    })).unwrap();
    let command = winbridge_runner::resolve(&config(root), &request).unwrap();
    store.accept(request, command, None).unwrap()
}
fn value(record: &TaskRecord) -> serde_json::Value {
    serde_json::to_value(record).unwrap()
}
fn seed_old_output(store: &mut Store, root: &Path) -> String {
    let task = accept(store, root, "old-output");
    store
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"kept output", 4096)
        .unwrap();
    store
        .finish(&task.task_id, TaskStatus::Succeeded, Some(0), "confirmed")
        .unwrap();
    let mut observed = store.task_snapshot(&task.task_id).unwrap();
    observed.record.finished_at_ms = Some(now_ms() - 40 * 86_400_000);
    store.save(&observed).unwrap();
    task.task_id
}
fn cleanup(store: &Store) -> winbridge_protocol::ApplyOutputCleanup {
    let plan = store
        .preview_output_cleanup(&winbridge_protocol::OutputCleanupQuery {
            project_id: None,
            older_than_days: 30,
        })
        .unwrap()
        .plan;
    winbridge_protocol::ApplyOutputCleanup {
        idempotency_key: "cleanup-cas".into(),
        plan,
    }
}

#[test]
fn stale_connection_cannot_erase_cancel_or_output_and_current_exit_still_commits() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "cancel-exit");
    store
        .transition(&task.task_id, TaskEvent::LaunchIntent)
        .unwrap();
    store
        .transition(
            &task.task_id,
            TaskEvent::ProcessStarted { process_id: 4242 },
        )
        .unwrap();
    let mut other = Store::open(&path).unwrap();
    let mut stale = other.task_snapshot(&task.task_id).unwrap();
    assert_eq!(stale.revision(), 2);
    store.cancel(&task.task_id).unwrap();
    store
        .append_output(
            &task.task_id,
            OutputStream::Stdout,
            0,
            b"actual output",
            4096,
        )
        .unwrap();
    stale.record.reason = Some("stale result".into());
    assert!(other.save(&stale).unwrap_err().contains("stale"));
    // A process owner completes against current state, not its launch revision.
    let finished = other
        .finish(&task.task_id, TaskStatus::Succeeded, Some(0), "actual exit")
        .unwrap();
    assert!(finished.cancel_requested);
    assert_eq!(finished.output_bytes, 13);
    assert_eq!(finished.status, TaskStatus::Succeeded);
    assert_eq!(other.task_snapshot(&task.task_id).unwrap().revision(), 5);
    let duplicate = store
        .finish(&task.task_id, TaskStatus::Failed, Some(1), "late exit")
        .unwrap();
    assert_eq!(value(&duplicate), value(&finished));
    assert_eq!(store.task_snapshot(&task.task_id).unwrap().revision(), 5);
}

#[test]
fn stale_launch_and_even_noop_observations_are_rejected_before_effect() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let task = accept(&mut store, root.path(), "launch-cas");
    let old = store.task_snapshot(&task.task_id).unwrap();
    store
        .append_output(&task.task_id, OutputStream::Stderr, 0, b"diagnostic", 4096)
        .unwrap();
    assert!(store
        .transition_at(&task.task_id, old.revision(), TaskEvent::LaunchIntent)
        .unwrap_err()
        .contains("stale revision"));
    assert_eq!(store.get(&task.task_id).unwrap().status, TaskStatus::Queued);
    store
        .transition_at(&task.task_id, 1, TaskEvent::LaunchIntent)
        .unwrap();
    store
        .transition(
            &task.task_id,
            TaskEvent::ProcessStarted { process_id: 4242 },
        )
        .unwrap();
    let old = store.task_snapshot(&task.task_id).unwrap();
    store.cancel(&task.task_id).unwrap();
    assert!(store
        .transition_at(
            &task.task_id,
            old.revision(),
            TaskEvent::ProcessStarted { process_id: 4242 }
        )
        .unwrap_err()
        .contains("stale revision"));
    assert!(store
        .transition_at(&task.task_id, u64::MAX, TaskEvent::CancellationRequested)
        .unwrap_err()
        .contains("range"));
    assert!(store.get(&task.task_id).unwrap().cancel_requested);
}

#[test]
fn revisions_survive_reopen_and_prevent_aba_with_identical_records() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "aba");
    let old = store.task_snapshot(&task.task_id).unwrap();
    let advanced = store.save(&old).unwrap();
    assert_eq!(value(&old.record), value(&advanced.record));
    drop(store);
    let mut reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.task_snapshot(&task.task_id).unwrap().revision(), 1);
    assert!(reopened.save(&old).unwrap_err().contains("stale"));
    reopened.save(&advanced).unwrap();
    assert_eq!(reopened.task_snapshot(&task.task_id).unwrap().revision(), 2);
}

#[test]
fn snapshot_cannot_be_retargeted_to_another_task_with_the_same_revision() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let first = accept(&mut store, root.path(), "identity-1");
    let second = accept(&mut store, root.path(), "identity-2");
    let mut observed = store.task_snapshot(&first.task_id).unwrap();
    observed.record.task_id = second.task_id.clone();
    assert!(store.save(&observed).unwrap_err().contains("identity"));
    assert_eq!(value(&store.get(&first.task_id).unwrap()), value(&first));
    assert_eq!(value(&store.get(&second.task_id).unwrap()), value(&second));
    for edit in 0..3 {
        let mut changed = store.task_snapshot(&first.task_id).unwrap();
        match edit {
            0 => changed.record.submission.project_id = "different".into(),
            1 => changed.record.submission.idempotency_key = "different".into(),
            _ => changed.record.submission.timeout_ms += 1,
        }
        assert!(store.save(&changed).unwrap_err().contains("not stored"));
        assert_eq!(value(&store.get(&first.task_id).unwrap()), value(&first));
    }
}

#[test]
fn partial_cleanup_delete_or_ignored_receipt_cannot_claim_success() {
    for trigger in [
        "CREATE TRIGGER ignore_delete BEFORE DELETE ON output WHEN OLD.sequence=2 BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER ignore_receipt BEFORE INSERT ON output_cleanup BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let root = TempDir::new().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        let id = seed_old_output(&mut store, root.path());
        store.append_output(&id, OutputStream::Stderr, 0, b"more output", 4096).unwrap();
        let request = cleanup(&store);
        let before = store.task_snapshot(&id).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch(trigger).unwrap();
        assert!(store.apply_output_cleanup(&request).unwrap_err().contains("transaction rejected"));
        assert_eq!(store.output(&id, 0).unwrap().len(), 2);
        assert_eq!(value(&store.get(&id).unwrap()), value(&before.record));
        assert_eq!(store.task_snapshot(&id).unwrap().revision(), before.revision());
        assert_eq!(db.query_row("SELECT count(*) FROM output_cleanup", [], |row| row.get::<_,i64>(0)).unwrap(), 0);
    }
}

#[test]
fn ignored_task_write_rolls_back_output_and_does_not_consume_sequence() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "output-update");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER ignore_write BEFORE UPDATE ON tasks BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"output", 4096)
        .unwrap_err()
        .contains("not stored"));
    assert!(store.output(&task.task_id, 0).unwrap().is_empty());
    assert_eq!(store.task_snapshot(&task.task_id).unwrap().revision(), 0);
    assert_eq!(value(&store.get(&task.task_id).unwrap()), value(&task));
    db.execute_batch("DROP TRIGGER ignore_write;").unwrap();
    store
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"output", 4096)
        .unwrap();
    let chunks = store.output(&task.task_id, 0).unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].sequence, 1);
    assert_eq!(chunks[0].bytes, b"output");
    assert_eq!(store.task_snapshot(&task.task_id).unwrap().revision(), 1);
}

#[test]
fn ignored_output_insert_cannot_increase_the_saved_byte_counter() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "output-insert");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER ignore_output BEFORE INSERT ON output BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .append_output(&task.task_id, OutputStream::Stderr, 0, b"discarded", 4096)
        .unwrap_err()
        .contains("Output chunk was not stored"));
    assert!(store.output(&task.task_id, 0).unwrap().is_empty());
    assert_eq!(store.task_snapshot(&task.task_id).unwrap().revision(), 0);
    assert_eq!(value(&store.get(&task.task_id).unwrap()), value(&task));
}

#[test]
fn ignored_terminal_attention_rolls_back_outcome_and_revision() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "terminal-atomic");
    store
        .transition(&task.task_id, TaskEvent::LaunchIntent)
        .unwrap();
    let before = store.task_snapshot(&task.task_id).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER ignore_attention BEFORE INSERT ON attention BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(store
        .finish(
            &task.task_id,
            TaskStatus::Failed,
            Some(1),
            "confirmed failure"
        )
        .unwrap_err()
        .contains("Outcome attention was not stored"));
    let after = store.task_snapshot(&task.task_id).unwrap();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(value(&after.record), value(&before.record));
    assert!(store.list_attention(0, false).unwrap().records.is_empty());
    db.execute_batch("DROP TRIGGER ignore_attention;").unwrap();
    store
        .finish(
            &task.task_id,
            TaskStatus::Failed,
            Some(1),
            "confirmed failure",
        )
        .unwrap();
    assert_eq!(
        store.task_snapshot(&task.task_id).unwrap().revision(),
        before.revision() + 1
    );
    assert_eq!(store.list_attention(0, false).unwrap().records.len(), 1);
}

#[test]
fn cleanup_write_failure_rolls_back_output_deletion_metadata_and_dedup_key() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let id = seed_old_output(&mut store, root.path());
    let request = cleanup(&store);
    let before = store.task_snapshot(&id).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER ignore_cleanup BEFORE UPDATE ON tasks BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .apply_output_cleanup(&request)
        .unwrap_err()
        .contains("not stored"));
    assert_eq!(store.output(&id, 0).unwrap()[0].bytes, b"kept output");
    assert_eq!(value(&store.get(&id).unwrap()), value(&before.record));
    assert_eq!(
        store.task_snapshot(&id).unwrap().revision(),
        before.revision()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM output_cleanup", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER ignore_cleanup;").unwrap();
    let removed = store.apply_output_cleanup(&request).unwrap();
    assert_eq!(removed.removed_bytes, 11);
    assert!(store.output(&id, 0).unwrap().is_empty());
    assert_eq!(
        store.task_snapshot(&id).unwrap().revision(),
        before.revision() + 1
    );
    let replay = store.apply_output_cleanup(&request).unwrap();
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(removed).unwrap()
    );
    assert_eq!(
        store.task_snapshot(&id).unwrap().revision(),
        before.revision() + 1
    );
}

#[test]
fn cleanup_preview_rejects_aba_even_when_record_and_output_bytes_match() {
    let root = TempDir::new().unwrap();
    let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
    let id = seed_old_output(&mut store, root.path());
    let request = cleanup(&store);
    let observed = store.task_snapshot(&id).unwrap();
    store.save(&observed).unwrap();
    assert_eq!(value(&store.get(&id).unwrap()), value(&observed.record));
    assert!(store
        .apply_output_cleanup(&request)
        .unwrap_err()
        .contains("preview changed"));
    assert_eq!(store.output(&id, 0).unwrap()[0].bytes, b"kept output");
}

#[test]
fn ignored_acceptance_cannot_ack_a_nonexistent_task() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let template = accept(&mut store, root.path(), "template");
    let mut request = template.submission.clone();
    request.idempotency_key = "ignored-accept".into();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER ignore_accept BEFORE INSERT ON tasks BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(store
        .accept(request.clone(), template.resolved_command.clone(), None)
        .unwrap_err()
        .contains("acknowledgement rejected"));
    assert!(store.existing(&request).unwrap().is_none());
    db.execute_batch("DROP TRIGGER ignore_accept;").unwrap();
    let accepted = store
        .accept(request.clone(), template.resolved_command, None)
        .unwrap();
    assert_eq!(
        store.existing(&request).unwrap().unwrap().task_id,
        accepted.task_id
    );
}

fn schema_six(path: &Path) {
    let db = Connection::open(path).unwrap();
    db.execute_batch("DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; DROP TABLE queue_roots; DROP TABLE queue_commands; DROP TABLE queue_control; DROP INDEX queue_pending_tasks; DROP INDEX queue_pending_imports; DROP INDEX queue_pending_workflows; DROP TABLE journal_admission; DROP TABLE resource_leases; DROP TABLE execution_attempts; DROP TABLE journal_authority; DROP TRIGGER imports_revision_guard; ALTER TABLE imports DROP COLUMN revision; DROP TRIGGER workflows_revision_guard; ALTER TABLE workflows DROP COLUMN revision; DROP TRIGGER applications_revision_guard; ALTER TABLE applications DROP COLUMN revision; DROP TRIGGER attention_revision_guard; ALTER TABLE attention DROP COLUMN revision; DROP TRIGGER tasks_revision_guard; ALTER TABLE tasks DROP COLUMN revision; PRAGMA user_version=6;").unwrap();
}

#[test]
fn schema_six_migration_backs_up_exact_payload_records_output_and_attention() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "migrate");
    store
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"old output", 4096)
        .unwrap();
    store
        .finish(&task.task_id, TaskStatus::Failed, Some(7), "old failure")
        .unwrap();
    let old_attention = serde_json::to_value(store.list_attention(0, false).unwrap()).unwrap();
    drop(store);
    schema_six(&path);
    let row = |db: &Connection| {
        db.query_row(
            "SELECT payload,record FROM tasks WHERE id=?1",
            [&task.task_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap()
    };
    let before = row(&Connection::open(&path).unwrap());
    let mut migrated = Store::open(&path).unwrap();
    assert_eq!(migrated.task_snapshot(&task.task_id).unwrap().revision(), 0);
    assert_eq!(row(&Connection::open(&path).unwrap()), before);
    assert_eq!(
        migrated.output(&task.task_id, 0).unwrap()[0].bytes,
        b"old output"
    );
    assert_eq!(
        serde_json::to_value(migrated.list_attention(0, false).unwrap()).unwrap(),
        old_attention
    );
    let retry = migrated
        .accept(task.submission.clone(), task.resolved_command, None)
        .unwrap();
    assert_eq!(retry.task_id, task.task_id);
    drop(migrated);
    let backups = || {
        fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("journal-v6-")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(backups().len(), 1);
    let backup =
        Connection::open_with_flags(&backups()[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(row(&backup), before);
    assert_eq!(
        backup
            .query_row(
                "SELECT bytes FROM output WHERE task=?1",
                [&task.task_id],
                |r| r.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        b"old output"
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM attention", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(backup);
    drop(Store::open(&path).unwrap());
    assert_eq!(backups().len(), 1, "reopen must not repeat the migration");
}

#[test]
fn failed_migration_keeps_schema_and_record_and_has_a_consistent_backup() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "migration-fault");
    drop(store);
    schema_six(&path);
    let db = Connection::open(&path).unwrap();
    // Reserved-name collision faults the migration after ALTER, inside its transaction.
    db.execute_batch("CREATE TRIGGER tasks_revision_guard BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(Store::open(&path).err().unwrap().contains("already exists"));
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('tasks') WHERE name='revision'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let saved: String = db
        .query_row(
            "SELECT record FROM tasks WHERE id=?1",
            [&task.task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
        value(&task)
    );
    assert_eq!(
        fs::read_dir(root.path())
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("journal-v6-"))
            .count(),
        1
    );
}

#[test]
fn unguarded_updates_and_revision_overflow_cannot_change_task_or_output() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "guard");
    let db = Connection::open(&path).unwrap();
    for sql in [
        "UPDATE tasks SET record=record",
        "UPDATE tasks SET revision=revision+2",
        "UPDATE tasks SET revision=revision+1,payload='changed'",
    ] {
        assert!(db.execute(sql, []).is_err());
    }
    db.execute_batch(
        "DROP TRIGGER tasks_revision_guard; UPDATE tasks SET revision=9223372036854775807;",
    )
    .unwrap();
    assert!(store
        .append_output(&task.task_id, OutputStream::Stdout, 0, b"unconfirmed", 4096)
        .unwrap_err()
        .contains("revision exhausted"));
    assert!(store.output(&task.task_id, 0).unwrap().is_empty());
    assert_eq!(value(&store.get(&task.task_id).unwrap()), value(&task));
    drop(store);
    assert_eq!(
        Store::open(&path)
            .unwrap()
            .task_snapshot(&task.task_id)
            .unwrap()
            .revision(),
        i64::MAX as u64
    );
}

#[test]
fn concurrent_equal_key_acceptance_returns_one_committed_task() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut first = Store::open(&path).unwrap();
    let mut second = Store::open(&path).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let barrier1 = barrier.clone();
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier1.wait();
            accept(&mut first, root.path(), "concurrent").task_id
        });
        let b = scope.spawn(|| {
            barrier.wait();
            accept(&mut second, root.path(), "concurrent").task_id
        });
        assert_eq!(a.join().unwrap(), b.join().unwrap());
    });
    assert_eq!(first.all().unwrap().len(), 1);
}

#[test]
#[ignore = "requires an explicit schema-6 reference worker; native TaskCasOnly gate runs it"]
fn legacy_schema_six_worker_refuses_new_journal_without_mutating_it() {
    let reference = std::env::var_os("WINBRIDGE_TASK_CAS_REFERENCE_WORKER")
        .expect("explicit schema-6 reference worker required");
    use sha2::{Digest, Sha256};
    use std::{
        io::Read,
        thread,
        time::{Duration, Instant},
    };
    let run = |arguments: Vec<String>, root: &Path| {
        let command = winbridge_protocol::ResolvedCommand {
            executable: Path::new(&reference).to_string_lossy().into_owned(),
            arguments,
            working_directory: root.to_string_lossy().into_owned(),
            timeout_ms: 15_000,
        };
        let (mut child, stdout, stderr) =
            winbridge_platform::ManagedChild::spawn(&command).unwrap();
        let drain = |reader: winbridge_platform::OutputReader| {
            thread::spawn(move || {
                let mut bytes = Vec::new();
                reader.take(65537).read_to_end(&mut bytes).unwrap();
                assert!(bytes.len() <= 65536, "reference diagnostic exceeds limit");
                bytes
            })
        };
        let out = drain(stdout);
        let err = drain(stderr);
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "reference binary did not refuse the new schema"
            );
            thread::sleep(Duration::from_millis(10));
        };
        (status, out.join().unwrap(), err.join().unwrap())
    };
    let root = TempDir::new().unwrap();
    let cfg = config(root.path());
    fs::create_dir(&cfg.state_directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cfg.state_directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = cfg.state_directory.join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let task = accept(&mut store, root.path(), "reference-refusal");
    drop(store);
    let before = fs::read(&path).unwrap();
    let config_path = root.path().join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    let (status, descriptor, _) = run(vec!["--describe".into()], root.path());
    assert!(status.success());
    let descriptor: winbridge_protocol::RuntimeDescriptor =
        serde_json::from_slice(&descriptor).unwrap();
    let (status, stdout, stderr) = run(
        vec![
            "--config".into(),
            config_path.to_string_lossy().into_owned(),
            "serve".into(),
        ],
        root.path(),
    );
    let error = String::from_utf8(stderr).unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(
        error.contains(&format!(
            "journal schema {} is unsupported; expected 6",
            winbridge_worker::store::JOURNAL_SCHEMA_VERSION
        )),
        "{error}"
    );
    assert!(stdout.is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
    let store = Store::open(&path).unwrap();
    assert_eq!(value(&store.get(&task.task_id).unwrap()), value(&task));
    assert_eq!(store.task_snapshot(&task.task_id).unwrap().revision(), 0);
    assert!(!root.path().join("runs.log").exists());
    if let Some(report) = std::env::var_os("WINBRIDGE_TASK_CAS_REPORT") {
        fs::write(Path::new(&report).join("task-cas-reference-proof.json"),serde_json::to_vec_pretty(&json!({
            "schemaVersion":1,"platform":if cfg!(windows){"windows"}else{"linux"},"passed":true,
            "build":winbridge_protocol::build_identity(),"referenceBuild":descriptor.build,
            "referenceExecutable":reference.to_string_lossy(),"referenceSha256":format!("{:x}",Sha256::digest(fs::read(&reference).unwrap())),
            "journalSchema":JOURNAL_SCHEMA_VERSION,"journalUnchanged":true,"taskId":task.task_id,"exitCode":status.code(),"reason":error
        })).unwrap()).unwrap();
    }
}
