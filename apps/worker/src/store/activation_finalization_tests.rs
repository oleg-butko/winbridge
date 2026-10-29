//! SQLite/locked-state tests only. Synthetic graph/checkpoints do not prove
//! authenticated V8, an OS keeper, physical quiescence or Active readiness.
use super::*;
use crate::store::activation_service_open::ServiceRuntimeState;
use std::path::Path;
use winbridge_update::activation::lifetime;

fn fixture(root: &Path, finish: bool) -> (ServiceRuntimeState, WorkerFinalizationCommand) {
    let (prepared, _, _) = crate::store::lifetime_fixture(root);
    let mut state = prepared.into_runtime(|| Ok(())).unwrap();
    if finish {
        state.store.finish_service_initialization().unwrap();
    }
    let s = &state.command;
    let writer = &s.binding.writer_command;
    let p = &writer.binding.physical;
    let mut keeper = p.process.clone();
    keeper.process_id += 1;
    let at = now_ms().max(s.issued_at_ms);
    let old = WorkerLifetimeActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        mode: WorkerLifetimeActivationMode::SupervisedQuarantine,
        issued_at_ms: at,
        deadline_at_ms: s.deadline_at_ms,
        binding: WorkerLifetimeActivationBinding {
            update_id: writer.binding.update_id.clone(),
            attempt_id: writer.binding.attempt_id.clone(),
            launch_id: writer.binding.launch_id.clone(),
            updater_owner_id: writer.binding.updater_owner_id.clone(),
            updater_epoch: writer.binding.updater_epoch,
            root_revision: 1,
            controller_command_id: uuid::Uuid::new_v4().to_string(),
            controller_record_sha256: "a".repeat(64),
            local_record_sha256: "b".repeat(64),
            keeper,
            job_name: format!("Global\\WinBridge-Activation-{}", "c".repeat(64)),
            compatibility: winbridge_protocol::WorkerRuntimeCompatibility {
                schema_version: 1,
                kind: "worker_runtime_compatibility".into(),
                identity: s.binding.expected_worker.clone(),
                context: p.context.clone(),
                generation: p.generation.clone(),
                worker_config_sha256: writer.binding.worker_config_sha256.clone(),
                worker_config_schema: s.binding.worker_config_schema,
                worker_journal_schema: writer.binding.worker_journal_schema,
                updater_journal_schema: writer.binding.updater_journal_schema,
                protocol_version: s.binding.protocol_version,
                observed_at_ms: at,
            },
            authority: s.binding.writer_receipt.authority.clone(),
        },
    };
    lifetime::validate_command_envelope(&old).unwrap();
    let command = WorkerFinalizationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: at,
        binding: ActivationFinalizationBinding {
            lifetime_command: old,
            lifetime_record_sha256: "1".repeat(64),
            shutdown_record_sha256: "2".repeat(64),
            installation_id: uuid::Uuid::new_v4().to_string(),
            scope_sha256: "3".repeat(64),
            committed_root_revision: 2,
            expected_lease_revision: state.shutdown.lease.revision,
            source_manifest_sha256: state.shutdown.lease.spec.target_manifest_sha256.clone(),
            resident_request_id: uuid::Uuid::new_v4().to_string(),
            resident_observation_sha256: "4".repeat(64),
        },
    };
    finalization::validate_command_envelope(&command).unwrap();
    (state, command)
}

fn bytes(s: &ServiceRuntimeState) -> (Vec<u8>, Option<Vec<u8>>) {
    crate::store::activation_open::tests::bytes(Path::new(&s.shutdown.state.state_directory))
}
fn release(
    s: &mut ServiceRuntimeState,
    command: &WorkerFinalizationCommand,
    checkpoint: impl FnMut() -> Result<()>,
) -> Result<ActivatedReleaseAdmission> {
    s.store
        .release_activated_service(&s.command, &s.shutdown, command, checkpoint)
}

