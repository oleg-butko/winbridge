//! Synthetic SQL tests, never a trusted admission/physical cleanup proof.
use super::*;
use crate::candidate_support::tests::{fixture, stages};
fn seed(c: &Connection) -> CandidateSupportRecord {
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE candidate_health(id TEXT PRIMARY KEY);")
        .unwrap();
    migrate(c).unwrap();
    let r = fixture();
    c.execute(
        "INSERT INTO candidate_health VALUES(?1)",
        [&r.intent.update_id],
    )
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    insert(&tx, &r).unwrap();
    tx.commit().unwrap();
    r
}
fn write(
    c: &mut Connection,
    old: &CandidateSupportRecord,
    next: &CandidateSupportRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next)?;
    tx.commit().map_err(storage)
}
#[test]
fn support_ledger_retains_causal_history_and_duplicate_cas_never_replays() {
    let mut c = Connection::open_in_memory().unwrap();
    let initial = seed(&c);
    let mut old = initial.clone();
    for next in stages(&initial) {
        write(&mut c, &old, &next).unwrap();
        assert!(write(&mut c, &old, &next).is_err());
        old = next;
    }
    assert_eq!(ledger(&c, &initial.intent).unwrap(), old);
    for sql in [
        "DELETE FROM candidate_support_intents",
        "UPDATE candidate_support_intents SET record=record",
        "DELETE FROM candidate_support_records",
        "UPDATE candidate_support_records SET revision=revision+1",
        "DELETE FROM candidate_support_history",
        "UPDATE candidate_support_history SET record=record",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
}
#[test]
fn ignored_support_history_or_record_write_rolls_back_receipt() {
    for table in ["candidate_support_records", "candidate_support_history"] {
        let mut c = Connection::open_in_memory().unwrap();
        let r = seed(&c);
        let verb = if table == "candidate_support_records" {
            "UPDATE"
        } else {
            "INSERT"
        };
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE {verb} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        assert!(write(&mut c, &r, &stages(&r)[0]).is_err());
        assert_eq!(ledger(&c, &r.intent).unwrap(), r);
    }
}
#[test]
fn support_reopen_keeps_creation_unknown_without_a_new_permit() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixture.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let r = seed(&c);
    let issued = stages(&r)[0].clone();
    write(&mut c, &r, &issued).unwrap();
    drop(c);
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = ledger(&c, &r.intent).unwrap();
    assert_eq!(saved, issued);
    assert!(saved.advance(&issued).is_err());
    assert!(crate::candidate_support::physically_quiescent(&saved).is_err());
}
#[test]
fn tampered_support_history_is_rejected_even_when_current_record_is_valid() {
    let mut c = Connection::open_in_memory().unwrap();
    let r = seed(&c);
    let issued = stages(&r)[0].clone();
    write(&mut c, &r, &issued).unwrap();
    c.execute_batch("DROP TRIGGER candidate_support_history_immutable")
        .unwrap();
    c.execute(
        "UPDATE candidate_support_history SET record=?1 WHERE revision=0",
        [encode(&issued).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &r.intent).is_err());
}

#[test]
fn schema_fifteen_migration_preserves_root_backs_up_and_fences_stale_callbacks() {
    use winbridge_protocol::update::{UpdateInstallationPin, UpdateRequest, UpdateSourcePin};
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(root.path(), &scope).unwrap();
    let i = old.info().unwrap();
    let owner = old.acquire_owner().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "support-migration".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: i.installation_id,
            scope_sha256: scope.clone(),
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let saved = old.accept(&owner, &request, &request.expected).unwrap();
    old.connection.execute_batch("DROP TABLE candidate_support_history;DROP TABLE candidate_support_records;DROP TABLE candidate_support_intents;PRAGMA user_version=15;").unwrap();
    let before = old.info().unwrap();
    // No real supervisor was spawned. Release the synthetic OS lease explicitly;
    // retain the old value to exercise the independent persistent epoch fence.
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(root.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, 16);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&saved.update_id).unwrap(), saved);
    assert_eq!(
        new.check_source_authority(&owner).unwrap_err().code,
        JournalErrorCode::StaleOwner
    );
    assert!(new.candidate_support(&saved.update_id).unwrap().is_none());
    for table in [
        "candidate_support_intents",
        "candidate_support_records",
        "candidate_support_history",
    ] {
        assert_eq!(
            new.connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let backups = fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("updates-v15-"))
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let c =
        Connection::open_with_flags(backups[0].path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 15);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
    assert_eq!(info(&c).unwrap().owner_epoch, before.owner_epoch);
}
