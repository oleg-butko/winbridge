//! SQL primitive tests. Synthetic rows never assert physical worker/process state.
use super::*;
fn fixture() -> (Connection, CompatibleDrainRecord) {
    let (support, r) = crate::compatible_drain::tests::blocked_record();
    r.validate(&support).unwrap();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE candidate_support_records(id TEXT PRIMARY KEY);CREATE TABLE drain_intents(id TEXT PRIMARY KEY);").unwrap();
    migrate(&c).unwrap();
    c.execute(
        "INSERT INTO candidate_support_records VALUES(?1)",
        [&r.update_id],
    )
    .unwrap();
    (c, r)
}
#[test]
fn blocked_metadata_creates_no_outbox_and_is_immutable_and_retained() {
    let (c, r) = fixture();
    put(&c, &r).unwrap();
    assert_eq!(load(&c, &r.update_id).unwrap(), Some((None, r.clone())));
    assert_eq!(
        c.query_row("SELECT count(*) FROM drain_intents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(put(&c, &r).is_err());
    for sql in [
        "UPDATE compatible_drain_records SET record=record",
        "DELETE FROM compatible_drain_records",
    ] {
        assert!(c.execute(sql, []).is_err());
    }
}
#[test]
fn ignored_link_write_rolls_back_the_same_transaction_outbox() {
    let (mut c, mut r) = fixture();
    r.decision = CompatibleDrainDecision::Admitted {
        drain_attempt_id: uuid::Uuid::new_v4().to_string(),
    };
    // Deliberately raw synthetic SQL: exercises transaction atomicity only.
    c.execute_batch("CREATE TRIGGER deny BEFORE INSERT ON compatible_drain_records BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        tx.execute("INSERT INTO drain_intents VALUES(?1)", [&r.update_id])
            .unwrap();
        assert!(put(&tx, &r).is_err());
    }
    assert_eq!(
        c.query_row("SELECT count(*) FROM drain_intents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(load(&c, &r.update_id).unwrap().is_none());
}
#[test]
fn admitted_link_requires_the_exact_existing_outbox_key() {
    let (c, mut r) = fixture();
    r.decision = CompatibleDrainDecision::Admitted {
        drain_attempt_id: uuid::Uuid::new_v4().to_string(),
    };
    assert!(
        put(&c, &r).is_err(),
        "Missing FK must not produce an admitted link"
    );
    c.execute("INSERT INTO drain_intents VALUES(?1)", [&r.update_id])
        .unwrap();
    put(&c, &r).unwrap();
    assert_eq!(
        load(&c, &r.update_id).unwrap().unwrap().0,
        Some(r.update_id)
    );
}

#[test]
fn schema_sixteen_migration_backs_up_and_fences_without_creating_decisions() {
    use winbridge_protocol::update::{UpdateInstallationPin, UpdateRequest, UpdateSourcePin};
    let temp = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(temp.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let i = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "production-migration".into(),
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
    old.connection
        .execute_batch("DROP TABLE IF EXISTS keeper_selections;DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history; DROP TABLE activation_records; DROP TABLE activation_intents; DROP TABLE compatible_drain_records;PRAGMA user_version=16;")
        .unwrap();
    let before = old.info().unwrap();
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(temp.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&saved.update_id).unwrap(), saved);
    assert_eq!(
        new.check_source_authority(&owner).unwrap_err().code,
        JournalErrorCode::StaleOwner
    );
    assert!(read(&new.connection, &saved.update_id).unwrap().is_none());
    let backups = std::fs::read_dir(temp.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v16-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let c = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 16);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
}
