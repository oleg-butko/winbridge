//! Portable SQL/FSM checks. No test reconstructs a live guard from saved data.
use super::*;
use crate::activation::{service::ActivationServiceState, ActivationWriterState};
use winbridge_protocol::{update::WorkerServiceActivationReceipt, WorkerRuntimeCompatibility};

fn fixture() -> (
    Connection,
    ActivationRecord,
    ActivationWriterRecord,
    ActivationServiceRecord,
) {
    let (physical, writer) = crate::activation::writer_view_fixture();
    let command = service::prepare_command(
        &physical,
        &writer,
        &Uuid::new_v4().to_string(),
        writer.updated_at_ms + 1,
    )
    .unwrap();
    let value = ActivationServiceRecord::issued(command).unwrap();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE activation_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    c.execute(
        "INSERT INTO activation_intents VALUES(?1)",
        [&physical.intent.source.update_id],
    )
    .unwrap();
    activation_writer::migrate(&c).unwrap();
    let issued = ActivationWriterRecord::issued(writer.command.clone()).unwrap();
    activation_writer::seed(
        &c,
        &issued,
        &physical,
        &writer.command.binding.expected_authority,
    )
    .unwrap();
    activation_writer::persist(
        &c,
        &issued,
        &writer,
        &physical,
        &writer.command.binding.expected_authority,
    )
    .unwrap();
    migrate(&c).unwrap();
    (c, physical, writer, value)
}

fn receipt(value: &ActivationServiceRecord) -> WorkerServiceActivationReceipt {
    let command = &value.command;
    let b = &command.binding;
    let w = &b.writer_command.binding;
    WorkerServiceActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: w.update_id.clone(),
        attempt_id: w.attempt_id.clone(),
        command_sha256: service::command_sha256(command).unwrap(),
        authority: b.writer_receipt.authority.clone(),
        compatibility: WorkerRuntimeCompatibility {
            schema_version: 1,
            kind: "worker_runtime_compatibility".into(),
            identity: b.expected_worker.clone(),
            context: w.physical.context.clone(),
            generation: w.physical.generation.clone(),
            worker_config_sha256: w.worker_config_sha256.clone(),
            worker_config_schema: b.worker_config_schema,
            worker_journal_schema: w.worker_journal_schema,
            updater_journal_schema: w.updater_journal_schema,
            protocol_version: b.protocol_version,
            observed_at_ms: command.issued_at_ms + 1,
        },
    }
}

fn append(
    c: &mut Connection,
    old: &ActivationServiceRecord,
    next: &ActivationServiceRecord,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next, physical, writer)?;
    tx.commit().map_err(storage)
}

fn read_value(
    c: &Connection,
    p: &ActivationRecord,
    w: &ActivationWriterRecord,
) -> Result<ActivationServiceRecord> {
    ledger(c, &p.intent.source.update_id, p, w)
}

#[test]
fn service_history_preserves_late_fact_without_new_epoch_window_or_reissue() {
    let (mut c, p, w, v) = fixture();
    seed(&c, &v, &p, &w).unwrap();
    let at = v.command.deadline_at_ms + 1000;
    let unknown = v.indeterminate("Private ACK lost".into(), at).unwrap();
    append(&mut c, &v, &unknown, &p, &w).unwrap();
    let confirmed = unknown.confirmed(receipt(&v), at + 1).unwrap();
    append(&mut c, &unknown, &confirmed, &p, &w).unwrap();
    assert_eq!(read_value(&c, &p, &w).unwrap(), confirmed);
    assert_eq!(confirmed.command, v.command);
    assert_eq!(
        receipt(&v).authority,
        v.command.binding.writer_receipt.authority
    );
    assert_eq!(confirmed.revision, 2);
    assert_eq!(
        append(&mut c, &v, &unknown, &p, &w).unwrap_err().code,
        JournalErrorCode::RevisionConflict
    );
    assert!(confirmed.indeterminate("retry".into(), at + 2).is_err());
    assert!(confirmed.confirmed(receipt(&v), at + 2).is_err());
    assert!(seed(&c, &v, &p, &w).is_err());
}

