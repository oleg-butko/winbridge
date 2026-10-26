//! Two actual TCP/WebSocket controller instances, one real Linux worker.
//! No connection to .203/.204 and no Windows processes/configuration involved.
use super::*;
use std::{net::SocketAddr, path::Path, time::Duration};
use tokio::{
    net::{TcpListener, TcpSocket},
    task::{JoinHandle, JoinSet},
    time::Instant,
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{
    CommandSource, ControllerReadinessProjection, FencedReplyOutcome, TaskRecord, TaskStatus,
    TaskSubmission, WorkerInfo,
};
use winbridge_worker::Worker;

fn listen(address: SocketAddr) -> TcpListener {
    let socket = TcpSocket::new_v4().unwrap();
    socket.set_reuseaddr(true).unwrap();
    socket.bind(address).unwrap();
    socket.listen(64).unwrap()
}
fn controller(root: &Path, name: &str, address: SocketAddr, instance: &str) -> Arc<Controller> {
    // Equal node IDs are intentional: copied Debian configs must not alias
    // connection slots. Configured endpoint IDs/session generations separate them.
    Controller::new(serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"debian",
        "stateDirectory":root.join(name),"controller":{"listenAddress":address,"workerNodeId":"worker"},
        "projects":[{"id":"a","sourceRoot":root.join("a"),"sourceMode":"commit"},{"id":"b","sourceRoot":root.join("b"),"sourceMode":"commit"}]})).unwrap()).unwrap()
}
fn server(controller: Arc<Controller>, listener: TcpListener) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept()=>{
                    let (stream,address)=accepted.unwrap();let c=controller.clone();
                    clients.spawn(async move {let _=c.accept(stream,address).await;});
                }
                _=clients.join_next(),if !clients.is_empty()=>{}
            }
        }
    })
}
struct Harness {
    worker: Arc<Worker>,
    a: Arc<Controller>,
    b: Arc<Controller>,
    worker_task: JoinHandle<winbridge_worker::Result<()>>,
    servers: Vec<JoinHandle<()>>,
    addresses: [SocketAddr; 2],
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.worker.stop();
        let deadline = std::time::Instant::now() + Duration::from_secs(6);
        while !self.worker.is_idle() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.worker_task.abort();
        for server in &self.servers {
            server.abort();
        }
        if !std::thread::panicking() {
            assert!(
                self.worker.is_idle(),
                "Owned fixture processes did not drain"
            );
        }
    }
}
impl Harness {
    fn new(root: &Path) -> Self {
        for id in ["a", "b"] {
            std::fs::create_dir(root.join(id)).unwrap();
        }
        let listeners = [
            listen("127.0.0.1:0".parse().unwrap()),
            listen("127.0.0.1:0".parse().unwrap()),
        ];
        let addresses = [
            listeners[0].local_addr().unwrap(),
            listeners[1].local_addr().unwrap(),
        ];
        let instance = format!("two-{}", uuid::Uuid::new_v4());
        let a = controller(root, "controller-a", addresses[0], &instance);
        let b = controller(root, "controller-b", addresses[1], &instance);
        let projects=["a","b"].map(|id|serde_json::json!({"id":id,"workspaceRoot":root.join(id),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}));
        let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"worker",
            "stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":"a","controllers":[
                {"id":"a","url":format!("ws://{}",addresses[0])},{"id":"b","url":format!("ws://{}",addresses[1])}]}},
            "workScheduling":{"maxConcurrentWork":2},"projects":projects})).unwrap();
        let worker = Worker::open(config).unwrap();
        let [la, lb] = listeners;
        let servers = vec![server(a.clone(), la), server(b.clone(), lb)];
        let worker_task = tokio::spawn(winbridge_worker::serve(worker.clone()));
        Self {
            worker,
            a,
            b,
            worker_task,
            servers,
            addresses,
        }
    }
    async fn connected(&self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !(self.a.status().unwrap().connected && self.b.status().unwrap().connected) {
            assert!(
                Instant::now() < deadline,
                "Both real controllers did not connect"
            );
            if self.worker_task.is_finished() {
                panic!("Worker stopped before connections")
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
async fn info(c: &Controller) -> WorkerInfo {
    let LocalResponse::Info { info } = c.call(LocalRequest::Info).await else {
        panic!("Network Info")
    };
    *info
}
async fn projection(c: &Controller) -> ControllerReadinessProjection {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let info = info(c).await;
        let catalog: winbridge_protocol::ContractCatalog =
            serde_json::from_value(info.contract_catalog.unwrap()).unwrap();
        if let Some(value) = catalog.controller_projection {
            let p: ControllerReadinessProjection = serde_json::from_value(value).unwrap();
            assert_eq!(p.controller_generation, c.generation);
            assert_eq!(Some(p.session_id.clone()), c.status().unwrap().session_id);
            return p;
        }
        assert!(
            Instant::now() < deadline,
            "Own controller projection missing"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn submission(project: &str, key: &str, long: bool) -> TaskSubmission {
    let script = if long {
        "printf '%s\\n' $$ >> starts.log; printf 'Привет ✓\\n'; exec sleep 60"
    } else {
        "printf '%s\\n' $$ >> starts.log; printf 'Привет ✓\\n'"
    };
    TaskSubmission {
        protocol_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        project_id: project.into(),
        idempotency_key: key.into(),
        timeout_ms: 60000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: "/bin/sh".into(),
            arguments: vec!["-c".into(), script.into()],
            working_directory: ".".into(),
        },
    }
}
fn envelope(context: &RequestContext, request: LocalRequest) -> FencedRequest {
    FencedRequest {
        request_contract_version: 2,
        context: context.clone(),
        request,
    }
}
async fn submit(c: &Controller, context: &RequestContext, request: TaskSubmission) -> TaskRecord {
    let r = c
        .call_fenced(envelope(context, LocalRequest::Submit { request }))
        .await;
    assert_eq!(r.outcome, FencedReplyOutcome::Responded, "{r:?}");
    let LocalResponse::Task { task } = r.response else {
        panic!("Network task acceptance")
    };
    *task
}
async fn task(c: &Controller, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } = c.call(LocalRequest::GetTask { task_id: id.into() }).await
    else {
        panic!("Task observation")
    };
    *task
}
async fn running(c: &Controller, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let t = task(c, id).await;
        if t.process_id.is_some() {
            assert_eq!(t.status, TaskStatus::Running);
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_real_websockets_share_queue_survive_one_disconnect_and_reconnect_without_replay() {
    let root = tempfile::tempdir().unwrap();
    let mut h = Harness::new(root.path());
    h.connected().await;
    let ia = info(&h.a).await;
    let ib = info(&h.b).await;
    assert_eq!(
        ia.controller_connection.as_ref().unwrap().url,
        format!("ws://{}", h.addresses[0])
    );
    assert_eq!(
        ib.controller_connection.as_ref().unwrap().url,
        format!("ws://{}", h.addresses[1])
    );
    assert_eq!(
        ia.controller_connection.as_ref().unwrap().session_id,
        h.a.status().unwrap().session_id
    );
    assert_eq!(
        ib.controller_connection.as_ref().unwrap().session_id,
        h.b.status().unwrap().session_id
    );
    let context = ia.fenced_requests.unwrap().context;
    assert_eq!(ib.fenced_requests.unwrap().context, context);
    let query = winbridge_protocol::ControllerConnectionsRequest {
        context: context.clone(),
    };
    let all = h.a.controller_connections(query.clone()).await.unwrap();
    assert_eq!(all.primary.as_deref(), Some("a"));
    assert_eq!(all.connections.len(), 2);
    assert!(all.connections.iter().all(|l| matches!(
        l.connection.state,
        winbridge_protocol::ConnectionState::Connected
    )));
    assert!(
        all.rpc.in_flight >= 1,
        "The observing WebSocket handler owns a slot too"
    );
    assert_eq!(
        all.connections
            .iter()
            .find(|l| l.id == "b")
            .unwrap()
            .connection
            .session_id,
        h.b.status().unwrap().session_id
    );
    projection(&h.a).await;
    projection(&h.b).await;
    let request = submission("a", "same-key", true);
    let (one, two) = tokio::join!(
        submit(&h.a, &context, request.clone()),
        submit(&h.b, &context, request.clone())
    );
    assert_eq!(one.task_id, two.task_id);
    running(&h.b, &one.task_id).await;
    let queued = submit(&h.b, &context, submission("a", "next", false)).await;
    assert_eq!(task(&h.b, &queued.task_id).await.status, TaskStatus::Queued);
    h.servers[0].abort();
    let _ = (&mut h.servers[0]).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let all = h.b.controller_connections(query.clone()).await.unwrap();
        let a = all.connections.iter().find(|l| l.id == "a").unwrap();
        assert!(matches!(
            all.connections
                .iter()
                .find(|l| l.id == "b")
                .unwrap()
                .connection
                .state,
            winbridge_protocol::ConnectionState::Connected
        ));
        if !matches!(
            a.connection.state,
            winbridge_protocol::ConnectionState::Connected
        ) && a.last_error.is_some()
        {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(task(&h.b, &one.task_id).await.status, TaskStatus::Running);
    let cancel =
        h.b.call_fenced(envelope(
            &context,
            LocalRequest::Cancel {
                task_id: one.task_id.clone(),
            },
        ))
        .await;
    assert_eq!(cancel.outcome, FencedReplyOutcome::Responded);
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let t = task(&h.b, &queued.task_id).await;
        if t.status.is_terminal() {
            assert_eq!(t.status, TaskStatus::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut config = h.a.config.clone();
    let old_generation = h.a.generation.clone();
    // New private state simulates a restarted Debian without adopting old sessions.
    config.state_directory = root.path().join("controller-a-restarted");
    h.a = Controller::new(config).unwrap();
    h.servers[0] = server(h.a.clone(), listen(h.addresses[0]));
    h.connected().await;
    assert_ne!(h.a.generation, old_generation);
    let fresh = info(&h.a).await;
    assert_eq!(
        fresh.controller_connection.unwrap().controller_generation,
        Some(h.a.generation.clone())
    );
    projection(&h.a).await;
    projection(&h.b).await;
    let all = h.b.controller_connections(query.clone()).await.unwrap();
    let a = all.connections.iter().find(|l| l.id == "a").unwrap();
    assert!(a.attempts.parse::<u64>().unwrap() >= 2);
    assert!(a.last_error.is_some());
    assert_eq!(
        a.connection.controller_generation.as_ref(),
        Some(&h.a.generation)
    );
    let address = winbridge_transport::endpoint(
        &h.worker.config.state_directory,
        &h.worker.config.instance_id,
    )
    .unwrap();
    let local = winbridge_transport::controller_connections(&address, &query)
        .await
        .unwrap();
    assert_eq!(local.worker_generation, all.worker_generation);
    assert_eq!(local.primary, all.primary);
    let duplicate = submit(&h.a, &context, request).await;
    assert_eq!(duplicate.task_id, one.task_id);
    assert_eq!(duplicate.status, TaskStatus::Cancelled);
    assert_eq!(
        std::fs::read_to_string(root.path().join("a/starts.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let LocalResponse::Output { chunks } =
        h.b.call(LocalRequest::ReadOutput {
            task_id: queued.task_id.clone(),
            after_sequence: 0,
        })
        .await
    else {
        panic!("Output")
    };
    assert!(
        String::from_utf8(chunks.into_iter().flat_map(|c| c.bytes).collect())
            .unwrap()
            .contains("Привет")
    );
    let mut foreign = context.clone();
    foreign.journal_incarnation = "foreign".into();
    let rejected =
        h.b.call_fenced(envelope(
            &foreign,
            LocalRequest::Submit {
                request: submission("b", "stale", false),
            },
        ))
        .await;
    // Existing controller preflight conservatively wraps local errors as
    // Unconfirmed; assert its explicit no-transmission evidence and side effect.
    assert_eq!(rejected.outcome, FencedReplyOutcome::Unconfirmed);
    assert!(matches!(rejected.response, LocalResponse::Error { message }
        if message.contains("Caller-pinned") && message.contains("not transmitted")));
    assert!(!root.path().join("b/starts.log").exists());
    h.worker.stop();
    (&mut h.worker_task).await.unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.a.status().unwrap().connected || h.b.status().unwrap().connected {
        assert!(
            Instant::now() < deadline,
            "Connection survived worker shutdown"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(Arc::strong_count(&h.worker), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn both_controllers_run_independent_projects_in_one_worker() {
    let root = tempfile::tempdir().unwrap();
    let h = Harness::new(root.path());
    h.connected().await;
    let context = info(&h.a).await.fenced_requests.unwrap().context;
    let a = submit(&h.a, &context, submission("a", "job-a", true)).await;
    let b = submit(&h.b, &context, submission("b", "job-b", true)).await;
    running(&h.a, &a.task_id).await;
    running(&h.b, &b.task_id).await;
    let one = task(&h.a, &a.task_id).await;
    let two = task(&h.b, &b.task_id).await;
    assert_eq!(
        (one.status, two.status),
        (TaskStatus::Running, TaskStatus::Running)
    );
    assert_ne!(one.process_id, two.process_id);
}
