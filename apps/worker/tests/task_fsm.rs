use rusqlite::Connection;
use std::{
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::{
    config::WorkerConfig,
    task::{TaskEffect, TaskEvent},
};
use winbridge_protocol::{
    CommandSource, LocalRequest, LocalResponse, TaskRecord, TaskStatus, TaskSubmission,
};
use winbridge_worker::{store::Store, Worker};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"fsm-tests","nodeId":"fsm-worker",
        "stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap()
}
fn submission(mode: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: "first-correlation".into(),
        project_id: "fixture".into(),
        idempotency_key: "fsm-task".into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
fn task(response: LocalResponse) -> TaskRecord {
    match response {
        LocalResponse::Task { task } => *task,
        other => panic!("Expected task: {other:?}"),
    }
}
fn wait(worker: &Arc<Worker>, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let record = task(worker.handle(LocalRequest::GetTask { task_id: id.into() }));
        if record.status.is_terminal() && worker.is_idle() {
            return record;
        }
        assert!(Instant::now() < deadline, "Task deadline: {record:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn failed_launch_commit_does_not_dispatch_and_a_later_schedule_executes_once() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path())).unwrap();
    let accepted = task(worker.handle(LocalRequest::Submit {
        request: submission("success"),
    }));
    let db = Connection::open(root.path().join("state/journal.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_launch BEFORE UPDATE OF record ON tasks
        WHEN json_extract(NEW.record,'$.status')='running' AND json_extract(NEW.record,'$.processId') IS NULL
        BEGIN SELECT RAISE(ABORT,'injected launch commit failure'); END;").unwrap();
    assert!(worker
        .schedule()
        .unwrap_err()
        .contains("injected launch commit failure"));
    assert!(worker.is_idle());
    assert!(!root.path().join("runs.log").exists());
    let unchanged = task(worker.handle(LocalRequest::GetTask {
        task_id: accepted.task_id.clone(),
    }));
    assert_eq!(
        serde_json::to_value(unchanged).unwrap(),
        serde_json::to_value(&accepted).unwrap()
    );
    db.execute_batch("DROP TRIGGER reject_launch;").unwrap();
    drop(db);
    worker.schedule().unwrap();
    let finished = wait(&worker, &accepted.task_id);
    assert_eq!(finished.status, TaskStatus::Succeeded);
    assert_eq!(finished.exit_code, Some(0));
    assert_eq!(
        std::fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn failed_pid_observation_is_indeterminate_and_never_replays_the_spawn() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path())).unwrap();
    let request = submission("long");
    let accepted = task(worker.handle(LocalRequest::Submit {
        request: request.clone(),
    }));
    let db = Connection::open(root.path().join("state/journal.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_pid BEFORE UPDATE OF record ON tasks
        WHEN json_extract(NEW.record,'$.processId') IS NOT NULL
        BEGIN SELECT RAISE(ABORT,'injected PID observation failure'); END;",
    )
    .unwrap();
    worker.schedule().unwrap();
    let unknown = wait(&worker, &accepted.task_id);
    assert_eq!(unknown.status, TaskStatus::Indeterminate);
    assert!(unknown.process_id.is_none());
    assert!(unknown
        .reason
        .as_ref()
        .unwrap()
        .contains("injected PID observation failure"));
    let runs_before = std::fs::read(root.path().join("runs.log")).ok();
    worker.schedule().unwrap();
    let mut retry = request;
    retry.request_id = "reconnect-correlation".into();
    let replay = task(worker.handle(LocalRequest::Submit { request: retry }));
    assert_eq!(replay.task_id, unknown.task_id);
    assert_eq!(replay.status, TaskStatus::Indeterminate);
    assert_eq!(replay.events.len(), unknown.events.len());
    assert!(worker.is_idle());
    assert_eq!(
        std::fs::read(root.path().join("runs.log")).ok(),
        runs_before
    );
}

#[test]
fn silently_ignored_launch_write_cannot_authorize_an_unjournaled_process() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path())).unwrap();
    let accepted = task(worker.handle(LocalRequest::Submit {
        request: submission("success"),
    }));
    let db = Connection::open(root.path().join("state/journal.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER ignore_launch BEFORE UPDATE OF record ON tasks
        BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();
    assert!(worker
        .schedule()
        .unwrap_err()
        .contains("Task transition was not stored"));
    assert!(worker.is_idle());
    assert!(!root.path().join("runs.log").exists());
    let unchanged = task(worker.handle(LocalRequest::GetTask {
        task_id: accepted.task_id.clone(),
    }));
    assert_eq!(
        serde_json::to_value(unchanged).unwrap(),
        serde_json::to_value(accepted).unwrap()
    );
}

#[test]
fn persisted_cancel_between_intent_and_pid_is_not_lost_and_duplicate_observation_is_noop() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let request = submission("long");
    let accepted = store
        .accept(
            request.clone(),
            winbridge_runner::resolve(&config(root.path()), &request).unwrap(),
            None,
        )
        .unwrap();
    let launch = store
        .transition(&accepted.task_id, TaskEvent::LaunchIntent)
        .unwrap();
    assert_eq!(launch.effect, TaskEffect::ExecuteAcceptedCommand);
    assert!(launch.record.process_id.is_none());
    let cancelled = store.cancel(&accepted.task_id).unwrap();
    assert_eq!(cancelled.status, TaskStatus::Running);
    assert!(cancelled.cancel_requested);
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let observed = store
        .transition(
            &accepted.task_id,
            TaskEvent::ProcessStarted { process_id: 4242 },
        )
        .unwrap();
    assert!(observed.record.cancel_requested);
    let duplicate = store
        .transition(
            &accepted.task_id,
            TaskEvent::ProcessStarted { process_id: 4242 },
        )
        .unwrap();
    assert_eq!(duplicate.effect, TaskEffect::None);
    assert_eq!(
        serde_json::to_value(&observed.record).unwrap(),
        serde_json::to_value(duplicate.record).unwrap()
    );
    // A confirmed exit may beat cancellation; this retains the existing execution loop semantics.
    let terminal = store
        .finish(
            &accepted.task_id,
            TaskStatus::Succeeded,
            Some(0),
            "Observed exit",
        )
        .unwrap();
    assert!(terminal.cancel_requested);
    assert!(store
        .transition(
            &accepted.task_id,
            TaskEvent::ProcessStarted { process_id: 9999 }
        )
        .is_err());
    assert!(store
        .transition(&accepted.task_id, TaskEvent::LaunchIntent)
        .is_err());
    assert!(store
        .finish(
            &accepted.task_id,
            TaskStatus::Running,
            None,
            "Invalid completion"
        )
        .is_err());
    assert_eq!(
        serde_json::to_value(store.get(&accepted.task_id).unwrap()).unwrap(),
        serde_json::to_value(terminal).unwrap()
    );
}