#[test]
fn service_ignored_admission_rolls_back_every_row() {
    for table in [
        "activation_service_intents",
        "activation_service_records",
        "activation_service_history",
    ] {
        let (mut c, p, w, v) = fixture();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(seed(&tx, &v, &p, &w).is_err());
        }
        for name in [
            "activation_service_intents",
            "activation_service_records",
            "activation_service_history",
        ] {
            assert_eq!(
                c.query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}

#[test]
fn service_ignored_current_or_history_append_cannot_ack_partial_success() {
    for (event, table) in [
        ("UPDATE", "activation_service_records"),
        ("INSERT", "activation_service_history"),
    ] {
        let (mut c, p, w, v) = fixture();
        seed(&c, &v, &p, &w).unwrap();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE {event} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        let next = v.confirmed(receipt(&v), v.updated_at_ms + 1).unwrap();
        assert!(append(&mut c, &v, &next, &p, &w).is_err());
        assert_eq!(read_value(&c, &p, &w).unwrap(), v);
    }
}

#[test]
fn service_sql_guards_retain_intent_command_history_and_confirmation() {
    let (mut c, p, w, v) = fixture();
    seed(&c, &v, &p, &w).unwrap();
    let next = v.confirmed(receipt(&v), v.updated_at_ms + 1).unwrap();
    append(&mut c, &v, &next, &p, &w).unwrap();
    for sql in [
        "UPDATE activation_service_intents SET command=command",
        "DELETE FROM activation_service_intents",
        "UPDATE activation_service_records SET revision=revision+1",
        "DELETE FROM activation_service_records",
        "UPDATE activation_service_history SET record=record",
        "DELETE FROM activation_service_history",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    let new_key =
        service::prepare_command(&p, &w, &Uuid::new_v4().to_string(), v.command.issued_at_ms)
            .unwrap();
    assert!(seed(
        &c,
        &ActivationServiceRecord::issued(new_key).unwrap(),
        &p,
        &w
    )
    .is_err());
    assert_eq!(read_value(&c, &p, &w).unwrap(), next);
}

#[test]
fn service_readback_rejects_foreign_physical_or_changed_writer_confirmation() {
    let (c, p, w, v) = fixture();
    seed(&c, &v, &p, &w).unwrap();
    let (_, foreign, _, _) = fixture();
    assert!(ledger(&c, &p.intent.source.update_id, &foreign, &w).is_err());
    let mut changed = w.clone();
    changed.updated_at_ms += 1;
    changed.validate().unwrap();
    assert!(read_value(&c, &p, &changed).is_err());
    let issued = ActivationWriterRecord::issued(w.command.clone()).unwrap();
    assert!(matches!(issued.state, ActivationWriterState::Issued {}));
    assert!(read_value(&c, &p, &issued).is_err());
}

#[test]
fn service_typed_readback_detects_index_rewrite_history_gap_and_forged_result() {
    for mutation in 0..4 {
        let (c, p, w, v) = fixture();
        seed(&c, &v, &p, &w).unwrap();
        match mutation {
            0 => {
                c.execute_batch("DROP TRIGGER activation_service_intent_immutable;")
                    .unwrap();
                c.execute(
                    "UPDATE activation_service_intents SET command_id=?1",
                    [Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            1 => {
                c.execute_batch("DROP TRIGGER activation_service_history_retained;DELETE FROM activation_service_history;").unwrap();
            }
            2 => {
                c.execute_batch("DROP TRIGGER activation_service_history_immutable;")
                    .unwrap();
                let changed = v
                    .indeterminate("Rewritten past".into(), v.updated_at_ms + 1)
                    .unwrap();
                c.execute(
                    "UPDATE activation_service_history SET record=?1 WHERE revision=0",
                    [encode(&changed).unwrap()],
                )
                .unwrap();
            }
            _ => {
                c.execute_batch("DROP TRIGGER activation_service_record_identity;")
                    .unwrap();
                let mut changed = serde_json::to_value(&v).unwrap();
                changed["state"]["canStartService"] = true.into();
                c.execute(
                    "UPDATE activation_service_records SET record=?1",
                    [changed.to_string()],
                )
                .unwrap();
            }
        }
        assert!(read_value(&c, &p, &w).is_err(), "mutation {mutation}");
    }
}

#[test]
fn cold_sqlite_reopen_keeps_saved_facts_without_offering_another_delivery() {
    let (source, p, w, v) = fixture();
    seed(&source, &v, &p, &w).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("service.sqlite");
    let mut c = Connection::open(&path).unwrap();
    rusqlite::backup::Backup::new(&source, &mut c)
        .unwrap()
        .run_to_completion(64, Duration::from_millis(10), None)
        .unwrap();
    let unknown = v
        .indeterminate("ACK lost".into(), v.updated_at_ms + 1)
        .unwrap();
    append(&mut c, &v, &unknown, &p, &w).unwrap();
    drop(c);
    let c = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = read_value(&c, &p, &w).unwrap();
    assert_eq!(saved, unknown);
    assert!(matches!(
        saved.state,
        ActivationServiceState::Indeterminate { .. }
    ));
    assert!(saved
        .indeterminate("send again".into(), saved.updated_at_ms + 1)
        .is_err());
    assert!(saved.advance(&v).is_err());
}

#[test]
fn service_migration_preserves_confirmed_writer_rows_and_worker_authority() {
    let (c, p, w, _) = fixture();
    c.execute_batch("DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;PRAGMA user_version=19;").unwrap();
    let before: String = c
        .query_row("SELECT record FROM activation_writer_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    migrate(&c).unwrap();
    let after: String = c
        .query_row("SELECT record FROM activation_writer_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    let decoded: ActivationWriterRecord = decode(&after).unwrap();
    assert_eq!(decoded, w);
    service::prepare_command(
        &p,
        &decoded,
        &Uuid::new_v4().to_string(),
        w.updated_at_ms + 1,
    )
    .unwrap();
}

#[test]
fn updater_nineteen_to_twenty_backs_up_fences_owner_and_retains_root_dedup() {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let before = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "service-migration".into(),
        source: winbridge_protocol::update::UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: before.installation_id.clone(),
            scope_sha256: scope.clone(),
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let saved = old.accept(&owner, &request, &request.expected).unwrap();
    old.connection.execute_batch("DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;PRAGMA user_version=19;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    assert_eq!(
        UpdateJournal::open(directory.path(), &scope)
            .err()
            .unwrap()
            .code,
        JournalErrorCode::OwnerBusy
    );
    assert_eq!(UpdateJournal::inspect(directory.path()).unwrap(), before);
    assert!(
        UpdateJournal::inspect_activation_service(directory.path(), &saved.update_id)
            .unwrap()
            .is_none()
    );
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(directory.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, 20);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&saved.update_id).unwrap(), saved);
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
                .starts_with("updates-v19-")
                && p.extension().is_some_and(|s| s == "sqlite")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let c = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 19);
    assert_eq!(info(&c).unwrap(), before);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
    let fresh = new.acquire_owner().unwrap();
    assert_eq!(
        new.accept(&fresh, &request, &request.expected).unwrap(),
        saved
    );
    assert!(new.activation_service(&saved.update_id).unwrap().is_none());
    assert!(
        UpdateJournal::inspect_activation_service(directory.path(), &saved.update_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn failed_service_migration_keeps_prior_schema_owner_and_foreign_table() {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    old.connection.execute_batch("DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;CREATE TABLE activation_service_history(user_data TEXT);INSERT INTO activation_service_history VALUES('preserve');PRAGMA user_version=19;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    assert_eq!(
        UpdateJournal::open(directory.path(), &scope)
            .err()
            .unwrap()
            .code,
        JournalErrorCode::Storage
    );
    assert_eq!(old.info().unwrap(), before);
    let value: String = old
        .connection
        .query_row(
            "SELECT user_data FROM activation_service_history",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "preserve");
    let partial: i64 = old.connection.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('activation_service_intents','activation_service_records','activation_service_intent_immutable')", [], |r| r.get(0)).unwrap();
    assert_eq!(partial, 0);
}
