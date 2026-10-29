//! Real same-Store/IPC/fences with synthetic runtime/keeper metadata. These
//! fixtures do not claim Windows authentication, Job membership or native V6.
use super::*;

pub(super) fn proposal(
    service: &LocalService,
) -> (
    WorkerLifetimeActivationCommand,
    WorkerLifetimeActivationReceipt,
) {
    let s = service.command();
    let writer = &s.binding.writer_command;
    let physical = &writer.binding.physical;
    let at = store::now_ms().max(s.issued_at_ms);
    let mut keeper = physical.process.clone();
    keeper.process_id += 1;
    let compatibility = winbridge_protocol::WorkerRuntimeCompatibility {
        schema_version: 1,
        kind: "worker_runtime_compatibility".into(),
        identity: s.binding.expected_worker.clone(),
        context: physical.context.clone(),
        generation: physical.generation.clone(),
        worker_config_sha256: writer.binding.worker_config_sha256.clone(),
        worker_config_schema: 1,
        worker_journal_schema: writer.binding.worker_journal_schema,
        updater_journal_schema: writer.binding.updater_journal_schema,
        protocol_version: winbridge_protocol::PROTOCOL_VERSION,
        observed_at_ms: at,
    };
    let command = WorkerLifetimeActivationCommand {
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
            compatibility,
            authority: s.binding.writer_receipt.authority.clone(),
        },
    };
    lifetime::validate_command_envelope(&command).unwrap();
    let receipt = WorkerLifetimeActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: command.binding.update_id.clone(),
        attempt_id: command.binding.attempt_id.clone(),
        command_sha256: lifetime::command_sha256(&command).unwrap(),
        keeper: command.binding.keeper.clone(),
        job_name: command.binding.job_name.clone(),
        compatibility: command.binding.compatibility.clone(),
        authority: command.binding.authority.clone(),
        mode: command.mode,
    };
    (command, receipt)
}

pub(super) fn fixture(root: &Path) -> LocalService {
    let (prepared, config, _) = crate::store::lifetime_fixture(root);
    LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap()
}

pub(super) fn finalization_proposal(
    running: &RunningLocalService,
    old: WorkerLifetimeActivationCommand,
) -> WorkerFinalizationCommand {
    let shutdown = &running
        .service
        .worker
        .local_activation
        .as_ref()
        .unwrap()
        .shutdown;
    WorkerFinalizationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: store::now_ms(),
        binding: ActivationFinalizationBinding {
            committed_root_revision: old.binding.root_revision + 1,
            lifetime_command: old,
            lifetime_record_sha256: "1".repeat(64),
            shutdown_record_sha256: "2".repeat(64),
            installation_id: uuid::Uuid::new_v4().to_string(),
            scope_sha256: "3".repeat(64),
            expected_lease_revision: shutdown.lease.revision,
            source_manifest_sha256: shutdown.lease.spec.target_manifest_sha256.clone(),
            resident_request_id: uuid::Uuid::new_v4().to_string(),
            resident_observation_sha256: "4".repeat(64),
        },
    }
}

#[test]
fn finalization_rejects_registry_content_and_contention_without_waiting_under_store_lock() {
    use super::finalization_service::require_quiet_registries;
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let worker = &service.worker;
    let _state = worker.store().unwrap();
    require_quiet_registries(worker).unwrap();
    worker.active_jobs.store(1, Ordering::Release);
    assert!(require_quiet_registries(worker).is_err());
    worker.active_jobs.store(0, Ordering::Release);
    worker.maintenance_running.store(true, Ordering::Release);
    assert!(require_quiet_registries(worker).is_err());
    worker.maintenance_running.store(false, Ordering::Release);
    let mut active = worker.active_projects.lock().unwrap();
    assert!(require_quiet_registries(worker)
        .unwrap_err()
        .contains("busy/poisoned"));
    active.insert("source".into(), "synthetic-never-run".into());
    drop(active);
    assert!(require_quiet_registries(worker).is_err());
    worker.active_projects.lock().unwrap().clear();
    worker
        .checks
        .lock()
        .unwrap()
        .insert("synthetic-check".into(), None);
    assert!(require_quiet_registries(worker).is_err());
    worker.checks.lock().unwrap().clear();
    worker
        .unreconciled_tasks
        .lock()
        .unwrap()
        .insert("synthetic".into(), "unknown".into());
    assert!(require_quiet_registries(worker).is_err());
    worker.unreconciled_tasks.lock().unwrap().clear();
    worker.maintenance.lock().unwrap().1 = Some("synthetic".into());
    assert!(require_quiet_registries(worker).is_err());
    worker.maintenance.lock().unwrap().1 = None;
    require_quiet_registries(worker).unwrap();
}

