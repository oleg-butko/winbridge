use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::Worker;
struct Owner(Arc<Worker>, bool);
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.1 {
            if let LocalResponse::Tasks { tasks } = self.0.handle(LocalRequest::ListTasks) {
                for t in tasks {
                    self.0.handle(LocalRequest::Cancel { task_id: t.task_id });
                }
            }
        }
        self.0.stop();
        let end = Instant::now() + Duration::from_secs(5);
        while !self.0.is_idle() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn config(root: &std::path::Path) -> WorkerConfig {
    for id in ["a", "b", "c"] {
        fs::create_dir(root.join(id)).unwrap();
    }
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("resources-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":(["a","b","c"].map(|id|serde_json::json!({"id":id,"workspaceRoot":root.join(id),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}))),"resourceScheduling":{"resources":[{"id":"build","kind":"build","capacity":3},{"id":"cache","kind":"cache","capacity":1}],"projects":[{"projectId":"a","task":[{"resourceId":"build","units":1},{"resourceId":"cache","units":1}]},{"projectId":"b","task":[{"resourceId":"build","units":1},{"resourceId":"cache","units":1}]},{"projectId":"c","task":[{"resourceId":"build","units":1}]}]}})).unwrap()
}
fn info(w: &Worker) -> WorkerInfo {
    let LocalResponse::Info { info } = w.handle(LocalRequest::Info) else {
        panic!("info")
    };
    *info
}
fn view(w: &Worker) -> ResourceSchedulingObservation {
    interpret_resource_scheduling(&info(w)).unwrap()
}
fn task(w: &Worker, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } = w.handle(LocalRequest::GetTask { task_id: id.into() })
    else {
        panic!("task")
    };
    *task
}
fn submit(w: &Worker, project: &str, key: &str) -> TaskRecord {
    let LocalResponse::Task { task } = w.handle(LocalRequest::Submit {
        request: TaskSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            project_id: project.into(),
            timeout_ms: 120000,
            snapshot_id: None,
            command: CommandSource::Direct {
                executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                arguments: vec!["long".into()],
                working_directory: ".".into(),
            },
        },
    }) else {
        panic!("submit")
    };
    *task
}
async fn until(mut check: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !check() {
        assert!(Instant::now() < end, "resource runtime deadline");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn started(w: &Worker, id: &str) -> bool {
    let t = task(w, id);
    t.status == TaskStatus::Running && t.process_id.is_some()
}
fn set(w: &Worker, key: &str, rev: &str, mode: QueueMode) {
    let reply = w.handle_queue_control(QueueControlRequest {
        queue_control_version: 1,
        context: w.request_context().unwrap(),
        command: QueueControlCommand::Set {
            command_id: key.into(),
            expected_revision: rev.into(),
            mode,
        },
    });
    assert!(matches!(reply.result, QueueControlResult::Applied { .. }));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn real_three_project_sets_ipc_cancel_and_frozen_request_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    let endpoint = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let worker = Worker::open(cfg.clone()).unwrap();
    let mut owner = Owner(worker.clone(), false);
    let context = worker.request_context().unwrap();
    let a = submit(&worker, "a", "a");
    let a_next = submit(&worker, "a", "a-next");
    let b = submit(&worker, "b", "b");
    let c = submit(&worker, "c", "c");
    let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
    until(|| {
        started(&worker, &a.task_id)
            && started(&worker, &c.task_id)
            && task(&worker, &b.task_id)
                .reason
                .as_ref()
                .is_some_and(|r| r.contains("cache"))
    })
    .await;
    assert!(task(&worker, &b.task_id).process_id.is_none());
    let loaded = view(&worker);
    assert_eq!(
        loaded
            .resources
            .iter()
            .find(|r| r.id == "build")
            .unwrap()
            .used_units,
        2
    );
    assert_eq!(
        loaded
            .resources
            .iter()
            .find(|r| r.id == "cache")
            .unwrap()
            .used_units,
        1
    );
    let timer = Instant::now();
    let LocalResponse::Info { info: remote } =
        winbridge_transport::call(&endpoint, &LocalRequest::Info)
            .await
            .unwrap()
    else {
        panic!("native IPC")
    };
    assert!(timer.elapsed() < Duration::from_secs(5));
    assert_eq!(
        interpret_resource_scheduling(&remote).unwrap().context,
        context
    );
    let duplicate = submit(&worker, "a", "a");
    assert_eq!(duplicate.task_id, a.task_id);
    assert_eq!(duplicate.process_id, task(&worker, &a.task_id).process_id);
    let page = winbridge_transport::operation_page(
        &endpoint,
        &OperationPageRequest {
            context: context.clone(),
            query: OperationQuery {
                project_id: Some("b".into()),
                kind: Some(HistoryKind::Task),
                search: b.task_id.clone(),
                limit: 5,
                cursor: None,
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(page.records.len(), 1);
    assert!(page.records[0].reason.as_ref().unwrap().contains("cache"));
    let result = winbridge_transport::call_fenced(
        &endpoint,
        &FencedRequest {
            request_contract_version: 2,
            context: context.clone(),
            request: LocalRequest::Cancel {
                task_id: a.task_id.clone(),
            },
        },
    )
    .await
    .unwrap();
    assert!(matches!(result.response, LocalResponse::Task { .. }));
    until(|| {
        task(&worker, &a.task_id).status == TaskStatus::Cancelled && started(&worker, &b.task_id)
    })
    .await;
    for t in [&a_next, &b, &c] {
        worker.handle(LocalRequest::Cancel {
            task_id: t.task_id.clone(),
        });
    }
    until(|| worker.is_idle() && view(&worker).owners.is_empty()).await;
    set(&worker, "pause-before-edit", "0", QueueMode::DispatchPaused);
    let frozen = submit(&worker, "b", "frozen");
    let generation = info(&worker).generation;
    owner.1 = true; // Clean restart deliberately preserves this accepted queued request.
    worker.stop();
    server.await.unwrap().unwrap();
    drop(owner);
    drop(worker);
    cfg.resource_scheduling
        .as_mut()
        .unwrap()
        .projects
        .iter_mut()
        .find(|p| p.project_id == "b")
        .unwrap()
        .task
        .retain(|r| r.resource_id != "cache");
    let worker = Worker::open(cfg).unwrap();
    let owner = Owner(worker.clone(), false);
    assert_eq!(worker.request_context().unwrap(), context);
    assert_ne!(info(&worker).generation, generation);
    let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
    assert!(task(&worker, &frozen.task_id).process_id.is_none());
    set(&worker, "explicit-resume", "1", QueueMode::Running);
    until(|| started(&worker, &frozen.task_id)).await;
    let restored = view(&worker);
    let restored_info = info(&worker);
    assert!(
        restored
            .owners
            .iter()
            .any(|o| o.resource_id == "cache" && o.operation_id == frozen.task_id),
        "accepted requirements must not be replaced by edited bindings"
    );
    worker.handle(LocalRequest::Cancel {
        task_id: frozen.task_id,
    });
    until(|| worker.is_idle() && view(&worker).owners.is_empty()).await;
    let final_view = view(&worker);
    let final_info = info(&worker);
    assert!(final_view.resources.iter().all(|r| r.used_units == 0));
    worker.stop();
    server.await.unwrap().unwrap();
    drop(owner);
    if let Some(path) = std::env::var_os("WINBRIDGE_RESOURCE_SCHEDULING_REPORT") {
        use std::io::Write;
        let proof = serde_json::json!({"schemaVersion":1,"passed":true,"platform":std::env::consts::OS,"build":build_identity(),"context":context,"loaded":loaded,"loadedInfo":remote,"restored":restored,"restoredInfo":restored_info,"final":final_view,"finalInfo":final_info,"checks":["three-project-complete-sets","shared-cache-blocker","fair-next-project","native-info-operation-page","fenced-cancel","duplicate-same-process","frozen-request-after-config-edit","restart-same-journal","zero-resource-cleanup"]});
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(std::path::PathBuf::from(path).join("resource-scheduling-proof.json"))
            .unwrap();
        f.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        f.sync_all().unwrap();
    }
}
#[test]
#[ignore = "requires explicit installed schema-13 reference; native gate executes it"]
fn previous_worker_refuses_schema_fourteen_without_mutating_journal() {
    let executable = std::env::var_os("WINBRIDGE_RESOURCE_LEGACY_WORKER")
        .expect("Explicit schema-13 exe required");
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    cfg.resource_scheduling = None;
    fs::create_dir_all(&cfg.state_directory).unwrap();
    let path = cfg.state_directory.join("journal.sqlite");
    drop(winbridge_worker::store::Store::open(&path).unwrap());
    // Keep this historical schema-13 -> schema-14 refusal fixture independent
    // of later migrations. No source-update record exists in this owned DB.
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; PRAGMA user_version=14;").unwrap();
    drop(c);
    let before = fs::read(&path).unwrap();
    let cfg_path = dir.path().join("worker.local.json");
    fs::write(&cfg_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    let mut command = std::process::Command::new(&executable);
    command
        .args(["--config", cfg_path.to_str().unwrap(), "serve"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > end {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Owned previous test worker did not refuse schema 14");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("journal schema 14 is unsupported"));
    assert_eq!(fs::read(path).unwrap(), before);
}
