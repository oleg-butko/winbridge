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
mod activity;

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
    activity: activity::ActivityLog,
}
impl Controller {
    pub fn new(config: ControllerConfig) -> Result<Arc<Self>> {
        config.validate()?;
        let activity = activity::ActivityLog::open(&config.state_directory)?;
        Ok(Arc::new(Self {
            config,
            generation: uuid::Uuid::new_v4().to_string(),
            state: Mutex::new(State::default()),
            activity,
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
            activity_log: Some(self.activity.status()),
        })
    }
    pub async fn call(&self, request: LocalRequest) -> LocalResponse {
        let result = async {
            if matches!(request, LocalRequest::ControllerStatus) {
                return Ok(LocalResponse::ControllerStatus { status: self.status()? });
            }
            let (sender, session_id, worker_address) = {
                let state = self.state.lock().map_err(|_| "controller session state poisoned")?;
                let session = state.active.as_ref().filter(|s| s.ready)
                    .ok_or("Windows worker is offline; last task state is retained. No command was automatically retried")?;
                if matches!(request, LocalRequest::ListAttentionEvents {..} | LocalRequest::AcknowledgeAttentionEvent {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::AttentionTargets)) {
                    return Err("Worker does not advertise attention_targets; request was not transmitted. Update worker/desktop together".into());
                }
                if matches!(request, LocalRequest::RequestAttention {..} | LocalRequest::ListAttention {..} | LocalRequest::AcknowledgeAttention {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::AttentionRequest)) {
                    return Err("Worker does not advertise attention_request; request was not transmitted. Update worker/desktop together".into());
                }
                if matches!(request,LocalRequest::SubmitWorkflow {..}|LocalRequest::ListWorkflows|LocalRequest::GetWorkflow {..}|LocalRequest::CancelWorkflow {..}|LocalRequest::ListApplications|LocalRequest::GetApplication {..}|LocalRequest::StopApplication {..}|LocalRequest::ReadApplicationLog {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::LogRead)) {
                    return Err("Worker does not advertise stage-4 workflow/log support; update the fixed runtime with the combined release".into());
                }
                if matches!(request, LocalRequest::ImportSnapshot {..} | LocalRequest::ListImports | LocalRequest::GetImport {..} | LocalRequest::CancelImport {..} | LocalRequest::PreviewSnapshot {..} | LocalRequest::ListSnapshots {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::WorkspaceImport)) {
                    return Err("Worker does not advertise workspace_import; update the fixed runtime and configure delivery in local JSON".into());
                }
                (session.sender.clone(), session.id.clone(), state.last_peer.as_ref().map(|(_, address)| address.to_string()))
            };
            if let LocalRequest::SubmitWorkflow { request: workflow } = &request {
                let mut required = Vec::new();
                if !workflow.artifacts.is_empty() { required.push("artifacts"); }
                if workflow.readiness.netwatch.is_some() { required.push("netwatch_health"); }
                if !required.is_empty() {
                    let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else { return Err("Cannot confirm worker workflow features before submission".into()); };
                    if let Some(feature) = required.iter().find(|f| !info.workflow_features.iter().any(|v| v == **f)) {
                        return Err(format!("Worker requires an update for workflow feature {feature}; request was not transmitted"));
                    }
                }
            }
            let method = request_method(&request);
            self.activity.record(serde_json::json!({"event":"rpc_requested","origin":"local_ipc","workerAddress":worker_address,"sessionId":session_id,"method":method}));
            let result = sender.call(request).await;
            let outcome = match &result {
                Ok(LocalResponse::Error { .. }) => "rejected",
                Ok(_) => "responded",
                Err(error) if error.ends_with("request rejected before transmission") => "rejected_before_transmission",
                Err(_) => "unconfirmed",
            };
            self.activity.record(serde_json::json!({"event":"rpc_completed","origin":"local_ipc","workerAddress":worker_address,"sessionId":session_id,"method":method,"outcome":outcome}));
            result
        }.await;
        result.unwrap_or_else(|message| LocalResponse::Error { message })
    }
    pub async fn accept(self: Arc<Self>, stream: TcpStream, address: SocketAddr) -> Result<()> {
        self.network_activity("tcp_accepted", address);
        let mut guard = ConnectionGuard {
            controller: Arc::clone(&self),
            address,
            session_id: None,
            outcome: "interrupted",
            error: None,
        };
        let result = self.accept_inner(stream, address, &mut guard).await;
        guard.outcome = if result.is_ok() { "ended" } else { "failed" };
        guard.error = result.as_ref().err().cloned();
        result
    }
    pub(crate) fn network_activity(&self, event: &str, address: SocketAddr) {
        self.activity.record(serde_json::json!({"event":event,"sourceIp":address.ip().to_string(),"sourceAddress":address.to_string()}));
    }
    async fn accept_inner(
        &self,
        stream: TcpStream,
        address: SocketAddr,
        guard: &mut ConnectionGuard,
    ) -> Result<()> {
        let deadline = tokio::time::Instant::now() + ws::HANDSHAKE_TIMEOUT;
        let handshake = tokio::time::timeout_at(deadline, async {
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
            tokio::time::timeout_at(deadline, ws::send(&mut socket, &SessionMessage::Rejected { message: "This pair already has an active worker session; stop the other worker or wait for heartbeat expiry".into() }))
                .await.map_err(|_| "Worker handshake timeout after 5 seconds".to_string())??;
            return Err("Duplicate worker session rejected".into());
        }
        guard.session_id = Some(id.clone());
        tokio::time::timeout_at(
            deadline,
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
                        capabilities: vec![
                            Capability::TaskSubmit,
                            Capability::TaskCancel,
                            Capability::WorkspaceImport,
                        ],
                    },
                },
            ),
        )
        .await
        .map_err(|_| "Worker handshake timeout after 5 seconds".to_string())??;
        if let Some(session) = self
            .state
            .lock()
            .map_err(|_| "controller session state poisoned")?
            .active
            .as_mut()
        {
            session.ready = true;
        }
        self.activity.record(serde_json::json!({"event":"worker_connected","sourceIp":address.ip().to_string(),"sourceAddress":address.to_string(),"sessionId":id,"nodeId":peer.node_id,"generation":peer.generation}));
        eprintln!("WinBridge worker connected: {address}; session {id}");
        ws::controller_session(socket, calls, &id).await
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

// Runs on return, cancellation and panic; never leaves a reserved session behind.
struct ConnectionGuard {
    controller: Arc<Controller>,
    address: SocketAddr,
    session_id: Option<String>,
    outcome: &'static str,
    error: Option<String>,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(id) = &self.session_id {
            let mut state = self
                .controller
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if state.active.as_ref().is_some_and(|s| &s.id == id) {
                state.active = None;
                state.last_error = Some(
                    self.error
                        .clone()
                        .unwrap_or_else(|| format!("Worker session {}", self.outcome)),
                );
            }
        }
        self.controller.activity.record(serde_json::json!({"event":"connection_closed","sourceIp":self.address.ip().to_string(),"sourceAddress":self.address.to_string(),"sessionId":self.session_id,"outcome":self.outcome}));
    }
}
fn request_method(request: &LocalRequest) -> &'static str {
    match request {
        LocalRequest::ListAttentionEvents { .. } => "list_attention_events",
        LocalRequest::AcknowledgeAttentionEvent { .. } => "acknowledge_attention_event",
        LocalRequest::RequestAttention { .. } => "request_attention",
        LocalRequest::ListAttention { .. } => "list_attention",
        LocalRequest::AcknowledgeAttention { .. } => "acknowledge_attention",
        LocalRequest::SubmitWorkflow { .. } => "submit_workflow",
        LocalRequest::ListWorkflows => "list_workflows",
        LocalRequest::GetWorkflow { .. } => "get_workflow",
        LocalRequest::CancelWorkflow { .. } => "cancel_workflow",
        LocalRequest::ListApplications => "list_applications",
        LocalRequest::GetApplication { .. } => "get_application",
        LocalRequest::StopApplication { .. } => "stop_application",
        LocalRequest::ReadApplicationLog { .. } => "read_application_log",
        LocalRequest::ListSnapshots { .. } => "list_snapshots",
        LocalRequest::ImportSnapshot { .. } => "import_snapshot",
        LocalRequest::ListImports => "list_imports",
        LocalRequest::GetImport { .. } => "get_import",
        LocalRequest::CancelImport { .. } => "cancel_import",
        LocalRequest::PreviewSnapshot { .. } => "preview_snapshot",
        LocalRequest::Info => "info",
        LocalRequest::ControllerStatus => "controller_status",
        LocalRequest::Submit { .. } => "submit",
        LocalRequest::ListTasks => "list_tasks",
        LocalRequest::GetTask { .. } => "get_task",
        LocalRequest::ReadOutput { .. } => "read_output",
        LocalRequest::Cancel { .. } => "cancel",
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
