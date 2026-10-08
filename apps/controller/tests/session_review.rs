use std::sync::Arc;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use winbridge_controller::Controller;
use winbridge_core::controller_config::ControllerConfig;
use winbridge_protocol::{
    Capability, Component, LocalRequest, LocalResponse, PeerIdentity, SessionMessage,
    PROTOCOL_VERSION,
};
use winbridge_transport::websocket as ws;

type Client = ws::WebSocketStream<ws::MaybeTlsStream<TcpStream>>;

async fn pair() -> (tempfile::TempDir, Arc<Controller>, Arc<TcpListener>) {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config: ControllerConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"review","nodeId":"debian",
        "stateDirectory":root.path().join("state"),
        "controller":{"listenAddress":listener.local_addr().unwrap(),"workerNodeId":"windows"}
    }))
    .unwrap();
    (root, Controller::new(config).unwrap(), Arc::new(listener))
}
async fn connect(
    controller: &Arc<Controller>,
    listener: &Arc<TcpListener>,
) -> (JoinHandle<ws::Result<()>>, Client) {
    connect_with_capabilities(
        controller,
        listener,
        vec![Capability::TaskSubmit, Capability::TaskCancel],
    )
    .await
}
async fn connect_with_capabilities(
    controller: &Arc<Controller>,
    listener: &Arc<TcpListener>,
    capabilities: Vec<Capability>,
) -> (JoinHandle<ws::Result<()>>, Client) {
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = Arc::clone(listener);
    let controller = Arc::clone(controller);
    let actor = tokio::spawn(async move {
        let (stream, address) = server.accept().await.unwrap();
        controller.accept(stream, address).await
    });
    let (mut client, _) = ws::connect_async_with_config(&url, Some(ws::config()), true)
        .await
        .unwrap();
    ws::send(
        &mut client,
        &SessionMessage::Hello {
            peer: PeerIdentity {
                protocol_version: PROTOCOL_VERSION,
                application_version: "review".into(),
                component: Component::Worker,
                instance_id: "review".into(),
                node_id: "windows".into(),
                generation: "worker-generation".into(),
                capabilities,
            },
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        ws::first(&mut client).await.unwrap(),
        SessionMessage::Welcome { .. }
    ));
    (actor, client)
}

#[tokio::test]
async fn task_attention_worker_rejects_target_requests_before_transmission() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (_root, controller, listener) = pair().await;
        let (actor, mut client) = connect_with_capabilities(&controller, &listener, vec![Capability::TaskSubmit, Capability::TaskCancel, Capability::AttentionRequest]).await;
        for request in [
            LocalRequest::ListAttentionEvents {after_sequence:0,pending_only:true},
            LocalRequest::AcknowledgeAttentionEvent {attention_id:"a".into()},
        ] {
            let LocalResponse::Error {message}=controller.call(request).await else {panic!("Expected capability refusal")};
            assert!(message.contains("attention_targets") && message.contains("not transmitted"));
        }
        assert!(controller.status().unwrap().connected);
        let caller=Arc::clone(&controller);
        let pending=tokio::spawn(async move {caller.call(LocalRequest::ListAttention {after_sequence:0,pending_only:true}).await});
        let SessionMessage::Request {id,request:LocalRequest::ListAttention {..}}=next_request(&mut client).await else {panic!("Only supported legacy request should reach the worker")};
        ws::send(&mut client,&SessionMessage::Response {id,response:LocalResponse::Error {message:"legacy feed response".into()}}).await.unwrap();
        assert!(matches!(pending.await.unwrap(),LocalResponse::Error {message} if message=="legacy feed response"));
        actor.abort();let _=actor.await;
    }).await.unwrap();
}
async fn next_request(client: &mut Client) -> SessionMessage {
    loop {
        match ws::first(client).await.unwrap() {
            SessionMessage::Ping { nonce } => ws::send(client, &SessionMessage::Pong { nonce })
                .await
                .unwrap(),
            message => return message,
        }
    }
}

