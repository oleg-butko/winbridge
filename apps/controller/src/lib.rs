//! Debian dispatch only. Windows owns acceptance, execution and the durable journal.
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::net::TcpStream;
use winbridge_core::controller_config::ControllerConfig;
use winbridge_protocol::{
    Capability, Component, ControllerStatus, FencedRequest, FencedResponse, LocalRequest,
    LocalResponse, PeerIdentity, RequestContext, SessionMessage, WireRequest, WireResponse,
    FENCED_REQUEST_CONTRACT, PROTOCOL_VERSION,
};
use winbridge_transport::websocket::{self as ws, Result, RpcSender};
mod activity;
pub mod mcp;
#[cfg(unix)]
mod operations;
#[cfg(unix)]
mod projection;
#[cfg(unix)]
pub mod readiness;
#[cfg(unix)]
pub mod readiness_store;
#[cfg(unix)]
mod verification;

pub fn publish(
    config: &ControllerConfig,
    project: &str,
    revision: &str,
) -> Result<winbridge_protocol::SnapshotReady> {
    Ok(publish_measured(config, project, revision)?.ready)
}

pub fn publish_measured(
    config: &ControllerConfig,
    project: &str,
    revision: &str,
) -> Result<winbridge_workspace_sync::PublicationMeasurement> {
    let source = config
        .projects
        .iter()
        .find(|p| p.id == project)
        .ok_or("Source project is not registered in controller JSON")?;
    winbridge_workspace_sync::publish_measured(
        config
            .delivery
            .as_ref()
            .ok_or("Configure delivery in controller local JSON")?,
        &source.source_root,
        &config.state_directory,
        project,
        revision,
    )
}

pub fn publish_delta_measured(
    config: &ControllerConfig,
    project: &str,
    revision: &str,
    base: &str,
) -> Result<winbridge_workspace_sync::PublicationMeasurement> {
    let source = config
        .projects
        .iter()
        .find(|p| p.id == project)
        .ok_or("Source project is not registered in controller JSON")?;
    winbridge_workspace_sync::publish_delta_measured(
        config
            .delivery
            .as_ref()
            .ok_or("Configure delivery in controller local JSON")?,
        &source.source_root,
        &config.state_directory,
        project,
        revision,
        base,
    )
}

