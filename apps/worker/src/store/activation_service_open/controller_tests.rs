//! Real state/IPC/network lifecycle, synthetic loaded-image metadata. No native
//! activation permission or controller-confirmation receipt is claimed here.
use super::*;
use winbridge_protocol::*;

fn command(
    service: &WorkerServiceActivationCommand,
    config: &Path,
) -> WorkerControllerActivationCommand {
    let physical = &service.binding.writer_command.binding.physical;
    let receipt = WorkerServiceActivationReceipt {
        schema_version: 1,
        command_id: service.command_id.clone(),
        update_id: service.binding.writer_command.binding.update_id.clone(),
        attempt_id: service.binding.writer_command.binding.attempt_id.clone(),
        command_sha256: winbridge_update::activation::service::command_sha256(service).unwrap(),
        authority: service.binding.writer_receipt.authority.clone(),
        compatibility: WorkerRuntimeCompatibility {
            schema_version: 1,
            kind: "worker_runtime_compatibility".into(),
            identity: service.binding.expected_worker.clone(),
            context: physical.context.clone(),
            generation: physical.generation.clone(),
            worker_config_sha256: service
                .binding
                .writer_command
                .binding
                .worker_config_sha256
                .clone(),
            worker_config_schema: 1,
            worker_journal_schema: crate::store::JOURNAL_SCHEMA_VERSION,
            updater_journal_schema: service
                .binding
                .writer_command
                .binding
                .updater_journal_schema,
            protocol_version: PROTOCOL_VERSION,
            observed_at_ms: service.issued_at_ms,
        },
    };
    let config: winbridge_core::config::WorkerConfig =
        serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
    WorkerControllerActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: crate::store::now_ms().max(service.issued_at_ms),
        deadline_at_ms: service.deadline_at_ms,
        binding: WorkerControllerActivationBinding {
            service_command: service.clone(),
            service_receipt: receipt,
            service_record_sha256: "a".repeat(64),
            local_record_sha256: "b".repeat(64),
            topology: winbridge_update::activation::controller::topology_from_config(&config, None)
                .unwrap(),
        },
    }
}

async fn info(endpoint: &str) -> Box<WorkerInfo> {
    let result: LocalResponse =
        winbridge_transport::supervisor::call(endpoint, std::process::id(), &LocalRequest::Info)
            .await
            .unwrap();
    let LocalResponse::Info { info } = result else {
        panic!("Info")
    };
    info
}
async fn connections(endpoint: &str) -> ControllerConnections {
    let info = info(endpoint).await;
    let request = ControllerConnectionsRequest {
        context: info.fenced_requests.as_ref().unwrap().context.clone(),
    };
    let envelope = controller_connections_envelope(&info, &request).unwrap();
    let result = winbridge_transport::supervisor::call(endpoint, std::process::id(), &envelope)
        .await
        .unwrap();
    decode_controller_connections(result, &info, &request).unwrap()
}

// Networking tests run on Debian. Native Windows tests must use the explicitly
// approved stable EXE path, never this hashed Cargo test executable.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_to_four_links_reuse_same_owner_and_are_joined_without_reopening_dispatch() {
    for count in 1..=4 {
        let root = tempfile::tempdir().unwrap();
        let mut listeners = Vec::new();
        let mut endpoints = Vec::new();
        for i in 0..count {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            endpoints.push(serde_json::json!({"id":format!("p{i}"),"url":format!("ws://{}",listener.local_addr().unwrap())}));
            listeners.push(listener);
        }
        let (prepared, config, task) = service_fixture_with(
            root.path(),
            serde_json::json!({"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":"p0","controllers":endpoints}}),
        );
        let command = command(prepared.command(), &config);
        let authority = prepared.command().binding.writer_receipt.authority.clone();
        let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
        let mut running = service.start().await.unwrap();
        let endpoint = running.endpoint().unwrap();
        let before = info(&endpoint).await;
        let mut checkpoints = 0;
        running
            .start_controller(command.clone(), || {
                checkpoints += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(checkpoints, 2);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let observation = connections(&endpoint).await;
            assert_eq!(observation.connections.len(), count);
            if observation.connections.iter().all(|l| l.attempts == "1") {
                break;
            }
            assert!(std::time::Instant::now() < until);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(running
            .start_controller(command.clone(), || panic!(
                "Duplicate dispatch reached checkpoint"
            ))
            .is_err());
        let query = ActivationControllerQuery {
            controller_observation_version: 1,
            command,
        };
        let observation: ActivationControllerObservation =
            winbridge_transport::supervisor::call(&endpoint, std::process::id(), &query)
                .await
                .unwrap();
        assert!(matches!(
            observation.result,
            ActivationControllerObservationResult::Waiting { .. }
        ));
        let after = info(&endpoint).await;
        assert_eq!(before.generation, after.generation);
        assert_eq!(
            before.fenced_requests.unwrap().context,
            after.fenced_requests.unwrap().context
        );
        assert!(!after.maintenance_available && !after.workflow_available);
        let response: LocalResponse = winbridge_transport::supervisor::call(
            &endpoint,
            std::process::id(),
            &LocalRequest::Submit {
                request: submission("controller-forbidden"),
            },
        )
        .await
        .unwrap();
        assert!(matches!(response, LocalResponse::Error { .. }));
        let observed = Store::open_read_only(&root.path().join("state/journal.sqlite")).unwrap();
        assert_eq!(observed.state_backup_authority().unwrap(), authority);
        assert_eq!(observed.recent().unwrap().first().unwrap().task_id, task);
        assert!(observed.update_frozen().unwrap());
        drop(observed);
        assert!(!lock_available(&root.path().join("state")));
        tokio::time::timeout(std::time::Duration::from_secs(2), running.close())
            .await
            .unwrap()
            .unwrap();
        assert!(lock_available(&root.path().join("state")));
        drop(listeners);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_topology_and_failed_checkpoint_consume_attempt_without_network_or_state_loss() {
    for fault in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let (prepared, config, _) = service_fixture(root.path());
        let mut command = command(prepared.command(), &config);
        if fault == 0 {
            command.binding.topology.controllers[0].url = "ws://127.0.0.1:1144".into();
        }
        if fault == 1 {
            command.binding.service_command.command_id = uuid::Uuid::new_v4().to_string();
        }
        let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
        let mut running = service.start().await.unwrap();
        let endpoint = running.endpoint().unwrap();
        let mut checks = 0;
        let result = running.start_controller(command.clone(), || {
            checks += 1;
            if checks == fault - 1 {
                Err("Original custody expired".into())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(running.start_controller(command, || Ok(())).is_err());
        assert!(connections(&endpoint)
            .await
            .connections
            .iter()
            .all(|l| l.attempts == "0"));
        assert!(!lock_available(&root.path().join("state")));
        running.close().await.unwrap();
        assert!(lock_available(&root.path().join("state")));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopped_local_ipc_cannot_start_network_and_cached_exit_is_closed_once() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let command = command(prepared.command(), &config);
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    let mut running = service.start().await.unwrap();
    running.stop_fixture();
    running.wait_until_stopped().await.unwrap();
    assert!(running
        .start_controller(command, || panic!("Stopped IPC reached startup checkpoint"))
        .is_err());
    assert!(!lock_available(&root.path().join("state")));
    running.close().await.unwrap();
    assert!(lock_available(&root.path().join("state")));
}
