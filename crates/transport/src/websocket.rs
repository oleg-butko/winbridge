//! Plain, bounded WebSocket sessions. A transport failure never retries a command.
use futures_util::{SinkExt, StreamExt};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot, watch},
    time::Instant,
};
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Message};
use winbridge_protocol::{
    FencedRequest, FencedResponse, FencedSessionMessage, LocalRequest, LocalResponse, SessionFrame,
    SessionMessage, WireRequest, WireResponse,
};

pub use tokio_tungstenite::{
    accept_async_with_config, client_async_with_config, connect_async_with_config, MaybeTlsStream,
    WebSocketStream,
};
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
    send_encoded(socket, encode(message)?).await
}
pub async fn send_frame<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    message: &SessionFrame,
) -> Result<()> {
    send_encoded(socket, encode(message)?).await
}
fn encode<T: serde::Serialize>(message: &T) -> Result<String> {
    let bytes = serde_json::to_string(message).map_err(|e| e.to_string())?;
    if bytes.len() > crate::MAX_FRAME_BYTES {
        return Err("WebSocket message exceeds 1 MiB".into());
    }
    Ok(bytes)
}
async fn send_encoded<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    bytes: String,
) -> Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Text(bytes.into())))
        .await
        .map_err(|_| "WebSocket write timeout".to_string())?
        .map_err(|e| e.to_string())
}

