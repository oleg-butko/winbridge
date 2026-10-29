//! SQL primitives/synthetic migration only, never original-custody authority.
use super::*;
fn fixture() -> (Connection, RuntimeReplacementRecord, ActivationRecord) {
    let (r, initial) = crate::activation::activation_fixture();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(
        "PRAGMA foreign_keys=ON;CREATE TABLE runtime_replacement_records(id TEXT PRIMARY KEY);",
    )
    .unwrap();
    migrate(&c).unwrap();
    c.execute(
        "INSERT INTO runtime_replacement_records VALUES(?1)",
        [&initial.intent.source.update_id],
    )
    .unwrap();
    (c, r, initial)
}
fn write(
    c: &mut Connection,
    old: &ActivationRecord,
    next: &ActivationRecord,
    r: &RuntimeReplacementRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next, r)?;
    tx.commit().map_err(storage)
}
#[test]
fn activation_ledger_retains_each_causal_step_and_rejects_stale_cas() {
    let (mut c, r, initial) = fixture();
    seed(&c, &initial, &r).unwrap();
    let all = crate::activation::activation_stages(&initial, &r);
    for pair in all.windows(2) {
        write(&mut c, &pair[0], &pair[1], &r).unwrap();
        assert!(write(&mut c, &pair[0], &pair[1], &r).is_err());
    }
    assert_eq!(&ledger(&c, &r).unwrap(), all.last().unwrap());
    for sql in [
        "UPDATE activation_intents SET record=record",
        "DELETE FROM activation_intents",
        "UPDATE activation_records SET revision=revision+1",
        "DELETE FROM activation_records",
        "UPDATE activation_history SET record=record",
        "DELETE FROM activation_history",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
}
#[test]
fn ignored_activation_admission_write_rolls_back_all_three_rows() {
    for table in [
        "activation_intents",
        "activation_records",
        "activation_history",
    ] {
        let (mut c, r, initial) = fixture();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(seed(&tx, &initial, &r).is_err());
        }
        for table in [
            "activation_intents",
            "activation_records",
            "activation_history",
        ] {
            assert_eq!(
                c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
#[test]
fn ignored_activation_receipt_history_rolls_back_current_state() {
    let (mut c, r, initial) = fixture();
    seed(&c, &initial, &r).unwrap();
    c.execute_batch(
        "CREATE TRIGGER deny BEFORE INSERT ON activation_history BEGIN SELECT RAISE(IGNORE);END;",
    )
    .unwrap();
    let next = initial.issue_create(201, &r).unwrap();
    assert!(write(&mut c, &initial, &next, &r).is_err());
    assert_eq!(ledger(&c, &r).unwrap(), initial);
}
#[test]
fn activation_history_rewrite_is_detected_independently_of_current_row() {
    let (mut c, r, initial) = fixture();
    seed(&c, &initial, &r).unwrap();
    let next = initial.issue_create(201, &r).unwrap();
    write(&mut c, &initial, &next, &r).unwrap();
    c.execute_batch("DROP TRIGGER activation_history_immutable")
        .unwrap();
    c.execute(
        "UPDATE activation_history SET record=?1 WHERE revision=0",
        [encode(&next).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &r).is_err());
}
#[test]
fn cold_activation_data_does_not_offer_a_second_creation_step() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("fixture.sqlite");
    let (source, r, initial) = fixture();
    let mut c = Connection::open(&file).unwrap();
    rusqlite::backup::Backup::new(&source, &mut c)
        .unwrap()
        .run_to_completion(64, std::time::Duration::from_millis(10), None)
        .unwrap();
    seed(&c, &initial, &r).unwrap();
    let issue = initial.issue_create(201, &r).unwrap();
    write(&mut c, &initial, &issue, &r).unwrap();
    drop(c);
    let c = Connection::open_with_flags(&file, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = ledger(&c, &r).unwrap();
    assert_eq!(saved, issue);
    assert!(saved.issue_create(202, &r).is_err());
}
#[test]
fn schema_seventeen_migration_preserves_history_backup_and_fences_old_owner() {
    use winbridge_protocol::update::{UpdateInstallationPin, UpdateRequest, UpdateSourcePin};
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let info = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "activation-migration".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: info.installation_id,
            scope_sha256: scope.clone(),
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let saved = old.accept(&owner, &request, &request.expected).unwrap();
    old.connection.execute_batch("DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history;DROP TABLE activation_records;DROP TABLE activation_intents;PRAGMA user_version=17;").unwrap();
    let before = old.info().unwrap();
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(directory.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&saved.update_id).unwrap(), saved);
    assert_eq!(
        old.check_source_authority(&owner).unwrap_err().code,
        JournalErrorCode::StaleOwner
    );
    assert!(read(&new.connection, &saved.update_id).unwrap().is_none());
    let backups = std::fs::read_dir(directory.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v17-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let c = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 17);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
}
