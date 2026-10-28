//! SQL primitives and data FSM only; no private channel or startup permission.
use super::*;
use crate::activation::{writer::command_sha256, ActivationWriterState};
use winbridge_protocol::update::WorkerJournalActivationReceipt;

fn fixture() -> (
    Connection,
    ActivationRecord,
    StateBackupAuthority,
    ActivationWriterRecord,
) {
    let (replacement, mut initial) = crate::activation::activation_fixture();
    initial.intent.context.updater_journal_schema = 19;
    let physical = crate::activation::activation_stages(&initial, &replacement)[6].clone();
    physical.validate(&replacement).unwrap();
    let old = StateBackupAuthority {
        incarnation: physical
            .intent
            .context
            .worker_context
            .journal_incarnation
            .clone(),
        epoch: 7,
        fingerprint_sha256: "a".repeat(64),
    };
    let cmd = writer::prepare_command(
        &physical,
        &old,
        &Uuid::new_v4().to_string(),
        physical.updated_at_ms + 1,
    )
    .unwrap();
    let value = ActivationWriterRecord::issued(cmd).unwrap();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE activation_intents(id TEXT PRIMARY KEY);")
        .unwrap();
    c.execute(
        "INSERT INTO activation_intents VALUES(?1)",
        [&value.command.binding.update_id],
    )
    .unwrap();
    migrate(&c).unwrap();
    (c, physical, old, value)
}
fn receipt(value: &ActivationWriterRecord) -> WorkerJournalActivationReceipt {
    WorkerJournalActivationReceipt {
        schema_version: 1,
        command_id: value.command.command_id.clone(),
        update_id: value.command.binding.update_id.clone(),
        attempt_id: value.command.binding.attempt_id.clone(),
        command_sha256: command_sha256(&value.command).unwrap(),
        authority: StateBackupAuthority {
            incarnation: value.command.binding.expected_authority.incarnation.clone(),
            epoch: 8,
            fingerprint_sha256: "b".repeat(64),
        },
        committed_at_ms: value.command.issued_at_ms + 1,
    }
}
fn append(
    c: &mut Connection,
    old: &ActivationWriterRecord,
    next: &ActivationWriterRecord,
    physical: &ActivationRecord,
    authority: &StateBackupAuthority,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next, physical, authority)?;
    tx.commit().map_err(storage)
}
#[test]
fn writer_causal_history_retains_late_confirmation_without_renewing_or_reissuing() {
    let (mut c, p, a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    let late = v.command.deadline_at_ms + 1000;
    let uncertain = v
        .indeterminate("Private ACK outcome unknown".into(), late)
        .unwrap();
    append(&mut c, &v, &uncertain, &p, &a).unwrap();
    let next = uncertain.confirmed(receipt(&v), late + 1).unwrap();
    append(&mut c, &uncertain, &next, &p, &a).unwrap();
    assert_eq!(
        ledger(&c, &v.command.binding.update_id, &p, &a).unwrap(),
        next
    );
    assert!(append(&mut c, &v, &uncertain, &p, &a).is_err());
    assert!(next.confirmed(receipt(&v), late + 2).is_err());
    assert!(next
        .indeterminate("Cannot undo fact".into(), late + 2)
        .is_err());
    assert_eq!(next.command, v.command);
    for sql in [
        "UPDATE activation_writer_intents SET command=command",
        "DELETE FROM activation_writer_intents",
        "UPDATE activation_writer_history SET record=record",
        "DELETE FROM activation_writer_history",
        "DELETE FROM activation_writer_records",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
}
#[test]
fn writer_ignored_admission_and_history_leave_no_partial_acceptance() {
    for table in [
        "activation_writer_intents",
        "activation_writer_records",
        "activation_writer_history",
    ] {
        let (mut c, p, a, v) = fixture();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(seed(&tx, &v, &p, &a).is_err());
        }
        for name in [
            "activation_writer_intents",
            "activation_writer_records",
            "activation_writer_history",
        ] {
            assert_eq!(
                c.query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    let (mut c, p, a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    c.execute_batch("CREATE TRIGGER deny BEFORE INSERT ON activation_writer_history BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    let next = v.confirmed(receipt(&v), v.updated_at_ms + 1).unwrap();
    assert!(append(&mut c, &v, &next, &p, &a).is_err());
    assert_eq!(ledger(&c, &v.command.binding.update_id, &p, &a).unwrap(), v);
}
#[test]
fn writer_intent_key_physical_header_and_rewritten_history_are_all_checked() {
    let (c, mut p, mut a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    assert!(seed(&c, &v, &p, &a).is_err());
    let original = p.clone();
    p.state_owned.as_mut().unwrap().generation = Uuid::new_v4().to_string();
    assert!(ledger(&c, &v.command.binding.update_id, &p, &a).is_err());
    let (_, foreign, _, _) = fixture(); // Different full physical record remains invalid.
    assert!(ledger(&c, &v.command.binding.update_id, &foreign, &a).is_err());
    a.epoch += 1;
    assert!(ledger(&c, &v.command.binding.update_id, &original, &a).is_err());
    let (c, p, a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    c.execute_batch("DROP TRIGGER activation_writer_intent_immutable;")
        .unwrap();
    c.execute(
        "UPDATE activation_writer_intents SET command_id=?1",
        [Uuid::new_v4().to_string()],
    )
    .unwrap();
    assert!(ledger(&c, &v.command.binding.update_id, &p, &a).is_err());
    let (c, p, a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    c.execute_batch("DROP TRIGGER activation_writer_history_immutable;")
        .unwrap();
    let changed = v
        .indeterminate("Rewritten history".into(), v.updated_at_ms + 1)
        .unwrap();
    c.execute(
        "UPDATE activation_writer_history SET record=?1 WHERE revision=0",
        [encode(&changed).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &v.command.binding.update_id, &p, &a).is_err());
}
#[test]
fn writer_fsm_rejects_forged_results_command_edits_and_history_skips() {
    let (_, _, _, v) = fixture();
    let next = v.confirmed(receipt(&v), v.updated_at_ms + 1).unwrap();
    v.advance(&next).unwrap();
    let mut bad = next.clone();
    bad.command.command_id = Uuid::new_v4().to_string();
    assert!(v.advance(&bad).is_err());
    let mut bad = receipt(&v);
    bad.authority.epoch += 1;
    assert!(v.confirmed(bad, v.updated_at_ms + 2).is_err());
    let mut bad = next.clone();
    bad.revision = 2;
    assert!(v.advance(&bad).is_err());
    let mut raw = serde_json::to_value(&next).unwrap();
    raw["state"]["force"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<ActivationWriterRecord>(raw).is_err());
    assert!(matches!(v.state, ActivationWriterState::Issued {}));
}
#[test]
fn updater_eighteen_to_nineteen_preserves_root_keys_backup_and_fences_old_owner() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(root.path(), &scope).unwrap();
    let owner = old.acquire_owner().unwrap();
    let before = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "writer-migration".into(),
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
    old.connection.execute_batch("DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;PRAGMA user_version=18;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    fs2::FileExt::unlock(&owner.lock).unwrap();
    let mut new = UpdateJournal::open(root.path(), &scope).unwrap();
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
    let backups: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|x| x.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v18-")
                && p.extension().is_some_and(|s| s == "sqlite")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let c = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 18);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
    assert_eq!(info(&c).unwrap().owner_epoch, before.owner_epoch);
    let fresh = new.acquire_owner().unwrap();
    assert_eq!(
        new.accept(&fresh, &request, &request.expected).unwrap(),
        saved
    );
}

#[test]
fn late_writer_fact_uses_frozen_six_after_current_activation_became_indeterminate() {
    let (mut c, p, a, v) = fixture();
    seed(&c, &v, &p, &a).unwrap();
    c.execute_batch("CREATE TABLE activation_history(id TEXT,revision INTEGER,record TEXT,PRIMARY KEY(id,revision));").unwrap();
    c.execute(
        "INSERT INTO activation_history VALUES(?1,6,?2)",
        params![p.intent.source.update_id, encode(&p).unwrap()],
    )
    .unwrap();
    let mut current = p.clone();
    current.revision = 7;
    current.updated_at_ms += 1;
    current.state = crate::activation::ActivationState::Indeterminate {
        previous: Box::new(crate::activation::ActivationState::StateOwned {}),
        reason: "Private ACK lost".into(),
    };
    let frozen = physical_prefix(&c, &current).unwrap();
    assert_eq!(frozen, p);
    let uncertain = v
        .indeterminate("Private ACK lost".into(), v.updated_at_ms + 1)
        .unwrap();
    append(&mut c, &v, &uncertain, &frozen, &a).unwrap();
    let next = uncertain
        .confirmed(receipt(&v), uncertain.updated_at_ms + 1)
        .unwrap();
    append(&mut c, &uncertain, &next, &frozen, &a).unwrap();
    assert_eq!(
        ledger(&c, &v.command.binding.update_id, &frozen, &a).unwrap(),
        next
    );
    current.state_owned.as_mut().unwrap().generation = Uuid::new_v4().to_string();
    assert!(physical_prefix(&c, &current).is_err());
}
