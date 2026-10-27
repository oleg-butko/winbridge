//! Linux-only model integration: two session origins -> one real Worker/Store.
//! No TCP, production config changes or Windows feature claims.
#![cfg(unix)]
use std::{
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use winbridge_core::{
    config::WorkerConfig,
    controller_sessions::{Peer, RpcOrigin, Sessions, Ticket, Topology},
};
use winbridge_protocol::*;
use winbridge_worker::Worker;

struct Owner {
    worker: Arc<Worker>,
    ids: Vec<String>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        for id in &self.ids {
            self.worker.handle(LocalRequest::Cancel {
                task_id: id.clone(),
            });
        }
        self.worker.stop();
        let deadline = Instant::now() + Duration::from_secs(6);
        while !self.worker.is_idle() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(self.worker.is_idle(), "Test process owner did not drain");
    }
}
struct Client {
    worker: Arc<Worker>,
    sessions: Arc<Mutex<Sessions>>,
    ticket: Ticket,
    context: RequestContext,
    next: AtomicUsize,
}
impl Client {
    fn call(&self, request: LocalRequest) -> (LocalResponse, RpcOrigin) {
        let rpc = format!("rpc-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let permit = self
            .sessions
            .lock()
            .unwrap()
            .admit(&self.ticket, rpc)
            .unwrap();
        let origin = permit.origin().clone();
        // Each simulated client retains its ORIGINAL worker context. Reconnect
        // never silently replaces the context/idempotency key of an old request.
        let response = self.worker.handle_fenced(FencedRequest {
            request_contract_version: 2,
            context: self.context.clone(),
            request,
        });
        assert!(self.sessions.lock().unwrap().complete(&permit));
        assert_eq!(
            response.outcome,
            FencedReplyOutcome::Responded,
            "{response:?}"
        );
        assert_eq!(response.context.as_ref(), Some(&self.context));
        (response.response, origin)
    }
    fn task(&self, id: &str) -> TaskRecord {
        let (LocalResponse::Task { task }, _) =
            self.call(LocalRequest::GetTask { task_id: id.into() })
        else {
            panic!("Task observation")
        };
        *task
    }
    fn submit(&self, request: TaskSubmission) -> (TaskRecord, RpcOrigin) {
        let (LocalResponse::Task { task }, origin) = self.call(LocalRequest::Submit { request })
        else {
            panic!("Task acceptance")
        };
        (*task, origin)
    }
}
fn setup(root: &std::path::Path) -> (Owner, Client, Client) {
    let mut projects = Vec::new();
    for id in ["a", "b"] {
        let path = root.join(id);
        fs::create_dir(&path).unwrap();
        projects.push(serde_json::json!({"id":id,"workspaceRoot":path,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}));
    }
    let config: WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("multi-model-{}",uuid::Uuid::new_v4()),
        "nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "workScheduling":{"maxConcurrentWork":2},"projects":projects})).unwrap();
    let worker = Worker::open(config).unwrap();
    let topology: Topology=serde_json::from_value(serde_json::json!({"schemaVersion":1,"primary":"debian-203","controllers":[
        {"id":"debian-203","url":"ws://192.168.44.203:1143"},{"id":"debian-204","url":"ws://192.168.44.204:1143"}]})).unwrap();
    let sessions = Arc::new(Mutex::new(
        Sessions::new(&topology, worker.generation.clone(), 4, 2, 10000).unwrap(),
    ));
    let clients = ["debian-203", "debian-204"].map(|id| {
        let ticket = sessions.lock().unwrap().begin(id).unwrap();
        sessions
            .lock()
            .unwrap()
            .connected(
                &ticket,
                Peer {
                    node: id.into(),
                    generation: "controller-generation".into(),
                    session: format!("session-{id}"),
                },
            )
            .unwrap();
        Client {
            worker: worker.clone(),
            sessions: sessions.clone(),
            ticket,
            context: worker.request_context().unwrap(),
            next: AtomicUsize::new(0),
        }
    });
    let [a, b] = clients;
    (
        Owner {
            worker,
            ids: Vec::new(),
        },
        a,
        b,
    )
}
fn request(project: &str, key: &str, mode: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: project.into(),
        idempotency_key: key.into(),
        timeout_ms: 15000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
fn until(worker: &Arc<Worker>, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !ready() {
        assert!(Instant::now() < deadline, "Model integration deadline");
        worker.schedule().unwrap();
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn two_origins_share_idempotency_queue_and_disconnect_does_not_cancel_accepted_work() {
    let temp = tempfile::tempdir().unwrap();
    let (mut owner, a, b) = setup(temp.path());
    let barrier = Barrier::new(2);
    let (first, duplicate) = thread::scope(|scope| {
        let a_thread = scope.spawn(|| {
            barrier.wait();
            a.submit(request("a", "same-key", "long"))
        });
        let b_thread = scope.spawn(|| {
            barrier.wait();
            let mut r = request("a", "same-key", "long");
            r.request_id = "other-correlation".into();
            b.submit(r)
        });
        (a_thread.join().unwrap(), b_thread.join().unwrap())
    });
    owner.ids.push(first.0.task_id.clone());
    assert_eq!(first.0.task_id, duplicate.0.task_id);
    assert_ne!(first.1.controller, duplicate.1.controller);
    until(&owner.worker, || {
        a.task(&first.0.task_id).process_id.is_some()
    });
    let second = b.submit(request("a", "next-key", "success")).0;
    owner.ids.push(second.task_id.clone());
    for _ in 0..5 {
        owner.worker.schedule().unwrap();
    }
    assert_eq!(b.task(&second.task_id).status, TaskStatus::Queued);
    a.sessions
        .lock()
        .unwrap()
        .disconnected(&a.ticket, "debian-203 lost".into());
    assert_eq!(b.task(&first.0.task_id).status, TaskStatus::Running);
    assert!(a
        .sessions
        .lock()
        .unwrap()
        .admit(&a.ticket, "stale".into())
        .is_err());
    // Trusted MVP intentionally shares task control; this is not per-agent ACL.
    b.call(LocalRequest::Cancel {
        task_id: first.0.task_id.clone(),
    });
    until(&owner.worker, || {
        b.task(&second.task_id).status.is_terminal()
    });
    assert_eq!(b.task(&first.0.task_id).status, TaskStatus::Cancelled);
    assert_eq!(b.task(&second.task_id).status, TaskStatus::Succeeded);
    let runs = fs::read_to_string(temp.path().join("a/runs.log")).unwrap();
    assert_eq!(
        runs.lines().count(),
        2,
        "Duplicate must never spawn another process"
    );
    assert_eq!(runs.matches(" long").count(), 1);
    assert_eq!(a.sessions.lock().unwrap().in_flight(), 0);
}

#[test]
fn different_projects_can_run_at_once_under_the_same_worker_capacity() {
    let temp = tempfile::tempdir().unwrap();
    let (mut owner, a, b) = setup(temp.path());
    let first = a.submit(request("a", "first", "long")).0;
    owner.ids.push(first.task_id.clone());
    let second = b.submit(request("b", "second", "long")).0;
    owner.ids.push(second.task_id.clone());
    until(&owner.worker, || {
        a.task(&first.task_id).process_id.is_some() && b.task(&second.task_id).process_id.is_some()
    });
    let one = a.task(&first.task_id);
    let two = b.task(&second.task_id);
    assert_eq!(
        (one.status, two.status),
        (TaskStatus::Running, TaskStatus::Running)
    );
    assert_ne!(one.process_id, two.process_id);
    assert_eq!(
        a.sessions.lock().unwrap().in_flight(),
        0,
        "RPC slots do not represent running jobs"
    );
}
