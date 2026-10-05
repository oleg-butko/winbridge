use std::{
    fs,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::{validate_relative, WorkerConfig};
use winbridge_protocol::{
    CommandSource, LocalRequest, LocalResponse, TaskRecord, TaskStatus, TaskSubmission,
};
use winbridge_worker::{store::Store, Worker};

fn config(root: &Path, limit: u64) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"test-instance","nodeId":"test-node",
        "stateDirectory":root.join("state"), "maxOutputBytesPerTask":limit,
        "worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,
            "recipes":[{"id":"success","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":["success"],"workingDirectory":".","timeoutMs":5000}]}]
    })).unwrap()
}
fn submission(key: &str, mode: &str, timeout: u64) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: format!("request-{key}"),
        project_id: "fixture".into(),
        idempotency_key: key.into(),
        timeout_ms: timeout,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
fn task(response: LocalResponse) -> TaskRecord {
    if let LocalResponse::Task { task } = response {
        *task
    } else {
        panic!("expected task, got {response:?}")
    }
}
fn submit(worker: &Worker, request: TaskSubmission) -> TaskRecord {
    task(worker.handle(LocalRequest::Submit { request }))
}
fn get(worker: &Worker, id: &str) -> TaskRecord {
    task(worker.handle(LocalRequest::GetTask { task_id: id.into() }))
}
fn wait(worker: &Arc<Worker>, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let current = get(worker, id);
        if current.status.is_terminal() {
            while !worker.is_idle() {
                thread::sleep(Duration::from_millis(5));
            }
            return current;
        }
        assert!(Instant::now() < deadline, "task timed out: {current:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_execution_both_streams_duplicate_payload_and_persisted_result() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), 16384);
    let worker = Worker::open(cfg.clone()).unwrap();
    let first = submit(&worker, submission("one", "success", 5000));
    let mut duplicate = submission("one", "success", 5000);
    duplicate.request_id = "reconnected-request".into();
    assert_eq!(first.task_id, submit(&worker, duplicate).task_id);
    assert!(matches!(
        worker.handle(LocalRequest::Submit {
            request: submission("one", "failure", 5000)
        }),
        LocalResponse::Error { .. }
    ));
    let completed = wait(&worker, &first.task_id);
    assert_eq!(completed.status, TaskStatus::Succeeded);
    assert_eq!(completed.exit_code, Some(0));
    let LocalResponse::Output { chunks } = worker.handle(LocalRequest::ReadOutput {
        task_id: first.task_id.clone(),
        after_sequence: 0,
    }) else {
        panic!()
    };
    assert!(chunks
        .iter()
        .any(|c| String::from_utf8_lossy(&c.bytes).contains("stdout: success")));
    assert!(chunks
        .iter()
        .any(|c| String::from_utf8_lossy(&c.bytes).contains("stderr: diagnostic")));
    assert_eq!(
        fs::read_to_string(root.path().join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(
        Worker::open(cfg.clone()).is_err(),
        "a second worker cannot own the same journal"
    );
    drop(worker);
    let reopened = Worker::open(cfg).unwrap();
    assert_eq!(get(&reopened, &first.task_id).status, TaskStatus::Succeeded);
    assert_eq!(
        submit(&reopened, submission("one", "success", 5000)).task_id,
        first.task_id
    );
}

#[test]
fn failure_timeout_tree_cancel_queued_cancel_and_recipe_share_service() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path(), 16384)).unwrap();
    let failed = submit(&worker, submission("failure", "failure", 5000));
    assert_eq!(wait(&worker, &failed.task_id).exit_code, Some(7));
    let timeout = submit(&worker, submission("timeout", "tree", 300));
    let result = wait(&worker, &timeout.task_id);
    assert_eq!(result.status, TaskStatus::Failed);
    assert!(result.reason.unwrap().contains("timeout"));
    let long = submit(&worker, submission("cancel-tree", "tree", 5000));
    worker.schedule().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while get(&worker, &long.task_id).output_bytes < 60 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let queued = submit(&worker, submission("queued", "success", 5000));
    worker.schedule().unwrap();
    assert_eq!(get(&worker, &queued.task_id).status, TaskStatus::Queued);
    assert_eq!(
        task(worker.handle(LocalRequest::Cancel {
            task_id: queued.task_id
        }))
        .status,
        TaskStatus::Cancelled
    );
    let cancellation = task(worker.handle(LocalRequest::Cancel {
        task_id: long.task_id.clone(),
    }));
    assert!(cancellation.cancel_requested || cancellation.status == TaskStatus::Cancelled);
    assert_eq!(wait(&worker, &long.task_id).status, TaskStatus::Cancelled);
    let mut recipe = submission("recipe", "success", 5000);
    recipe.command = CommandSource::Recipe {
        recipe_id: "success".into(),
        arguments: vec![],
    };
    let recipe = submit(&worker, recipe);
    assert_eq!(wait(&worker, &recipe.task_id).status, TaskStatus::Succeeded);
}

