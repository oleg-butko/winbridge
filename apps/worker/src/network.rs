use crate::{store::now_ms, Worker};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
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
    let mut backoff = 1_u64;
    loop {
        update(&worker, |s| {
            s.state = ConnectionState::Connecting;
        });
        let handshake = tokio::time::timeout(ws::HANDSHAKE_TIMEOUT, async {
            let (mut socket, _) = ws::connect_async_with_config(&url, Some(ws::config()), true).await.map_err(|e| e.to_string())?;
            ws::send(&mut socket, &SessionMessage::Hello { peer: PeerIdentity {
                protocol_version: PROTOCOL_VERSION, application_version: env!("CARGO_PKG_VERSION").into(),
                component: Component::Worker, instance_id: worker.config.instance_id.clone(),
                node_id: worker.config.node_id.clone(), generation: worker.generation.clone(),
                capabilities: vec![Capability::TaskSubmit, Capability::TaskCancel],
            }}).await?;
            match ws::first(&mut socket).await? {
                SessionMessage::Welcome { peer, session_id }
                    if peer.protocol_version == PROTOCOL_VERSION && peer.component == Component::Controller
                        && peer.instance_id == worker.config.instance_id
                        && winbridge_core::config::valid_id(&peer.node_id)
                        && winbridge_core::config::valid_id(&peer.generation)
                        && winbridge_core::config::valid_id(&session_id) => {
                    update(&worker, |s| {
                        s.state = ConnectionState::Connected; s.error = None;
                        s.session_id = Some(session_id); s.controller_node_id = Some(peer.node_id);
                        s.controller_generation = Some(peer.generation); s.last_confirmed_at_ms = Some(now_ms());
                    });
                    Ok(socket)
                }
                SessionMessage::Rejected { message } => Err(format!("Controller rejected handshake: {message}")),
                _ => Err("Controller handshake mismatch: protocolVersion/component/instanceId/generation; check both builds and pair configuration".into()),
            }
        }).await;
        let result = match handshake {
            Ok(Ok(socket)) => { backoff = 1; session(socket, Arc::clone(&worker)).await }
            Ok(Err(error)) => Err(error),
            Err(_) => Err("Controller handshake timeout after 5 seconds; check the configured local address and running controller".into()),
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
