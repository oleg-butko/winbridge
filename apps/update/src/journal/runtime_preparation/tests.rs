//! Private ledger tests use synthetic metadata, never a physical admission permit.
use super::*;
use crate::runtime_preparation::tests::{fixture, stages};
fn seed(c: &Connection) -> RuntimePreparationRecord {
    c.execute_batch(
        "PRAGMA foreign_keys=ON; CREATE TABLE state_backup_records(id TEXT PRIMARY KEY);",
    )
    .unwrap();
    migrate(c).unwrap();
    let r = fixture();
    c.execute(
        "INSERT INTO state_backup_records VALUES(?1)",
        [&r.plan.update_id],
    )
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    insert(&tx, &r).unwrap();
    tx.commit().unwrap();
    r
}
fn write(
    c: &mut Connection,
    old: &RuntimePreparationRecord,
    next: &RuntimePreparationRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next)?;
    tx.commit().map_err(storage)
}
#[test]
fn preparation_ledger_cas_and_causal_history_are_retained_until_terminal() {
    let mut c = Connection::open_in_memory().unwrap();
    let initial = seed(&c);
    let mut old = initial.clone();
    for next in stages(&initial) {
        write(&mut c, &old, &next).unwrap();
        assert!(write(&mut c, &old, &next).is_err());
        assert_eq!(ledger(&c, &initial.plan).unwrap(), next);
        old = next;
    }
    assert_eq!(old.phase, PreparationPhase::Prepared);
    for sql in [
        "UPDATE runtime_preparation_intents SET record=record",
        "DELETE FROM runtime_preparation_intents",
        "DELETE FROM runtime_preparation_records",
        "DELETE FROM runtime_preparation_history",
        "UPDATE runtime_preparation_history SET record=record",
        "UPDATE runtime_preparation_records SET revision=revision+1",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(ledger(&c, &initial.plan).unwrap(), old);
}
#[test]
fn preparation_ignored_admission_and_append_writes_roll_back_all_coupled_rows() {
    for table in [
        "runtime_preparation_intents",
        "runtime_preparation_records",
        "runtime_preparation_history",
    ] {
        let mut c = Connection::open_in_memory().unwrap();
        let r = seed(&c);
        let other = fixture();
        c.execute(
            "INSERT INTO state_backup_records VALUES(?1)",
            [&other.plan.update_id],
        )
        .unwrap();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(insert(&tx, &other).is_err());
        }
        for t in [
            "runtime_preparation_intents",
            "runtime_preparation_records",
            "runtime_preparation_history",
        ] {
            assert_eq!(
                c.query_row(
                    &format!("SELECT count(*) FROM {t} WHERE id=?1"),
                    [&other.plan.update_id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        }
        c.execute_batch("DROP TRIGGER deny").unwrap();
        if table != "runtime_preparation_intents" {
            let verb = if table == "runtime_preparation_records" {
                "UPDATE"
            } else {
                "INSERT"
            };
            c.execute_batch(&format!(
                "CREATE TRIGGER deny BEFORE {verb} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
            ))
            .unwrap();
            assert!(write(&mut c, &r, &r.issue_directory().unwrap()).is_err());
            assert_eq!(ledger(&c, &r.plan).unwrap(), r);
        }
    }
}
#[test]
fn preparation_reopen_of_issued_copy_and_unknown_history_never_reissues() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ledger.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let initial = seed(&c);
    let mut old = initial.clone();
    for next in stages(&initial).into_iter().take(3) {
        write(&mut c, &old, &next).unwrap();
        old = next;
    }
    drop(c);
    let mut c = Connection::open(&path).unwrap();
    let saved = ledger(&c, &initial.plan).unwrap();
    assert!(saved.issue_file(0).is_err());
    let unknown = saved.indeterminate("Copy outcome unknown".into()).unwrap();
    write(&mut c, &saved, &unknown).unwrap();
    drop(c);
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(ledger(&c, &initial.plan).unwrap(), unknown);
    assert!(unknown.issue_directory().is_err());
}
#[test]
fn preparation_history_tampering_is_detected_even_when_current_is_valid() {
    let mut c = Connection::open_in_memory().unwrap();
    let r = seed(&c);
    let next = r.issue_directory().unwrap();
    write(&mut c, &r, &next).unwrap();
    c.execute_batch("DROP TRIGGER runtime_preparation_history_immutable")
        .unwrap();
    c.execute(
        "UPDATE runtime_preparation_history SET record=?1 WHERE revision=0",
        [encode(&next).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &r.plan).is_err());
}
#[test]
fn schema_thirteen_migration_has_consistent_backup_and_preserves_old_rows() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("state");
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(&dir, &scope).unwrap();
    let info = j.info().unwrap();
    let request = winbridge_protocol::update::UpdateRequest {
        contract_version: 1,
        idempotency_key: "retained-preparation-migration".into(),
        source: winbridge_protocol::update::UpdateSourcePin {
            archive_name: "source.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: winbridge_protocol::update::UpdateInstallationPin {
            installation_id: info.installation_id.clone(),
            scope_sha256: scope.clone(),
            worker_build_id: "123".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let owner = j.acquire_owner().unwrap();
    let saved = j.accept(&owner, &request, &request.expected).unwrap();
    let before = j.info().unwrap();
    drop(owner);
    drop(j);
    {
        let c = Connection::open(dir.join("updates.sqlite")).unwrap();
        c.execute_batch("DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history; DROP TABLE activation_records; DROP TABLE activation_intents; DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history;DROP TABLE runtime_preparation_records;DROP TABLE runtime_preparation_intents;PRAGMA user_version=13;").unwrap();
    }
    let mut j = UpdateJournal::open(&dir, &scope).unwrap();
    let after = j.info().unwrap();
    assert_eq!(after.schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(j.get(&saved.update_id).unwrap(), saved);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert!(
        UpdateJournal::inspect_runtime_preparation(&dir, &saved.update_id)
            .unwrap()
            .is_none()
    );
    let backups = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|p| p.ok())
        .filter(|p| p.file_name().to_string_lossy().starts_with("updates-v13-"))
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let c =
        Connection::open_with_flags(backups[0].path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 13);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
    assert_eq!(
        c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