pub async fn receive<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
) -> Result<Option<SessionMessage>> {
    match receive_frame(socket).await? {
        None => Ok(None),
        Some(SessionFrame::Legacy(message)) => Ok(Some(message)),
        Some(
            SessionFrame::Fenced(_) | SessionFrame::Projection(_) | SessionFrame::Extension(_),
        ) => Err("Unexpected versioned frame before negotiation".into()),
    }
}
pub async fn receive_frame<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
) -> Result<Option<SessionFrame>> {
    match socket.next().await {
        Some(Ok(Message::Text(text))) => serde_json::from_str(&text)
            .map(Some)
            // Serde error text may echo an entire unknown field or enum value.
            .map_err(|e| {
                format!(
                    "Invalid WebSocket JSON ({:?}) at line {}, column {}",
                    e.classify(),
                    e.line(),
                    e.column()
                )
            }),
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
    pub request: WireRequest,
    pub reply: oneshot::Sender<Result<WireResponse>>,
}
#[derive(Clone)]
pub struct RpcSender(
    mpsc::Sender<RpcCall>,
    watch::Sender<Option<winbridge_protocol::ControllerReadinessProjection>>,
    mpsc::Sender<ExtensionCall>,
);
struct ExtensionCall {
    request: winbridge_protocol::ExtensionRequest,
    reply: oneshot::Sender<Result<winbridge_protocol::ExtensionResponse>>,
}
struct PendingExtension {
    sent: Instant,
    reply: oneshot::Sender<Result<winbridge_protocol::ExtensionResponse>>,
}
pub struct RpcChannels {
    calls: mpsc::Receiver<RpcCall>,
    projections: watch::Receiver<Option<winbridge_protocol::ControllerReadinessProjection>>,
    extensions: mpsc::Receiver<ExtensionCall>,
}
impl RpcChannels {
    pub async fn recv(&mut self) -> Option<RpcCall> {
        self.calls.recv().await
    }
    pub fn try_recv(&mut self) -> std::result::Result<RpcCall, mpsc::error::TryRecvError> {
        self.calls.try_recv()
    }
}
enum ReplyContract {
    Legacy,
    Fenced,
}
struct PendingReply {
    sent: Instant,
    contract: ReplyContract,
    reply: oneshot::Sender<Result<WireResponse>>,
}
pub fn rpc_channel() -> (RpcSender, RpcChannels) {
    let (sender, receiver) = mpsc::channel(32);
    let (projection_sender, projections) = watch::channel(None);
    let (extension_sender, extensions) = mpsc::channel(32);
    (
        RpcSender(sender, projection_sender, extension_sender),
        RpcChannels {
            calls: receiver,
            projections,
            extensions,
        },
    )
}
impl RpcSender {
    /// Caller must negotiate the exact extension/business contract on this session.
    pub async fn call_extension(
        &self,
        request: winbridge_protocol::ExtensionRequest,
    ) -> Result<winbridge_protocol::ExtensionResponse> {
        let (reply, response) = oneshot::channel();
        self.2
            .try_send(ExtensionCall {
                request: request.clone(),
                reply,
            })
            .map_err(|_| "Readonly extension channel unavailable/busy")?;
        let response = tokio::time::timeout(RPC_TIMEOUT, response)
            .await
            .map_err(|_| "Readonly extension query deadline; no fallback")?
            .map_err(|_| "Readonly extension connection lost")??;
        response.validate_for(&request)?;
        Ok(response)
    }
    /// Caller must first negotiate support on this exact worker session.
    /// Latest readonly projection replaces older pending metadata; no task ACKs are dropped.
    pub fn publish_projection(
        &self,
        projection: winbridge_protocol::ControllerReadinessProjection,
    ) -> Result<()> {
        self.1
            .send(Some(projection))
            .map_err(|_| "Projection session closed".into())
    }
    pub async fn call(&self, request: LocalRequest) -> Result<LocalResponse> {
        match self.call_wire(WireRequest::Legacy(request)).await? {
            WireResponse::Legacy(response) => Ok(*response),
            WireResponse::Fenced(_) => {
                Err("Wrong response contract; acceptance is unknown, no fallback".into())
            }
        }
    }
    pub async fn call_fenced(&self, request: FencedRequest) -> Result<FencedResponse> {
        let context = request.context.clone();
        match self
            .call_wire(WireRequest::Fenced(Box::new(request)))
            .await?
        {
            WireResponse::Fenced(response) => {
                response.validate_for(&context)?;
                Ok(*response)
            }
            WireResponse::Legacy(_) => {
                Err("Wrong response contract; acceptance is unknown, no fallback".into())
            }
        }
    }
    async fn call_wire(&self, request: WireRequest) -> Result<WireResponse> {
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
    mut channels: RpcChannels,
    generation: &str,
) -> Result<()> {
    let mut pending: HashMap<String, PendingReply> = HashMap::new();
    let mut pending_extensions: HashMap<String, PendingExtension> = HashMap::new();
    let mut counter = 0_u64;
    let mut nonce = 0_u64;
    let mut ping: Option<(u64, Instant)> = None;
    let mut next_ping = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    let mut projection_open = true;
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let now = Instant::now();
                if ping.is_some_and(|(_, sent)| now.duration_since(sent) >= HEARTBEAT_TIMEOUT) {
                    return Err("Worker heartbeat timeout; last task state is retained".into());
                }
                if pending.values().any(|reply| now.duration_since(reply.sent) >= RPC_TIMEOUT) || pending_extensions.values().any(|reply|now.duration_since(reply.sent)>=RPC_TIMEOUT) {
                    return Err("Worker response timeout; reconcile the same task/key without automatic retry".into());
                }
                if ping.is_none() && now >= next_ping {
                    nonce += 1;
                    send(&mut socket, &SessionMessage::Ping { nonce }).await?;
                    ping = Some((nonce, now));
                }
            }
            call=channels.extensions.recv()=> {
                let Some(call)=call else {return Ok(());};
                if call.reply.is_closed() {continue;}
                if pending.len()+pending_extensions.len()>=MAX_IN_FLIGHT {let _=call.reply.send(Err("Readonly extension query rejected: session busy".into()));continue;}
                counter+=1;
                let id=format!("{generation}-{counter}");
                let message=SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionRequest {id:id.clone(),request:call.request});
                let bytes=match encode(&message) {Ok(bytes)=>bytes,Err(error)=>{let _=call.reply.send(Err(error));continue;}};
                pending_extensions.insert(id,PendingExtension {sent:Instant::now(),reply:call.reply});
                send_encoded(&mut socket,bytes).await?;
            }
            changed = channels.projections.changed(), if projection_open => {
                if changed.is_err() { projection_open = false; continue; }
                let projection = channels.projections.borrow_and_update().clone();
                if let Some(projection) = projection {
                    send_frame(&mut socket, &SessionFrame::Projection(winbridge_protocol::ProjectionSessionMessage::ControllerProjection { projection: Box::new(projection) })).await?;
                }
            }
            call = channels.calls.recv() => {
                let Some(call) = call else { return Ok(()); };
                if call.reply.is_closed() { continue; }
                if pending.len()+pending_extensions.len() >= MAX_IN_FLIGHT {
                    let _ = call.reply.send(Err("Worker has 16 pending requests; retry after reconciliation".into()));
                    continue;
                }
                counter += 1;
                let id = format!("{generation}-{counter}");
                let contract = if matches!(call.request, WireRequest::Fenced(_)) { ReplyContract::Fenced } else { ReplyContract::Legacy };
                let message = match call.request {
                    WireRequest::Legacy(request) => SessionFrame::Legacy(SessionMessage::Request { id: id.clone(), request }),
                    WireRequest::Fenced(request) => SessionFrame::Fenced(FencedSessionMessage::Request { id: id.clone(), request: *request }),
                };
                let bytes = match encode(&message) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = call.reply.send(Err(format!("{error}; request rejected before transmission")));
                        continue;
                    }
                };
                // Insert before writing: losing this write/ACK leaves acceptance unknown.
                pending.insert(id.clone(), PendingReply { sent: Instant::now(), contract, reply: call.reply });
                send_encoded(&mut socket, bytes).await?;
            }
            message = receive_frame(&mut socket) => {
                match message? {
                    None => {},
                    Some(SessionFrame::Legacy(SessionMessage::Pong { nonce })) if ping.is_some_and(|(expected, _)| nonce == expected) => {
                        ping = None;
                        next_ping = Instant::now() + HEARTBEAT_INTERVAL;
                    }
                    Some(SessionFrame::Legacy(SessionMessage::Response { id, response })) => {
                        let Some(reply) = pending.remove(&id) else { return Err("Unknown WebSocket response correlation ID".into()); };
                        if !matches!(reply.contract, ReplyContract::Legacy) { return Err("Legacy response to a fenced request; acceptance is unknown".into()); }
                        let _ = reply.reply.send(Ok(WireResponse::Legacy(Box::new(response))));
                    }
                    Some(SessionFrame::Fenced(FencedSessionMessage::Response { id, response })) => {
                        let Some(reply) = pending.remove(&id) else { return Err("Unknown WebSocket response correlation ID".into()); };
                        if !matches!(reply.contract, ReplyContract::Fenced) { return Err("Fenced response to a legacy request; acceptance is unknown".into()); }
                        let _ = reply.reply.send(Ok(WireResponse::Fenced(Box::new(response))));
                    }
                    Some(SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionResponse {id,response}))=> {
                        let Some(pending)=pending_extensions.remove(&id) else {return Err("Unknown extension correlation ID".into());};
                        let _=pending.reply.send(Ok(response));
                    }
                    _ => return Err("Unexpected message in controller session".into()),
                }
            }
        }
    }
}
