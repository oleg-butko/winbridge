//! Real OS peer/IPC/mailbox, synthetic lifecycle. No native Active assertion.
use super::*;
use winbridge_transport::local_peer::{LocalPeer, LocalPeerStream};

async fn actual_peer() -> LocalPeer {
    #[cfg(unix)]
    {
        let (_, accepted) = tokio::net::UnixStream::pair().unwrap();
        accepted.local_peer().unwrap()
    }
    #[cfg(windows)]
    {
        let address = format!(r"\\.\pipe\WinBridge-v8-unit-{}", uuid::Uuid::new_v4());
        let server = winbridge_transport::create_pipe(&address, true).unwrap();
        let client = tokio::spawn(async move {
            winbridge_transport::supervisor::connect(&address, std::process::id())
                .await
                .unwrap()
        });
        server.connect().await.unwrap();
        let _client = client.await.unwrap();
        server.local_peer().unwrap()
    }
}
async fn fixture(root: &Path) -> (RunningLocalService, ActivationFinalizationRequest) {
    let service = super::tests::fixture(root);
    let (mut command, mut receipt) = super::tests::proposal(&service);
    // Synthetic keeper liveness is explicit. Only its PID matches the real
    // caller so the mailbox can be tested; native receiver checks stay intact.
    command.binding.keeper.process_id = std::process::id();
    receipt.keeper = command.binding.keeper.clone();
    receipt.command_sha256 = lifetime::command_sha256(&command).unwrap();
    let mut running = service.start().await.unwrap();
    running
        .seal_supervised_lifetime(
            command.clone(),
            receipt,
            LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true))),
        )
        .unwrap();
    let request = ActivationFinalizationRequest {
        finalization_version: 8,
        command: super::tests::finalization_proposal(&running, command),
    };
    (running, request)
}
async fn await_queued(running: &RunningLocalService) {
    let until = Instant::now() + Duration::from_secs(3);
    while running.finalization_receiver.as_ref().unwrap().is_empty() {
        assert!(
            Instant::now() < until,
            "Original request must enter mailbox"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn accepted_delivery_survives_caller_cancellation_and_mailbox_is_bounded() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, request) = fixture(root.path()).await;
    let worker = Arc::clone(&running.service.worker);
    let first_worker = Arc::clone(&worker);
    let first_request = request.clone();
    let peer = actual_peer().await;
    let first = tokio::spawn(async move {
        first_worker
            .handle_finalization_request(first_request, peer)
            .await
    });
    await_queued(&running).await;
    let busy = worker
        .handle_finalization_request(request.clone(), actual_peer().await)
        .await;
    assert!(
        matches!(busy.result, ActivationFinalizationResult::Rejected { ref message } if message.contains("not accepted"))
    );
    first.abort();
    let _ = first.await;
    let receiver = running.finalization_receiver.as_mut().unwrap();
    assert_eq!(receiver.max_capacity(), 1);
    let accepted = receiver.recv().await.unwrap();
    assert_eq!(accepted.request, request);
    assert_eq!(accepted.peer.process_id(), std::process::id());
    assert!(
        accepted.reply.is_closed(),
        "Caller loss must not remove the accepted message"
    );
    assert!(receiver.is_empty());
    assert!(worker.activation_read_only());
    assert!(running.service._config_pin.is_some());
    worker.check_local_service_scope().unwrap();
    drop(accepted);
    drop(worker);
    running.close().await.unwrap();
}

#[tokio::test]
async fn stopped_mailbox_reports_unknown_accepted_outcome_and_refuses_new_delivery() {
    for lose_owner in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let (mut running, request) = fixture(root.path()).await;
        let worker = Arc::clone(&running.service.worker);
        let first_worker = Arc::clone(&worker);
        let first_request = request.clone();
        let peer = actual_peer().await;
        let first = tokio::spawn(async move {
            first_worker
                .handle_finalization_request(first_request, peer)
                .await
        });
        await_queued(&running).await;
        if lose_owner {
            drop(running.finalization_receiver.take());
        } else {
            // Accepted before stop; post-queue refusal must not erase a possibly
            // earlier decision by reporting it as a rejected new delivery.
            worker.stop();
            let delivery = running
                .finalization_receiver
                .as_mut()
                .unwrap()
                .recv()
                .await
                .unwrap();
            running.process_finalization_delivery(delivery);
        }
        let response = first.await.unwrap();
        assert!(matches!(
            response.result,
            ActivationFinalizationResult::Indeterminate { .. }
        ));
        let refused = worker
            .handle_finalization_request(request, actual_peer().await)
            .await;
        assert!(
            matches!(refused.result, ActivationFinalizationResult::Rejected { ref message } if message.contains(if lose_owner { "not accepted" } else { "stopping" }))
        );
        assert!(worker.activation_read_only());
        worker.check_local_service_scope().unwrap();
        drop(worker);
        running.close().await.unwrap();
    }
}

#[tokio::test]
async fn real_ipc_rejects_cold_foreign_and_mixed_v8_without_queuing() {
    for supervised in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let service = super::tests::fixture(root.path());
        let (command, receipt) = super::tests::proposal(&service);
        let mut running = service.start().await.unwrap();
        if supervised {
            running
                .seal_supervised_lifetime(
                    command.clone(),
                    receipt,
                    LifetimeKeeper::Fixture(Arc::new(AtomicBool::new(true))),
                )
                .unwrap();
        }
        let request = ActivationFinalizationRequest {
            finalization_version: 8,
            command: super::tests::finalization_proposal(&running, command),
        };
        for fault in 0..5 {
            let mut raw = serde_json::to_value(&request).unwrap();
            match fault {
                0 => {}
                1 => raw["force"] = true.into(),
                2 => raw["finalizationVersion"] = true.into(),
                3 => raw["finalizationVersion"] = 7.into(),
                _ => raw["finalizationObservationVersion"] = 1.into(),
            }
            let response: ActivationFinalizationResponse = winbridge_transport::supervisor::call(
                &running.endpoint().unwrap(),
                std::process::id(),
                &raw,
            )
            .await
            .unwrap();
            assert!(matches!(
                response.result,
                ActivationFinalizationResult::Rejected { .. }
            ));
            assert!(running.finalization_receiver.as_ref().unwrap().is_empty());
            assert!(running.service.worker.activation_read_only());
            running.service.worker.check_local_service_scope().unwrap();
        }
        running.close().await.unwrap();
    }
}

#[tokio::test]
async fn original_owner_loop_receives_delivery_but_fixture_cannot_pass_native_checks() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, request) = fixture(root.path()).await;
    let worker = Arc::clone(&running.service.worker);
    let endpoint = running.endpoint().unwrap();
    let owner = tokio::spawn(async move {
        let exit = running.hold_supervised_lifetime().await;
        running.close().await.unwrap();
        exit
    });
    let response: ActivationFinalizationResponse =
        winbridge_transport::supervisor::call(&endpoint, std::process::id(), &request)
            .await
            .unwrap();
    assert!(!matches!(
        response.result,
        ActivationFinalizationResult::Active { .. }
    ));
    assert!(worker.activation_read_only());
    worker.check_local_service_scope().unwrap();
    worker.stop();
    drop(worker);
    assert!(tokio::time::timeout(Duration::from_secs(5), owner)
        .await
        .unwrap()
        .unwrap()
        .is_err());
}