#[tokio::test]
async fn pending_watchdog_accepts_exact_sql_release_without_opening_dispatch_or_minting_active() {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (old, entry) = proposal(&service);
    let mut running = service.start().await.unwrap();
    running
        .seal_supervised_lifetime(
            old.clone(),
            entry,
            LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true))),
        )
        .unwrap();
    let worker = Arc::clone(&running.service.worker);
    let scope = worker.local_activation.as_ref().unwrap();
    let resident = match &*scope.lifetime.lock().unwrap() {
        ServiceLifetime::Supervised(value) => Arc::clone(value),
        _ => panic!("Supervised"),
    };
    let command = finalization_proposal(&running, old.clone());
    running
        .seal_finalization_pending(command.clone(), &resident)
        .unwrap();
    let late = scope.deadline + Duration::from_secs(1);
    worker.check_local_service_scope_at(late).unwrap();
    let before = worker.request_context().unwrap();
    let release = worker
        .store()
        .unwrap()
        .release_activated_service(&scope.command, &scope.shutdown, &command, || Ok(()))
        .unwrap();
    assert!(matches!(
        release,
        store::activation_finalization::ActivatedReleaseAdmission::Committed(_)
    ));
    // This was the crucial SQL/mode gap: the old exact-Quiesced watchdog would
    // fail here, despite this process still owning the same Store and keeper.
    worker.check_local_service_scope_at(late).unwrap();
    assert_eq!(worker.request_context().unwrap(), before);
    assert!(matches!(
        worker
            .handle_lifetime_observation(ActivationLifetimeQuery {
                lifetime_observation_version: 1,
                command: old,
            })
            .result,
        ActivationLifetimeObservationResult::Rejected { .. }
    ));
    let winbridge_protocol::LocalResponse::Info { info } =
        worker.handle(winbridge_protocol::LocalRequest::Info)
    else {
        panic!("Info")
    };
    assert!(
        !info.maintenance_available && !info.workflow_available && !info.snapshot_import_available
    );
    assert!(!info.fenced_requests.unwrap().legacy_mutations_allowed);
    assert!(matches!(
        *scope.lifetime.lock().unwrap(),
        ServiceLifetime::ReleasePending(_)
    ));
    drop(worker);
    running.close().await.unwrap();
}

