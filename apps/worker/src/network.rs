use crate::{store::now_ms, Worker};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    task::JoinSet,
    time::Instant,
};
use winbridge_protocol::{
    Capability, Component, ConnectionState, LocalResponse, PeerIdentity, SessionMessage,
    PROTOCOL_VERSION,
};
use winbridge_transport::websocket::{self as ws, Result};

fn update(worker: &Worker, change: impl FnOnce(&mut winbridge_protocol::ControllerConnection)) {
    if let Ok(mut state) = worker.controller_connection.lock() {
        if let Some(state) = state.as_mut() {
            change(state);
        }
    }
}
pub async fn connect(worker: Arc<Worker>) {
    let Some(url) = worker.config.worker.controller_url.clone() else {
        return;
    };
    let address = match winbridge_core::controller_config::validate_controller_url(&url) {
        Ok(address) => address,
        Err(error) => {
            update(&worker, |s| {
                s.state = ConnectionState::Disconnected;
                s.error = Some(error);
            });
            return;
        }
    };
    let executable = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "the running winbridge-worker.exe".into());
    let mut backoff = 1_u64;
    loop {
        update(&worker, |s| {
            s.state = ConnectionState::Connecting;
        });
        let mut stage = "TCP connection";
        let handshake = tokio::time::timeout(ws::HANDSHAKE_TIMEOUT, async {
            let stream = TcpStream::connect(address).await.map_err(|error| {
                let detail = if error.raw_os_error() == Some(10013) {
                    format!("Windows denied socket access (WSAEACCES 10013). Check outbound TCP firewall/security permission for this exact executable: {executable}")
                } else if error.kind() == std::io::ErrorKind::ConnectionRefused {
                    "Connection refused; check that the Debian controller is listening".into()
                } else { "Check local reachability and outbound TCP permission".into() };
                format!("TCP connection to {address} failed: {error}. {detail}")
            })?;
            stream.set_nodelay(true).map_err(|e| format!("TCP socket setup for {address} failed: {e}"))?;
            stage = "WebSocket upgrade (TCP connected)";
            let (mut socket, _) = ws::client_async_with_config(&url, stream, Some(ws::config()))
                .await.map_err(|e| format!("WebSocket upgrade at {url} failed after TCP connected: {e}"))?;
            stage = "WinBridge Hello/Welcome exchange (TCP and WebSocket connected)";
            ws::send(&mut socket, &SessionMessage::Hello { peer: PeerIdentity {
                protocol_version: PROTOCOL_VERSION, application_version: env!("CARGO_PKG_VERSION").into(),
                component: Component::Worker, instance_id: worker.config.instance_id.clone(),
                node_id: worker.config.node_id.clone(), generation: worker.generation.clone(),
                capabilities: if worker.config.delivery.is_some() { vec![Capability::TaskSubmit, Capability::TaskCancel, Capability::WorkspaceImport,Capability::LogRead] } else { vec![Capability::TaskSubmit, Capability::TaskCancel] },
            }}).await?;
            match ws::first(&mut socket).await? {
                SessionMessage::Welcome { peer, session_id }
                    if peer.protocol_version == PROTOCOL_VERSION && peer.component == Component::Controller
                        && peer.instance_id == worker.config.instance_id
                        && winbridge_core::config::valid_id(&peer.node_id)
                        && winbridge_core::config::valid_id(&peer.generation)
                        && winbridge_core::config::valid_id(&session_id)
                        && !peer.application_version.is_empty() && peer.application_version.len() <= 80
                        && peer.capabilities.len() <= 16
                        && peer.capabilities.contains(&Capability::TaskSubmit)
                        && peer.capabilities.contains(&Capability::TaskCancel) => {
                    update(&worker, |s| {
                        s.state = ConnectionState::Connected; s.error = None;
                        s.session_id = Some(session_id); s.controller_node_id = Some(peer.node_id);
                        s.controller_generation = Some(peer.generation); s.last_confirmed_at_ms = Some(now_ms());
                    });
                    Ok(socket)
                }
                SessionMessage::Rejected { message } => Err(format!("Controller rejected handshake: {message}")),
                _ => Err("Controller handshake mismatch: protocolVersion/component/instanceId/nodeId/generation/appVersion/capabilities; check both builds and pair configuration".into()),
            }
        }).await;
        let result = match handshake {
            Ok(Ok(socket)) => { backoff = 1; session(socket, Arc::clone(&worker)).await }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(format!("{stage} timeout within the 5-second connection deadline at {address}; check the Debian listener and firewall permission for {executable}")),
        };
        let message = result
            .err()
            .unwrap_or_else(|| "Controller session ended".into());
        eprintln!("WinBridge controller {url}: {message}; reconnect in {backoff}s, accepted tasks continue");
        update(&worker, |s| {
            s.state = ConnectionState::Disconnected;
            s.error = Some(message);
        });
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(10);
    }
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: ws::WebSocketStream<S>,
    worker: Arc<Worker>,
) -> Result<()> {
    let mut handlers: JoinSet<(String, LocalResponse)> = JoinSet::new();
    let mut active_ids = HashSet::new();
    let mut last_ping = Instant::now();
    let mut last_nonce = 0;
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = timer.tick() => {
                if last_ping.elapsed() >= ws::HEARTBEAT_INTERVAL + ws::HEARTBEAT_TIMEOUT {
                    return Err("Controller heartbeat timeout; accepted tasks continue locally".into());
                }
            }
            result = handlers.join_next(), if !handlers.is_empty() => {
                let (id, response) = result.ok_or("Request handler disappeared")?.map_err(|e| e.to_string())?;
                active_ids.remove(&id);
                let mut message = SessionMessage::Response { id: id.clone(), response };
                if serde_json::to_vec(&message).map_err(|e| e.to_string())?.len() > winbridge_transport::MAX_FRAME_BYTES {
                    message = SessionMessage::Response { id, response: LocalResponse::Error { message: "Response exceeds 1 MiB; request an individual task/output page".into() } };
                }
                ws::send(&mut socket, &message).await?;
            }
            message = ws::receive(&mut socket) => match message? {
                None => {},
                Some(SessionMessage::Ping { nonce }) if nonce > last_nonce => {
                    ws::send(&mut socket, &SessionMessage::Pong { nonce }).await?;
                    last_nonce = nonce; last_ping = Instant::now();
                    update(&worker, |s| { s.last_confirmed_at_ms = Some(now_ms()); });
                }
                Some(SessionMessage::Request { id, request }) => {
                    if id.is_empty() || id.len() > 160 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') || active_ids.contains(&id) {
                        return Err("Invalid/duplicate WebSocket correlation ID".into());
                    }
                    if handlers.len() >= ws::MAX_IN_FLIGHT {
                        ws::send(&mut socket, &SessionMessage::Response { id, response: LocalResponse::Error { message: "Worker has 16 active requests; request rejected before acceptance".into() } }).await?;
                        continue;
                    }
                    active_ids.insert(id.clone());
                    let worker = Arc::clone(&worker);
                    handlers.spawn(async move {
                        let result = tokio::time::timeout(ws::RPC_TIMEOUT, tokio::task::spawn_blocking(move || worker.handle(request))).await;
                        let response = match result {
                            Ok(Ok(response)) => response,
                            Ok(Err(error)) => LocalResponse::Error { message: format!("Worker handler failed: {error}; reconcile the same task/key") },
                            Err(_) => LocalResponse::Error { message: "Worker handler timeout; acceptance is unknown, reconcile the same task/key without automatic retry".into() },
                        };
                        (id, response)
                    });
                }
                _ => return Err("Unexpected message in worker session".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_protocol::LocalRequest;

    fn test_worker(root: &std::path::Path, address: std::net::SocketAddr) -> Arc<Worker> {
        Worker::open(serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"instanceId":"review","nodeId":"worker",
            "stateDirectory":root.join("state"),
            "worker":{"controllerUrl":format!("ws://{address}"),"keepRunningOnUiExit":true},
            "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
        })).unwrap()).unwrap()
    }
    async fn connection_error(worker: &Worker) -> String {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
                    panic!("Local handler unavailable")
                };
                let connection = info.controller_connection.unwrap();
                if matches!(connection.state, ConnectionState::Disconnected) {
                    return connection.error.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn refused_tcp_reports_network_stage_without_breaking_local_handler() {
        let root = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let worker = test_worker(root.path(), address);
        let connector = tokio::spawn(connect(Arc::clone(&worker)));
        let error = connection_error(&worker).await;
        assert!(
            error.starts_with("TCP connection to") && error.contains("Connection refused"),
            "{error}"
        );
        assert!(
            matches!(worker.handle(LocalRequest::ListTasks), LocalResponse::Tasks { tasks } if tasks.is_empty())
        );
        connector.abort();
        let _ = connector.await;
    }
    #[tokio::test]
    async fn timeout_distinguishes_websocket_upgrade_from_hello_welcome() {
        for websocket_upgraded in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let worker = test_worker(root.path(), listener.local_addr().unwrap());
            let connector = tokio::spawn(connect(Arc::clone(&worker)));
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = if websocket_upgraded {
                Some(
                    ws::accept_async_with_config(stream, Some(ws::config()))
                        .await
                        .unwrap(),
                )
            } else {
                // Hold TCP open, deliberately never send an HTTP upgrade response.
                let error = connection_error(&worker).await;
                assert!(
                    error.starts_with("WebSocket upgrade (TCP connected) timeout"),
                    "{error}"
                );
                drop(stream);
                None
            };
            if let Some(socket) = socket.as_mut() {
                assert!(matches!(
                    ws::first(socket).await.unwrap(),
                    SessionMessage::Hello { .. }
                ));
                let error = connection_error(&worker).await;
                assert!(
                    error.starts_with("WinBridge Hello/Welcome exchange"),
                    "{error}"
                );
                assert!(error.contains("5-second connection deadline"));
            }
            connector.abort();
            let _ = connector.await;
        }
    }

    #[tokio::test]
    async fn invalid_welcome_does_not_publish_connected_state_or_break_local_handler() {
        tokio::time::timeout(Duration::from_secs(3), async {
            for missing_capability in [true, false] {
                let root = tempfile::tempdir().unwrap();
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let config = serde_json::from_value(serde_json::json!({
                    "schemaVersion":1,"instanceId":"review","nodeId":"worker",
                    "stateDirectory":root.path().join("state"),
                    "worker":{"controllerUrl":format!("ws://{}", listener.local_addr().unwrap()),"keepRunningOnUiExit":true},
                    "projects":[{"id":"fixture","workspaceRoot":root.path(),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
                })).unwrap();
                let worker = Worker::open(config).unwrap();
                let connector = tokio::spawn(connect(Arc::clone(&worker)));
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = ws::accept_async_with_config(stream, Some(ws::config())).await.unwrap();
                assert!(matches!(ws::first(&mut socket).await.unwrap(), SessionMessage::Hello { .. }));
                ws::send(&mut socket, &SessionMessage::Welcome {
                    session_id:"session".into(), peer:PeerIdentity {
                        protocol_version:PROTOCOL_VERSION, component:Component::Controller,
                        instance_id:"review".into(), node_id:"controller".into(), generation:"generation".into(),
                        application_version:if missing_capability { "test".into() } else { String::new() },
                        capabilities:if missing_capability { vec![Capability::TaskSubmit] } else { vec![Capability::TaskSubmit,Capability::TaskCancel] },
                    },
                }).await.unwrap();
                assert!(ws::first(&mut socket).await.is_err(), "Worker must close an incompatible session");
                let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else { panic!("Local handler must remain available") };
                let connection = info.controller_connection.unwrap();
                assert!(matches!(connection.state, ConnectionState::Disconnected));
                assert!(connection.session_id.is_none());
                assert!(connection.error.unwrap().contains("capabilities"));
                assert!(matches!(worker.handle(LocalRequest::ListTasks), LocalResponse::Tasks { tasks } if tasks.is_empty()));
                connector.abort();
                let _ = connector.await;
            }
        }).await.unwrap();
    }
}
