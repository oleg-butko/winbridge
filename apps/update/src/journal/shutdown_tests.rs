//! Ledger-only tests. Synthetic metadata never proves executable health or OS exit.
use super::*;
fn ledger(c: &Connection) -> (ShutdownRecord, WorkerShutdownReceipt) {
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE drain_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    migrate(c).unwrap();
    let (intent, receipt, _) = crate::shutdown::tests::fixture();
    let initial = ShutdownRecord::prepare(intent).unwrap();
    let id = &initial.intent.binding.update_id;
    c.execute("INSERT INTO drain_intents VALUES(?1)", [id])
        .unwrap();
    seed(c, &initial).unwrap();
    (initial, receipt)
}
fn seed(c: &Connection, initial: &ShutdownRecord) -> Result<()> {
    let id = &initial.intent.binding.update_id;
    one(c
        .execute(
            "INSERT INTO shutdown_intents VALUES(?1,?2)",
            params![id, encode(&initial.intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO shutdown_records VALUES(?1,0,?2)",
            params![id, encode(initial)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO shutdown_history VALUES(?1,0,?2)",
            params![id, encode(initial)?],
        )
        .map_err(storage)?)?;
    Ok(())
}
fn write(c: &mut Connection, old: &ShutdownRecord, next: &ShutdownRecord) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next)?;
    tx.commit().map_err(storage)
}
#[test]
fn shutdown_cas_retains_immutable_intent_receipt_and_causal_history() {
    let mut c = Connection::open_in_memory().unwrap();
    let (initial, receipt) = ledger(&c);
    let issued = initial.issue(101).unwrap();
    write(&mut c, &initial, &issued).unwrap();
    assert!(write(&mut c, &initial, &issued).is_err());
    let ack = issued.acknowledge(201, receipt.clone()).unwrap();
    let mut changed = ack.clone();
    if let ShutdownDeliveryState::Acknowledged { receipt, .. } = &mut changed.delivery {
        receipt.process.birth.push('1');
    }
    assert!(write(&mut c, &issued, &changed).is_err());
    write(&mut c, &issued, &ack).unwrap();
    assert_eq!(delivery(&c, &initial.intent).unwrap(), ack);
    assert_eq!(ack.acknowledge(202, receipt).unwrap(), ack);
    let id = &initial.intent.binding.update_id;
    assert!(c
        .execute(
            "UPDATE shutdown_intents SET record=record WHERE id=?1",
            [id]
        )
        .is_err());
    assert!(c
        .execute("DELETE FROM shutdown_records WHERE id=?1", [id])
        .is_err());
    assert!(c
        .execute("DELETE FROM shutdown_history WHERE id=?1", [id])
        .is_err());
    assert!(c
        .execute(
            "UPDATE shutdown_history SET record=record WHERE id=?1",
            [id]
        )
        .is_err());
    // This ledger-only write models data retention; it cannot acquire the
    // private physical guard required by the public combined journal API.
    let physical = crate::shutdown::ShutdownExitEvidence {
        process: initial.intent.binding.worker.process.clone(),
        state: initial.intent.binding.worker_state.clone(),
        runtime_identity: initial
            .intent
            .binding
            .loaded_worker
            .runtime_identity
            .clone(),
        exact_process_exited: true,
        instance_lock_acquired: true,
        observed_at_ms: 300,
    };
    let exited = ack.confirm_exit(301, physical).unwrap();
    write(&mut c, &ack, &exited).unwrap();
    assert_eq!(delivery(&c, &initial.intent).unwrap(), exited);
    assert!(c
        .execute(
            "UPDATE shutdown_records SET revision=revision+1,record=record",
            []
        )
        .is_err());
}
#[test]
fn shutdown_reopen_preserves_uncertainty_and_reconciles_exact_ack_without_resend() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ledger.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let (initial, receipt) = ledger(&c);
    let issued = initial.issue(101).unwrap();
    write(&mut c, &initial, &issued).unwrap();
    drop(c);
    let mut c = Connection::open(path).unwrap();
    let issued = delivery(&c, &initial.intent).unwrap();
    assert!(issued.issue(102).is_err());
    let unknown = issued.uncertain(102, "ACK lost".into()).unwrap();
    write(&mut c, &issued, &unknown).unwrap();
    assert!(delivery(&c, &initial.intent).unwrap().issue(103).is_err());
    let ack = unknown.acknowledge(201, receipt).unwrap();
    write(&mut c, &unknown, &ack).unwrap();
    assert_eq!(delivery(&c, &initial.intent).unwrap(), ack);
    assert_eq!(
        c.query_row("SELECT count(*) FROM shutdown_history", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4
    );
}
#[test]
fn shutdown_ignored_history_write_rolls_back_ack_and_root_probe() {
    let mut c = Connection::open_in_memory().unwrap();
    let (initial, receipt) = ledger(&c);
    let issued = initial.issue(101).unwrap();
    write(&mut c, &initial, &issued).unwrap();
    let ack = issued.acknowledge(201, receipt).unwrap();
    c.execute_batch("CREATE TABLE parent_probe(phase TEXT);INSERT INTO parent_probe VALUES('stopping');CREATE TRIGGER deny_shutdown_ack BEFORE INSERT ON shutdown_history WHEN json_extract(NEW.record,'$.delivery.state')='acknowledged' BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE parent_probe SET phase='replacing'", [])
            .unwrap();
        assert!(persist(&tx, &issued, &ack).is_err());
    }
    assert_eq!(delivery(&c, &initial.intent).unwrap(), issued);
    assert_eq!(
        c.query_row("SELECT phase FROM parent_probe", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "stopping"
    );
    let unknown = issued.uncertain(202, "ACK storage denied".into()).unwrap();
    write(&mut c, &issued, &unknown).unwrap();
    assert_eq!(delivery(&c, &initial.intent).unwrap(), unknown);
}
#[test]
fn shutdown_ignored_exit_write_keeps_ack_and_parent_until_atomic_retry() {
    let mut c = Connection::open_in_memory().unwrap();
    let (initial, receipt) = ledger(&c);
    let issued = initial.issue(101).unwrap();
    write(&mut c, &initial, &issued).unwrap();
    let ack = issued.acknowledge(201, receipt).unwrap();
    write(&mut c, &issued, &ack).unwrap();
    let physical = crate::shutdown::ShutdownExitEvidence {
        process: initial.intent.binding.worker.process.clone(),
        state: initial.intent.binding.worker_state.clone(),
        runtime_identity: initial
            .intent
            .binding
            .loaded_worker
            .runtime_identity
            .clone(),
        exact_process_exited: true,
        instance_lock_acquired: true,
        observed_at_ms: 300,
    };
    let exited = ack.confirm_exit(301, physical).unwrap();
    // Data ledger probe only, not an OS proof. The consuming public API must
    // additionally hold ShutdownExitGuard across the real root transaction.
    c.execute_batch("CREATE TABLE parent_probe(phase TEXT);INSERT INTO parent_probe VALUES('stopping');CREATE TRIGGER deny_shutdown_exit BEFORE INSERT ON shutdown_history WHEN json_extract(NEW.record,'$.delivery.state')='exited' BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE parent_probe SET phase='replacing'", [])
            .unwrap();
        assert!(persist(&tx, &ack, &exited).is_err());
    }
    assert_eq!(delivery(&c, &initial.intent).unwrap(), ack);
    assert_eq!(
        c.query_row("SELECT phase FROM parent_probe", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "stopping"
    );
    c.execute_batch("DROP TRIGGER deny_shutdown_exit;").unwrap();
    let tx = c.transaction().unwrap();
    tx.execute("UPDATE parent_probe SET phase='replacing'", [])
        .unwrap();
    persist(&tx, &ack, &exited).unwrap();
    tx.commit().unwrap();
    assert_eq!(delivery(&c, &initial.intent).unwrap(), exited);
}
#[test]
fn shutdown_ignored_intent_history_does_not_commit_prepared_or_parent_probe() {
    let mut c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE drain_intents(id TEXT PRIMARY KEY);CREATE TABLE parent_probe(phase TEXT);INSERT INTO parent_probe VALUES('quiesced');").unwrap();
    migrate(&c).unwrap();
    let (intent, _, _) = crate::shutdown::tests::fixture();
    let initial = ShutdownRecord::prepare(intent).unwrap();
    let id = &initial.intent.binding.update_id;
    c.execute("INSERT INTO drain_intents VALUES(?1)", [id])
        .unwrap();
    c.execute_batch("CREATE TRIGGER deny_shutdown_intent BEFORE INSERT ON shutdown_history BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE parent_probe SET phase='stopping'", [])
            .unwrap();
        assert!(seed(&tx, &initial).is_err());
    }
    assert_eq!(
        c.query_row("SELECT count(*) FROM shutdown_intents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT phase FROM parent_probe", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "quiesced"
    );
}
fn request(j: &mut UpdateJournal) -> UpdateRequest {
    let i = j.info().unwrap();
    UpdateRequest {
        contract_version: 1,
        idempotency_key: "shutdown-migration".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            source_commit: "c".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: i.installation_id,
            scope_sha256: i.scope_sha256,
            worker_build_id: "old-build".into(),
            worker_config_sha256: "d".repeat(64),
            baseline_manifest_sha256: "e".repeat(64),
        },
    }
}
#[test]
fn schema_eleven_migration_retains_history_backs_up_fences_owner_and_invents_no_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    let a = j.acquire_owner().unwrap();
    let req = request(&mut j);
    let before = j.accept(&a, &req, &req.expected).unwrap();
    let epoch = a.epoch();
    let installation = j.info().unwrap().installation_id;
    assert_eq!(j.info().unwrap().schema_version, UPDATE_SCHEMA_VERSION);
    // Migration cannot race the real owner lock.
    j.connection.execute_batch("DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history; DROP TABLE activation_records; DROP TABLE activation_intents; DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; DROP TABLE shutdown_history;DROP TABLE shutdown_records;DROP TABLE shutdown_intents;PRAGMA user_version=11;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    assert!(UpdateJournal::open(root.path(), &scope).is_err());
    assert_eq!(j.info().unwrap().owner_epoch, epoch);
    drop(a);
    drop(j);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    assert_eq!(j.info().unwrap().schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(j.info().unwrap().installation_id, installation);
    assert!(j.info().unwrap().owner_epoch > epoch);
    assert_eq!(j.get(&before.update_id).unwrap(), before);
    assert_eq!(read(&j.connection, &before.update_id).unwrap(), None);
    let backup = fs::read_dir(root.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v11-")
        })
        .unwrap();
    let c = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 11);
    assert_eq!(record(&c, &before.update_id).unwrap(), before);
    assert_eq!(
        c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
#[test]
fn raw_lifecycle_cannot_stop_or_approve_exit_without_durable_driver_and_os_proof() {
    let root = tempfile::tempdir().unwrap();
    let mut j = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
    let a = j.acquire_owner().unwrap();
    let req = request(&mut j);
    let mut root = j.accept(&a, &req, &req.expected).unwrap();
    for (n, event) in [
        UpdateEvent::BeginPreparation,
        UpdateEvent::SourceVerified,
        UpdateEvent::BuildSucceeded,
    ]
    .into_iter()
    .enumerate()
    {
        root = j
            .advance(
                &a,
                &root.update_id,
                &UpdateCommand {
                    idempotency_key: format!("metadata-{n}"),
                    expected_revision: root.revision,
                    event,
                    reason: "Metadata-only fixture".into(),
                },
            )
            .unwrap();
    }
    for (n, event, driver) in [
        (0, UpdateEvent::CandidateAccepted, Driver::Health),
        (1, UpdateEvent::BeginDrain, Driver::Drain),
        (
            2,
            UpdateEvent::Quiesce {
                dispatch_closed: true,
                accepted_operations: 0,
                physical_owners: 0,
                uncertain_owners: 0,
            },
            Driver::Drain,
        ),
    ] {
        root = j
            .advance_with_driver(
                &a,
                &root.update_id,
                &UpdateCommand {
                    idempotency_key: format!("trusted-metadata-{n}"),
                    expected_revision: root.revision,
                    event,
                    reason: "Synthetic metadata; no physical approval".into(),
                },
                driver,
                |_, _| Ok(()),
            )
            .unwrap();
    }
    let mut command = UpdateCommand {
        idempotency_key: "raw-shutdown".into(),
        expected_revision: root.revision,
        event: UpdateEvent::BeginShutdown,
        reason: "Caller requests stop".into(),
    };
    assert!(j.advance(&a, &root.update_id, &command).is_err());
    assert_eq!(j.get(&root.update_id).unwrap(), root);
    root = j
        .advance_with_driver(&a, &root.update_id, &command, Driver::Shutdown, |_, _| {
            Ok(())
        })
        .unwrap();
    command.expected_revision = root.revision;
    command.idempotency_key = "raw-exit".into();
    command.event = UpdateEvent::ShutdownVerified;
    assert!(j.advance(&a, &root.update_id, &command).is_err());
    assert_eq!(j.get(&root.update_id).unwrap(), root);
    // Private metadata-only fixture, not OS exit evidence. Even a caller that
    // reaches Replacing cannot declare files/activation/controller verified.
    root = j
        .advance_with_driver(&a, &root.update_id, &command, Driver::Shutdown, |_, _| {
            Ok(())
        })
        .unwrap();
    let transitions: i64 = j
        .connection
        .query_row("SELECT COUNT(*) FROM transitions", [], |r| r.get(0))
        .unwrap();
    for (n, event) in [
        UpdateEvent::FilesVerified,
        UpdateEvent::LocalHealthVerified,
        UpdateEvent::ControllerConfirmed,
    ]
    .into_iter()
    .enumerate()
    {
        let command = UpdateCommand {
            idempotency_key: format!("raw-post-cutover-{n}"),
            expected_revision: root.revision,
            event,
            reason: "No physical runtime proof".into(),
        };
        let error = j.advance(&a, &root.update_id, &command).unwrap_err();
        assert_eq!(error.code, JournalErrorCode::InvalidRequest);
        assert!(error.message.contains("trusted drivers"));
        assert_eq!(j.get(&root.update_id).unwrap(), root);
        assert_eq!(
            j.connection
                .query_row("SELECT COUNT(*) FROM transitions", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            transitions
        );
    }
}
