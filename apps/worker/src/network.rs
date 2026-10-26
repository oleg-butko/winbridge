use crate::{
    controller_links::{Links, RpcGuard},
    Worker,
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::Instant,
};
use winbridge_core::controller_sessions::{Endpoint, Peer, Ticket};
use winbridge_protocol::{
    Capability, Component, FencedSessionMessage, PeerIdentity, SessionFrame, SessionMessage,
    WireRequest, WireResponse, PROTOCOL_VERSION,
};
#[cfg(test)]
use winbridge_protocol::{ConnectionState, LocalResponse};
use winbridge_transport::websocket::{self as ws, Result};

struct ConnectionGuard {
    links: Arc<Mutex<Links>>,
    ticket: Ticket,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut links) = self.links.lock() {
            links.disconnected(&self.ticket, "Controller loop stopped".into());
        }
    }
}
pub(crate) struct NetworkTask {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}
impl NetworkTask {
    pub async fn close(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = (&mut self.task).await;
    }
}
impl Drop for NetworkTask {
    fn drop(&mut self) {
        // Let the supervisor cancel AND join its children. Aborting only the
        // parent would return before child connection owners dropped the Worker.
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
pub(crate) fn start(worker: Arc<Worker>) -> NetworkTask {
    let (stop, receiver) = oneshot::channel();
    NetworkTask {
        stop: Some(stop),
        task: tokio::spawn(connect(worker, receiver)),
    }
}
async fn connect(worker: Arc<Worker>, mut stop: oneshot::Receiver<()>) {
    let topology = match worker.config.worker.topology() {
        Ok(Some(t)) => t,
        Ok(None) => return,
        Err(e) => {
            eprintln!("Controller configuration error: {e}");
            return;
        }
    };
    let mut tasks = JoinSet::new();
    for endpoint in topology.controllers {
        tasks.spawn(connect_endpoint(worker.clone(), endpoint));
    }
    loop {
        tokio::select! {
            _ = &mut stop => {
                tasks.shutdown().await;
                return;
            }
            result = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = result {
                    eprintln!("Controller connection owner failed: {error}; other sessions remain active");
                }
            }
        }
    }
}
async fn connect_endpoint(worker: Arc<Worker>, endpoint: Endpoint) {
    let Some(links) = worker.controllers.clone() else {
        return;
    };
    let url = endpoint.url;
    let address = match winbridge_core::controller_config::validate_controller_url(&url) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Controller endpoint invalid: {e}");
            return;
        }
    };
    let executable = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "the running winbridge-worker.exe".into());
    let mut backoff = 1_u64;
    loop {
        let ticket = match links
            .lock()
            .map_err(|_| "Controller state lock poisoned")
            .and_then(|mut l| {
                l.begin(&endpoint.id)
                    .map_err(|_| "Controller attempt unavailable")
            }) {
            Ok(t) => t,
            Err(error) => {
                eprintln!("Controller {}: {error}", endpoint.id);
                return;
            }
        };
        let guard = ConnectionGuard {
            links: links.clone(),
            ticket: ticket.clone(),
        };
        let mut stage = "TCP connection";
        let handshake=tokio::time::timeout(ws::HANDSHAKE_TIMEOUT,async {
            let stream=TcpStream::connect(address).await.map_err(|error|{
                let detail=if error.raw_os_error()==Some(10013){format!("Windows denied socket access (WSAEACCES 10013). Check outbound TCP firewall/security permission for this exact executable: {executable}")}
                else if error.kind()==std::io::ErrorKind::ConnectionRefused {"Connection refused; check that the Debian controller is listening".into()}
                else {"Check local reachability and outbound TCP permission".into()};
                format!("TCP connection to {address} failed: {error}. {detail}")
            })?;
            stream.set_nodelay(true).map_err(|e|format!("TCP socket setup for {address} failed: {e}"))?;
            stage="WebSocket upgrade (TCP connected)";
            let (mut socket,_)=ws::client_async_with_config(&url,stream,Some(ws::config())).await.map_err(|e|format!("WebSocket upgrade at {url} failed after TCP connected: {e}"))?;
            stage="WinBridge Hello/Welcome exchange (TCP and WebSocket connected)";
            ws::send(&mut socket,&SessionMessage::Hello {peer:PeerIdentity {
                protocol_version:PROTOCOL_VERSION,application_version:env!("CARGO_PKG_VERSION").into(),component:Component::Worker,
                instance_id:worker.config.instance_id.clone(),node_id:worker.config.node_id.clone(),generation:worker.generation.clone(),
                capabilities:if worker.config.delivery.is_some(){vec![Capability::TaskSubmit,Capability::TaskCancel,Capability::WorkspaceImport,Capability::WorkspaceDelta,Capability::LogRead,Capability::AttentionRequest,Capability::AttentionTargets,Capability::HistoryRead]}
                    else{vec![Capability::TaskSubmit,Capability::TaskCancel,Capability::AttentionRequest,Capability::AttentionTargets,Capability::HistoryRead]},
            }}).await?;
            match ws::first(&mut socket).await? {
                SessionMessage::Welcome {peer,session_id} if peer.protocol_version==PROTOCOL_VERSION && peer.component==Component::Controller
                    && peer.instance_id==worker.config.instance_id && winbridge_core::config::valid_id(&peer.node_id)
                    && winbridge_core::config::valid_id(&peer.generation) && winbridge_core::config::valid_id(&session_id)
                    && !peer.application_version.is_empty() && peer.application_version.len()<=80 && peer.capabilities.len()<=16
                    && peer.capabilities.contains(&Capability::TaskSubmit) && peer.capabilities.contains(&Capability::TaskCancel)=>{
                    let connected=links.lock().map_err(|_|"Controller state lock poisoned")?.connected(&ticket,Peer {node:peer.node_id,generation:peer.generation,session:session_id})?;
                    if !connected {return Err("Controller attempt superseded".into())}
                    Ok(socket)
                }
                SessionMessage::Rejected {message}=>Err(format!("Controller rejected handshake: {message}")),
                _=>Err("Controller handshake mismatch: protocolVersion/component/instanceId/nodeId/generation/appVersion/capabilities; check both builds and pair configuration".into()),
            }
        }).await;
        let result=match handshake {Ok(Ok(socket))=>{backoff=1;session(socket,worker.clone(),links.clone(),ticket.clone()).await},Ok(Err(e))=>Err(e),Err(_)=>Err(format!("{stage} timeout within the 5-second connection deadline at {address}; check the Debian listener and firewall permission for {executable}"))};
        let message = result
            .err()
            .unwrap_or_else(|| "Controller session ended".into());
        eprintln!("WinBridge controller [{}] {url}: {message}; reconnect in {backoff}s, accepted tasks continue",endpoint.id);
        if let Ok(mut l) = links.lock() {
            l.disconnected(&ticket, message);
        }
        drop(guard);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(10);
    }
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: ws::WebSocketStream<S>,
    worker: Arc<Worker>,
    links: Arc<Mutex<Links>>,
    ticket: Ticket,
) -> Result<()> {
    let mut handlers: JoinSet<(String, WireResponse)> = JoinSet::new();
    let mut active_ids = HashSet::new();
    let mut extensions: JoinSet<(String, winbridge_protocol::ExtensionResponse)> = JoinSet::new();
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
                let mut message = SessionFrame::response(id.clone(), response);
                if serde_json::to_vec(&message).map_err(|e| e.to_string())?.len() > winbridge_transport::MAX_FRAME_BYTES {
                    let response = match message {
                        SessionFrame::Legacy(SessionMessage::Response { response, .. }) => WireResponse::Legacy(Box::new(response)),
                        SessionFrame::Fenced(FencedSessionMessage::Response { response, .. }) => WireResponse::Fenced(Box::new(response)),
                        _ => unreachable!(),
                    };
                    message = SessionFrame::response(id, response.with_error("Response exceeds 1 MiB; request an individual task/output page".into()));
                }
                ws::send_frame(&mut socket, &message).await?;
            }
            result=extensions.join_next(),if !extensions.is_empty()=> {
                let (id,mut response)=result.ok_or("Extension handler disappeared")?.map_err(|e|e.to_string())?;
                active_ids.remove(&id);
                if serde_json::to_vec(&response).map_err(|e|e.to_string())?.len()>winbridge_transport::MAX_FRAME_BYTES-512 {response=winbridge_protocol::ExtensionResponse::rejected(response.contract,winbridge_protocol::ExtensionError::QueryFailed,"Operation page exceeds frame budget; reduce limit".into());}
                ws::send_frame(&mut socket,&SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionResponse {id,response})).await?;
            }
            message = ws::receive_frame(&mut socket) => match message? {
                None => {},
                Some(SessionFrame::Legacy(SessionMessage::Ping { nonce })) if nonce > last_nonce => {
                    ws::send(&mut socket, &SessionMessage::Pong { nonce }).await?;
                    last_nonce = nonce; last_ping = Instant::now();
                    links.lock().map_err(|_|"Controller state lock poisoned")?.heartbeat(&ticket);
                }
                Some(SessionFrame::Projection(winbridge_protocol::ProjectionSessionMessage::ControllerProjection { projection })) => {
                    let _ = worker.accept_projection(&ticket, *projection)?;
                }
                Some(SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionRequest {id,request}))=> {
                    if id.is_empty() || id.len()>160 || !id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-') || active_ids.contains(&id) {return Err("Invalid/duplicate extension correlation ID".into());}
                    if handlers.len()+extensions.len()>=ws::MAX_IN_FLIGHT {
                        ws::send_frame(&mut socket,&SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionResponse {id,response:winbridge_protocol::ExtensionResponse::rejected(request.contract,winbridge_protocol::ExtensionError::QueryFailed,"Readonly query rejected before dispatch: session busy".into())})).await?;continue;
                    }
                    let guard=match RpcGuard::admit(&links,&ticket,id.clone()) {
                        Ok(g)=>g,Err(error)=>{ws::send_frame(&mut socket,&SessionFrame::Extension(winbridge_protocol::ExtensionSessionMessage::ExtensionResponse {id,response:winbridge_protocol::ExtensionResponse::rejected(request.contract,winbridge_protocol::ExtensionError::QueryFailed,format!("Readonly request rejected before dispatch: {error}"))})).await?;continue}
                    };
                    active_ids.insert(id.clone());let worker=Arc::clone(&worker);
                    extensions.spawn(async move {
                        let contract=request.contract.clone();
                        let response=match tokio::time::timeout(ws::RPC_TIMEOUT,tokio::task::spawn_blocking(move||{let _guard=guard;worker.handle_extension(request)})).await {
                            Ok(Ok(response))=>response,
                            _=>winbridge_protocol::ExtensionResponse::rejected(contract,winbridge_protocol::ExtensionError::QueryFailed,"Readonly query unavailable/deadline; no command acceptance".into()),
                        };(id,response)
                    });
                }
                Some(message) => {
                    let (id, request) = match message {
                        SessionFrame::Legacy(SessionMessage::Request { id, request }) => (id, WireRequest::Legacy(request)),
                        SessionFrame::Fenced(FencedSessionMessage::Request { id, request }) => (id, WireRequest::Fenced(Box::new(request))),
                        _ => return Err("Unexpected message in worker session".into()),
                    };
                    if id.is_empty() || id.len() > 160 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') || active_ids.contains(&id) {
                        return Err("Invalid/duplicate WebSocket correlation ID".into());
                    }
                    if handlers.len()+extensions.len() >= ws::MAX_IN_FLIGHT {
                        ws::send_frame(&mut socket, &SessionFrame::response(id, WireResponse::rejected_for(&request, "Worker has 16 active requests; request rejected before acceptance".into()))).await?;
                        continue;
                    }
                    let guard=match RpcGuard::admit(&links,&ticket,id.clone()) {
                        Ok(g)=>g,Err(error)=>{ws::send_frame(&mut socket,&SessionFrame::response(id,WireResponse::rejected_for(&request,format!("Request rejected before acceptance: {error}")))).await?;continue}
                    };
                    active_ids.insert(id.clone());
                    let worker = Arc::clone(&worker);
                    let ticket=ticket.clone();
                    handlers.spawn(async move {
                        let fallback = WireResponse::rejected_for(&request, String::new());
                        let result = tokio::time::timeout(ws::RPC_TIMEOUT, tokio::task::spawn_blocking(move || {
                            let _guard=guard;
                            if request.request().local_maintenance_only() { WireResponse::rejected_for(&request, "Runtime settings and output cleanup are available through local desktop IPC only".into()) }
                            else { worker.handle_controller_wire(&ticket, request) }
                        })).await;
                        let response = match result {
                            Ok(Ok(response)) => response,
                            Ok(Err(error)) => fallback.with_error(format!("Worker handler failed: {error}; reconcile the same task/key and pinned request scope")),
                            Err(_) => fallback.with_error("Worker handler timeout; acceptance is unknown, reconcile the same task/key and pinned request scope without automatic retry".into()),
                        };
                        (id, response)
                    });
                }
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
    #[tokio::test]
    async fn closing_owner_drains_every_endpoint_before_returning() {
        let root = tempfile::tempdir().unwrap();
        let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = test_worker(root.path(), a.local_addr().unwrap())
            .config
            .clone();
        // Drop the temporary worker before reopening its private journal.
        config.worker.controller_url = None;
        config.worker.controller_topology = Some(
            serde_json::from_value(serde_json::json!({
                "schemaVersion":1,"primary":"a","controllers":[
                    {"id":"a","url":format!("ws://{}",a.local_addr().unwrap())},
                    {"id":"b","url":format!("ws://{}",b.local_addr().unwrap())}]
            }))
            .unwrap(),
        );
        let worker = Worker::open(config).unwrap();
        let owner = start(worker.clone());
        let streams = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(a.accept(), b.accept())
        })
        .await
        .unwrap();
        streams.0.as_ref().unwrap();
        streams.1.as_ref().unwrap();
        // Both tasks are suspended mid-upgrade. close must cancel and JOIN them.
        owner.close().await;
        assert_eq!(
            Arc::strong_count(&worker),
            1,
            "Connection owner retained worker lock"
        );
        let links = worker.controllers.as_ref().unwrap().lock().unwrap();
        assert!(links
            .links
            .values()
            .all(|l| matches!(l.connection.state, ConnectionState::Disconnected)));
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
        let connector = start(Arc::clone(&worker));
        let error = connection_error(&worker).await;
        assert!(
            error.starts_with("TCP connection to") && error.contains("Connection refused"),
            "{error}"
        );
        assert!(
            matches!(worker.handle(LocalRequest::ListTasks), LocalResponse::Tasks { tasks } if tasks.is_empty())
        );
        connector.close().await;
    }
    #[tokio::test]
    async fn timeout_distinguishes_websocket_upgrade_from_hello_welcome() {
        for websocket_upgraded in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let worker = test_worker(root.path(), listener.local_addr().unwrap());
            let connector = start(Arc::clone(&worker));
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
            connector.close().await;
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
                let connector = start(Arc::clone(&worker));
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
                connector.close().await;
            }
        }).await.unwrap();
    }
}
