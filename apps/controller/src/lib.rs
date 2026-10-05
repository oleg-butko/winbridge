//! Debian dispatch only. Windows owns acceptance, execution and the durable journal.
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::net::TcpStream;
use winbridge_core::controller_config::ControllerConfig;
use winbridge_protocol::{
    Capability, Component, ControllerStatus, LocalRequest, LocalResponse, PeerIdentity,
    SessionMessage, PROTOCOL_VERSION,
};
use winbridge_transport::websocket::{self as ws, Result, RpcSender};

struct Session {
    sender: RpcSender,
    id: String,
    ready: bool,
}
#[derive(Default)]
struct State {
    active: Option<Session>,
    last_peer: Option<(PeerIdentity, SocketAddr)>,
    last_error: Option<String>,
}
pub struct Controller {
    pub config: ControllerConfig,
    pub generation: String,
    state: Mutex<State>,
}
impl Controller {
    pub fn new(config: ControllerConfig) -> Result<Arc<Self>> {
        config.validate()?;
        Ok(Arc::new(Self {
            config,
            generation: uuid::Uuid::new_v4().to_string(),
            state: Mutex::new(State::default()),
        }))
    }
    pub fn status(&self) -> Result<ControllerStatus> {
        let state = self
            .state
            .lock()
            .map_err(|_| "controller session state poisoned")?;
        Ok(ControllerStatus {
            instance_id: self.config.instance_id.clone(),
            node_id: self.config.node_id.clone(),
            generation: self.generation.clone(),
            listen_address: self.config.controller.listen_address.to_string(),
            connected: state.active.as_ref().is_some_and(|s| s.ready),
            session_id: state.active.as_ref().map(|s| s.id.clone()),
            worker_node_id: state.last_peer.as_ref().map(|(p, _)| p.node_id.clone()),
            worker_generation: state.last_peer.as_ref().map(|(p, _)| p.generation.clone()),
            worker_address: state.last_peer.as_ref().map(|(_, a)| a.to_string()),
            last_error: state.last_error.clone(),
        })
    }
    pub async fn call(&self, request: LocalRequest) -> LocalResponse {
        let result = async {
            if matches!(request, LocalRequest::ControllerStatus) {
                return Ok(LocalResponse::ControllerStatus { status: self.status()? });
            }
            let sender = self.state.lock().map_err(|_| "controller session state poisoned")?
                .active.as_ref().filter(|s| s.ready).map(|s| s.sender.clone())
                .ok_or("Windows worker is offline; last task state is retained. No command was automatically retried")?;
            sender.call(request).await
        }.await;
        result.unwrap_or_else(|message| LocalResponse::Error { message })
    }
    pub async fn accept(self: Arc<Self>, stream: TcpStream, address: SocketAddr) -> Result<()> {
        if !self
            .config
            .controller
            .allowed_worker_addresses
            .iter()
            .any(|ip| std::net::IpAddr::V4(*ip) == address.ip())
        {
            return Err(format!(
                "Worker address {} is not in controller.allowedWorkerAddresses",
                address.ip()
            ));
        }
        let handshake = tokio::time::timeout(ws::HANDSHAKE_TIMEOUT, async {
            let mut socket = ws::accept_async_with_config(stream, Some(ws::config()))
                .await
                .map_err(|e| e.to_string())?;
            let message = ws::first(&mut socket).await?;
            let reason = match &message {
                SessionMessage::Hello { peer } => self.validate_peer(peer).err(),
                _ => Some("First message must be worker hello".into()),
            };
            if let Some(message) = reason {
                ws::send(
                    &mut socket,
                    &SessionMessage::Rejected {
                        message: message.clone(),
                    },
                )
                .await?;
                return Err(message);
            }
            let SessionMessage::Hello { peer } = message else {
                unreachable!()
            };
            Ok((socket, peer))
        })
        .await
        .map_err(|_| "Worker handshake timeout after 5 seconds".to_string())??;
        let (mut socket, peer) = handshake;
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, calls) = ws::rpc_channel();
        let reserved = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "controller session state poisoned")?;
            if state.active.is_some() {
                false
            } else {
                state.last_peer = Some((peer.clone(), address));
                state.last_error = None;
                state.active = Some(Session {
                    sender,
                    id: id.clone(),
                    ready: false,
                });
                true
            }
        };
        if !reserved {
            ws::send(&mut socket, &SessionMessage::Rejected { message: "This pair already has an active worker session; stop the other worker or wait for heartbeat expiry".into() }).await?;
            return Err("Duplicate worker session rejected".into());
        }
        let result = async {
            ws::send(
                &mut socket,
                &SessionMessage::Welcome {
                    session_id: id.clone(),
                    peer: PeerIdentity {
                        protocol_version: PROTOCOL_VERSION,
                        application_version: env!("CARGO_PKG_VERSION").into(),
                        component: Component::Controller,
                        instance_id: self.config.instance_id.clone(),
                        node_id: self.config.node_id.clone(),
                        generation: self.generation.clone(),
                        capabilities: vec![Capability::TaskSubmit, Capability::TaskCancel],
                    },
                },
            )
            .await?;
            if let Some(session) = self
                .state
                .lock()
                .map_err(|_| "controller session state poisoned")?
                .active
                .as_mut()
            {
                session.ready = true;
            }
            eprintln!("WinBridge worker connected: {address}; session {id}");
            ws::controller_session(socket, calls, &id).await
        }
        .await;
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "controller session state poisoned")?;
            if state.active.as_ref().is_some_and(|s| s.id == id) {
                state.active = None;
                state.last_error = result
                    .as_ref()
                    .err()
                    .cloned()
                    .or(Some("Worker session ended".into()));
            }
        }
        result
    }
    fn validate_peer(&self, peer: &PeerIdentity) -> Result<()> {
        if peer.protocol_version != PROTOCOL_VERSION {
            return Err(format!("Unsupported protocolVersion {}; controller expects {PROTOCOL_VERSION}. Update both builds", peer.protocol_version));
        }
        if peer.component != Component::Worker
            || peer.instance_id != self.config.instance_id
            || peer.node_id != self.config.controller.worker_node_id
        {
            return Err(
                "Worker component/instanceId/nodeId mismatch; use the configured local pair".into(),
            );
        }
        if !winbridge_core::config::valid_id(&peer.generation)
            || peer.application_version.is_empty()
            || peer.application_version.len() > 80
            || peer.capabilities.len() > 16
            || !peer.capabilities.contains(&Capability::TaskSubmit)
            || !peer.capabilities.contains(&Capability::TaskCancel)
        {
            return Err("Worker handshake needs a valid generation/appVersion and task_submit/task_cancel capabilities".into());
        }
        Ok(())
    }
}

pub fn endpoint(config: &ControllerConfig) -> String {
    config
        .state_directory
        .join(format!("controller-{}.sock", config.instance_id))
        .to_string_lossy()
        .into_owned()
}

#[cfg(unix)]
mod server;
#[cfg(unix)]
pub use server::serve;
#[cfg(not(unix))]
pub async fn serve(_controller: Arc<Controller>) -> Result<()> {
    Err("Controller listener runs in Debian; use winbridge-worker on Windows".into())
}
