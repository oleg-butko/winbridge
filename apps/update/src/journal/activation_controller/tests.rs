//! SQL/FSM primitives and synthetic migrations; never original process custody.
use super::*;
use crate::activation::controller::ActivationControllerState;

fn fixture() -> (
    Connection,
    Graph,
    ActivationControllerRecord,
    winbridge_protocol::update::WorkerControllerActivationReceipt,
) {
    let f: controller::TestFixture = controller::test_fixture();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE activation_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    c.execute(
        "INSERT INTO activation_intents VALUES(?1)",
        [&f.physical.intent.source.update_id],
    )
    .unwrap();
    activation_writer::migrate(&c).unwrap();
    let issued = ActivationWriterRecord::issued(f.writer.command.clone()).unwrap();
    activation_writer::seed(
        &c,
        &issued,
        &f.physical,
        &f.writer.command.binding.expected_authority,
    )
    .unwrap();
    activation_writer::persist(
        &c,
        &issued,
        &f.writer,
        &f.physical,
        &f.writer.command.binding.expected_authority,
    )
    .unwrap();
    activation_service::migrate(&c).unwrap();
    let issued = ActivationServiceRecord::issued(f.service.command.clone()).unwrap();
    activation_service::seed(&c, &issued, &f.physical, &f.writer).unwrap();
    activation_service::persist(&c, &issued, &f.service, &f.physical, &f.writer).unwrap();
    migrate(&c).unwrap();
    let value = ActivationControllerRecord::issued(f.command).unwrap();
    (
        c,
        Graph {
            physical: f.physical,
            writer: f.writer,
            service: f.service,
            local: f.local,
        },
        value,
        f.receipt,
    )
}
fn append(
    c: &mut Connection,
    old: &ActivationControllerRecord,
    next: &ActivationControllerRecord,
    graph: &Graph,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next, graph)?;
    tx.commit().map_err(storage)
}
fn get(
    c: &Connection,
    old: &ActivationControllerRecord,
    graph: &Graph,
) -> Result<ActivationControllerRecord> {
    ledger(c, &old.command.binding.service_receipt.update_id, graph)
}
#[test]
fn controller_history_retains_late_confirmation_without_new_epoch_window_or_reissue() {
    let (mut c, g, v, r) = fixture();
    seed(&c, &v, &g).unwrap();
    let unknown = v
        .indeterminate("Private ACK lost".into(), v.command.deadline_at_ms + 1)
        .unwrap();
    append(&mut c, &v, &unknown, &g).unwrap();
    let confirmed = unknown.confirmed(r, unknown.updated_at_ms + 1).unwrap();
    append(&mut c, &unknown, &confirmed, &g).unwrap();
    assert_eq!(get(&c, &v, &g).unwrap(), confirmed);
    assert_eq!(confirmed.command, v.command);
    assert!(append(&mut c, &v, &unknown, &g).is_err());
    assert!(append(&mut c, &confirmed, &confirmed, &g).is_err());
    assert_eq!(
        g.service.command.binding.writer_receipt.authority,
        confirmed.command.binding.service_receipt.authority
    );
}
#[test]
fn controller_sql_retains_keys_current_and_each_history_row() {
    let (c, g, v, _) = fixture();
    seed(&c, &v, &g).unwrap();
    for sql in [
        "UPDATE activation_controller_intents SET command=command",
        "DELETE FROM activation_controller_intents",
        "UPDATE activation_controller_records SET revision=revision+1",
        "DELETE FROM activation_controller_records",
        "UPDATE activation_controller_history SET record=record",
        "DELETE FROM activation_controller_history",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
}
#[test]
fn controller_admission_ignored_write_rolls_back_all_three_rows() {
    for table in [
        "activation_controller_intents",
        "activation_controller_records",
        "activation_controller_history",
    ] {
        let (mut c, g, v, _) = fixture();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(seed(&tx, &v, &g).is_err());
        }
        for table in [
            "activation_controller_intents",
            "activation_controller_records",
            "activation_controller_history",
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
fn controller_ignored_current_or_history_receipt_keeps_previous_state() {
    for (table, event) in [
        ("activation_controller_records", "UPDATE"),
        ("activation_controller_history", "INSERT"),
    ] {
        let (mut c, g, v, r) = fixture();
        seed(&c, &v, &g).unwrap();
        let next = v.confirmed(r, v.command.issued_at_ms + 10).unwrap();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE {event} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        assert!(append(&mut c, &v, &next, &g).is_err());
        assert_eq!(get(&c, &v, &g).unwrap(), v);
    }
}
#[test]
fn controller_record_cannot_be_seeded_from_confirmation_or_another_graph() {
    let (c, mut g, v, r) = fixture();
    let confirmed = v.confirmed(r, v.command.issued_at_ms + 10).unwrap();
    assert!(seed(&c, &confirmed, &g).is_err());
    g.local.intent.attempt_id = Uuid::new_v4().to_string();
    assert!(seed(&c, &v, &g).is_err());
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM activation_controller_intents",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn controller_reader_detects_changed_intent_current_prefix_and_history_independently() {
    for corruption in 0..4 {
        let (mut c, g, v, r) = fixture();
        seed(&c, &v, &g).unwrap();
        let confirmed = v.confirmed(r, v.command.issued_at_ms + 10).unwrap();
        append(&mut c, &v, &confirmed, &g).unwrap();
        match corruption {
            0 => {
                c.execute_batch("DROP TRIGGER activation_controller_intent_immutable;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_controller_intents SET command_id='foreign'",
                    [],
                )
                .unwrap();
            }
            1 => {
                c.execute_batch("DROP TRIGGER activation_controller_history_retained;DELETE FROM activation_controller_history WHERE revision=0;").unwrap();
            }
            2 => {
                c.execute_batch("DROP TRIGGER activation_controller_history_immutable;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_controller_history SET record=?1 WHERE revision=0",
                    [encode(&confirmed).unwrap()],
                )
                .unwrap();
            }
            _ => {
                c.execute_batch("DROP TRIGGER activation_controller_record_identity;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_controller_records SET record=?1",
                    [encode(&v).unwrap()],
                )
                .unwrap();
            }
        }
        assert!(get(&c, &v, &g).is_err(), "mutation {corruption}");
    }
}
#[test]
fn same_key_returns_only_data_and_another_frozen_selection_is_a_key_conflict() {
    let (_c, _g, v, _) = fixture();
    let t = v.command.binding.topology.clone();
    let key = v.command.command_id.clone();
    assert_eq!(existing_for_key(v.clone(), &key, &t).unwrap(), v);
    assert_eq!(
        existing_for_key(v.clone(), "other-key", &t)
            .unwrap_err()
            .code,
        JournalErrorCode::KeyConflict
    );
    let mut changed = t;
    changed.confirmation_endpoint_id = "peer1".into();
    assert_eq!(
        existing_for_key(v, &key, &changed).unwrap_err().code,
        JournalErrorCode::KeyConflict
    );
}

#[test]
fn expired_original_checkpoint_before_or_after_insert_cannot_commit_an_intent() {
    for fail_at in [1, 2] {
        let (mut c, g, v, _) = fixture();
        let mut calls = 0;
        {
            let tx = c.transaction().unwrap();
            let result = seed_checked(&tx, &v, &g, || {
                calls += 1;
                if calls == fail_at {
                    Err(bad("Original monotonic window or custody ended"))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err());
        }
        assert_eq!(calls, fail_at);
        for table in [
            "activation_controller_intents",
            "activation_controller_records",
            "activation_controller_history",
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
fn cold_sqlite_observation_retains_uncertainty_without_reissuing() {
    let (mut c, g, v, _) = fixture();
    seed(&c, &v, &g).unwrap();
    let unknown = v
        .indeterminate(
            "No authenticated receipt".into(),
            v.command.deadline_at_ms + 1,
        )
        .unwrap();
    append(&mut c, &v, &unknown, &g).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("copy.sqlite");
    {
        let mut dest = Connection::open(&path).unwrap();
        rusqlite::backup::Backup::new(&c, &mut dest)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let cold = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let data = get(&cold, &v, &g).unwrap();
    assert_eq!(data, unknown);
    assert!(matches!(
        data.state,
        ActivationControllerState::Indeterminate { .. }
    ));
    assert!(data.advance(&v).is_err());
    assert!(data
        .indeterminate("retry".into(), data.updated_at_ms + 1)
        .is_err());
}
fn old_twenty() -> (tempfile::TempDir, UpdateJournal, String) {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let j = UpdateJournal::open(directory.path(), &scope).unwrap();
    // Synthetic, originally empty latest tables only; not actual retained data.
    j.connection.execute_batch("DROP TABLE IF EXISTS bootstrap_reservations;DROP TABLE IF EXISTS keeper_selections;DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE activation_controller_history;DROP TABLE activation_controller_records;DROP TABLE activation_controller_intents;PRAGMA user_version=20;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    (directory, j, scope)
}
#[test]
fn updater_twenty_to_current_backs_up_retains_root_dedup_and_fences_old_owner() {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let before = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "controller-schema-migration".into(),
        source: winbridge_protocol::update::UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: before.installation_id,
            scope_sha256: scope.clone(),
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let root = old.accept(&owner, &request, &request.expected).unwrap();
    // Build metadata with its own current SDK first, then form the synthetic
    // old schema. A new SDK must refuse writes on an unmigrated old database.
    old.connection.execute_batch("DROP TABLE IF EXISTS bootstrap_reservations;DROP TABLE IF EXISTS keeper_selections;DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE activation_controller_history;DROP TABLE activation_controller_records;DROP TABLE activation_controller_intents;PRAGMA user_version=20;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    assert!(
        UpdateJournal::inspect_activation_controller(directory.path(), &root.update_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(old.info().unwrap(), before);
    assert_eq!(
        UpdateJournal::open(directory.path(), &scope)
            .err()
            .unwrap()
            .code,
        JournalErrorCode::OwnerBusy
    );
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(directory.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.scope_sha256, before.scope_sha256);
    assert_eq!(after.active_update_id, before.active_update_id);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&root.update_id).unwrap(), root);
    assert_eq!(
        old.check_source_authority(&owner).unwrap_err().code,
        JournalErrorCode::StaleOwner
    );
    let backups: Vec<_> = fs::read_dir(directory.path())
        .unwrap()
        .map(|x| x.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v20-")
                && p.extension().is_some_and(|v| v == "sqlite")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let archived =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&archived).unwrap(), 20);
    assert_eq!(info(&archived).unwrap(), before);
    assert_eq!(record(&archived, &root.update_id).unwrap(), root);
    assert_eq!(
        archived
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let fresh = new.acquire_owner().unwrap();
    assert_eq!(
        new.accept(&fresh, &request, &request.expected).unwrap(),
        root
    );
    assert!(new
        .activation_controller(&root.update_id)
        .unwrap()
        .is_none());
}
#[test]
fn failed_controller_migration_preserves_schema_owner_and_a_foreign_table() {
    let (directory, mut old, scope) = old_twenty();
    old.connection.execute_batch("CREATE TABLE activation_controller_history(user_data TEXT);INSERT INTO activation_controller_history VALUES('preserve');").unwrap();
    let before = old.info().unwrap();
    assert_eq!(
        UpdateJournal::open(directory.path(), &scope)
            .err()
            .unwrap()
            .code,
        JournalErrorCode::Storage
    );
    assert_eq!(old.info().unwrap(), before);
    assert_eq!(
        old.connection
            .query_row(
                "SELECT user_data FROM activation_controller_history",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "preserve"
    );
    assert_eq!(old.connection.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('activation_controller_intents','activation_controller_records','activation_controller_intent_immutable')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
