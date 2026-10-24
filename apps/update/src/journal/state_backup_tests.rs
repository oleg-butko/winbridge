//! SQLite ledger tests with synthetic metadata; no helper or OS proof is created.
use super::*;
use crate::state_backup::tests::{cleanup, fixture, input, process, report};
use winbridge_protocol::update::{UpdateInstallationPin, UpdateSourcePin};

fn seed(c: &Connection) -> StateBackupRecord {
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE shutdown_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    migrate(c).unwrap();
    let r = fixture();
    c.execute(
        "INSERT INTO shutdown_intents VALUES(?1)",
        [r.intent.update_id()],
    )
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    insert(&tx, &r).unwrap();
    tx.commit().unwrap();
    r
}
fn write(c: &mut Connection, old: &StateBackupRecord, next: &StateBackupRecord) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next)?;
    tx.commit().map_err(storage)
}
fn stages(r: &StateBackupRecord) -> Vec<StateBackupRecord> {
    let issued = r.issue(input(r), 401).unwrap();
    let grant = issued.grant_resume(process(r), 402).unwrap();
    let run = grant.resumed(403).unwrap();
    let clean = run.finished(cleanup(&run), 406).unwrap();
    let verified = clean.verify(report(&clean), 410).unwrap();
    vec![issued, grant, run, clean, verified]
}
#[test]
fn backup_ledger_cas_persists_one_exact_attempt_and_immutable_causal_history() {
    let mut c = Connection::open_in_memory().unwrap();
    let initial = seed(&c);
    let mut previous = initial.clone();
    for next in stages(&initial) {
        write(&mut c, &previous, &next).unwrap();
        assert!(write(&mut c, &previous, &next).is_err());
        assert_eq!(ledger(&c, &initial.intent).unwrap(), next);
        previous = next;
    }
    assert_eq!(previous.phase, StateBackupPhase::Verified);
    assert_eq!(
        c.query_row("SELECT count(*) FROM state_backup_history", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        6
    );
    for sql in [
        "UPDATE state_backup_intents SET record=record",
        "DELETE FROM state_backup_intents",
        "DELETE FROM state_backup_records",
        "DELETE FROM state_backup_history",
        "UPDATE state_backup_history SET record=record",
        "UPDATE state_backup_records SET revision=revision+1",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(ledger(&c, &initial.intent).unwrap(), previous);
}
#[test]
fn backup_reopen_retains_pid_resume_and_terminal_uncertainty_without_new_issue() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ledger.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let initial = seed(&c);
    let issued = initial.issue(input(&initial), 401).unwrap();
    write(&mut c, &initial, &issued).unwrap();
    drop(c);
    let mut c = Connection::open(&path).unwrap();
    let saved = ledger(&c, &initial.intent).unwrap();
    assert_eq!(saved, issued);
    assert!(saved.issue(input(&initial), 402).is_err());
    let grant = saved.grant_resume(process(&initial), 402).unwrap();
    write(&mut c, &saved, &grant).unwrap();
    let unknown = grant
        .indeterminate("Resume outcome not known".into(), 403)
        .unwrap();
    write(&mut c, &grant, &unknown).unwrap();
    drop(c);
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = ledger(&c, &initial.intent).unwrap();
    assert_eq!(saved, unknown);
    assert_eq!(saved.process, grant.process);
    assert!(saved.resumed(404).is_err());
    assert!(saved.issue(input(&initial), 404).is_err());
}
#[test]
fn backup_ignored_history_or_current_write_rolls_back_transition() {
    for table in ["state_backup_records", "state_backup_history"] {
        let mut c = Connection::open_in_memory().unwrap();
        let initial = seed(&c);
        let next = initial.issue(input(&initial), 401).unwrap();
        let verb = if table == "state_backup_records" {
            "UPDATE"
        } else {
            "INSERT"
        };
        c.execute_batch(&format!(
            "CREATE TRIGGER deny_backup BEFORE {verb} ON {table} BEGIN SELECT RAISE(IGNORE); END;"
        ))
        .unwrap();
        assert!(write(&mut c, &initial, &next).is_err());
        assert_eq!(ledger(&c, &initial.intent).unwrap(), initial);
        c.execute_batch("DROP TRIGGER deny_backup").unwrap();
        write(&mut c, &initial, &next).unwrap();
        assert_eq!(ledger(&c, &initial.intent).unwrap(), next);
    }
}
#[test]
fn backup_ignored_admission_history_leaves_no_intent_or_attempt() {
    let mut c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE shutdown_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    migrate(&c).unwrap();
    let r = fixture();
    c.execute(
        "INSERT INTO shutdown_intents VALUES(?1)",
        [r.intent.update_id()],
    )
    .unwrap();
    c.execute_batch("CREATE TRIGGER deny_backup BEFORE INSERT ON state_backup_history BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        assert!(insert(&tx, &r).is_err());
    }
    for table in [
        "state_backup_intents",
        "state_backup_records",
        "state_backup_history",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn backup_history_rejects_skipped_phase_changed_pid_and_corrupt_saved_append() {
    let mut c = Connection::open_in_memory().unwrap();
    let initial = seed(&c);
    let stages = stages(&initial);
    assert!(write(&mut c, &initial, &stages[1]).is_err());
    write(&mut c, &initial, &stages[0]).unwrap();
    write(&mut c, &stages[0], &stages[1]).unwrap();
    let mut changed = stages[2].clone();
    changed.process.as_mut().unwrap().birth.push('9');
    changed.validate().unwrap(); // Individually valid; causally invalid.
    assert!(write(&mut c, &stages[1], &changed).is_err());
    assert_eq!(ledger(&c, &initial.intent).unwrap(), stages[1]);
    c.execute_batch("DROP TRIGGER state_backup_history_immutable;")
        .unwrap();
    let mut corrupt = stages[0].clone();
    corrupt.updated_at_ms = 402; // Still valid in isolation, but after the next grant.
    corrupt.input_file.as_mut().unwrap().file_identity.push('x');
    corrupt.validate().unwrap();
    c.execute(
        "UPDATE state_backup_history SET record=?1 WHERE revision=1",
        [encode(&corrupt).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &initial.intent).is_err());
}
#[test]
fn schema_twelve_migration_backs_up_fences_owner_and_does_not_invent_backup() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    let a = j.acquire_owner().unwrap();
    let i = j.info().unwrap();
    let req = UpdateRequest {
        contract_version: 1,
        idempotency_key: "backup-migration".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            source_commit: "c".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: i.installation_id.clone(),
            scope_sha256: scope.clone(),
            worker_build_id: "old-build".into(),
            worker_config_sha256: "d".repeat(64),
            baseline_manifest_sha256: "e".repeat(64),
        },
    };
    let before = j.accept(&a, &req, &req.expected).unwrap();
    j.connection.execute_batch("DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; PRAGMA user_version=12; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    assert_eq!(read(&j.connection, &before.update_id).unwrap(), None);
    let bytes = fs::read(root.path().join("updates.sqlite")).unwrap();
    assert!(matches!(
        UpdateJournal::open(root.path(), &scope),
        Err(JournalError {
            code: JournalErrorCode::OwnerBusy,
            ..
        })
    ));
    assert_eq!(fs::read(root.path().join("updates.sqlite")).unwrap(), bytes);
    assert_eq!(j.info().unwrap().owner_epoch, a.epoch());
    drop(a);
    drop(j);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    assert_eq!(j.info().unwrap().schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(j.info().unwrap().installation_id, i.installation_id);
    assert!(j.info().unwrap().owner_epoch > i.owner_epoch);
    assert_eq!(j.get(&before.update_id).unwrap(), before);
    assert_eq!(read(&j.connection, &before.update_id).unwrap(), None);
    let path = fs::read_dir(root.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v12-")
        })
        .unwrap();
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 12);
    assert_eq!(info(&c).unwrap().owner_epoch, i.owner_epoch);
    assert_eq!(record(&c, &before.update_id).unwrap(), before);
    assert_eq!(
        c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(c.prepare("SELECT * FROM state_backup_intents").is_err());
}

#[test]
fn export_inspection_without_saved_attempt_is_readonly_and_never_migrates_old_schema() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    let a = j.acquire_owner().unwrap();
    let r = fixture();
    let input = input(&r);
    let process = process(&r);
    let original = j.info().unwrap();
    j.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let path = root.path().join("updates.sqlite");
    let bytes = fs::read(&path).unwrap();
    let denied = UpdateJournal::inspect_state_backup_export(
        root.path(),
        r.intent.update_id(),
        &process,
        &input,
    )
    .unwrap_err();
    assert_eq!(denied.code, JournalErrorCode::NotFound);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(j.info().unwrap(), original);
    j.connection.execute_batch("DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; PRAGMA user_version=12; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = fs::read(&path).unwrap();
    let denied = UpdateJournal::inspect_state_backup_export(
        root.path(),
        r.intent.update_id(),
        &process,
        &input,
    )
    .unwrap_err();
    assert_eq!(denied.code, JournalErrorCode::UnsupportedSchema);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(schema(&j.connection).unwrap(), 12);
    assert_eq!(j.info().unwrap().owner_epoch, a.epoch());
}
