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
            "DROP TABLE keeper_selections;PRAGMA user_version=23;PRAGMA wal_checkpoint(TRUNCATE);",
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
    assert_eq!(after.schema_version, 24);
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
