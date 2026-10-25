use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::Worker;
fn config(root: &std::path::Path) -> WorkerConfig {
    for id in ["a", "b"] {
        fs::create_dir(root.join(id)).unwrap()
    }
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("queue-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects": (["a","b"].map(|id|serde_json::json!({"id":id,"workspaceRoot":root.join(id),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}))),"workScheduling":{"maxConcurrentWork":1},"queuePolicy":{"maxPendingRoots":2,"maxPendingRootsPerProject":1}})).unwrap()
}
fn info(w: &Worker) -> WorkerInfo {
    let LocalResponse::Info { info } = w.handle(LocalRequest::Info) else {
        panic!("info")
    };
    *info
}
fn task(w: &Worker, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } = w.handle(LocalRequest::GetTask { task_id: id.into() })
    else {
        panic!("task")
    };
    *task
}
fn submission(project: &str, key: &str, mode: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        project_id: project.into(),
        request_id: key.into(),
        idempotency_key: key.into(),
        timeout_ms: 60000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
async fn until(mut check: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !check() {
        assert!(Instant::now() < end, "queue runtime deadline");
        tokio::time::sleep(Duration::from_millis(10)).await
    }
}
async fn wait_ipc(endpoint: &str) {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if winbridge_transport::call(endpoint, &LocalRequest::Info)
            .await
            .is_ok()
        {
            return;
        }
        assert!(Instant::now() < end, "IPC startup");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn change(context: &RequestContext, key: &str, rev: &str, mode: QueueMode) -> QueueControlRequest {
    QueueControlRequest {
        queue_control_version: 1,
        context: context.clone(),
        command: QueueControlCommand::Set {
            command_id: key.into(),
            expected_revision: rev.into(),
            mode,
        },
    }
}
struct Owner(Arc<Worker>);
impl Drop for Owner {
    fn drop(&mut self) {
        if let LocalResponse::Tasks { tasks } = self.0.handle(LocalRequest::ListTasks) {
            for t in tasks {
                self.0.handle(LocalRequest::Cancel { task_id: t.task_id });
            }
        }
        self.0.stop();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn local_ipc_pause_live_owner_duplicate_quota_cancel_receipt_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let endpoint = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let worker = Worker::open(cfg.clone()).unwrap();
    let owner = Owner(worker.clone());
    let context = worker.request_context().unwrap();
    let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
    wait_ipc(&endpoint).await;
    let pause = change(&context, "initial-pause", "0", QueueMode::DispatchPaused);
    let start = Instant::now();
    let first = loop {
        match winbridge_transport::queue_control(&endpoint, &pause).await {
            Ok(r) => break r,
            Err(e) => {
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "startup IPC: {e}"
                );
                tokio::time::sleep(Duration::from_millis(10)).await
            }
        }
    };
    assert!(matches!(first.result, QueueControlResult::Applied { .. }));
    let mut accepted = Vec::new();
    for project in ["a", "b"] {
        let request = LocalRequest::Submit {
            request: submission(project, project, "long"),
        };
        let reply = winbridge_transport::call_fenced(
            &endpoint,
            &FencedRequest {
                request_contract_version: 2,
                context: context.clone(),
                request,
            },
        )
        .await
        .unwrap();
        let LocalResponse::Task { task } = reply.response else {
            panic!("admit")
        };
        accepted.push(*task);
    }
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(accepted
        .iter()
        .all(|t| task(&worker, &t.task_id).process_id.is_none()));
    assert_eq!(
        interpret_queue_control(&info(&worker))
            .unwrap()
            .pending_roots,
        2
    );
    assert!(matches!(
        worker.handle(LocalRequest::Submit {
            request: submission("b", "full", "long")
        }),
        LocalResponse::Error { .. }
    ));
    let duplicate = worker.handle(LocalRequest::Submit {
        request: submission("a", "a", "long"),
    });
    let LocalResponse::Task { task: dup } = duplicate else {
        panic!("duplicate")
    };
    assert_eq!(dup.task_id, accepted[0].task_id);
    let page = winbridge_transport::operation_page(
        &endpoint,
        &OperationPageRequest {
            context: context.clone(),
            query: OperationQuery {
                project_id: None,
                kind: Some(HistoryKind::Task),
                search: accepted[0].task_id.clone(),
                limit: 5,
                cursor: None,
            },
        },
    )
    .await
    .unwrap();
    assert!(page.records[0]
        .reason
        .as_ref()
        .unwrap()
        .contains("Запуск приостановлен"));
    let unknown = worker.handle_extension(ExtensionRequest {
        extension_version: 1,
        contract: ContractRef {
            id: QUEUE_CONTROL_CONTRACT.into(),
            version: 1,
        },
        context: context.clone(),
        payload: serde_json::to_value(&pause).unwrap(),
    });
    assert!(matches!(unknown.result, ExtensionResult::Error { .. }));
    let mut foreign = pause.clone();
    foreign.context.journal_incarnation = "foreign".into();
    assert!(winbridge_transport::queue_control(&endpoint, &foreign)
        .await
        .is_err());
    winbridge_transport::queue_control(
        &endpoint,
        &change(&context, "resume", "1", QueueMode::Running),
    )
    .await
    .unwrap();
    until(|| task(&worker, &accepted[0].task_id).process_id.is_some()).await;
    let pid = task(&worker, &accepted[0].task_id).process_id;
    let pause_both = change(&context, "pause-both", "2", QueueMode::Paused);
    winbridge_transport::queue_control(&endpoint, &pause_both)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(task(&worker, &accepted[0].task_id).process_id, pid);
    assert_eq!(
        task(&worker, &accepted[0].task_id).status,
        TaskStatus::Running
    );
    assert!(task(&worker, &accepted[1].task_id).process_id.is_none());
    let output = winbridge_transport::call_fenced(
        &endpoint,
        &FencedRequest {
            request_contract_version: 2,
            context: context.clone(),
            request: LocalRequest::ReadOutput {
                task_id: accepted[0].task_id.clone(),
                after_sequence: 0,
            },
        },
    )
    .await
    .unwrap();
    assert!(matches!(output.response, LocalResponse::Output { .. }));
    for t in &accepted {
        winbridge_transport::call_fenced(
            &endpoint,
            &FencedRequest {
                request_contract_version: 2,
                context: context.clone(),
                request: LocalRequest::Cancel {
                    task_id: t.task_id.clone(),
                },
            },
        )
        .await
        .unwrap();
    }
    until(|| {
        worker.is_idle()
            && accepted
                .iter()
                .all(|t| task(&worker, &t.task_id).status == TaskStatus::Cancelled)
    })
    .await;
    assert_eq!(
        interpret_queue_control(&info(&worker))
            .unwrap()
            .pending_roots,
        0
    );
    assert_eq!(
        interpret_work_scheduling(&info(&worker))
            .unwrap()
            .occupied_slots,
        0
    );
    worker.stop();
    server.await.unwrap().unwrap();
    drop(owner);
    drop(worker);
    let worker = Worker::open(cfg).unwrap();
    let owner = Owner(worker.clone());
    assert_eq!(worker.request_context().unwrap(), context);
    let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
    wait_ipc(&endpoint).await;
    let replay = winbridge_transport::queue_control(&endpoint, &pause_both)
        .await
        .unwrap();
    assert!(
        matches!(replay.result,QueueControlResult::Applied{ref receipt,..} if receipt.to_revision=="3")
    );
    assert_eq!(
        interpret_queue_control(&info(&worker)).unwrap().mode,
        QueueMode::Paused
    );
    let missing = winbridge_transport::queue_control(
        &endpoint,
        &QueueControlRequest {
            queue_control_version: 1,
            context: context.clone(),
            command: QueueControlCommand::Observe {
                command_id: Some("unknown-command".into()),
            },
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        missing.result,
        QueueControlResult::Observed { receipt: None, .. }
    ));
    assert!(matches!(
        worker.handle(LocalRequest::Submit {
            request: submission("a", "new", "success")
        }),
        LocalResponse::Error { .. }
    ));
    winbridge_transport::queue_control(
        &endpoint,
        &change(&context, "final-resume", "3", QueueMode::Running),
    )
    .await
    .unwrap();
    let LocalResponse::Task { task: new } = worker.handle(LocalRequest::Submit {
        request: submission("a", "new", "success"),
    }) else {
        panic!("resume admit")
    };
    until(|| task(&worker, &new.task_id).status == TaskStatus::Succeeded && worker.is_idle()).await;
    let proof = serde_json::json!({"schemaVersion":1,"passed":true,"platform":std::env::consts::OS,"build":build_identity(),"context":context,"finalQueue":interpret_queue_control(&info(&worker)),"checks":["local-only-envelope","root-quota","duplicate-full-queue","dispatch-pause-no-launch","live-owner-survives-pause","cancel-output-readable","receipt-restart-replay","no-automatic-resume","zero-cleanup-slots"]});
    worker.stop();
    server.await.unwrap().unwrap();
    drop(owner);
    if let Some(path) = std::env::var_os("WINBRIDGE_QUEUE_CONTROL_REPORT") {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(std::path::Path::new(&path).join("queue-control-proof.json"))
            .unwrap();
        f.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        f.sync_all().unwrap();
    }
}
#[test]
fn fair_scheduler_resolves_queued_work_from_a_removed_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let LocalResponse::Task { task: old } = worker.handle(LocalRequest::Submit {
        request: submission("b", "removed", "success"),
    }) else {
        panic!("old admission")
    };
    worker.stop();
    drop(worker);
    cfg.projects.retain(|p| p.id == "a");
    let worker = Worker::open(cfg).unwrap();
    worker.schedule().unwrap();
    assert_eq!(task(&worker, &old.task_id).status, TaskStatus::Failed);
    assert!(task(&worker, &old.task_id).process_id.is_none());
    assert_eq!(
        interpret_queue_control(&info(&worker))
            .unwrap()
            .pending_roots,
        0
    );
}

#[test]
#[ignore = "requires the explicit installed schema-12 reference executable; native gate runs it"]
fn previous_worker_refuses_schema_thirteen_without_mutating_the_journal() {
    let executable =
        std::env::var_os("WINBRIDGE_QUEUE_LEGACY_WORKER").expect("Explicit legacy worker required");
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    cfg.queue_policy = None;
    cfg.work_scheduling = None;
    fs::create_dir_all(&cfg.state_directory).unwrap();
    let path = cfg.state_directory.join("journal.sqlite");
    let store = winbridge_worker::store::Store::open(&path).unwrap();
    drop(store);
    // This retained gate deliberately exercises schema 13 against schema 12.
    // A4.3 separately checks current schema 14 against the actual installed exe.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE worker_journal_activations; DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; PRAGMA user_version=13; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(db);
    let bytes = fs::read(&path).unwrap();
    let config_path = dir.path().join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    let description = std::process::Command::new(&executable)
        .arg("--describe")
        .output()
        .unwrap();
    assert!(description.status.success());
    let previous: serde_json::Value = serde_json::from_slice(&description.stdout).unwrap();
    assert_ne!(
        previous["build"],
        serde_json::to_value(build_identity()).unwrap()
    );
    let mut command = std::process::Command::new(&executable);
    command.args(["--config", config_path.to_str().unwrap(), "serve"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Reference worker did not refuse the new journal; only the owned test process was stopped")
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("journal schema 13 is unsupported"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(path).unwrap(),
        bytes,
        "old worker must not mutate or migrate the new journal"
    );
}
