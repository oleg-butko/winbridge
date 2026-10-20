//! Ledger/transaction tests. Synthetic rows never claim executable health or OS quiescence.
use super::*;
use winbridge_protocol::update::{
    UpdateInstallationPin, UpdateRequest, UpdateSourcePin, WorkerUpdatePhase, WorkerUpdateSpec,
};

fn ledger(c: &Connection) -> (DrainIntent, WorkerUpdateLease) {
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE candidate_health(id TEXT PRIMARY KEY);")
        .unwrap();
    migrate(c).unwrap();
    let b = DrainBinding {
        update_id: Uuid::new_v4().to_string(),
        launch_id: Uuid::new_v4().to_string(),
        owner_id: Uuid::new_v4().to_string(),
        owner_epoch: 1,
        parent_revision: 10,
        accepted_at_ms: 100,
        source_sha256: "a".repeat(64),
        health_sha256: "b".repeat(64),
    };
    let lease = WorkerUpdateLease {
        spec: WorkerUpdateSpec {
            update_id: b.update_id.clone(),
            instance_id: "ledger".into(),
            journal_incarnation: Uuid::new_v4().to_string(),
            worker_generation: Uuid::new_v4().to_string(),
            updater_owner_id: b.owner_id.clone(),
            updater_epoch: 1,
            source_directory: "metadata-only".into(),
            baseline_manifest_sha256: "c".repeat(64),
            target_manifest_sha256: "d".repeat(64),
            projects: vec!["source".into()],
        },
        revision: 1,
        phase: WorkerUpdatePhase::SourceHeld,
        accepted_at_ms: 1,
        updated_at_ms: 2,
        reason: "Metadata fixture".into(),
    };
    let intent = DrainIntent::from_lease(b, &lease, Uuid::new_v4().to_string()).unwrap();
    c.execute(
        "INSERT INTO candidate_health VALUES(?1)",
        [&intent.binding.update_id],
    )
    .unwrap();
    c.execute(
        "INSERT INTO drain_intents VALUES(?1,?2)",
        params![intent.binding.update_id, encode(&intent).unwrap()],
    )
    .unwrap();
    for ordinal in 0..3 {
        let r = DrainCommandRecord::prepare(&intent, ordinal).unwrap();
        c.execute(
            "INSERT INTO drain_commands VALUES(?1,?2,0,?3)",
            params![r.update_id, ordinal as i64, encode(&r).unwrap()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO drain_history VALUES(?1,?2,0,?3)",
            params![r.update_id, ordinal as i64, encode(&r).unwrap()],
        )
        .unwrap();
    }
    (intent, lease)
}
fn receipt(intent: &DrainIntent, lease: &WorkerUpdateLease, ordinal: usize) -> WorkerUpdateLease {
    WorkerUpdateLease {
        revision: intent.commands[ordinal].expected_revision + 1,
        phase: [
            WorkerUpdatePhase::Draining,
            WorkerUpdatePhase::Quiescing,
            WorkerUpdatePhase::Quiesced,
        ][ordinal],
        reason: intent.commands[ordinal].reason.clone(),
        ..lease.clone()
    }
}
fn write(
    c: &mut Connection,
    intent: &DrainIntent,
    old: &DrainCommandRecord,
    next: &DrainCommandRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, intent, old, next)?;
    tx.commit().map_err(storage)
}
#[test]
fn command_history_cas_and_previous_receipt_prevent_forked_or_reordered_delivery() {
    let mut c = Connection::open_in_memory().unwrap();
    let (intent, lease) = ledger(&c);
    let prepared = commands(&c, &intent).unwrap();
    let early = prepared[1].issue(&intent, 101).unwrap();
    assert!(write(&mut c, &intent, &prepared[1], &early).is_err());
    assert_eq!(commands(&c, &intent).unwrap(), prepared);
    let issued = prepared[0].issue(&intent, 101).unwrap();
    write(&mut c, &intent, &prepared[0], &issued).unwrap();
    assert!(write(&mut c, &intent, &prepared[0], &issued).is_err());
    let done = issued
        .confirm(&intent, 102, receipt(&intent, &lease, 0))
        .unwrap();
    write(&mut c, &intent, &issued, &done).unwrap();
    assert!(c.execute("UPDATE drain_commands SET revision=revision+1,record=record WHERE update_id=?1 AND ordinal=0",[&intent.binding.update_id]).is_err());
    assert!(c
        .execute(
            "UPDATE drain_intents SET record=record WHERE id=?1",
            [&intent.binding.update_id]
        )
        .is_err());
    assert!(c
        .execute(
            "DELETE FROM drain_history WHERE update_id=?1",
            [&intent.binding.update_id]
        )
        .is_err());
    assert!(c
        .execute(
            "DELETE FROM drain_commands WHERE update_id=?1",
            [&intent.binding.update_id]
        )
        .is_err());
    let second = commands(&c, &intent).unwrap()[1].clone();
    write(
        &mut c,
        &intent,
        &second,
        &second.issue(&intent, 103).unwrap(),
    )
    .unwrap();
}
#[test]
fn reopen_keeps_issued_unknown_and_exact_receipt_without_recreating_intent() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ledger.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let (intent, lease) = ledger(&c);
    let initial = commands(&c, &intent).unwrap()[0].clone();
    let issued = initial.issue(&intent, 101).unwrap();
    write(&mut c, &intent, &initial, &issued).unwrap();
    drop(c);
    let mut c = Connection::open(path).unwrap();
    let old = commands(&c, &intent).unwrap()[0].clone();
    assert!(old.issue(&intent, 102).is_err());
    let unknown = old.uncertain(&intent, 102, "ACK lost".into()).unwrap();
    write(&mut c, &intent, &old, &unknown).unwrap();
    let done = unknown
        .confirm(&intent, 103, receipt(&intent, &lease, 0))
        .unwrap();
    write(&mut c, &intent, &unknown, &done).unwrap();
    assert_eq!(commands(&c, &intent).unwrap()[0], done);
    assert_eq!(
        done.confirm(&intent, 104, receipt(&intent, &lease, 0))
            .unwrap(),
        done
    );
    let count: i64 = c
        .query_row(
            "SELECT count(*) FROM drain_history WHERE ordinal=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 4);
}
#[test]
fn ignored_child_history_write_rolls_back_coupled_parent_and_command_receipt() {
    let mut c = Connection::open_in_memory().unwrap();
    let (intent, lease) = ledger(&c);
    for ordinal in 0..2 {
        let old = commands(&c, &intent).unwrap()[ordinal].clone();
        let issued = old.issue(&intent, 101).unwrap();
        write(&mut c, &intent, &old, &issued).unwrap();
        let done = issued
            .confirm(&intent, 102, receipt(&intent, &lease, ordinal))
            .unwrap();
        write(&mut c, &intent, &issued, &done).unwrap();
    }
    let old = commands(&c, &intent).unwrap()[2].clone();
    let issued = old.issue(&intent, 103).unwrap();
    write(&mut c, &intent, &old, &issued).unwrap();
    let done = issued
        .confirm(&intent, 104, receipt(&intent, &lease, 2))
        .unwrap();
    // This probe models the enclosing transaction's root row. The complete
    // lifecycle/health authority path is reserved for the real native gate.
    c.execute_batch("CREATE TABLE parent_probe(phase TEXT);INSERT INTO parent_probe VALUES('draining');CREATE TRIGGER deny_receipt BEFORE INSERT ON drain_history WHEN NEW.ordinal=2 AND NEW.revision=2 BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    {
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE parent_probe SET phase='quiesced'", [])
            .unwrap();
        assert!(persist(&tx, &intent, &issued, &done).is_err());
    }
    assert_eq!(
        c.query_row("SELECT phase FROM parent_probe", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "draining"
    );
    assert_eq!(commands(&c, &intent).unwrap()[2], issued);
    c.execute_batch("DROP TRIGGER deny_receipt;").unwrap();
    let tx = c.transaction().unwrap();
    tx.execute("UPDATE parent_probe SET phase='quiesced'", [])
        .unwrap();
    persist(&tx, &intent, &issued, &done).unwrap();
    tx.commit().unwrap();
    assert_eq!(commands(&c, &intent).unwrap()[2], done);
}
fn request(j: &mut UpdateJournal) -> UpdateRequest {
    let i = j.info().unwrap();
    UpdateRequest {
        contract_version: 1,
        idempotency_key: "drain-migration".into(),
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
fn schema_ten_migration_preserves_root_history_backs_up_and_invents_no_drain_authority() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    let a = j.acquire_owner().unwrap();
    let r = request(&mut j);
    let old = j.accept(&a, &r, &r.expected).unwrap();
    let epoch = a.epoch();
    drop(a);
    j.connection.execute_batch("DROP TABLE drain_history;DROP TABLE drain_commands;DROP TABLE drain_intents;PRAGMA user_version=10;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    // Preserve a typed schema10 snapshot as data. The deliberately incomplete
    // artifact provenance below MUST NOT be mistaken for executable authority.
    let mut historic = crate::health::tests::verified();
    historic
        .context
        .as_mut()
        .unwrap()
        .compatibility
        .updater_journal_schema = 10;
    historic
        .result
        .as_mut()
        .unwrap()
        .rehearsal
        .as_mut()
        .unwrap()
        .updater_journal_schema = 10;
    historic
        .result
        .as_mut()
        .unwrap()
        .final_compatibility
        .as_mut()
        .unwrap()
        .updater_journal_schema = 10;
    crate::health::validate_success(&historic).unwrap();
    let raw = encode(&historic).unwrap();
    j.connection
        .execute(
            "INSERT INTO candidate_artifact_sets VALUES(?1,'{}')",
            [&old.update_id],
        )
        .unwrap();
    j.connection
        .execute(
            "INSERT INTO candidate_health VALUES(?1,?2,?3)",
            params![old.update_id, historic.revision as i64, raw],
        )
        .unwrap();
    drop(j);
    let mut j = UpdateJournal::open(root.path(), &scope).unwrap();
    assert_eq!(j.info().unwrap().schema_version, 11);
    assert!(j.info().unwrap().owner_epoch > epoch);
    assert_eq!(j.get(&old.update_id).unwrap(), old);
    let retained: String = j
        .connection
        .query_row(
            "SELECT record FROM candidate_health WHERE id=?1",
            [&old.update_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, raw);
    let decoded: crate::health::HealthRecord = decode(&retained).unwrap();
    crate::health::validate_success(&decoded).unwrap();
    assert!(crate::health::validate_fresh_success(&decoded).is_err());
    assert!(health::read(&j.connection, &old.update_id).is_err());
    assert_eq!(
        UpdateJournal::inspect_drain(root.path(), &old.update_id).unwrap(),
        DrainSnapshot {
            intent: None,
            commands: vec![],
            quiesced_revision: None
        }
    );
    let backup = fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v10-")
        })
        .unwrap();
    let c = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 10);
    assert_eq!(record(&c, &old.update_id).unwrap(), old);
    assert_eq!(
        c.query_row(
            "SELECT record FROM candidate_health WHERE id=?1",
            [&old.update_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        raw
    );
    assert_eq!(
        c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
#[test]
fn raw_lifecycle_cannot_admit_drain_or_quiesce_with_agent_supplied_zero_counters() {
    let root = tempfile::tempdir().unwrap();
    let mut j = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
    let a = j.acquire_owner().unwrap();
    let req = request(&mut j);
    let mut r = j.accept(&a, &req, &req.expected).unwrap();
    for (n, event) in [
        UpdateEvent::BeginPreparation,
        UpdateEvent::SourceVerified,
        UpdateEvent::BuildSucceeded,
    ]
    .into_iter()
    .enumerate()
    {
        r = j
            .advance(
                &a,
                &r.update_id,
                &UpdateCommand {
                    idempotency_key: format!("metadata-{n}"),
                    expected_revision: r.revision,
                    event,
                    reason: "Metadata-only fixture".into(),
                },
            )
            .unwrap();
    }
    let cmd = UpdateCommand {
        idempotency_key: "metadata-accepted".into(),
        expected_revision: r.revision,
        event: UpdateEvent::CandidateAccepted,
        reason: "No executable approval claimed".into(),
    };
    r = j
        .advance_with_driver(&a, &r.update_id, &cmd, Driver::Health, |_, _| Ok(()))
        .unwrap();
    let cmd = UpdateCommand {
        idempotency_key: "raw-drain".into(),
        expected_revision: r.revision,
        event: UpdateEvent::BeginDrain,
        reason: "Agent payload".into(),
    };
    assert!(j.advance(&a, &r.update_id, &cmd).is_err());
    assert_eq!(j.get(&r.update_id).unwrap(), r);
    r = j
        .advance_with_driver(&a, &r.update_id, &cmd, Driver::Drain, |_, _| Ok(()))
        .unwrap();
    let cmd = UpdateCommand {
        idempotency_key: "raw-quiesce".into(),
        expected_revision: r.revision,
        event: quiesce(),
        reason: "Agent supplied zeros".into(),
    };
    assert!(j.advance(&a, &r.update_id, &cmd).is_err());
    assert_eq!(j.get(&r.update_id).unwrap(), r);
}
