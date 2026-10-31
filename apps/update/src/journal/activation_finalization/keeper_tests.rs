//! Real SQLite selection/rollback with synthetic process metadata; not custody.
use super::*;
use crate::journal::keeper::{self, KeeperSelectionPermit};
fn setup() -> (Fixture, SupervisorRecord, KeeperSelectionPermit, u64) {
    let mut f = fixture();
    let pending = commit(&mut f).unwrap();
    let at = f.data.receipt.compatibility.observed_at_ms + 2;
    let confirmed = pending
        .confirmed(f.data.receipt.clone(), &f.data.shutdown, at)
        .unwrap();
    persist(&mut f, &confirmed).unwrap();
    supervisor::migrate(&f.c).unwrap();
    f.c.pragma_update(None, "user_version", UPDATE_SCHEMA_VERSION)
        .unwrap();
    let b = &f.data.command.binding.lifetime_command.binding;
    let s = SupervisorRecord {
        schema_version: 1,
        spec: SupervisorSpec {
            launch_id: b.launch_id.clone(),
            update_id: b.update_id.clone(),
            parent: f
                .data
                .snapshot
                .controller
                .physical
                .intent
                .context
                .previous_worker
                .clone(),
            parent_owner_id: uuid::Uuid::new_v4().to_string(),
            parent_epoch: b.updater_epoch - 1,
            token_sha256: "a".repeat(64),
            config_path: "K:\\fixture\\config.json".into(),
            config_sha256: "b".repeat(64),
            runtime_directory: "K:\\fixture".into(),
            image_path: "K:\\fixture\\updater.exe".into(),
            image_sha256: "c".repeat(64),
            image_identity: "fixture".into(),
            excluded_domain: None,
            endpoint: "fixture".into(),
            accepted_at_ms: 1,
            worker: Some(f.data.shutdown.intent.binding.worker.clone()),
        },
        phase: SupervisorPhase::Active,
        revision: 3,
        process: Some(b.keeper.clone()),
        owner_id: Some(f.a.owner_id().into()),
        owner_epoch: Some(f.a.epoch()),
        updated_at_ms: at,
        reason: "Synthetic active coordinator".into(),
    };
    f.c.execute(
        "INSERT INTO supervisor_launches VALUES(?1,?2,?3,?4,1,?5)",
        params![
            s.spec.update_id,
            s.spec.launch_id,
            encode(&s.spec).unwrap(),
            s.revision as i64,
            encode(&s).unwrap()
        ],
    )
    .unwrap();
    f.c.execute(
        "INSERT INTO supervisor_receipts VALUES(?1,?2,?3)",
        params![
            s.spec.launch_id,
            encode(&SupervisorPhase::Active).unwrap(),
            encode(&s).unwrap()
        ],
    )
    .unwrap();
    let query = ActivationFinalizationQuery {
        finalization_observation_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: f.data.command.clone(),
    };
    let permit = KeeperSelectionPermit {
        capture: FinalizationConfirmationPermit {
            observation: ActivationFinalizationObservation {
                finalization_observation_version: 1,
                request_id: Some(query.request_id.clone()),
                result: ActivationFinalizationObservationResult::Observed {
                    receipt: Box::new(f.data.receipt.clone()),
                    observed_at_ms: at + 1,
                },
            },
            query,
            observation_deadline: std::time::Instant::now() + winbridge_transport::IPC_TIMEOUT,
        },
    };
    assert_eq!(supervisor::read(&f.c, &s.spec.launch_id).unwrap(), s);
    (f, s, permit, at + 2)
}
fn selected(
    f: &mut Fixture,
    s: &SupervisorRecord,
    p: &KeeperSelectionPermit,
    at: u64,
) -> Result<keeper::KeeperSelection> {
    let tx = f.c.transaction().unwrap();
    let result = keeper::select(
        &tx,
        &f.a,
        s,
        &f.data.snapshot,
        &f.data.shutdown,
        p,
        || Ok(at),
        || Ok(()),
    )?;
    tx.commit().map_err(storage)?;
    Ok(result)
}
#[test]
fn selected_keeper_releases_only_the_coordinator_slot_and_is_immutable_history() {
    let (mut f, s, p, at) = setup();
    let root = record(&f.c, &s.spec.update_id).unwrap();
    let finalization = get(&f).unwrap();
    let role = selected(&mut f, &s, &p, at).unwrap();
    assert_eq!(role.selected.phase, SupervisorPhase::Keeper);
    assert_eq!(role.selected.revision, s.revision + 1);
    assert_eq!(
        supervisor::read(&f.c, &s.spec.launch_id).unwrap(),
        role.selected
    );
    assert!(!supervisor::active(&f.c).unwrap());
    assert_eq!(record(&f.c, &s.spec.update_id).unwrap(), root);
    assert_eq!(get(&f).unwrap(), finalization);
    assert!(
        selected(&mut f, &s, &p, at).is_err(),
        "A row is not another selection permit"
    );
    assert!(f
        .c
        .execute("UPDATE keeper_selections SET selection=selection", [])
        .is_err());
    assert!(f.c.execute("DELETE FROM keeper_selections", []).is_err());
    assert!(f
        .c
        .execute("UPDATE supervisor_launches SET revision=revision+1", [])
        .is_err());
}
#[test]
fn readonly_keeper_nomination_survives_next_owner_without_adopting_it() {
    let (mut f, s, p, at) = setup();
    let selection = selected(&mut f, &s, &p, at).unwrap();
    // A later owner does not rewrite the historical keeper's selected epoch.
    f.c.execute(
        "UPDATE installation SET epoch=epoch+1,owner=?1",
        [uuid::Uuid::new_v4().to_string()],
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("updates.sqlite");
    let mut c = Connection::open(&path).unwrap();
    rusqlite::backup::Backup::new(&f.c, &mut c)
        .unwrap()
        .run_to_completion(64, Duration::from_millis(1), None)
        .unwrap();
    let before = installation_info(&c).unwrap();
    for id in [&s.spec.update_id, &s.spec.launch_id] {
        assert_eq!(
            UpdateJournal::inspect_keeper_selection(directory.path(), id).unwrap(),
            Some(selection.clone())
        );
    }
    assert!(UpdateJournal::inspect_keeper_selection(
        directory.path(),
        &uuid::Uuid::new_v4().to_string()
    )
    .unwrap()
    .is_none());
    assert_eq!(installation_info(&c).unwrap(), before);
    // A format that predates keeper selection must be refused, never migrated
    // by this readonly route, even when a copied table happens to exist.
    c.pragma_update(None, "user_version", 23).unwrap();
    assert_eq!(
        UpdateJournal::inspect_keeper_selection(directory.path(), &s.spec.launch_id)
            .unwrap_err()
            .code,
        JournalErrorCode::UnsupportedSchema
    );
    assert_eq!(schema(&c).unwrap(), 23);
}
#[test]
fn failed_last_checkpoint_or_ignored_selection_never_retires_the_coordinator() {
    for fault in 0..4 {
        let (mut f, s, p, at) = setup();
        if fault == 0 {
            f.c.execute_batch("CREATE TRIGGER ignore_keeper BEFORE INSERT ON keeper_selections BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        }
        if fault == 1 {
            f.c.execute_batch("CREATE TRIGGER ignore_keeper_receipt BEFORE INSERT ON supervisor_receipts BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        }
        let tx = f.c.transaction().unwrap();
        let mut checks = 0;
        let result = keeper::select(
            &tx,
            &f.a,
            &s,
            &f.data.snapshot,
            &f.data.shutdown,
            &p,
            || Ok(at),
            || {
                checks += 1;
                if checks == 2 {
                    if fault == 2 {
                        return Err("Original child lost after selection writes".into());
                    }
                    if fault == 3 {
                        tx.execute("UPDATE installation SET epoch=epoch+1", [])
                            .unwrap();
                    }
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "fault {fault}");
        drop(tx);
        assert_eq!(supervisor::read(&f.c, &s.spec.launch_id).unwrap(), s);
        assert!(supervisor::active(&f.c).unwrap());
        let rows: i64 =
            f.c.query_row("SELECT count(*) FROM keeper_selections", [], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 0);
    }
}
#[test]
fn stale_capture_and_clock_rollback_cannot_retire_the_original_owner() {
    for fault in 0..4 {
        let (mut f, s, mut p, at) = setup();
        if fault == 0 {
            p.capture.observation_deadline = std::time::Instant::now();
        }
        if fault == 1 {
            p.capture.query.request_id = uuid::Uuid::new_v4().to_string();
        }
        if fault == 2 {
            f.c.execute("UPDATE installation SET epoch=epoch+1", [])
                .unwrap();
        }
        let tx = f.c.transaction().unwrap();
        let mut ticks = 0;
        let result = keeper::select(
            &tx,
            &f.a,
            &s,
            &f.data.snapshot,
            &f.data.shutdown,
            &p,
            || {
                ticks += 1;
                Ok(if fault == 3 && ticks > 1 { at - 3 } else { at })
            },
            || Ok(()),
        );
        assert!(result.is_err(), "fault {fault}");
        drop(tx);
        assert_eq!(supervisor::read(&f.c, &s.spec.launch_id).unwrap(), s);
        assert!(supervisor::active(&f.c).unwrap());
    }
}
#[test]
fn retained_keeper_without_its_exact_selection_or_fact_fails_closed() {
    for fault in 0..2 {
        let (mut f, s, p, at) = setup();
        let role = selected(&mut f, &s, &p, at).unwrap();
        if fault == 0 {
            f.c.execute_batch(
                "DROP TRIGGER keeper_selection_retained;DELETE FROM keeper_selections;",
            )
            .unwrap();
        } else {
            f.c.execute_batch("DROP TRIGGER activation_finalization_record_identity;UPDATE activation_finalization_records SET record=json_set(record,'$.updatedAtMs',0);").unwrap();
        }
        assert!(supervisor::read(&f.c, &role.selected.spec.launch_id).is_err());
    }
}

#[test]
fn schema23_copy_migration_preserves_root_epoch_backup_and_invents_no_keeper() {
    let root = tempfile::tempdir().unwrap();
    let scope = "a".repeat(64);
    let mut old = UpdateJournal::open(root.path(), &scope).unwrap();
    let a = old.acquire_owner().unwrap();
    let installed = old.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "keeper-schema23".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: installed.installation_id,
            scope_sha256: scope.clone(),
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let saved = old.accept(&a, &request, &request.expected).unwrap();
    old.connection
        .execute_batch(
            "DROP TABLE bootstrap_reservations;DROP TABLE keeper_selections;PRAGMA user_version=23;PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .unwrap();
    let before = old.info().unwrap();
    assert_eq!(
        UpdateJournal::open(root.path(), &scope).err().unwrap().code,
        JournalErrorCode::OwnerBusy
    );
    assert_eq!(old.info().unwrap(), before);
    fs2::FileExt::unlock(&a.lock).unwrap();
    let mut new = UpdateJournal::open(root.path(), &scope).unwrap();
    let after = new.info().unwrap();
    assert_eq!(after.schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(new.get(&saved.update_id).unwrap(), saved);
    let count: i64 = new
        .connection
        .query_row("SELECT count(*) FROM keeper_selections", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        old.accept(&a, &request, &request.expected)
            .unwrap_err()
            .code,
        JournalErrorCode::StaleOwner
    );
    let backups = std::fs::read_dir(root.path())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v23-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let mut c = connection(&backups[0], true).unwrap();
    let tx = c.transaction().unwrap();
    assert_eq!(schema(&tx).unwrap(), 23);
    assert_eq!(record(&tx, &saved.update_id).unwrap(), saved);
    assert_eq!(info(&tx).unwrap(), before);
}

#[test]
fn bootstrap_data_binds_the_historical_keeper_without_recreating_live_authority() {
    let (mut f, s, p, at) = setup();
    let role = selected(&mut f, &s, &p, at).unwrap();
    let crate::activation::finalization::ActivationFinalizationState::Confirmed { receipt } =
        &role.finalization.state
    else {
        unreachable!()
    };
    let b = &role.finalization.command.binding;
    let old = &b.lifetime_command.binding;
    let mut intent = crate::bootstrap::tests::fixture();
    intent.previous = keeper_binding_from_receipt(
        old.update_id.clone(),
        old.launch_id.clone(),
        old.keeper.clone(),
        old.job_name.clone(),
        receipt,
        Some(role.sha256().unwrap()),
    );
    intent.update.request.expected.installation_id = b.installation_id.clone();
    intent.update.request.expected.scope_sha256 = b.scope_sha256.clone();
    intent.update.request.expected.worker_build_id = receipt
        .compatibility
        .identity
        .descriptor
        .build
        .build_id
        .clone();
    intent.update.request.expected.worker_config_sha256 =
        receipt.compatibility.worker_config_sha256.clone();
    intent.update.request.expected.baseline_manifest_sha256 = b.source_manifest_sha256.clone();
    intent.parent_epoch = old.updater_epoch + 1;
    intent.excluded_domain.host_id = old.compatibility.identity.process.host_id.clone();
    intent.excluded_domain.session_id = old.compatibility.identity.process.session_id;
    intent.update.accepted_at_ms = at + 1;
    intent.update.updated_at_ms = at + 1;
    intent.prepared_at_ms = at + 1;
    intent.validate_against(&role).unwrap();
    for fault in 0..6 {
        let mut changed = intent.clone();
        match fault {
            0 => changed.previous.selection_sha256 = Some("0".repeat(64)),
            1 => changed.update.request.expected.worker_build_id = "foreign".into(),
            2 => changed.update.request.expected.scope_sha256 = "0".repeat(64),
            3 => changed.parent_epoch = old.updater_epoch,
            4 => changed.parent_owner_id = old.updater_owner_id.clone(),
            _ => {
                changed.update.accepted_at_ms = at - 1;
                changed.update.updated_at_ms = at - 1;
            }
        }
        assert!(changed.validate_against(&role).is_err(), "fault {fault}");
    }
    // Pure data validation did not allocate a second supervisor or acquire owner.
    assert_eq!(
        supervisor::read(&f.c, &s.spec.launch_id).unwrap(),
        role.selected
    );
}

fn delegation_setup() -> (Fixture, crate::bootstrap::BootstrapIntent, SupervisorSpec) {
    let (mut f, s, p, at) = setup();
    let role = selected(&mut f, &s, &p, at).unwrap();
    let crate::activation::finalization::ActivationFinalizationState::Confirmed { receipt } =
        &role.finalization.state
    else {
        unreachable!()
    };
    let b = &role.finalization.command.binding;
    let old = &b.lifetime_command.binding;
    let mut intent = crate::bootstrap::tests::fixture();
    intent.previous = keeper_binding_from_receipt(
        old.update_id.clone(),
        old.launch_id.clone(),
        old.keeper.clone(),
        old.job_name.clone(),
        receipt,
        Some(role.sha256().unwrap()),
    );
    intent.update.request.expected.installation_id = b.installation_id.clone();
    intent.update.request.expected.scope_sha256 = b.scope_sha256.clone();
    intent.update.request.expected.worker_build_id = receipt
        .compatibility
        .identity
        .descriptor
        .build
        .build_id
        .clone();
    intent.update.request.expected.worker_config_sha256 =
        receipt.compatibility.worker_config_sha256.clone();
    intent.update.request.expected.baseline_manifest_sha256 = b.source_manifest_sha256.clone();
    intent.parent_epoch = old.updater_epoch + 1;
    intent.parent_owner_id = Uuid::new_v4().to_string();
    intent.excluded_domain.host_id = old.compatibility.identity.process.host_id.clone();
    intent.excluded_domain.session_id = old.compatibility.identity.process.session_id;
    intent.update.accepted_at_ms = at + 1;
    intent.update.updated_at_ms = at + 1;
    intent.prepared_at_ms = at + 1;
    f.a.epoch = intent.parent_epoch;
    f.a.owner_id = intent.parent_owner_id.clone();
    f.c.execute(
        "UPDATE installation SET epoch=?1,owner=?2",
        params![f.a.epoch as i64, f.a.owner_id],
    )
    .unwrap();
    let raw = encode(&intent.update).unwrap();
    let payload = encode(&intent.update.request).unwrap();
    f.c.execute(
        "INSERT INTO updates VALUES(?1,?2,?3,?4,?5,0,1,?6)",
        params![
            intent.update.update_id,
            intent.update.request.idempotency_key,
            payload,
            crate::sha256(payload.as_bytes()),
            encode(&UpdatePhase::Requested).unwrap(),
            raw
        ],
    )
    .unwrap();
    append_transition(
        &f.c,
        &UpdateTransition {
            sequence: 0,
            update_id: intent.update.update_id.clone(),
            revision: 0,
            previous_phase: None,
            phase: UpdatePhase::Requested,
            occurred_at_ms: at + 1,
            owner_epoch: f.a.epoch,
            owner_id: f.a.owner_id.clone(),
            command: None,
        },
        None,
        &raw,
    )
    .unwrap();
    let mut spec = s.spec;
    spec.update_id = intent.update.update_id.clone();
    spec.launch_id = intent.launch_id.clone();
    spec.parent = intent.previous.worker.clone();
    spec.parent_epoch = f.a.epoch;
    spec.parent_owner_id = f.a.owner_id.clone();
    spec.token_sha256 = intent.token_sha256.clone();
    spec.image_sha256 = intent.image.sha256.clone();
    spec.image_identity = intent.image.file_identity.clone();
    spec.config_sha256 = intent.config.sha256.clone();
    spec.config_path = format!("{}\\winbridge-update.local.json", spec.runtime_directory);
    spec.image_path = format!(
        "{}\\update\\{}\\supervisor\\winbridge-update.exe",
        spec.runtime_directory, intent.update.update_id
    );
    spec.excluded_domain = Some(intent.excluded_domain.job_name.clone());
    spec.accepted_at_ms = intent.prepared_at_ms;
    let worker = spec.worker.as_mut().unwrap();
    worker.process = intent.previous.worker.clone();
    worker.generation = intent.previous.generation.clone();
    worker.context = intent.previous.context.clone();
    worker.build = receipt.compatibility.identity.descriptor.build.clone();
    worker.config_sha256 = intent.update.request.expected.worker_config_sha256.clone();
    (f, intent, spec)
}
#[test]
fn bootstrap_reservation_and_supervisor_intent_commit_atomically_without_creation() {
    let (mut f, intent, spec) = delegation_setup();
    let tx = f.c.transaction().unwrap();
    let mut checks = 0;
    let crate::journal::bootstrap::ReservationWrite::New(saved) =
        crate::journal::bootstrap::reserve(
            &tx,
            &f.a,
            intent.clone(),
            spec.clone(),
            |_, ancestor| {
                assert!(ancestor.is_none());
                checks += 1;
                Ok(())
            },
        )
        .unwrap()
    else {
        panic!("A new transaction must identify its own writes")
    };
    assert_eq!(checks, 2);
    tx.commit().unwrap();
    assert_eq!(
        crate::journal::bootstrap::read(&f.c, &intent.launch_id).unwrap(),
        Some(saved.clone())
    );
    assert_eq!(
        supervisor::read(&f.c, &intent.launch_id).unwrap(),
        saved.supervisor
    );
    assert_eq!(saved.supervisor.phase, SupervisorPhase::Intent);
    assert!(saved.supervisor.process.is_none());
    // Outgoing A->B must not masquerade as the reservation that created A.
    // Otherwise a later B->C lookup can confuse generations and block progress.
    assert!(
        crate::journal::bootstrap::read(&f.c, &intent.previous.launch_id)
            .unwrap()
            .is_none()
    );
    let tx = f.c.transaction().unwrap();
    let crate::journal::bootstrap::ReservationWrite::Existing(existing) =
        crate::journal::bootstrap::reserve(&tx, &f.a, intent.clone(), spec.clone(), |_, _| {
            panic!("Existing data must not repeat an effect checkpoint")
        })
        .unwrap()
    else {
        panic!("Existing data must never identify a fresh write")
    };
    assert_eq!(existing, saved);
    tx.commit().unwrap();
    assert!(f
        .c
        .execute("UPDATE bootstrap_reservations SET record=record", [])
        .is_err());
    assert!(f
        .c
        .execute("DELETE FROM bootstrap_reservations", [])
        .is_err());
}
#[test]
fn bootstrap_failed_checkpoint_and_ignored_reservation_roll_back_supervisor_too() {
    for fault in 0..4 {
        let (mut f, intent, spec) = delegation_setup();
        if fault == 0 {
            f.c.execute_batch("CREATE TRIGGER ignore_bootstrap BEFORE INSERT ON bootstrap_reservations BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        }
        if fault == 1 {
            f.c.execute_batch("CREATE TRIGGER ignore_bootstrap_supervisor BEFORE INSERT ON supervisor_receipts BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        }
        let tx = f.c.transaction().unwrap();
        let mut checks = 0;
        assert!(
            crate::journal::bootstrap::reserve(&tx, &f.a, intent.clone(), spec, |_, _| {
                checks += 1;
                if checks == 2 {
                    if fault == 2 {
                        return Err("Original keeper lost after writes".into());
                    }
                    if fault == 3 {
                        tx.execute("UPDATE installation SET epoch=epoch+1", [])
                            .unwrap();
                    }
                }
                Ok(())
            })
            .is_err()
        );
        drop(tx);
        assert!(
            matches!(supervisor::read(&f.c,&intent.launch_id),Err(e)if e.code==JournalErrorCode::NotFound)
        );
        assert!(
            crate::journal::bootstrap::read(&f.c, &intent.update.update_id)
                .unwrap()
                .is_none()
        );
        assert!(!supervisor::active(&f.c).unwrap());
    }
}
#[test]
fn bootstrap_changed_payload_parent_or_current_root_cannot_get_another_reservation() {
    let (mut f, intent, spec) = delegation_setup();
    let tx = f.c.transaction().unwrap();
    crate::journal::bootstrap::reserve(&tx, &f.a, intent.clone(), spec.clone(), |_, _| Ok(()))
        .unwrap();
    tx.commit().unwrap();
    for fault in 0..5 {
        let mut changed = intent.clone();
        let mut next = spec.clone();
        match fault {
            0 => {
                changed.token_sha256 = "a".repeat(64);
                next.token_sha256 = changed.token_sha256.clone();
            }
            1 => changed.parent_epoch += 1,
            2 => changed.previous.selection_sha256 = Some("0".repeat(64)),
            3 => next.image_path = "K:\\other\\winbridge-update.exe".into(),
            _ => {
                changed.update.update_id = Uuid::new_v4().to_string();
                changed.launch_id = Uuid::new_v4().to_string();
                next.update_id = changed.update.update_id.clone();
                next.launch_id = changed.launch_id.clone();
            }
        }
        let tx = f.c.transaction().unwrap();
        assert!(
            crate::journal::bootstrap::reserve(&tx, &f.a, changed, next, |_, _| Ok(())).is_err()
        );
    }
}

#[test]
fn known_bootstrap_commit_is_not_recreated_after_caller_loss_or_owner_change() {
    use crate::journal::bootstrap::BootstrapReservationAdmission;
    let (f, intent, spec) = delegation_setup();
    let mut journal = UpdateJournal {
        directory: f.a.directory.clone(),
        connection: f.c,
    };
    let mut checks = 0;
    let BootstrapReservationAdmission::Committed(known) = journal
        .commit_bootstrap_reservation(&f.a, intent.clone(), spec.clone(), |_, _| {
            checks += 1;
            Ok(())
        })
        .unwrap()
    else {
        panic!("Only the original successful COMMIT returns a known fact")
    };
    assert_eq!(checks, 2);
    assert_eq!(known.reservation().intent, intent);
    journal.require_known_bootstrap(&f.a, &known).unwrap();
    let saved = known.reservation().clone(); // Serialisable data only.
    drop(known); // Model a caller losing its only opaque commit result.
    let BootstrapReservationAdmission::Existing(existing) = journal
        .commit_bootstrap_reservation(&f.a, intent.clone(), spec.clone(), |_, _| {
            panic!("A duplicate must not run live admission again")
        })
        .unwrap()
    else {
        panic!("Saved data must not reconstruct a fresh commit fact")
    };
    assert_eq!(*existing, saved);
    journal
        .connection
        .execute("UPDATE installation SET epoch=epoch+1", [])
        .unwrap();
    assert_eq!(
        journal
            .commit_bootstrap_reservation(&f.a, intent, spec, |_, _| Ok(()))
            .err()
            .unwrap()
            .code,
        JournalErrorCode::StaleOwner
    );
}

#[test]
fn failed_actual_bootstrap_commit_returns_no_fact_and_rolls_back_both_records() {
    let (f, intent, spec) = delegation_setup();
    let mut journal = UpdateJournal {
        directory: f.a.directory.clone(),
        connection: f.c,
    };
    // A deferred FK error occurs at real SQLite COMMIT, after all reservation
    // writes/readback/checkpoints succeeded. No fake commit callback is used.
    journal.connection.execute_batch("CREATE TABLE bootstrap_commit_failure(id TEXT REFERENCES updates(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_bootstrap_commit AFTER INSERT ON bootstrap_reservations BEGIN INSERT INTO bootstrap_commit_failure VALUES('missing-root');END;").unwrap();
    let mut checks = 0;
    let error = journal
        .commit_bootstrap_reservation(&f.a, intent.clone(), spec.clone(), |_, _| {
            checks += 1;
            Ok(())
        })
        .err()
        .expect("Failed COMMIT must not mint an opaque fact");
    assert_eq!(
        checks, 2,
        "Failure must occur after both physical checkpoints"
    );
    assert!(error.to_string().contains("COMMIT unconfirmed"));
    assert!(journal.connection.is_autocommit());
    assert!(
        crate::journal::bootstrap::read(&journal.connection, &intent.launch_id)
            .unwrap()
            .is_none()
    );
    assert!(
        matches!(supervisor::read(&journal.connection,&intent.launch_id),Err(e)if e.code==JournalErrorCode::NotFound)
    );
    assert!(!supervisor::active(&journal.connection).unwrap());
    assert_eq!(
        journal.get(&intent.update.update_id).unwrap(),
        intent.update
    );
}

#[test]
fn known_bootstrap_fact_requires_original_owner_root_and_unstarted_supervisor() {
    use crate::journal::BootstrapReservationAdmission;
    for fault in 0..3 {
        let (f, intent, spec) = delegation_setup();
        let mut journal = UpdateJournal {
            directory: f.a.directory.clone(),
            connection: f.c,
        };
        let BootstrapReservationAdmission::Committed(known) = journal
            .commit_bootstrap_reservation(&f.a, intent.clone(), spec, |_, _| Ok(()))
            .unwrap()
        else {
            panic!("New reservation must commit")
        };
        match fault {
            0 => {
                journal
                    .connection
                    .execute("UPDATE installation SET epoch=epoch+1", [])
                    .unwrap();
            }
            1 => {
                let mut next = known.reservation().supervisor.clone();
                next.phase = SupervisorPhase::Spawned;
                next.revision += 1;
                next.updated_at_ms += 1;
                next.process = Some(intent.previous.coordinator.clone());
                supervisor::persist(&journal.connection, &known.reservation().supervisor, &next)
                    .unwrap();
            }
            _ => {
                // Advance only synthetic SQL metadata. This is neither a
                // baseline admission nor evidence of physical preparation.
                let mut next = intent.update.clone();
                next.phase = UpdatePhase::Preparing;
                next.revision = 1;
                next.updated_at_ms += 1;
                let raw = encode(&next).unwrap();
                let tx = journal.connection.transaction().unwrap();
                tx.execute(
                    "UPDATE updates SET phase=?1,revision=1,record=?2 WHERE id=?3",
                    params![encode(&next.phase).unwrap(), raw, next.update_id],
                )
                .unwrap();
                append_transition(
                    &tx,
                    &UpdateTransition {
                        sequence: 0,
                        update_id: next.update_id.clone(),
                        revision: 1,
                        previous_phase: Some(UpdatePhase::Requested),
                        phase: next.phase,
                        occurred_at_ms: next.updated_at_ms,
                        owner_epoch: f.a.epoch,
                        owner_id: f.a.owner_id.clone(),
                        command: None,
                    },
                    None,
                    &raw,
                )
                .unwrap();
                tx.commit().unwrap();
            }
        }
        // Immutable reservation remains readable; it is not an effect permit.
        assert!(
            crate::journal::bootstrap::read(&journal.connection, &intent.launch_id)
                .unwrap()
                .is_some()
        );
        assert!(
            journal.require_known_bootstrap(&f.a, &known).is_err(),
            "fault {fault}"
        );
    }
}

#[test]
fn known_spawned_guard_requires_the_original_commit_directory_owner_and_exact_receipt() {
    use crate::journal::BootstrapReservationAdmission;
    for fault in 0..3 {
        let (f, intent, spec) = delegation_setup();
        let mut journal = UpdateJournal {
            directory: f.a.directory.clone(),
            connection: f.c,
        };
        let BootstrapReservationAdmission::Committed(known) = journal
            .commit_bootstrap_reservation(&f.a, intent.clone(), spec, |_, _| Ok(()))
            .unwrap()
        else {
            panic!("Only original COMMIT creates a known bootstrap fact")
        };
        let mut process = intent.previous.coordinator.clone();
        process.process_id += 100;
        process.birth.push('1');
        let mut checks = 0;
        let spawned = journal
            .commit_bootstrap_spawned(&f.a, &known, &process, || {
                checks += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(checks, 2);
        assert_eq!(spawned.record().process.as_ref(), Some(&process));
        journal
            .require_bootstrap_spawned(&f.a, &known, &spawned)
            .unwrap();
        assert!(
            journal.require_known_bootstrap(&f.a, &known).is_err(),
            "Original Intent guard remains strict after Spawned"
        );
        assert!(journal
            .commit_bootstrap_spawned(&f.a, &known, &process, || panic!(
                "Existing Spawned must not mint another fact"
            ))
            .is_err());
        match fault {
            0 => journal.directory = journal.directory.join("copied-headers"),
            1 => {
                journal
                    .connection
                    .execute("UPDATE installation SET epoch=epoch+1", [])
                    .unwrap();
            }
            _ => {
                journal
                    .advance_supervisor(
                        &f.a,
                        &intent.launch_id,
                        1,
                        SupervisorPhase::Indeterminate,
                        None,
                        "Independent terminal change",
                    )
                    .unwrap();
            }
        }
        assert!(
            journal
                .require_bootstrap_spawned(&f.a, &known, &spawned)
                .is_err(),
            "fault {fault}"
        );
    }
}
#[test]
fn failed_original_spawned_commit_keeps_bootstrap_intent_without_a_fact() {
    use crate::journal::BootstrapReservationAdmission;
    let (f, intent, spec) = delegation_setup();
    let mut journal = UpdateJournal {
        directory: f.a.directory.clone(),
        connection: f.c,
    };
    let BootstrapReservationAdmission::Committed(known) = journal
        .commit_bootstrap_reservation(&f.a, intent.clone(), spec, |_, _| Ok(()))
        .unwrap()
    else {
        panic!()
    };
    journal.connection.execute_batch(r#"CREATE TABLE spawned_commit_failure(id TEXT REFERENCES updates(id) DEFERRABLE INITIALLY DEFERRED);
CREATE TRIGGER fail_spawned_commit AFTER INSERT ON supervisor_receipts WHEN NEW.phase='"spawned"' BEGIN INSERT INTO spawned_commit_failure VALUES('missing-root');END;"#).unwrap();
    let mut checks = 0;
    let error = journal
        .commit_bootstrap_spawned(&f.a, &known, &intent.previous.coordinator, || {
            checks += 1;
            Ok(())
        })
        .err()
        .expect("Actual COMMIT failure must not mint a fact");
    assert_eq!(checks, 2);
    assert!(error.to_string().contains("COMMIT unconfirmed"));
    assert!(journal.connection.is_autocommit());
    journal.require_known_bootstrap(&f.a, &known).unwrap();
    assert_eq!(
        journal.supervisor(&intent.launch_id).unwrap().phase,
        SupervisorPhase::Intent
    );
    let count: i64 = journal
        .connection
        .query_row(
            "SELECT count(*) FROM supervisor_receipts WHERE launch_id=?1 AND phase=?2",
            params![intent.launch_id, encode(&SupervisorPhase::Spawned).unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn original_spawned_checkpoints_rollback_the_bootstrap_receipt_on_either_failure() {
    use crate::journal::BootstrapReservationAdmission;
    for failure in 1..=2 {
        let (f, intent, spec) = delegation_setup();
        let mut journal = UpdateJournal {
            directory: f.a.directory.clone(),
            connection: f.c,
        };
        let BootstrapReservationAdmission::Committed(known) = journal
            .commit_bootstrap_reservation(&f.a, intent.clone(), spec, |_, _| Ok(()))
            .unwrap()
        else {
            panic!()
        };
        let mut checks = 0;
        assert!(journal
            .commit_bootstrap_spawned(&f.a, &known, &intent.previous.coordinator, || {
                checks += 1;
                if checks == failure {
                    Err("Physical scope changed".into())
                } else {
                    Ok(())
                }
            })
            .is_err());
        assert_eq!(checks, failure);
        journal.require_known_bootstrap(&f.a, &known).unwrap();
        assert_eq!(
            journal.supervisor(&intent.launch_id).unwrap(),
            known.reservation().supervisor
        );
    }
}