#[test]
fn activated_release_commits_once_preserves_history_pause_and_same_authority() {
    let root = tempfile::tempdir().unwrap();
    let (mut s, command) = fixture(root.path(), true);
    let authority = s.store.state_backup_authority().unwrap();
    let old = s.shutdown.clone();
    let context = winbridge_protocol::RequestContext {
        instance_id: command
            .binding
            .lifetime_command
            .binding
            .compatibility
            .context
            .instance_id
            .clone(),
        journal_incarnation: authority.incarnation.clone(),
    };
    let queue = s.store.queue_observation(context.clone()).unwrap();
    assert_eq!(queue.mode, winbridge_protocol::QueueMode::Paused);
    assert!(s
        .store
        .advance_worker_update(
            &old.lease.spec,
            &finalization::release_command(&command).unwrap(),
            WorkerUpdateBlockers {
                accepted_operations: 0,
                logical_owners: 0,
                in_flight_effects: 0,
                physical_owners: 0,
                uncertain_owners: 0
            }
        )
        .unwrap_err()
        .contains("shutdown accepted"));
    assert!(s
        .store
        .observe_service_finalization_release(&s.command, &old, &command)
        .unwrap()
        .is_none());
    let mut checkpoints = 0;
    let ActivatedReleaseAdmission::Committed(permit) = release(&mut s, &command, || {
        checkpoints += 1;
        Ok(())
    })
    .unwrap() else {
        panic!("only a known first COMMIT mints a permit")
    };
    assert_eq!(checkpoints, 3);
    assert_eq!(permit.command(), &command);
    let lease = permit.lease().clone();
    assert_eq!(
        lease,
        released(
            &old,
            &finalization::release_command(&command).unwrap(),
            lease.updated_at_ms
        )
        .unwrap()
    );
    assert!(s.store.active_worker_update().unwrap().is_none());
    assert_eq!(s.store.state_backup_authority().unwrap(), authority);
    assert_eq!(
        s.store
            .worker_shutdown_receipt(&old.update_id, &old.command)
            .unwrap(),
        Some(old.clone())
    );
    let after = s.store.queue_observation(context).unwrap();
    assert_eq!(after.mode, queue.mode);
    assert_eq!(after.revision, queue.revision);
    assert_eq!(s.store.recent().unwrap().len(), 1);
    assert!(!crate::store::activation_open::tests::lock_available(
        Path::new(&old.state.state_directory)
    ));
    // SQL completion is insufficient for the old supervised watchdog. The new
    // pending-mode branch must use the exact release observer instead.
    assert!(s
        .store
        .check_supervised_service_identity(&s.command, &old)
        .is_err());
    assert_eq!(
        s.store
            .observe_service_finalization_release(&s.command, &old, &command)
            .unwrap(),
        Some(lease.clone())
    );
    let before = bytes(&s);
    assert!(
        matches!(release(&mut s, &command, || Ok(())).unwrap(), ActivatedReleaseAdmission::Observed(saved) if *saved == lease)
    );
    assert_eq!(bytes(&s), before);
}

#[test]
fn activated_release_rolls_back_every_checkpoint_and_ignored_write() {
    for fault in 0..5 {
        let root = tempfile::tempdir().unwrap();
        let (mut s, command) = fixture(root.path(), true);
        match fault {
            3 => s.store.connection.execute_batch("CREATE TRIGGER ignore_release BEFORE UPDATE ON worker_update_leases BEGIN SELECT RAISE(IGNORE); END;").unwrap(),
            4 => s.store.connection.execute_batch("CREATE TRIGGER ignore_release BEFORE INSERT ON worker_update_commands BEGIN SELECT RAISE(IGNORE); END;").unwrap(),
            _ => {}
        }
        let before = bytes(&s);
        let mut calls = 0;
        assert!(
            release(&mut s, &command, || {
                calls += 1;
                if fault < 3 && calls == fault + 1 {
                    Err("fixture original owner lost".into())
                } else {
                    Ok(())
                }
            })
            .is_err(),
            "fault {fault}"
        );
        assert_eq!(bytes(&s), before, "fault {fault}");
        assert_eq!(
            s.store.active_worker_update().unwrap(),
            Some(s.shutdown.lease.clone())
        );
        assert!(s
            .store
            .observe_service_finalization_release(&s.command, &s.shutdown, &command)
            .unwrap()
            .is_none());
    }
}