#[tokio::test]
async fn pending_mode_refuses_foreign_owner_replay_or_dead_keeper_and_never_reopens_state() {
    for fault in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let service = fixture(root.path());
        let (old, entry) = proposal(&service);
        let mut running = service.start().await.unwrap();
        let live = Arc::new(AtomicBool::new(true));
        running
            .seal_supervised_lifetime(
                old.clone(),
                entry,
                LifetimeKeeper::Fixture(Arc::clone(&live)),
            )
            .unwrap();
        let worker = Arc::clone(&running.service.worker);
        let scope = worker.local_activation.as_ref().unwrap();
        let resident = match &*scope.lifetime.lock().unwrap() {
            ServiceLifetime::Supervised(value) => Arc::clone(value),
            _ => panic!("Supervised"),
        };
        let mut command = finalization_proposal(&running, old);
        let expected = if fault == 0 {
            Arc::new(ResidentLifetime {
                command: resident.command.clone(),
                receipt: resident.receipt.clone(),
                keeper: LifetimeKeeper::Fixture(Arc::clone(&live)),
            })
        } else {
            Arc::clone(&resident)
        };
        if fault == 1 {
            command.binding.expected_lease_revision += 1;
        }
        if fault == 2 {
            live.store(false, Ordering::Release);
        }
        if fault == 3 {
            running
                .seal_finalization_pending(command.clone(), &resident)
                .unwrap();
        }
        assert!(running
            .seal_finalization_pending(command, &expected)
            .is_err());
        assert_eq!(
            worker.store().unwrap().active_worker_update().unwrap(),
            Some(scope.shutdown.lease.clone())
        );
        if fault != 3 {
            assert!(matches!(
                *scope.lifetime.lock().unwrap(),
                ServiceLifetime::Supervised(_)
            ));
        }
        drop(worker);
        running.close().await.unwrap();
    }
}

#[tokio::test]
async fn resident_queries_cannot_enter_a_mode_spoof_a_keeper_or_change_store_authority() {
    for supervised in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let service = fixture(root.path());
        let (command, entry) = proposal(&service);
        let mut running = service.start().await.unwrap();
        let authority = running
            .service
            .worker
            .store()
            .unwrap()
            .state_backup_authority()
            .unwrap();
        let context = running.service.worker.request_context().unwrap();
        if supervised {
            running
                .seal_supervised_lifetime(
                    command.clone(),
                    entry,
                    LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true))),
                )
                .unwrap();
        }
        let query = ActivationResidentQuery {
            resident_observation_version: ACTIVATION_RESIDENT_OBSERVATION_CONTRACT,
            request_id: uuid::Uuid::new_v4().to_string(),
            command,
        };
        for mutation in 0..5 {
            let mut raw = serde_json::to_value(&query).unwrap();
            match mutation {
                0 => {}
                1 => raw["residentObservationVersion"] = 6.into(),
                2 => raw["release"] = true.into(),
                3 => raw["processId"] = query.command.binding.keeper.process_id.into(),
                _ => raw["requestId"] = "x".repeat(4096).into(),
            }
            let observed: ActivationResidentObservation = winbridge_transport::supervisor::call(
                &running.endpoint().unwrap(),
                std::process::id(),
                &raw,
            )
            .await
            .unwrap();
            let ActivationResidentObservationResult::Rejected { message } = observed.result else {
                panic!("query must not adopt resident mode");
            };
            if mutation == 0 {
                assert!(
                    message.contains(if supervised {
                        "actual original keeper peer"
                    } else {
                        "already supervised runtime"
                    }),
                    "{message}"
                );
            }
            if mutation >= 2 {
                assert!(observed.request_id.is_none());
            }
        }
        assert_eq!(running.service.worker.request_context().unwrap(), context);
        assert_eq!(
            running
                .service
                .worker
                .store()
                .unwrap()
                .state_backup_authority()
                .unwrap(),
            authority
        );
        assert_eq!(
            running
                .service
                .worker
                .check_local_service_scope_at(running.service.deadline + Duration::from_secs(1))
                .is_ok(),
            supervised
        );
        running.close().await.unwrap();
    }
}

