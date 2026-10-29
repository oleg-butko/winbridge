//! Real SQLite atomicity and retained migration; synthetic graph data never
//! claim original OS custody, a live worker or a finalization delivery permit.
use super::*;

struct Fixture {
    _directory: tempfile::TempDir,
    c: Connection,
    a: UpdateAuthority,
    data: crate::activation::finalization::tests::Fixture,
    intent: ActivationFinalizationIntent,
}
fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let mut data = finalization::test_fixture();
    data.root.reason = Some("Локальное состояние подтверждено; metadata fixture".into());
    let root = &data.root;
    let b = &data.command.binding.lifetime_command.binding;
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;PRAGMA application_id=1463965008;
CREATE TABLE installation(singleton INTEGER PRIMARY KEY CHECK(singleton=1),installation TEXT NOT NULL,scope TEXT NOT NULL,epoch INTEGER NOT NULL,owner TEXT NOT NULL);
CREATE TABLE updates(id TEXT PRIMARY KEY,key TEXT NOT NULL UNIQUE,payload TEXT NOT NULL,payload_sha TEXT NOT NULL,phase TEXT NOT NULL,revision INTEGER NOT NULL,active INTEGER NOT NULL,record TEXT NOT NULL);
CREATE UNIQUE INDEX single_active_update ON updates(active) WHERE active=1;
CREATE TABLE transitions(sequence INTEGER PRIMARY KEY AUTOINCREMENT,update_id TEXT NOT NULL REFERENCES updates(id),revision INTEGER NOT NULL,command_key TEXT,payload TEXT,transition TEXT NOT NULL,record TEXT NOT NULL,UNIQUE(update_id,revision),UNIQUE(update_id,command_key));
CREATE TABLE activation_lifetime_records(id TEXT PRIMARY KEY);").unwrap();
    migrate(&c).unwrap();
    c.execute(
        "INSERT INTO installation VALUES(1,?1,?2,?3,?4)",
        params![
            data.command.binding.installation_id,
            data.command.binding.scope_sha256,
            b.updater_epoch as i64,
            b.updater_owner_id
        ],
    )
    .unwrap();
    c.execute(
        "INSERT INTO activation_lifetime_records VALUES(?1)",
        [&root.update_id],
    )
    .unwrap();
    let a = UpdateAuthority {
        lock: owner_lock(directory.path()).unwrap(),
        directory: directory.path().canonicalize().unwrap(),
        installation_id: data.command.binding.installation_id.clone(),
        scope_sha256: data.command.binding.scope_sha256.clone(),
        epoch: b.updater_epoch,
        owner_id: b.updater_owner_id.clone(),
    };
    let mut initial = root.clone();
    initial.phase = UpdatePhase::Requested;
    initial.revision = 0;
    initial.updated_at_ms = initial.accepted_at_ms;
    initial.reason = None;
    let payload = encode(&initial.request).unwrap();
    let raw = encode(&initial).unwrap();
    c.execute(
        "INSERT INTO updates VALUES(?1,?2,?3,?4,?5,0,1,?6)",
        params![
            root.update_id,
            root.request.idempotency_key,
            payload,
            crate::sha256(payload.as_bytes()),
            encode(&initial.phase).unwrap(),
            raw
        ],
    )
    .unwrap();
    append_transition(
        &c,
        &UpdateTransition {
            sequence: 0,
            update_id: root.update_id.clone(),
            revision: 0,
            previous_phase: None,
            phase: initial.phase,
            occurred_at_ms: initial.updated_at_ms,
            owner_epoch: a.epoch(),
            owner_id: a.owner_id().into(),
            command: None,
        },
        None,
        &raw,
    )
    .unwrap();
    let actual_edges = [
        UpdateEvent::BeginPreparation,
        UpdateEvent::SourceVerified,
        UpdateEvent::BuildSucceeded,
        UpdateEvent::CandidateAccepted,
        UpdateEvent::BeginDrain,
        UpdateEvent::Quiesce {
            dispatch_closed: true,
            accepted_operations: 0,
            physical_owners: 0,
            uncertain_owners: 0,
        },
        UpdateEvent::BeginShutdown,
        UpdateEvent::ShutdownVerified,
        UpdateEvent::FilesVerified,
        UpdateEvent::LocalHealthVerified,
    ];
    let padding = root.revision as usize - actual_edges.len();
    let events =
        std::iter::repeat_n(UpdateEvent::RuntimeBaselineAdmitted, padding).chain(actual_edges);
    for event in events {
        let old = initial.clone();
        initial.phase = advance_update(old.phase, event).unwrap();
        initial.revision += 1;
        initial.updated_at_ms = root.updated_at_ms;
        let command = if event == UpdateEvent::LocalHealthVerified {
            UpdateCommand {
                idempotency_key: format!(
                    "local-service-verified-{}",
                    data.snapshot.controller.service.command.command_id
                ),
                expected_revision: old.revision,
                event,
                reason: root.reason.clone().unwrap(),
            }
        } else {
            UpdateCommand {
                idempotency_key: format!("fixture-edge-{}", old.revision),
                expected_revision: old.revision,
                event,
                reason: "Synthetic legal lifecycle metadata".into(),
            }
        };
        initial.reason = Some(command.reason.clone());
        let raw = encode(&initial).unwrap();
        c.execute(
            "UPDATE updates SET record=?1,revision=?2,phase=?3,active=?4 WHERE id=?5",
            params![
                raw,
                initial.revision as i64,
                encode(&initial.phase).unwrap(),
                update_blocks_installation(initial.phase),
                root.update_id
            ],
        )
        .unwrap();
        append_transition(
            &c,
            &UpdateTransition {
                sequence: 0,
                update_id: root.update_id.clone(),
                revision: initial.revision,
                previous_phase: Some(old.phase),
                phase: initial.phase,
                occurred_at_ms: initial.updated_at_ms,
                owner_epoch: a.epoch(),
                owner_id: a.owner_id().into(),
                command: Some(command.clone()),
            },
            Some(&encode(&command).unwrap()),
            &raw,
        )
        .unwrap();
    }
    assert_eq!(record(&c, &root.update_id).unwrap(), *root);
    let intent = ActivationFinalizationIntent {
        command: data.command.clone(),
        root_before: root.clone(),
        query: data.query.clone(),
        observation: data.observed.clone(),
    };
    intent.validate(&data.snapshot, &data.shutdown).unwrap();
    Fixture {
        _directory: directory,
        c,
        a,
        data,
        intent,
    }
}
fn commit(f: &mut Fixture) -> Result<ActivationFinalizationRecord> {
    let tx = f.c.transaction().map_err(storage)?;
    let value = seed_and_root(&tx, &f.a, &f.intent, &f.data.snapshot, &f.data.shutdown)?;
    tx.commit().map_err(storage)?;
    Ok(value)
}
fn get(f: &Fixture) -> Result<ActivationFinalizationRecord> {
    ledger(
        &f.c,
        &f.data.root.update_id,
        &f.data.snapshot,
        &f.data.shutdown,
    )
    .map(|(_, v)| v)
}
fn rows(f: &Fixture) -> [i64; 3] {
    [
        "activation_finalization_intents",
        "activation_finalization_records",
        "activation_finalization_history",
    ]
    .map(|t| {
        f.c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .unwrap()
    })
}
fn persist(f: &mut Fixture, next: &ActivationFinalizationRecord) -> Result<()> {
    let old = get(f)?;
    old.advance(next, &f.data.shutdown).map_err(bad)?;
    let tx = f.c.transaction().map_err(storage)?;
    let raw = encode(next)?;
    let id = &f.data.root.update_id;
    one(tx.execute("UPDATE activation_finalization_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id,old.revision as i64]).map_err(storage)?)?;
    one(tx
        .execute(
            "INSERT INTO activation_finalization_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(&tx, id, &f.data.snapshot, &f.data.shutdown)?.1 != *next {
        return Err(bad("Test append readback differs"));
    }
    tx.commit().map_err(storage)
}

#[test]
fn root_commit_and_release_pending_are_atomic_but_pending_still_blocks_next_update() {
    let mut f = fixture();
    let pending = commit(&mut f).unwrap();
    assert_eq!(rows(&f), [1, 1, 1]);
    assert_eq!(
        record(&f.c, &f.data.root.update_id).unwrap().phase,
        UpdatePhase::Committed
    );
    assert_eq!(installation_info(&f.c).unwrap().active_update_id, None);
    assert!(pending.blocks_next_update(&f.data.shutdown).unwrap());
    assert_eq!(get(&f).unwrap(), pending);
    assert!(commit(&mut f).is_err());
    assert_eq!(rows(&f), [1, 1, 1]);
}

#[test]
fn ignored_write_or_interruption_rolls_back_root_intent_current_and_history() {
    for (table, event) in [
        ("activation_finalization_intents", "INSERT"),
        ("activation_finalization_records", "INSERT"),
        ("activation_finalization_history", "INSERT"),
        ("updates", "UPDATE"),
        ("transitions", "INSERT"),
    ] {
        let mut f = fixture();
        let count: i64 =
            f.c.query_row("SELECT count(*) FROM transitions", [], |r| r.get(0))
                .unwrap();
        f.c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE {event} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        assert!(commit(&mut f).is_err(), "{table}");
        assert_eq!(rows(&f), [0, 0, 0]);
        assert_eq!(record(&f.c, &f.data.root.update_id).unwrap(), f.data.root);
        assert_eq!(
            f.c.query_row("SELECT count(*) FROM transitions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            count
        );
    }
    let f = fixture();
    {
        let tx = f.c.unchecked_transaction().unwrap();
        seed_and_root(&tx, &f.a, &f.intent, &f.data.snapshot, &f.data.shutdown).unwrap();
    }
    assert_eq!(rows(&f), [0, 0, 0]);
    assert_eq!(record(&f.c, &f.data.root.update_id).unwrap(), f.data.root);
}

#[test]
fn release_unknown_can_be_reconciled_once_without_rewriting_committed_root() {
    let mut f = fixture();
    let pending = commit(&mut f).unwrap();
    let root = record(&f.c, &f.data.root.update_id).unwrap();
    let unknown = pending
        .indeterminate("Release ACK unknown".into(), pending.updated_at_ms + 1)
        .unwrap();
    persist(&mut f, &unknown).unwrap();
    let confirmed = unknown
        .confirmed(
            f.data.receipt.clone(),
            &f.data.shutdown,
            f.data.receipt.compatibility.observed_at_ms,
        )
        .unwrap();
    persist(&mut f, &confirmed).unwrap();
    assert_eq!(get(&f).unwrap(), confirmed);
    assert_eq!(rows(&f), [1, 1, 3]);
    assert!(!confirmed.blocks_next_update(&f.data.shutdown).unwrap());
    assert_eq!(record(&f.c, &f.data.root.update_id).unwrap(), root);
    assert!(persist(&mut f, &pending).is_err());
    assert!(persist(&mut f, &confirmed).is_err());
}

#[test]
fn finalization_sql_rejects_null_ids_mutations_deletion_and_terminal_append() {
    let mut f = fixture();
    commit(&mut f).unwrap();
    for sql in [
        "UPDATE activation_finalization_intents SET intent=intent",
        "DELETE FROM activation_finalization_intents",
        "UPDATE activation_finalization_records SET revision=revision+1",
        "DELETE FROM activation_finalization_records",
        "UPDATE activation_finalization_history SET record=record",
        "DELETE FROM activation_finalization_history",
    ] {
        assert!(f.c.execute(sql, []).is_err(), "{sql}");
    }
    let raw = encode(&f.intent).unwrap();
    assert!(f
        .c
        .execute(
            "INSERT INTO activation_finalization_intents VALUES(NULL,'another',?1)",
            [raw]
        )
        .is_err());
    let pending = get(&f).unwrap();
    let confirmed = pending
        .confirmed(
            f.data.receipt.clone(),
            &f.data.shutdown,
            f.data.receipt.compatibility.observed_at_ms,
        )
        .unwrap();
    persist(&mut f, &confirmed).unwrap();
    assert!(f
        .c
        .execute(
            "UPDATE activation_finalization_records SET revision=revision+1",
            []
        )
        .is_err());
}

#[test]
fn finalization_read_detects_coherent_aliases_missing_rows_and_root_edge_changes() {
    for mutation in 0..8 {
        let mut f = fixture();
        commit(&mut f).unwrap();
        match mutation {
            0=>f.c.execute_batch("DROP TRIGGER activation_finalization_history_retained;DELETE FROM activation_finalization_history;").unwrap(),
            1=>f.c.execute_batch("DROP TRIGGER activation_finalization_record_retained;PRAGMA foreign_keys=OFF;DELETE FROM activation_finalization_records;").unwrap(),
            2=>{ f.c.execute_batch("DROP TRIGGER activation_finalization_intent_immutable;").unwrap(); f.c.execute("UPDATE activation_finalization_intents SET command_id=?1,intent=json_set(intent,'$.command.commandId',?1)",[Uuid::new_v4().to_string()]).unwrap(); },
            3=>f.c.execute_batch("DROP TRIGGER activation_finalization_record_identity;UPDATE activation_finalization_records SET record=json_set(record,'$.updatedAtMs',99999999);").unwrap(),
            4=>{let id=&f.data.root.update_id;f.c.execute("UPDATE transitions SET transition=json_set(transition,'$.ownerEpoch',999) WHERE update_id=?1 AND revision=?2",params![id,f.data.root.revision as i64+1]).unwrap();},
            5=>{f.c.execute("UPDATE transitions SET transition=json_set(transition,'$.command.idempotencyKey','another') WHERE update_id=?1 AND revision=?2",params![f.data.root.update_id,f.data.root.revision as i64]).unwrap();},
            6=>{f.data.snapshot.controller.local.intent.context.channel_nonce_sha256="0".repeat(64);},
            _=>{f.c.execute_batch("DROP TRIGGER activation_finalization_intent_immutable;UPDATE activation_finalization_intents SET intent=json_set(intent,'$.observation.result.frame.sequence',999);").unwrap();},
        }
        assert!(get(&f).is_err(), "{mutation}");
    }
}

#[test]
fn stale_authority_cannot_write_finalization_or_change_any_root_bytes() {
    let mut f = fixture();
    f.c.execute("UPDATE installation SET epoch=epoch+1", [])
        .unwrap();
    assert_eq!(
        commit(&mut f).unwrap_err().code,
        JournalErrorCode::StaleOwner
    );
    assert_eq!(rows(&f), [0, 0, 0]);
    assert_eq!(record(&f.c, &f.data.root.update_id).unwrap(), f.data.root);
}

#[test]
fn schema22_copy_migration_preserves_root_history_and_refuses_live_owner() {
    let directory = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(directory.path(), &scope).unwrap();
    let installation = old.info().unwrap();
    let authority = old.acquire_owner().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "retained22-request".into(),
        source: UpdateSourcePin {
            archive_name: "source.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: installation.installation_id,
            scope_sha256: installation.scope_sha256,
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let saved = old.accept(&authority, &request, &request.expected).unwrap();
    drop(authority);
    old.connection.execute_batch("DROP TABLE activation_finalization_history;DROP TABLE activation_finalization_records;DROP TABLE activation_finalization_intents;PRAGMA user_version=22;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let before = old.info().unwrap();
    let lock = owner_lock(directory.path()).unwrap();
    assert_eq!(
        UpdateJournal::open(directory.path(), &scope)
            .err()
            .unwrap()
            .code,
        JournalErrorCode::OwnerBusy
    );
    assert_eq!(old.info().unwrap(), before);
    drop(lock);
    drop(old);
    let mut new = UpdateJournal::open(directory.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, 23);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(after.active_update_id, before.active_update_id);
    assert_eq!(blocking(&new.connection).unwrap(), None);
    assert_eq!(record(&new.connection, &saved.update_id).unwrap(), saved);
    let backups = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v22-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = connection(&backups[0], true).unwrap();
    assert_eq!(info(&backup).unwrap(), before);
    assert_eq!(record(&backup, &saved.update_id).unwrap(), saved);
    assert_eq!(
        backup
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name LIKE 'activation_finalization_%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn pending_receiver_data_require_original_resident_receipt_and_exact_unconfirmed_decision() {
    let f = fixture();
    let record = ActivationFinalizationRecord::pending(f.intent.command.clone()).unwrap();
    let saved = RetainedFinalizationSnapshot {
        intent: f.intent.clone(),
        record,
        lifetime: f.data.snapshot.clone(),
        shutdown: f.data.shutdown.clone(),
    };
    let original = &f.intent.command.binding.lifetime_command;
    let crate::activation::lifetime::ActivationLifetimeState::Confirmed { receipt: entry } =
        &saved.lifetime.lifetime.state
    else {
        unreachable!()
    };
    let actual = &original.binding.compatibility.identity.process;
    require_resident_prefix(&saved, actual, original, entry, &f.intent.command).unwrap();
    let mut unknown = saved.clone();
    unknown.record = unknown
        .record
        .indeterminate("ACK unknown".into(), unknown.record.updated_at_ms + 1)
        .unwrap();
    require_resident_prefix(&unknown, actual, original, entry, &f.intent.command).unwrap();
    for mutation in 0..8 {
        let mut changed = saved.clone();
        let mut process = actual.clone();
        let mut old = original.clone();
        let mut receipt = entry.as_ref().clone();
        let mut command = f.intent.command.clone();
        match mutation {
            0 => process.birth.push('0'),
            1 => old.command_id = Uuid::new_v4().to_string(),
            2 => receipt.compatibility.observed_at_ms += 1,
            3 => command.command_id = Uuid::new_v4().to_string(),
            4 => {
                changed
                    .lifetime
                    .controller
                    .local
                    .created
                    .as_mut()
                    .unwrap()
                    .process
                    .process_id += 1
            }
            5 => changed
                .lifetime
                .controller
                .local
                .intent
                .context
                .supervisor
                .birth
                .push('0'),
            6 => {
                changed.record = changed
                    .record
                    .confirmed(
                        f.data.receipt.clone(),
                        &f.data.shutdown,
                        f.data.receipt.compatibility.observed_at_ms,
                    )
                    .unwrap()
            }
            _ => changed.intent.command.binding.scope_sha256 = "0".repeat(64),
        }
        assert!(
            require_resident_prefix(&changed, &process, &old, &receipt, &command).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn cold_pending_receiver_cannot_create_intent_or_change_empty_journal_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(temp.path(), &scope).unwrap();
    let before = j.info().unwrap();
    let f = finalization::test_fixture();
    let old = &f.command.binding.lifetime_command;
    let crate::activation::lifetime::ActivationLifetimeState::Confirmed { receipt } =
        &f.snapshot.lifetime.state
    else {
        unreachable!()
    };
    assert!(UpdateJournal::inspect_pending_finalization_receiver(
        temp.path(),
        &old.binding.compatibility.identity.process,
        old,
        receipt,
        &f.command
    )
    .is_err());
    assert_eq!(j.info().unwrap(), before);
    assert_eq!(
        j.connection
            .query_row(
                "SELECT count(*) FROM activation_finalization_intents",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