#[test]
fn activated_release_refuses_scope_mutations_and_rebound_saved_keys() {
    let root = tempfile::tempdir().unwrap();
    let (mut s, command) = fixture(root.path(), true);
    for field in 0..15 {
        let mut bad = command.clone();
        match field {
            0 => bad.binding.expected_lease_revision += 1,
            1 => {
                bad.binding.source_manifest_sha256 =
                    s.shutdown.lease.spec.baseline_manifest_sha256.clone()
            }
            2 => bad.binding.lifetime_command.binding.update_id = uuid::Uuid::new_v4().to_string(),
            3 => bad.binding.lifetime_command.binding.attempt_id = uuid::Uuid::new_v4().to_string(),
            4 => bad.binding.lifetime_command.binding.launch_id = uuid::Uuid::new_v4().to_string(),
            5 => {
                bad.binding.lifetime_command.binding.updater_owner_id =
                    uuid::Uuid::new_v4().to_string()
            }
            6 => bad.binding.lifetime_command.binding.updater_epoch += 1,
            7 => bad.binding.lifetime_command.binding.authority.epoch += 1,
            8 => {
                bad.binding
                    .lifetime_command
                    .binding
                    .compatibility
                    .generation = uuid::Uuid::new_v4().to_string()
            }
            9 => {
                bad.binding
                    .lifetime_command
                    .binding
                    .compatibility
                    .identity
                    .set_id = uuid::Uuid::new_v4().to_string()
            }
            10 => {
                bad.binding
                    .lifetime_command
                    .binding
                    .compatibility
                    .worker_config_sha256 = "5".repeat(64)
            }
            11 => bad.binding.lifetime_command.deadline_at_ms += 1,
            12 => bad.command_id = s.command.command_id.clone(),
            13 => bad.command_id = s.command.binding.writer_command.command_id.clone(),
            _ => bad.command_id = s.shutdown.command.command_id.clone(),
        }
        let before = bytes(&s);
        assert!(
            release(&mut s, &bad, || panic!(
                "invalid scope before physical checkpoint"
            ))
            .is_err(),
            "mutation {field}"
        );
        assert_eq!(bytes(&s), before);
    }
    release(&mut s, &command, || Ok(())).unwrap();
    for field in 0..2 {
        let mut bad = command.clone();
        if field == 0 {
            bad.command_id = uuid::Uuid::new_v4().to_string();
        } else {
            bad.binding.source_manifest_sha256 = "6".repeat(64);
        }
        let before = bytes(&s);
        assert!(release(&mut s, &bad, || Ok(())).is_err());
        assert!(s
            .store
            .observe_service_finalization_release(&s.command, &s.shutdown, &bad)
            .is_err());
        assert_eq!(bytes(&s), before);
    }
}

#[test]
fn activated_release_cannot_bypass_initialization_stale_authority_or_readonly_store() {
    for fault in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let (mut s, command) = fixture(root.path(), fault != 0);
        if fault == 1 {
            s.store
                .connection
                .execute(
                    "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
        }
        if fault == 2 {
            let readonly = Store::open_read_only(
                &Path::new(&s.shutdown.state.state_directory).join("journal.sqlite"),
            )
            .unwrap();
            s.store = readonly;
        }
        let before = bytes(&s);
        assert!(release(&mut s, &command, || Ok(())).is_err());
        assert_eq!(bytes(&s), before);
    }
}

#[test]
fn pending_release_observer_refuses_another_active_update_in_the_sql_mode_gap() {
    let root = tempfile::tempdir().unwrap();
    let (mut s, command) = fixture(root.path(), true);
    release(&mut s, &command, || Ok(())).unwrap();
    let mut other = s.shutdown.lease.spec.clone();
    other.update_id = uuid::Uuid::new_v4().to_string();
    // The Store API is lower-level than updater23's pending admission barrier.
    // Simulate a violated barrier; the pending watchdog must fail closed.
    s.store.begin_worker_update(other).unwrap();
    let before = bytes(&s);
    assert!(s
        .store
        .observe_service_finalization_release(&s.command, &s.shutdown, &command)
        .unwrap_err()
        .contains("Another active update"));
    assert_eq!(bytes(&s), before);
}

#[test]
fn activated_release_refuses_unexpected_work_and_a_future_command_without_mutating() {
    for fault in 0..2 {
        let root = tempfile::tempdir().unwrap();
        let (mut s, mut command) = fixture(root.path(), true);
        if fault == 0 {
            // Ordinary admission is frozen. Inject an inconsistent accepted
            // record to verify the release transaction's own logical checks.
            s.store
                .connection
                .execute(
                    "UPDATE tasks SET revision=revision+1,record=json_set(record,'$.status','queued')",
                    [],
                )
                .unwrap();
        } else {
            command.issued_at_ms += 60_000;
        }
        let before = bytes(&s);
        let error = release(&mut s, &command, || Ok(())).err().unwrap();
        assert!(
            error.contains(if fault == 0 {
                "accepted work"
            } else {
                "future"
            }),
            "{error}"
        );
        assert_eq!(bytes(&s), before);
    }
}