#[tokio::test]
async fn original_window_still_gates_entry_and_a_query_cannot_arm_a_mode() {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (command, receipt) = proposal(&service);
    let mut running = service.start().await.unwrap();
    let query = ActivationLifetimeQuery {
        lifetime_observation_version: 1,
        command: command.clone(),
    };
    let observed: ActivationLifetimeObservation = winbridge_transport::supervisor::call(
        &running.endpoint().unwrap(),
        std::process::id(),
        &query,
    )
    .await
    .unwrap();
    assert!(matches!(
        observed.result,
        ActivationLifetimeObservationResult::Rejected { .. }
    ));
    for change in 0..3 {
        let mut raw = serde_json::to_value(&query).unwrap();
        match change {
            0 => raw["cancel"] = true.into(),
            1 => raw["lifetimeObservationVersion"] = true.into(),
            _ => raw["command"]["mode"] = "normal".into(),
        }
        let observed: ActivationLifetimeObservation = winbridge_transport::supervisor::call(
            &running.endpoint().unwrap(),
            std::process::id(),
            &raw,
        )
        .await
        .unwrap();
        assert!(matches!(
            observed.result,
            ActivationLifetimeObservationResult::Rejected { .. }
        ));
    }
    let expired = running.service.deadline + Duration::from_secs(1);
    assert!(running
        .service
        .worker
        .check_local_service_scope_at(expired)
        .is_err());
    assert!(running
        .seal_supervised_lifetime_with_clock(
            command,
            receipt,
            LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true))),
            || expired
        )
        .unwrap_err()
        .contains("monotonic"));
    running.close().await.unwrap();
}

#[tokio::test]
async fn accepted_mode_keeps_the_same_store_and_frozen_dispatch_after_verification_expires() {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (command, receipt) = proposal(&service);
    let mut running = service.start().await.unwrap();
    let before = running.service.worker.request_context().unwrap();
    let authority = running
        .service
        .worker
        .store()
        .unwrap()
        .state_backup_authority()
        .unwrap();
    let keeper = Arc::new(AtomicBool::new(true));
    let entered = running
        .seal_supervised_lifetime(
            command.clone(),
            receipt.clone(),
            LifetimeKeeper::Fixture(keeper.clone()),
        )
        .unwrap();
    lifetime::validate_receipt(&entered, &command).unwrap();
    assert!(running
        .seal_supervised_lifetime(
            command.clone(),
            receipt,
            LifetimeKeeper::Fixture(keeper.clone())
        )
        .is_err());
    running
        .service
        .worker
        .check_local_service_scope_at(running.service.deadline + Duration::from_secs(1))
        .unwrap();
    running.service.worker.check_local_service_scope().unwrap();
    assert_eq!(running.service.worker.request_context().unwrap(), before);
    assert_eq!(
        running
            .service
            .worker
            .store()
            .unwrap()
            .state_backup_authority()
            .unwrap(),
        authority
    );
    let reply = running
        .service
        .worker
        .handle(winbridge_protocol::LocalRequest::Submit {
            request: winbridge_protocol::TaskSubmission {
                protocol_version: 1,
                request_id: "never-run".into(),
                project_id: "source".into(),
                idempotency_key: "never-run".into(),
                timeout_ms: 1000,
                snapshot_id: None,
                command: winbridge_protocol::CommandSource::Direct {
                    executable: "never-executed".into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                },
            },
        });
    assert!(matches!(
        reply,
        winbridge_protocol::LocalResponse::Error { .. }
    ));
    // Synthetic metadata must still fail the native normal-IPC evidence path.
    assert!(matches!(
        running
            .service
            .worker
            .handle_lifetime_observation(ActivationLifetimeQuery {
                lifetime_observation_version: 1,
                command,
            })
            .result,
        ActivationLifetimeObservationResult::Rejected { .. }
    ));
    keeper.store(false, Ordering::Release);
    let result = tokio::time::timeout(Duration::from_secs(5), running.hold_supervised_lifetime())
        .await
        .unwrap();
    assert!(result.unwrap_err().contains("keeper lost"));
    running.close().await.unwrap();
}