#[tokio::test]
async fn history_requests_are_refused_without_transmission_to_older_workers() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (_root, controller, listener) = pair().await;
        let (actor, mut client) = connect_with_capabilities(
            &controller,
            &listener,
            vec![
                Capability::TaskSubmit,
                Capability::TaskCancel,
                Capability::AttentionTargets,
            ],
        )
        .await;
        for request in [
            LocalRequest::History {
                query: Default::default(),
            },
            LocalRequest::PrepareTaskDiagnostic {
                request: winbridge_protocol::DiagnosticRequest {
                    task_id: "old".into(),
                    include_command: false,
                    include_output: false,
                    redact_values: vec![],
                },
            },
            LocalRequest::StorageUsage,
        ] {
            let LocalResponse::Error { message } = controller.call(request).await else {
                panic!("Expected capability refusal")
            };
            assert!(message.contains("history_read") && message.contains("not transmitted"));
        }
        assert!(controller.status().unwrap().connected);
        let caller = Arc::clone(&controller);
        let pending = tokio::spawn(async move { caller.call(LocalRequest::ListTasks).await });
        let SessionMessage::Request {
            id,
            request: LocalRequest::ListTasks,
        } = next_request(&mut client).await
        else {
            panic!("Only the supported read may reach the worker")
        };
        ws::send(
            &mut client,
            &SessionMessage::Response {
                id,
                response: LocalResponse::Tasks { tasks: vec![] },
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            pending.await.unwrap(),
            LocalResponse::Tasks { .. }
        ));
        actor.abort();
        let _ = actor.await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancelled_actor_clears_session_and_allows_reconnect_without_retry() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (_root, controller, listener) = pair().await;
        let (actor, mut client) = connect(&controller, &listener).await;
        assert!(controller.status().unwrap().connected);
        let caller = Arc::clone(&controller);
        let pending = tokio::spawn(async move { caller.call(LocalRequest::Info).await });
        assert!(matches!(
            next_request(&mut client).await,
            SessionMessage::Request { .. }
        ));
        actor.abort();
        assert!(actor.await.unwrap_err().is_cancelled());
        assert!(!controller.status().unwrap().connected);
        assert!(controller
            .status()
            .unwrap()
            .last_error
            .unwrap()
            .contains("interrupted"));
        let LocalResponse::Error { message } = pending.await.unwrap() else {
            panic!("Expected unknown outcome")
        };
        assert!(message.contains("unknown"));
        let (second, _client) = connect(&controller, &listener).await;
        assert!(controller.status().unwrap().connected);
        second.abort();
        let _ = second.await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn oversized_outbound_request_is_not_sent_and_does_not_disconnect_peer() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (_root, controller, listener) = pair().await;
        let (actor, mut client) = connect(&controller, &listener).await;
        let LocalResponse::Error { message } = controller.call(LocalRequest::GetTask {
            task_id: "x".repeat(winbridge_transport::MAX_FRAME_BYTES),
        }).await else { panic!("Expected preflight rejection") };
        assert!(message.contains("rejected before transmission"));
        assert!(controller.status().unwrap().connected);
        let caller = Arc::clone(&controller);
        let pending = tokio::spawn(async move { caller.call(LocalRequest::Info).await });
        let SessionMessage::Request { id, request: LocalRequest::Info } = next_request(&mut client).await else {
            panic!("Only the subsequent Info request should reach the peer")
        };
        ws::send(&mut client, &SessionMessage::Response { id, response: LocalResponse::Error { message: "fixture response".into() }}).await.unwrap();
        assert!(matches!(pending.await.unwrap(), LocalResponse::Error { message } if message == "fixture response"));
        assert!(controller.status().unwrap().connected);
        actor.abort();
        let _ = actor.await;
    }).await.unwrap();
}

#[tokio::test]
async fn legacy_worker_rejects_attention_before_transmission_and_keeps_session() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (_root, controller, listener) = pair().await;
        let (actor, mut client) = connect(&controller, &listener).await;
        for request in [
            LocalRequest::RequestAttention { request: winbridge_protocol::AttentionSubmission {request_id:"one".into(),idempotency_key:"one".into(),task_id:"task".into(),severity:winbridge_protocol::AttentionSeverity::Warning,message:"check".into()} },
            LocalRequest::ListAttention {after_sequence:0,pending_only:true},
            LocalRequest::AcknowledgeAttention {attention_id:"event".into()},
        ] {
            let LocalResponse::Error {message}=controller.call(request).await else {panic!("Expected capability refusal")};
            assert!(message.contains("not transmitted"));
        }
        assert!(controller.status().unwrap().connected);
        let caller=Arc::clone(&controller);
        let pending=tokio::spawn(async move {caller.call(LocalRequest::Info).await});
        let SessionMessage::Request {id,request:LocalRequest::Info}=next_request(&mut client).await else {panic!("Attention must not reach old worker")};
        ws::send(&mut client,&SessionMessage::Response {id,response:LocalResponse::Error {message:"fixture response".into()}}).await.unwrap();
        assert!(matches!(pending.await.unwrap(),LocalResponse::Error {message} if message=="fixture response"));
        actor.abort();let _=actor.await;
    }).await.unwrap();
}
