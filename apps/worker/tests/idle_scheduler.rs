//! Large retained command history must not become the scheduler's idle workload.
use rusqlite::{params, Connection};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker};

fn config(root: &Path) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":format!("idle-{}",uuid::Uuid::new_v4()),"nodeId":"idle-test",
        "stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap()
}

#[test]
fn version_and_artifact_queries_need_no_config_and_keep_json_clean() {
    let root = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_winbridge-worker");
    let version = std::process::Command::new(exe)
        .arg("--version")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8(version.stdout)
        .unwrap()
        .starts_with(&format!("WinBridge worker {} ", env!("CARGO_PKG_VERSION"))));
    let artifact = std::process::Command::new(exe)
        .arg("--artifact-info")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(artifact.status.success());
    assert!(artifact.stderr.is_empty());
    let identity: serde_json::Value = serde_json::from_slice(&artifact.stdout).unwrap();
    assert_eq!(identity["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn seed(config: &WorkerConfig) {
    std::fs::create_dir(&config.state_directory).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(
        &config.state_directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let path = config.state_directory.join("journal.sqlite");
    drop(Store::open(&path).unwrap());
    let mut db = Connection::open(path).unwrap();
    let tx = db.transaction().unwrap();
    let mut record: TaskRecord = serde_json::from_str(include_str!(
        "../../../tests/fixtures/legacy-v1/task-record.json"
    ))
    .unwrap();
    record.status = TaskStatus::Succeeded;
    record.finished_at_ms = Some(1002);
    record.exit_code = Some(0);
    record.submission.project_id = "fixture".into();
    record.resolved_command.arguments = vec!["x".repeat(32768)];
    for index in 0..256 {
        record.task_id = format!("historical-{index}");
        record.submission.idempotency_key = format!("historical-{index}");
        record.submission.request_id.clear();
        tx.execute(
            "INSERT INTO tasks(id,project,key,payload,record) VALUES(?1,?2,?3,?4,?5)",
            params![
                record.task_id,
                record.submission.project_id,
                record.submission.idempotency_key,
                serde_json::to_string(&record.submission).unwrap(),
                serde_json::to_string(&record).unwrap()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

#[test]
fn retained_history_does_not_block_new_work_or_change_historical_records() {
    let temp = tempfile::tempdir().unwrap();
    let cfg = config(temp.path());
    seed(&cfg);
    let path = cfg.state_directory.join("journal.sqlite");
    let db = Connection::open(&path).unwrap();
    let before: Vec<(String, i64)> = db
        .prepare("SELECT record,revision FROM tasks ORDER BY rowid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let worker = Worker::open(cfg).unwrap();
    for _ in 0..10 {
        worker.schedule().unwrap();
    }
    let request = TaskSubmission {
        protocol_version: 1,
        request_id: "new".into(),
        project_id: "fixture".into(),
        idempotency_key: "new".into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec!["success".into()],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
        request: request.clone(),
    }) else {
        panic!("submit")
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task: current } = worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) else {
            panic!("lookup")
        };
        if current.status.is_terminal() {
            assert_eq!(current.status, TaskStatus::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let LocalResponse::Task { task: duplicate } = worker.handle(LocalRequest::Submit { request })
    else {
        panic!("duplicate")
    };
    assert_eq!(duplicate.task_id, task.task_id);
    worker.stop();
    while !worker.is_idle() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let after: Vec<(String, i64)> = db
        .prepare("SELECT record,revision FROM tasks WHERE id LIKE 'historical-%' ORDER BY rowid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(before, after);
    assert_eq!(
        db.query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        257
    );
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[ignore = "Explicit isolated baseline/candidate CPU experiment; requires WINBRIDGE_IDLE_BASELINE_WORKER"]
async fn native_idle_cpu_baseline_and_candidate() {
    use std::{
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Child, Command, Stdio},
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            created: *mut u64,
            exited: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
    }
    fn cpu(child: &Child) -> f64 {
        let (mut created, mut exited, mut kernel, mut user) = (0, 0, 0, 0);
        assert_ne!(
            unsafe {
                GetProcessTimes(
                    child.as_raw_handle(),
                    &mut created,
                    &mut exited,
                    &mut kernel,
                    &mut user,
                )
            },
            0
        );
        (kernel + user) as f64 / 10000.0
    }
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let baseline = std::env::var_os("WINBRIDGE_IDLE_BASELINE_WORKER")
        .expect("Pinned baseline executable required");
    let mut results = Vec::new();
    for (label, executable) in [
        ("baseline", baseline),
        ("candidate", env!("CARGO_BIN_EXE_winbridge-worker").into()),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        seed(&cfg);
        let config_path = temp.path().join("worker.json");
        std::fs::write(&config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
        let endpoint =
            winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
        let log = temp.path().join("worker.log");
        let mut child = Owned(
            Command::new(executable)
                .args(["--config"])
                .arg(&config_path)
                .arg("serve")
                .env("WINBRIDGE_SCHEDULER_DIAGNOSTICS", "1")
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(&log).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "worker exited: {}",
                std::fs::read_to_string(&log).unwrap()
            );
            if let Ok(LocalResponse::Info { info }) =
                winbridge_transport::call(&endpoint, &LocalRequest::Info).await
            {
                assert_eq!(info.active_task_count, Some(0));
                break;
            }
            assert!(Instant::now() < deadline, "worker readiness deadline");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let before = cpu(&child.0);
        let start = Instant::now();
        tokio::time::sleep(Duration::from_secs(33)).await;
        let wall = start.elapsed().as_secs_f64() * 1000.0;
        let consumed = cpu(&child.0) - before;
        let LocalResponse::Info { info } =
            winbridge_transport::call(&endpoint, &LocalRequest::Info)
                .await
                .unwrap()
        else {
            panic!("health")
        };
        assert_eq!(info.active_task_count, Some(0));
        assert!(child.0.try_wait().unwrap().is_none());
        drop(child);
        let stderr = std::fs::read_to_string(&log).unwrap();
        if label == "candidate" {
            assert!(
                stderr.contains("WinBridge scheduler:"),
                "diagnostic window missing: {stderr}"
            );
        }
        println!(
            "\nIDLE_CPU {}",
            serde_json::json!({"variant":label,"rows":256,"argumentBytesPerRecord":32768,
            "wallMs":wall,"cpuMs":consumed,"percentOfOneCore":100.0*consumed/wall,"stderr":stderr})
        );
        results.push(consumed / wall);
    }
    // A broad experimental acceptance bound, not a timing-sensitive routine test.
    assert!(
        results[1] < results[0] * 0.5,
        "candidate must halve idle CPU: {results:?}"
    );
}