#[tokio::test]
async fn an_otherwise_valid_command_cannot_change_the_resident_generation_or_authority() {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (command, receipt) = proposal(&service);
    let mut running = service.start().await.unwrap();
    for change in 0..2 {
        let mut c = command.clone();
        let mut r = receipt.clone();
        if change == 0 {
            c.binding.compatibility.generation = uuid::Uuid::new_v4().to_string();
            r.compatibility = c.binding.compatibility.clone();
        } else {
            c.binding.authority.epoch += 1;
            r.authority = c.binding.authority.clone();
        }
        r.command_sha256 = lifetime::command_sha256(&c).unwrap();
        lifetime::validate_receipt(&r, &c).unwrap();
        assert!(running
            .seal_supervised_lifetime(
                c,
                r,
                LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true)))
            )
            .unwrap_err()
            .contains("resident service binding"));
    }
    running.close().await.unwrap();
}

#[tokio::test]
async fn a_changed_actual_worker_authority_blocks_entry_and_resident_continuation() {
    for entered in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let service = fixture(root.path());
        let (command, receipt) = proposal(&service);
        let mut running = service.start().await.unwrap();
        let live = Arc::new(AtomicBool::new(true));
        if entered {
            running
                .seal_supervised_lifetime(
                    command.clone(),
                    receipt.clone(),
                    LifetimeKeeper::Fixture(live.clone()),
                )
                .unwrap();
        }
        let external = rusqlite::Connection::open(
            running
                .service
                .worker
                .config
                .state_directory
                .join("journal.sqlite"),
        )
        .unwrap();
        external
            .execute(
                "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
                [uuid::Uuid::new_v4().to_string()],
            )
            .unwrap();
        assert!(running.service.worker.check_local_service_scope().is_err());
        if !entered {
            assert!(running
                .seal_supervised_lifetime(command, receipt, LifetimeKeeper::Fixture(live))
                .is_err());
        }
        drop(external);
        running.close().await.unwrap();
    }
}

#[test]
fn receiver_config_pin_handoff_retains_the_real_service_pin_until_service_drop() {
    let temp = tempfile::tempdir().unwrap();
    let service = fixture(temp.path());
    let path = service
        .worker
        .config_path
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let initial = options.open(&path).unwrap();
    let id = winbridge_platform::file_identity(&initial).unwrap();
    assert_eq!(
        winbridge_platform::file_identity(service._config_pin.as_ref().unwrap()).unwrap(),
        id
    );
    #[cfg(windows)]
    assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
    service.retire_receiver_config_pin(initial).unwrap();
    assert_eq!(
        winbridge_platform::file_identity(service._config_pin.as_ref().unwrap()).unwrap(),
        id
    );
    assert!(matches!(
        *service
            .worker
            .local_activation
            .as_ref()
            .unwrap()
            .lifetime
            .lock()
            .unwrap(),
        ServiceLifetime::Verification
    ));
    #[cfg(windows)]
    assert!(
        std::fs::OpenOptions::new().write(true).open(&path).is_err(),
        "Retiring the duplicate must not expose a writable config"
    );
    drop(service);
    assert!(
        std::fs::OpenOptions::new().write(true).open(&path).is_ok(),
        "No hidden receiver pin may survive LocalService drop"
    );
}

#[test]
fn receiver_config_pin_handoff_refuses_a_foreign_file_without_releasing_service_pin() {
    let temp = tempfile::tempdir().unwrap();
    let service = fixture(temp.path());
    let path = temp.path().join("foreign-config.json");
    std::fs::write(&path, b"{}\n").unwrap();
    let foreign = std::fs::File::open(&path).unwrap();
    let retained =
        winbridge_platform::file_identity(service._config_pin.as_ref().unwrap()).unwrap();
    assert_ne!(
        retained,
        winbridge_platform::file_identity(&foreign).unwrap()
    );
    assert!(service.retire_receiver_config_pin(foreign).is_err());
    assert_eq!(
        retained,
        winbridge_platform::file_identity(service._config_pin.as_ref().unwrap()).unwrap()
    );
    assert_eq!(
        service
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .unwrap()
            .phase,
        winbridge_protocol::update::WorkerUpdatePhase::Quiesced
    );
}
