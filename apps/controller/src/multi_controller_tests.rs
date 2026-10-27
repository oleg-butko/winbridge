//! Configurable TCP/WebSocket controller instances, one real Linux worker.
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
fn controller(
    root: &Path,
    name: &str,
    address: SocketAddr,
    instance: &str,
    projects: &[String],
) -> Arc<Controller> {
    // Equal node IDs are intentional: copied Debian configs must not alias
    // connection slots. Configured endpoint IDs/session generations separate them.
    let projects: Vec<_> = projects
        .iter()
        .map(|id| serde_json::json!({"id":id,"sourceRoot":root.join(id),"sourceMode":"commit"}))
        .collect();
    Controller::new(serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"debian",
        "stateDirectory":root.join(name),"controller":{"listenAddress":address,"workerNodeId":"worker"},
        "projects":projects})).unwrap()).unwrap()
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
    controllers: Vec<Arc<Controller>>,
    worker_task: JoinHandle<winbridge_worker::Result<()>>,
    servers: Vec<JoinHandle<()>>,
    addresses: Vec<SocketAddr>,
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
        Self::with_controllers(root, 2, 0)
    }
    fn with_controllers(root: &Path, count: usize, primary: usize) -> Self {
        assert!((1..=winbridge_protocol::MAX_CONTROLLERS).contains(&count));
        assert!(primary < count);
        let ids: Vec<_> = (0..count)
            .map(|i| char::from(b'a' + i as u8).to_string())
            .collect();
        for id in &ids {
            std::fs::create_dir(root.join(id)).unwrap();
        }
        let listeners: Vec<_> = (0..count)
            .map(|_| listen("127.0.0.1:0".parse().unwrap()))
            .collect();
        let addresses: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
        let instance = format!("multi-{}", uuid::Uuid::new_v4());
        let controllers: Vec<_> = ids
            .iter()
            .zip(&addresses)
            .map(|(id, address)| {
                controller(root, &format!("controller-{id}"), *address, &instance, &ids)
            })
            .collect();
        let projects:Vec<_> = ids.iter().map(|id|serde_json::json!({"id":id,"workspaceRoot":root.join(id),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]})).collect();
        let endpoints: Vec<_> = ids
            .iter()
            .zip(&addresses)
            .map(|(id, address)| serde_json::json!({"id":id,"url":format!("ws://{address}")}))
            .collect();
        let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"worker",
            "stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":ids[primary],"controllers":endpoints}},
            "workScheduling":{"maxConcurrentWork":count},"projects":projects})).unwrap();
        let worker = Worker::open(config).unwrap();
        let servers = controllers
            .iter()
            .cloned()
            .zip(listeners)
            .map(|(c, l)| server(c, l))
            .collect();
        let worker_task = tokio::spawn(winbridge_worker::serve(worker.clone()));
        Self {
            worker,
            controllers,
            worker_task,
            servers,
            addresses,
        }
    }
    async fn connected(&self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self
            .controllers
            .iter()
            .all(|c| c.status().unwrap().connected)
        {
            assert!(
                Instant::now() < deadline,
                "Not all configured controllers connected"
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
    let ia = info(&h.controllers[0]).await;
    let ib = info(&h.controllers[1]).await;
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
        h.controllers[0].status().unwrap().session_id
    );
    assert_eq!(
        ib.controller_connection.as_ref().unwrap().session_id,
        h.controllers[1].status().unwrap().session_id
    );
    let context = ia.fenced_requests.unwrap().context;
    assert_eq!(ib.fenced_requests.unwrap().context, context);
    let query = winbridge_protocol::ControllerConnectionsRequest {
        context: context.clone(),
    };
    let all = h.controllers[0]
        .controller_connections(query.clone())
        .await
        .unwrap();
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
        h.controllers[1].status().unwrap().session_id
    );
    projection(&h.controllers[0]).await;
    projection(&h.controllers[1]).await;
    let request = submission("a", "same-key", true);
    let retry = TaskSubmission {
        request_id: "other-correlation".into(),
        ..request.clone()
    };
    let (one, two) = tokio::join!(
        submit(&h.controllers[0], &context, request.clone()),
        submit(&h.controllers[1], &context, retry)
    );
    assert_eq!(one.task_id, two.task_id);
    let creator_query = winbridge_protocol::OperationAttributionRequest {
        context: context.clone(),
        query: winbridge_protocol::OperationAttributionQuery {
            kind: winbridge_protocol::HistoryKind::Task,
            operation_id: one.task_id.clone(),
        },
    };
    let initial = h.controllers[0]
        .operation_attribution(creator_query.clone())
        .await
        .unwrap()
        .creator
        .unwrap();
    assert!(
        matches!(&initial,winbridge_protocol::OperationCreator::Controller {controller_id,..} if controller_id=="a"||controller_id=="b")
    );
    assert_eq!(
        h.controllers[1]
            .operation_attribution(creator_query.clone())
            .await
            .unwrap()
            .creator,
        Some(initial.clone())
    );
    running(&h.controllers[1], &one.task_id).await;
    let queued = submit(&h.controllers[1], &context, submission("a", "next", false)).await;
    assert_eq!(
        task(&h.controllers[1], &queued.task_id).await.status,
        TaskStatus::Queued
    );
    h.servers[0].abort();
    let _ = (&mut h.servers[0]).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let all = h.controllers[1]
            .controller_connections(query.clone())
            .await
            .unwrap();
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
    assert_eq!(
        task(&h.controllers[1], &one.task_id).await.status,
        TaskStatus::Running
    );
    let cancel = h.controllers[1]
        .call_fenced(envelope(
            &context,
            LocalRequest::Cancel {
                task_id: one.task_id.clone(),
            },
        ))
        .await;
    assert_eq!(cancel.outcome, FencedReplyOutcome::Responded);
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let t = task(&h.controllers[1], &queued.task_id).await;
        if t.status.is_terminal() {
            assert_eq!(t.status, TaskStatus::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut config = h.controllers[0].config.clone();
    let old_generation = h.controllers[0].generation.clone();
    // New private state simulates a restarted Debian without adopting old sessions.
    config.state_directory = root.path().join("controller-a-restarted");
    h.controllers[0] = Controller::new(config).unwrap();
    h.servers[0] = server(h.controllers[0].clone(), listen(h.addresses[0]));
    h.connected().await;
    assert_ne!(h.controllers[0].generation, old_generation);
    let fresh = info(&h.controllers[0]).await;
    assert_eq!(
        fresh.controller_connection.unwrap().controller_generation,
        Some(h.controllers[0].generation.clone())
    );
    projection(&h.controllers[0]).await;
    projection(&h.controllers[1]).await;
    let all = h.controllers[1]
        .controller_connections(query.clone())
        .await
        .unwrap();
    let a = all.connections.iter().find(|l| l.id == "a").unwrap();
    assert!(a.attempts.parse::<u64>().unwrap() >= 2);
    assert!(a.last_error.is_some());
    assert_eq!(
        a.connection.controller_generation.as_ref(),
        Some(&h.controllers[0].generation)
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
    let duplicate = submit(&h.controllers[0], &context, request).await;
    assert_eq!(duplicate.task_id, one.task_id);
    assert_eq!(duplicate.status, TaskStatus::Cancelled);
    assert_eq!(
        h.controllers[0]
            .operation_attribution(creator_query)
            .await
            .unwrap()
            .creator,
        Some(initial)
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("a/starts.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let LocalResponse::Output { chunks } = h.controllers[1]
        .call(LocalRequest::ReadOutput {
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
    let rejected = h.controllers[1]
        .call_fenced(envelope(
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
    while h.controllers[0].status().unwrap().connected
        || h.controllers[1].status().unwrap().connected
    {
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
    let context = info(&h.controllers[0])
        .await
        .fenced_requests
        .unwrap()
        .context;
    let a = submit(&h.controllers[0], &context, submission("a", "job-a", true)).await;
    let b = submit(&h.controllers[1], &context, submission("b", "job-b", true)).await;
    running(&h.controllers[0], &a.task_id).await;
    running(&h.controllers[1], &b.task_id).await;
    let one = task(&h.controllers[0], &a.task_id).await;
    let two = task(&h.controllers[1], &b.task_id).await;
    assert_eq!(
        (one.status, two.status),
        (TaskStatus::Running, TaskStatus::Running)
    );
    assert_ne!(one.process_id, two.process_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn different_controller_cancel_keeps_creator_and_first_cancellation_actor() {
    use winbridge_protocol::*;
    let root = tempfile::tempdir().unwrap();
    let h = Harness::new(root.path());
    h.connected().await;
    let context = info(&h.controllers[0])
        .await
        .fenced_requests
        .unwrap()
        .context;
    let accepted = submit(
        &h.controllers[0],
        &context,
        submission("a", "cancel-origin", true),
    )
    .await;
    running(&h.controllers[1], &accepted.task_id).await;
    let query = OperationAttributionRequest {
        context: context.clone(),
        query: OperationAttributionQuery {
            kind: HistoryKind::Task,
            operation_id: accepted.task_id.clone(),
        },
    };
    assert!(h.controllers[0]
        .operation_cancellation(query.clone())
        .await
        .unwrap()
        .intent
        .is_none());
    let response = h.controllers[1]
        .call_fenced(envelope(
            &context,
            LocalRequest::Cancel {
                task_id: accepted.task_id.clone(),
            },
        ))
        .await;
    assert_eq!(response.outcome, FencedReplyOutcome::Responded);
    let first = h.controllers[0]
        .operation_cancellation(query.clone())
        .await
        .unwrap()
        .intent
        .unwrap();
    assert!(
        matches!(&first.actor,Some(OperationCreator::Controller {controller_id,..}) if controller_id=="b")
    );
    let duplicate = h.controllers[0]
        .call_fenced(envelope(
            &context,
            LocalRequest::Cancel {
                task_id: accepted.task_id,
            },
        ))
        .await;
    assert_eq!(duplicate.outcome, FencedReplyOutcome::Responded);
    let again = h.controllers[1]
        .operation_cancellation(query.clone())
        .await
        .unwrap()
        .intent
        .unwrap();
    assert_eq!(
        (again.actor, again.requested_at_ms),
        (first.actor, first.requested_at_ms)
    );
    assert!(
        matches!(h.controllers[1].operation_attribution(query).await.unwrap().creator,Some(OperationCreator::Controller {controller_id,..}) if controller_id=="a")
    );
}

async fn multiple_controllers_roundtrip(count: usize) {
    use winbridge_protocol::*;
    let root = tempfile::tempdir().unwrap();
    let primary = count - 1;
    let interrupted = count - 2;
    let mut h = Harness::with_controllers(root.path(), count, primary);
    h.connected().await;
    let context = info(&h.controllers[0])
        .await
        .fenced_requests
        .unwrap()
        .context;
    let query = ControllerConnectionsRequest {
        context: context.clone(),
    };
    let ids: Vec<_> = h
        .worker
        .config
        .worker
        .topology()
        .unwrap()
        .unwrap()
        .controllers
        .into_iter()
        .map(|c| c.id)
        .collect();
    let mut tasks = vec![];
    for (i, controller) in h.controllers.iter().enumerate() {
        let observed = info(controller).await;
        assert_eq!(observed.fenced_requests.unwrap().context, context);
        assert_eq!(
            observed.controller_connection.unwrap().url,
            format!("ws://{}", h.addresses[i])
        );
        projection(controller).await;
        let all = controller
            .controller_connections(query.clone())
            .await
            .unwrap();
        assert_eq!(all.primary.as_deref(), Some(ids[primary].as_str()));
        assert_eq!(all.connections.len(), count);
        assert!(all
            .connections
            .iter()
            .all(|l| matches!(l.connection.state, ConnectionState::Connected)));
        assert_eq!(all.rpc.limit, NETWORK_RPC_LIMIT);
        assert!(all
            .connections
            .iter()
            .all(|l| l.rpc.limit == CONTROLLER_RPC_LIMIT));
        tasks.push(
            submit(
                controller,
                &context,
                submission(&ids[i], &format!("job-{i}"), true),
            )
            .await,
        );
    }
    for (i, controller) in h.controllers.iter().enumerate() {
        running(controller, &tasks[i].task_id).await;
        // Every peer retries the first peer's original logical request; one process/creator survives.
        let duplicate = submit(controller, &context, tasks[0].submission.clone()).await;
        assert_eq!(duplicate.task_id, tasks[0].task_id);
    }
    assert_eq!(
        tasks
            .iter()
            .map(|t| t.task_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        count
    );
    let queued = submit(
        &h.controllers[primary],
        &context,
        submission(&ids[0], "same-project-next", false),
    )
    .await;
    assert_eq!(
        task(&h.controllers[primary], &queued.task_id).await.status,
        TaskStatus::Queued
    );
    let origin_query = |index: usize| OperationAttributionRequest {
        context: context.clone(),
        query: OperationAttributionQuery {
            kind: HistoryKind::Task,
            operation_id: tasks[index].task_id.clone(),
        },
    };
    let historical_creator = h.controllers[primary]
        .operation_attribution(origin_query(interrupted))
        .await
        .unwrap()
        .creator
        .unwrap();
    assert!(
        matches!(&historical_creator,OperationCreator::Controller {controller_id,..} if controller_id==&ids[interrupted])
    );
    h.servers[interrupted].abort();
    let _ = (&mut h.servers[interrupted]).await;
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let all = h.controllers[primary]
            .controller_connections(query.clone())
            .await
            .unwrap();
        let lost = all
            .connections
            .iter()
            .find(|l| l.id == ids[interrupted])
            .unwrap();
        assert!(all
            .connections
            .iter()
            .filter(|l| l.id != ids[interrupted])
            .all(|l| matches!(l.connection.state, ConnectionState::Connected)));
        if !matches!(lost.connection.state, ConnectionState::Connected) && lost.last_error.is_some()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Disconnected peer remained fresh"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for (i, controller) in h
        .controllers
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != interrupted)
    {
        assert_eq!(
            task(controller, &tasks[interrupted].task_id).await.status,
            TaskStatus::Running
        );
        let source = controller
            .operation_attribution(origin_query(i))
            .await
            .unwrap()
            .creator
            .unwrap();
        assert!(
            matches!(source,OperationCreator::Controller {controller_id,..} if controller_id==ids[i])
        );
    }
    let mut config = h.controllers[interrupted].config.clone();
    let old_generation = h.controllers[interrupted].generation.clone();
    config.state_directory = root.path().join(format!("restarted-{interrupted}"));
    h.controllers[interrupted] = Controller::new(config).unwrap();
    h.servers[interrupted] = server(
        h.controllers[interrupted].clone(),
        listen(h.addresses[interrupted]),
    );
    h.connected().await;
    assert_ne!(h.controllers[interrupted].generation, old_generation);
    for controller in &h.controllers {
        projection(controller).await;
        assert_eq!(
            controller
                .operation_attribution(origin_query(interrupted))
                .await
                .unwrap()
                .creator,
            Some(historical_creator.clone())
        );
    }
    for (i, accepted) in tasks.iter().enumerate() {
        let actor = (i + 1) % count;
        let response = h.controllers[actor]
            .call_fenced(envelope(
                &context,
                LocalRequest::Cancel {
                    task_id: accepted.task_id.clone(),
                },
            ))
            .await;
        assert_eq!(response.outcome, FencedReplyOutcome::Responded);
        let intent = h.controllers[primary]
            .operation_cancellation(origin_query(i))
            .await
            .unwrap()
            .intent
            .unwrap();
        assert!(
            matches!(intent.actor,Some(OperationCreator::Controller {controller_id,..}) if controller_id==ids[actor])
        );
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let queued = task(&h.controllers[primary], &queued.task_id).await;
        if queued.status.is_terminal() {
            assert_eq!(queued.status, TaskStatus::Succeeded);
            let mut all_terminal = true;
            for accepted in &tasks {
                all_terminal &= task(&h.controllers[primary], &accepted.task_id)
                    .await
                    .status
                    .is_terminal();
            }
            if all_terminal {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Shared project queue did not advance after cancellation"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(
            task(&h.controllers[primary], &tasks[i].task_id)
                .await
                .status,
            TaskStatus::Cancelled
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(id).join("starts.log"))
                .unwrap()
                .lines()
                .count(),
            if i == 0 { 2 } else { 1 }
        );
    }
    h.worker.stop();
    (&mut h.worker_task).await.unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while h.controllers.iter().any(|c| c.status().unwrap().connected) {
        assert!(
            Instant::now() < deadline,
            "A controller survived worker shutdown"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(Arc::strong_count(&h.worker), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_and_four_controllers_share_one_runtime_with_non_first_primary() {
    tokio::time::timeout(Duration::from_secs(70), async {
        for count in [3, 4] {
            multiple_controllers_roundtrip(count).await;
        }
    })
    .await
    .expect("Three/four-controller runtime deadline");
}
