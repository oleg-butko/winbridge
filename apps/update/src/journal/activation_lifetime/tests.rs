//! SQL/FSM primitives and synthetic migration, never kernel custody or admission.
use super::*;
use crate::activation::lifetime::ActivationLifetimeState;
use winbridge_protocol::update::WorkerLifetimeActivationReceipt;

fn fixture() -> (
    Connection,
    ActivationControllerSnapshot,
    ActivationLifetimeRecord,
    WorkerLifetimeActivationReceipt,
) {
    let (graph, command, receipt) = lifetime::test_fixture();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(
        "PRAGMA foreign_keys=ON;PRAGMA application_id=1463965008;CREATE TABLE activation_controller_records(id TEXT PRIMARY KEY);",
    )
    .unwrap();
    c.execute(
        "INSERT INTO activation_controller_records VALUES(?1)",
        [&command.binding.update_id],
    )
    .unwrap();
    migrate(&c).unwrap();
    (
        c,
        graph,
        ActivationLifetimeRecord::issued(command).unwrap(),
        receipt,
    )
}
fn append(
    c: &mut Connection,
    old: &ActivationLifetimeRecord,
    next: &ActivationLifetimeRecord,
    graph: &ActivationControllerSnapshot,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next, graph)?;
    tx.commit().map_err(storage)
}
fn get(
    c: &Connection,
    old: &ActivationLifetimeRecord,
    graph: &ActivationControllerSnapshot,
) -> Result<ActivationLifetimeRecord> {
    ledger(c, &old.command.binding.update_id, graph)
}
fn rows(c: &Connection) -> Vec<i64> {
    [
        "activation_lifetime_intents",
        "activation_lifetime_records",
        "activation_lifetime_history",
    ]
    .iter()
    .map(|table| {
        c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn root_history(c: &Connection, id: &str) -> Result<Vec<(i64, String, String)>> {
    let mut q = c.prepare("SELECT sequence,transition,record FROM transitions WHERE update_id=?1 ORDER BY sequence").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    Ok(rows)
}

#[test]
fn lifetime_causal_history_retains_late_fact_and_rejects_terminal_replay_or_stale_cas() {
    let (mut c, g, v, receipt) = fixture();
    seed(&c, &v, &g).unwrap();
    let unknown = v
        .indeterminate("ACK unknown".into(), v.command.deadline_at_ms + 1)
        .unwrap();
    append(&mut c, &v, &unknown, &g).unwrap();
    let confirmed = unknown
        .confirmed(receipt, unknown.updated_at_ms + 1)
        .unwrap();
    append(&mut c, &unknown, &confirmed, &g).unwrap();
    assert_eq!(get(&c, &v, &g).unwrap(), confirmed);
    assert_eq!(confirmed.command, v.command);
    assert!(append(&mut c, &v, &unknown, &g).is_err());
    assert!(append(&mut c, &confirmed, &confirmed, &g).is_err());
    assert_eq!(rows(&c), [1, 1, 3]);
}

#[test]
fn lifetime_sql_preserves_keys_scope_revision_and_all_history_rows() {
    let (c, g, v, _) = fixture();
    seed(&c, &v, &g).unwrap();
    // SQLite TEXT PRIMARY KEY alone permits NULL, and NULL foreign keys are
    // exempt from lookup. Such unaddressable rows must not enter this ledger.
    let mut null_command = serde_json::to_value(&v.command).unwrap();
    let another_key = Uuid::new_v4().to_string();
    null_command["commandId"] = serde_json::json!(another_key);
    null_command["binding"]["updateId"] = serde_json::Value::Null;
    assert!(c
        .execute(
            "INSERT INTO activation_lifetime_intents VALUES(NULL,?1,?2)",
            params![another_key, serde_json::to_string(&null_command).unwrap()]
        )
        .is_err());
    let mut null_record = serde_json::to_value(&v).unwrap();
    null_record["command"] = null_command;
    assert!(c
        .execute(
            "INSERT INTO activation_lifetime_records VALUES(NULL,0,?1)",
            [serde_json::to_string(&null_record).unwrap()]
        )
        .is_err());
    for sql in [
        "UPDATE activation_lifetime_intents SET command=command",
        "DELETE FROM activation_lifetime_intents",
        "UPDATE activation_lifetime_records SET revision=revision+1",
        "DELETE FROM activation_lifetime_records",
        "UPDATE activation_lifetime_history SET record=record",
        "DELETE FROM activation_lifetime_history",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    c.execute_batch("DROP TRIGGER activation_lifetime_intent_immutable;DROP TRIGGER activation_lifetime_record_identity;DROP TRIGGER activation_lifetime_history_immutable;").unwrap();
    for sql in [
        "UPDATE activation_lifetime_intents SET command_id='other'",
        "UPDATE activation_lifetime_records SET revision=1",
        "UPDATE activation_lifetime_records SET id='other'",
        "UPDATE activation_lifetime_history SET revision=1",
        "UPDATE activation_lifetime_history SET id='other'",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(get(&c, &v, &g).unwrap(), v);
}

#[test]
fn lifetime_ignored_inserts_or_append_never_leave_partial_acceptance() {
    for table in [
        "activation_lifetime_intents",
        "activation_lifetime_records",
        "activation_lifetime_history",
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
        assert_eq!(rows(&c), [0, 0, 0], "{table}");
    }
    for (table, event) in [
        ("activation_lifetime_records", "UPDATE"),
        ("activation_lifetime_history", "INSERT"),
    ] {
        let (mut c, g, v, r) = fixture();
        seed(&c, &v, &g).unwrap();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE {event} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        let next = v.confirmed(r, v.updated_at_ms + 2).unwrap();
        assert!(append(&mut c, &v, &next, &g).is_err());
        assert_eq!(get(&c, &v, &g).unwrap(), v);
        assert_eq!(rows(&c), [1, 1, 1]);
    }
}

#[test]
fn lifetime_future_custody_checkpoints_cannot_commit_after_original_window_loss() {
    for fail_at in [1, 2] {
        let (mut c, g, v, _) = fixture();
        let mut calls = 0;
        {
            let tx = c.transaction().unwrap();
            assert!(seed_checked(&tx, &v, &g, || {
                calls += 1;
                if calls == fail_at {
                    Err(bad("Original window/custody ended"))
                } else {
                    Ok(())
                }
            })
            .is_err());
        }
        assert_eq!(calls, fail_at);
        assert_eq!(rows(&c), [0, 0, 0]);
    }
}

#[test]
fn lifetime_partial_ledger_is_corruption_not_absent_permission_to_reissue() {
    for (table, trigger) in [
        (
            "activation_lifetime_intents",
            "activation_lifetime_intent_retained",
        ),
        (
            "activation_lifetime_records",
            "activation_lifetime_record_retained",
        ),
        (
            "activation_lifetime_history",
            "activation_lifetime_history_retained",
        ),
    ] {
        let (c, g, v, _) = fixture();
        seed(&c, &v, &g).unwrap();
        c.execute_batch(&format!(
            "PRAGMA foreign_keys=OFF;DROP TRIGGER {trigger};DELETE FROM {table};"
        ))
        .unwrap();
        assert_eq!(
            read(&c, &v.command.binding.update_id).unwrap_err().code,
            JournalErrorCode::Corrupt,
            "{table}"
        );
    }
}

#[test]
fn lifetime_history_gap_rewrite_and_coordinated_command_change_are_detected() {
    for mutation in 0..4 {
        let (mut c, g, v, r) = fixture();
        seed(&c, &v, &g).unwrap();
        let next = v.confirmed(r, v.updated_at_ms + 2).unwrap();
        append(&mut c, &v, &next, &g).unwrap();
        match mutation {
            0 => {
                c.execute_batch("DROP TRIGGER activation_lifetime_history_retained;DELETE FROM activation_lifetime_history WHERE revision=0;").unwrap();
            }
            1 => {
                let mut bad = v.clone();
                bad.command.deadline_at_ms += 1;
                c.execute_batch("DROP TRIGGER activation_lifetime_history_immutable;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_lifetime_history SET record=?1 WHERE revision=0",
                    [encode(&bad).unwrap()],
                )
                .unwrap();
            }
            2 => {
                c.execute_batch("DROP TRIGGER activation_lifetime_record_identity;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_lifetime_records SET revision=0,record=?1",
                    [encode(&v).unwrap()],
                )
                .unwrap();
            }
            _ => {
                // Rewrite all command copies coherently; the retained graph is
                // independent and must still reject extension of its deadline.
                c.execute_batch("DROP TRIGGER activation_lifetime_intent_immutable;DROP TRIGGER activation_lifetime_record_identity;DROP TRIGGER activation_lifetime_history_immutable;").unwrap();
                let mut changed = v.command.clone();
                changed.deadline_at_ms += 1;
                let changed = ActivationLifetimeRecord::issued(changed).unwrap();
                c.execute(
                    "UPDATE activation_lifetime_intents SET command=?1",
                    [encode(&changed.command).unwrap()],
                )
                .unwrap();
                c.execute(
                    "UPDATE activation_lifetime_records SET revision=0,record=?1",
                    [encode(&changed).unwrap()],
                )
                .unwrap();
                c.execute_batch("DROP TRIGGER activation_lifetime_history_retained;DELETE FROM activation_lifetime_history WHERE revision=1;").unwrap();
                c.execute(
                    "UPDATE activation_lifetime_history SET record=?1 WHERE revision=0",
                    [encode(&changed).unwrap()],
                )
                .unwrap();
            }
        }
        assert!(get(&c, &v, &g).is_err(), "mutation {mutation}");
    }
}

#[test]
fn lifetime_same_key_is_retained_data_and_changed_graph_cannot_rebind() {
    let (c, mut g, v, _) = fixture();
    seed(&c, &v, &g).unwrap();
    assert_eq!(
        existing_for_key(v.clone(), &v.command.command_id).unwrap(),
        v
    );
    assert_eq!(
        existing_for_key(v.clone(), "different-key")
            .unwrap_err()
            .code,
        JournalErrorCode::KeyConflict
    );
    assert!(seed(&c, &v, &g).is_err());
    assert_eq!(rows(&c), [1, 1, 1]);
    g.controller.updated_at_ms += 1;
    assert!(get(&c, &v, &g).is_err());
}

#[test]
fn lifetime_existing_key_cannot_silently_change_controller_policy_selection() {
    let (_, g, v, _) = fixture();
    let topology = g.controller.command.binding.topology.clone();
    assert_eq!(
        existing_for_request(v.clone(), &v.command.command_id, &topology, &g).unwrap(),
        v
    );
    let mut changed = topology;
    changed.confirmation_endpoint_id = "peer1".into();
    assert_eq!(
        existing_for_request(v.clone(), &v.command.command_id, &changed, &g)
            .unwrap_err()
            .code,
        JournalErrorCode::KeyConflict
    );
    assert_eq!(
        existing_for_request(v, "another-key", &changed, &g)
            .unwrap_err()
            .code,
        JournalErrorCode::KeyConflict
    );
}

#[test]
fn cold_lifetime_copy_keeps_unknown_outcome_without_recreating_permission() {
    let (mut c, g, v, _) = fixture();
    seed(&c, &v, &g).unwrap();
    let unknown = v
        .indeterminate("No authenticated ACK".into(), v.command.deadline_at_ms + 1)
        .unwrap();
    append(&mut c, &v, &unknown, &g).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("copy.sqlite");
    {
        let mut dst = Connection::open(&file).unwrap();
        rusqlite::backup::Backup::new(&c, &mut dst)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let cold = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = get(&cold, &v, &g).unwrap();
    assert_eq!(saved, unknown);
    assert!(matches!(
        saved.state,
        ActivationLifetimeState::Indeterminate { .. }
    ));
    assert!(saved.advance(&v).is_err());
    assert!(saved
        .indeterminate("retry".into(), saved.updated_at_ms + 1)
        .is_err());
}

#[test]
fn lifetime_causal_read_transaction_survives_concurrent_confirmation() {
    let (source, g, v, r) = fixture();
    seed(&source, &v, &g).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("wal.sqlite");
    {
        let mut dst = Connection::open(&file).unwrap();
        rusqlite::backup::Backup::new(&source, &mut dst)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let mut reader = Connection::open(&file).unwrap();
    reader.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    let mut writer = Connection::open(&file).unwrap();
    let tx = reader.transaction().unwrap();
    assert!(present(&tx, &v.command.binding.update_id).unwrap());
    let next = v.confirmed(r, v.updated_at_ms + 2).unwrap();
    append(&mut writer, &v, &next, &g).unwrap();
    assert_eq!(get(&tx, &v, &g).unwrap(), v);
    tx.commit().unwrap();
    assert_eq!(get(&reader, &v, &g).unwrap(), next);
}

fn old_twenty_one() -> (tempfile::TempDir, UpdateJournal, String) {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let journal = UpdateJournal::open(directory.path(), &scope).unwrap();
    journal.connection.execute_batch("DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE activation_lifetime_history;DROP TABLE activation_lifetime_records;DROP TABLE activation_lifetime_intents;PRAGMA user_version=21;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    (directory, journal, scope)
}

#[test]
fn lifetime_schema_twenty_one_migration_keeps_backup_root_key_history_and_fences_old_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let before = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "lifetime-migration".into(),
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
    old.connection.execute_batch("DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE activation_lifetime_history;DROP TABLE activation_lifetime_records;DROP TABLE activation_lifetime_intents;PRAGMA user_version=21;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    let history = root_history(&old.connection, &root.update_id).unwrap();
    assert!(
        UpdateJournal::inspect_activation_lifetime(directory.path(), &root.update_id)
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
    assert_eq!(
        after,
        UpdateJournalInfo {
            schema_version: UPDATE_SCHEMA_VERSION,
            owner_epoch: before.owner_epoch + 1,
            ..before.clone()
        }
    );
    assert_eq!(new.get(&root.update_id).unwrap(), root);
    assert_eq!(
        root_history(&new.connection, &root.update_id).unwrap(),
        history
    );
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
                .starts_with("updates-v21-")
                && p.extension().is_some_and(|v| v == "sqlite")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(info(&backup).unwrap(), before);
    assert_eq!(record(&backup, &root.update_id).unwrap(), root);
    assert_eq!(root_history(&backup, &root.update_id).unwrap(), history);
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(new.info().unwrap(), after);
    let owner = new.acquire_owner().unwrap();
    assert_eq!(
        new.accept(&owner, &request, &request.expected).unwrap(),
        root
    );
    assert!(new.activation_lifetime(&root.update_id).unwrap().is_none());
}

#[test]
fn lifetime_failed_migration_preserves_schema_owner_and_foreign_table() {
    let (directory, mut old, scope) = old_twenty_one();
    old.connection.execute_batch("CREATE TABLE activation_lifetime_history(user_data TEXT);INSERT INTO activation_lifetime_history VALUES('preserve');").unwrap();
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
                "SELECT user_data FROM activation_lifetime_history",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "preserve"
    );
    assert_eq!(old.connection.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('activation_lifetime_intents','activation_lifetime_records','activation_lifetime_intent_immutable')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
