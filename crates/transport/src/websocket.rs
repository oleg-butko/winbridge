//! Plain, bounded WebSocket sessions. A transport failure never retries a command.
use futures_util::{SinkExt, StreamExt};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Message};
use winbridge_protocol::{LocalRequest, LocalResponse, SessionMessage};

pub use tokio_tungstenite::{accept_async_with_config, connect_async_with_config, WebSocketStream};
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub const RPC_TIMEOUT: Duration = Duration::from_secs(8);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_IN_FLIGHT: usize = 16;
pub type Result<T> = std::result::Result<T, String>;

pub fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(8192)
        .write_buffer_size(0)
        .max_write_buffer_size(2 * crate::MAX_FRAME_BYTES)
        .max_message_size(Some(crate::MAX_FRAME_BYTES))
        .max_frame_size(Some(crate::MAX_FRAME_BYTES))
}

pub async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    message: &SessionMessage,
) -> Result<()> {
    let bytes = serde_json::to_string(message).map_err(|e| e.to_string())?;
    if bytes.len() > crate::MAX_FRAME_BYTES {
        return Err("WebSocket message exceeds 1 MiB".into());
    }
    tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Text(bytes.into())))
        .await
        .map_err(|_| "WebSocket write timeout".to_string())?
        .map_err(|e| e.to_string())
}

pub async fn receive<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
) -> Result<Option<SessionMessage>> {
    match socket.next().await {
        Some(Ok(Message::Text(text))) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("Invalid WebSocket JSON: {e}")),
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
            // Tungstenite queues the control pong. Bound its flush too.
            tokio::time::timeout(WRITE_TIMEOUT, socket.flush())
                .await
                .map_err(|_| "WebSocket control write timeout".to_string())?
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        Some(Ok(Message::Close(_))) | None => Err("WebSocket connection closed".into()),
        Some(Ok(_)) => Err("WebSocket requires JSON text messages".into()),
        Some(Err(error)) => Err(format!("WebSocket: {error}")),
    }
}

pub async fn first<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
) -> Result<SessionMessage> {
    loop {
        if let Some(message) = receive(socket).await? {
            return Ok(message);
        }
    }
}

pub struct RpcCall {
    pub request: LocalRequest,
    pub reply: oneshot::Sender<Result<LocalResponse>>,
}
#[derive(Clone)]
pub struct RpcSender(mpsc::Sender<RpcCall>);
pub fn rpc_channel() -> (RpcSender, mpsc::Receiver<RpcCall>) {
    let (sender, receiver) = mpsc::channel(32);
    (RpcSender(sender), receiver)
}
impl RpcSender {
    pub async fn call(&self, request: LocalRequest) -> Result<LocalResponse> {
        let (reply, response) = oneshot::channel();
        self.0
            .try_send(RpcCall { request, reply })
            .map_err(|e| format!("Worker session unavailable or busy: {e}"))?;
        tokio::time::timeout(RPC_TIMEOUT, response).await
            .map_err(|_| "Worker response timeout; acceptance is unknown. Reconcile using the same task ID/idempotency key; no automatic retry occurred".to_string())?
            .map_err(|_| "Worker connection lost; acceptance is unknown. Reconcile the same task ID/idempotency key; no automatic retry occurred".to_string())?
    }
}

pub async fn controller_session<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: WebSocketStream<S>,
    mut calls: mpsc::Receiver<RpcCall>,
    generation: &str,
) -> Result<()> {
    let mut pending: HashMap<String, (Instant, oneshot::Sender<Result<LocalResponse>>)> =
        HashMap::new();
    let mut counter = 0_u64;
    let mut nonce = 0_u64;
    let mut ping: Option<(u64, Instant)> = None;
    let mut next_ping = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let now = Instant::now();
                if ping.is_some_and(|(_, sent)| now.duration_since(sent) >= HEARTBEAT_TIMEOUT) {
                    return Err("Worker heartbeat timeout; last task state is retained".into());
                }
                if pending.values().any(|(sent, _)| now.duration_since(*sent) >= RPC_TIMEOUT) {
                    return Err("Worker response timeout; reconcile the same task/key without automatic retry".into());
                }
                if ping.is_none() && now >= next_ping {
                    nonce += 1;
                    send(&mut socket, &SessionMessage::Ping { nonce }).await?;
                    ping = Some((nonce, now));
                }
            }
            call = calls.recv() => {
                let Some(call) = call else { return Ok(()); };
                if call.reply.is_closed() { continue; }
                if pending.len() >= MAX_IN_FLIGHT {
                    let _ = call.reply.send(Err("Worker has 16 pending requests; retry after reconciliation".into()));
                    continue;
                }
                counter += 1;
                let id = format!("{generation}-{counter}");
                // Insert before writing: losing this write/ACK leaves acceptance unknown.
                pending.insert(id.clone(), (Instant::now(), call.reply));
                send(&mut socket, &SessionMessage::Request { id, request: call.request }).await?;
            }
            message = receive(&mut socket) => {
                match message? {
                    None => {},
                    Some(SessionMessage::Pong { nonce }) if ping.is_some_and(|(expected, _)| nonce == expected) => {
                        ping = None;
                        next_ping = Instant::now() + HEARTBEAT_INTERVAL;
                    }
                    Some(SessionMessage::Response { id, response }) => {
                        let Some((_, reply)) = pending.remove(&id) else { return Err("Unknown WebSocket response correlation ID".into()); };
                        let _ = reply.send(Ok(response));
                    }
                    _ => return Err("Unexpected message in controller session".into()),
                }
            }
        }
    }
}
