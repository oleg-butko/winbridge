//! Real IPC reply timing with synthetic lifecycle; no native Active claim.
use super::finalization_mailbox_tests::{await_queued, fixture};
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn accepted_v8_reply_outlives_generic_ipc_deadline_without_a_second_delivery() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, request) = fixture(root.path()).await;
    let endpoint = running.endpoint().unwrap();
    let key = request.command.command_id.clone();
    let client = tokio::spawn(async move {
        winbridge_transport::supervisor::call_with_timeout::<_, ActivationFinalizationResponse>(
            &endpoint,
            std::process::id(),
            &request,
            Duration::from_secs(20),
        )
        .await
    });
    await_queued(&running).await;
    let delivery = running
        .finalization_receiver
        .as_mut()
        .unwrap()
        .recv()
        .await
        .unwrap();
    assert_eq!(delivery.peer.process_id(), std::process::id());
    let mut incomplete =
        winbridge_transport::supervisor::connect(&running.endpoint().unwrap(), std::process::id())
            .await
            .unwrap();
    incomplete.write_all(&1024u32.to_le_bytes()).await.unwrap();
    incomplete
        .write_all(b"{\"finalizationVersion\":8")
        .await
        .unwrap();
    // Real OS IPC and the production connection handler. The synthetic owner
    // deliberately returns NO Active fact or SQL effect after the usual10s.
    tokio::time::sleep(winbridge_transport::IPC_TIMEOUT + Duration::from_millis(100)).await;
    assert!(
        !delivery.reply.is_closed(),
        "Accepted V8 reply must survive the generic deadline"
    );
    let mut byte = [0];
    let incomplete_read = tokio::time::timeout(Duration::from_secs(1), incomplete.read(&mut byte))
        .await
        .expect("Incomplete framing must keep its original10s deadline");
    assert!(
        matches!(incomplete_read, Ok(0) | Err(_)),
        "A partial V8 envelope must not renew framing time"
    );
    let response = ActivationFinalizationResponse {
        finalization_version: 8,
        command_id: Some(key),
        result: ActivationFinalizationResult::Indeterminate {
            message: "Original accepted request retained; no native Active claim".into(),
        },
    };
    delivery.reply.send(response.clone()).unwrap();
    assert_eq!(client.await.unwrap().unwrap(), response);
    assert!(running.finalization_receiver.as_ref().unwrap().is_empty());
    assert!(running.service.worker.activation_read_only());
    assert!(running.service._config_pin.is_some());
    running.close().await.unwrap();
}