#[test]
fn flood_does_not_block_without_ui_and_quota_marks_loss() {
    let root = TempDir::new().unwrap();
    let worker = Worker::open(config(root.path(), 4096)).unwrap();
    let flood = submit(&worker, submission("flood", "flood", 5000));
    let result = wait(&worker, &flood.task_id);
    assert_eq!(result.status, TaskStatus::Succeeded);
    assert_eq!(result.output_bytes, 4096);
    assert!(result.output_truncated);
}

#[test]
fn launch_intent_recovery_is_indeterminate_and_queued_survives() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), 16384);
    let request = submission("recovery", "success", 5000);
    let command = cfg.resolve(&request).unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let mut accepted = store.accept(request.clone(), command).unwrap();
    accepted.status = TaskStatus::Running;
    Store::event(&mut accepted, "Launch intent");
    store.save(&accepted).unwrap();
    let queued = store
        .accept(
            submission("queued", "success", 5000),
            cfg.resolve(&submission("queued", "success", 5000)).unwrap(),
        )
        .unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    store.recover().unwrap();
    assert_eq!(
        store.get(&accepted.task_id).unwrap().status,
        TaskStatus::Indeterminate
    );
    assert_eq!(
        store.get(&queued.task_id).unwrap().status,
        TaskStatus::Queued
    );
    assert_eq!(
        store.existing(&request).unwrap().unwrap().task_id,
        accepted.task_id
    );
    assert!(!root.path().join("runs.log").exists());
}

#[test]
fn path_and_config_policy_rejects_traversal_unknown_fields_and_invalid_permissions() {
    for path in [
        "../outside",
        "C:\\outside",
        r"\\host\share",
        "NUL.txt",
        "COM¹",
        "folder.",
        "a//b",
        "x:stream",
        "\\?\\C:\\x",
    ] {
        assert!(validate_relative(path).is_err(), "{path}");
    }
    assert!(validate_relative("src/nested file").is_ok());
    let root = TempDir::new().unwrap();
    let mut cfg = config(root.path(), 16384);
    cfg.schema_version = 2;
    assert!(cfg.validate().unwrap_err().contains("schemaVersion"));
    cfg.schema_version = 1;
    cfg.projects[0].allow_direct_commands = false;
    assert!(cfg.resolve(&submission("denied", "success", 1000)).is_err());
    let mut value = serde_json::json!({"schemaVersion":1});
    value["typo"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<WorkerConfig>(value).is_err());
    #[cfg(unix)]
    {
        let outside = TempDir::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert!(winbridge_core::config::scoped_directory(root.path(), "escape").is_err());
    }
}

#[tokio::test]
async fn framing_rejects_oversize_before_allocating_payload() {
    use tokio::io::AsyncWriteExt;
    let (mut sender, mut reader) = tokio::io::duplex(32);
    sender
        .write_u32_le(winbridge_transport::MAX_FRAME_BYTES as u32 + 1)
        .await
        .unwrap();
    assert!(winbridge_transport::read_frame(&mut reader).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn ipc_startup_preserves_an_existing_regular_file() {
    let root = TempDir::new().unwrap();
    let cfg = config(root.path(), 16384);
    let worker = Worker::open(cfg.clone()).unwrap();
    let address = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    fs::write(&address, "manually maintained data").unwrap();
    assert!(winbridge_worker::serve(worker)
        .await
        .unwrap_err()
        .contains("refusing to remove"));
    assert_eq!(
        fs::read_to_string(address).unwrap(),
        "manually maintained data"
    );
}