pub fn publish_resumable_measured(
    config: &ControllerConfig,
    project: &str,
    revision: &str,
    base: Option<&str>,
) -> Result<winbridge_workspace_sync::PublicationMeasurement> {
    let source = config
        .projects
        .iter()
        .find(|p| p.id == project)
        .ok_or("Source project is not registered in controller JSON")?;
    winbridge_workspace_sync::publish_resumable_measured(
        config
            .delivery
            .as_ref()
            .ok_or("Configure delivery in controller local JSON")?,
        &source.source_root,
        &config.state_directory,
        project,
        revision,
        base,
    )
}

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
    #[cfg(unix)]
    verification_runs: Mutex<std::collections::HashSet<String>>,
    #[cfg(unix)]
    verification_history: Mutex<Option<verification::RecentPlans>>,
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
            #[cfg(unix)]
            verification_runs: Mutex::new(std::collections::HashSet::new()),
            #[cfg(unix)]
            verification_history: Mutex::new(None),
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
        match self.call_scoped(request, None).await {
            Ok(WireResponse::Legacy(response)) => *response,
            Ok(WireResponse::Fenced(_)) => LocalResponse::Error {
                message: "Wrong response contract; no automatic retry".into(),
            },
            Err(message) => LocalResponse::Error { message },
        }
    }
    pub async fn call_fenced(&self, request: FencedRequest) -> FencedResponse {
        if request.request_contract_version != FENCED_REQUEST_CONTRACT {
            return FencedResponse::rejected(
                "Unsupported request contract; no automatic fallback".into(),
            );
        }
        match self
            .call_scoped(request.request, Some(request.context))
            .await
        {
            Ok(WireResponse::Fenced(response)) => *response,
            Ok(WireResponse::Legacy(_)) => FencedResponse::unconfirmed(
                "Wrong response contract; acceptance is unknown, no automatic retry".into(),
            ),
            Err(message) => FencedResponse::unconfirmed(message),
        }
    }
    pub async fn call_wire(&self, request: WireRequest) -> WireResponse {
        match request {
            WireRequest::Legacy(request) => {
                WireResponse::Legacy(Box::new(self.call(request).await))
            }
            WireRequest::Fenced(request) => {
                WireResponse::Fenced(Box::new(self.call_fenced(*request).await))
            }
        }
    }
    async fn call_scoped(
        &self,
        request: LocalRequest,
        context: Option<RequestContext>,
    ) -> Result<WireResponse> {
        if request.local_maintenance_only() {
            return Err("Settings/output cleanup require Windows desktop local IPC; request was not transmitted".into());
        }
        let result = async {
            if matches!(request, LocalRequest::ControllerStatus) {
                if context.is_some() { return Err("Controller status is a local observation, not a worker-scoped contract-2 request".into()); }
                return Ok(WireResponse::Legacy(Box::new(LocalResponse::ControllerStatus { status: self.status()? })));
            }
            let (sender, session_id, worker_address) = {
                let state = self.state.lock().map_err(|_| "controller session state poisoned")?;
                let session = state.active.as_ref().filter(|s| s.ready)
                    .ok_or("Windows worker is offline; last task state is retained. No command was automatically retried")?;
                if matches!(request, LocalRequest::History {..} | LocalRequest::PrepareTaskDiagnostic {..} | LocalRequest::StorageUsage) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::HistoryRead)) {
                    return Err("Worker does not advertise history_read; request was not transmitted. Update worker/desktop together".into());
                }
                if matches!(request, LocalRequest::ListAttentionEvents {..} | LocalRequest::AcknowledgeAttentionEvent {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::AttentionTargets)) {
                    return Err("Worker does not advertise attention_targets; request was not transmitted. Update worker/desktop together".into());
                }
                if matches!(request, LocalRequest::RequestAttention {..} | LocalRequest::ListAttention {..} | LocalRequest::AcknowledgeAttention {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::AttentionRequest)) {
                    return Err("Worker does not advertise attention_request; request was not transmitted. Update worker/desktop together".into());
                }
                if matches!(request,LocalRequest::SubmitWorkflow {..}|LocalRequest::ListWorkflows|LocalRequest::GetWorkflow {..}|LocalRequest::CancelWorkflow {..}|LocalRequest::ListApplications|LocalRequest::GetApplication {..}|LocalRequest::StopApplication {..}|LocalRequest::ReadApplicationLog {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::LogRead)) {
                    return Err("Worker does not advertise stage-4 workflow/log support; update the fixed runtime with the combined release".into());
                }
                if matches!(request, LocalRequest::ImportDeltaSnapshot {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::WorkspaceDelta)) {
                    return Err("Worker does not support delta snapshots; update worker or import a full package".into());
                }
                if matches!(request, LocalRequest::ImportSnapshot {..} | LocalRequest::ImportDeltaSnapshot {..} | LocalRequest::ListImports | LocalRequest::GetImport {..} | LocalRequest::CancelImport {..} | LocalRequest::PreviewSnapshot {..} | LocalRequest::ListSnapshots {..}) && !state.last_peer.as_ref().is_some_and(|(p,_)|p.capabilities.contains(&Capability::WorkspaceImport)) {
                    return Err("Worker does not advertise workspace_import; update the fixed runtime and configure delivery in local JSON".into());
                }
                (session.sender.clone(), session.id.clone(), state.last_peer.as_ref().map(|(_, address)| address.to_string()))
            };
            if let Some(context) = &context {
                if context.instance_id != self.config.instance_id { return Err("Request instance does not match this controller; original request was not transmitted".into()); }
                let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else { return Err("Cannot discover request contract before transmission".into()); };
                let supported = info.fenced_requests.as_ref().filter(|info| info.request_contract_version == FENCED_REQUEST_CONTRACT).ok_or("Worker does not support request contract 2; request was not transmitted and no v1 fallback occurred")?;
                if &supported.context != context { return Err("Caller-pinned journal incarnation no longer matches worker; original request was not transmitted. Do not rebind this logical retry to the new scope".into()); }
            }
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
            let result = match context {
                Some(context) => sender.call_fenced(FencedRequest { request_contract_version: FENCED_REQUEST_CONTRACT, context, request }).await.map(|response| WireResponse::Fenced(Box::new(response))),
                None => sender.call(request).await.map(|response|WireResponse::Legacy(Box::new(response))),
            };
            let outcome = match &result {
                Ok(WireResponse::Fenced(response)) if response.outcome == winbridge_protocol::FencedReplyOutcome::Unconfirmed => "unconfirmed",
                Ok(WireResponse::Fenced(response)) if response.outcome == winbridge_protocol::FencedReplyOutcome::RejectedBeforeDispatch => "rejected_before_dispatch",
                Ok(response) if matches!(response.response(), LocalResponse::Error { .. }) => "responded_error",
                Ok(_) => "responded",
                Err(error) if error.ends_with("request rejected before transmission") => "rejected_before_transmission",
                Err(_) => "unconfirmed",
            };
            self.activity.record(serde_json::json!({"event":"rpc_completed","origin":"local_ipc","workerAddress":worker_address,"sessionId":session_id,"method":method,"outcome":outcome}));
            result
        }.await;
        result
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
                    sender: sender.clone(),
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
        #[cfg(unix)]
        let mut publishers = tokio::task::JoinSet::new();
        #[cfg(unix)]
        {
            let owner = Arc::clone(&guard.controller);
            let projection_session = id.clone();
            publishers.spawn(async move {
                if let Err(error) = projection::publish(owner, sender, projection_session).await {
                    eprintln!("Readonly readiness projection stopped: {error}; task execution and RPC session continue");
                }
            });
        }
        let result = ws::controller_session(socket, calls, &id).await;
        #[cfg(unix)]
        publishers.shutdown().await;
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
        LocalRequest::GetRuntimeSettings => "get_runtime_settings",
        LocalRequest::SaveRuntimeSettings { .. } => "save_runtime_settings",
        LocalRequest::PreviewOutputCleanup { .. } => "preview_output_cleanup",
        LocalRequest::ApplyOutputCleanup { .. } => "apply_output_cleanup",
        LocalRequest::History { .. } => "history",
        LocalRequest::PrepareTaskDiagnostic { .. } => "prepare_task_diagnostic",
        LocalRequest::StorageUsage => "storage_usage",
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
        LocalRequest::ImportDeltaSnapshot { .. } => "import_delta_snapshot",
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
