#![cfg(unix)]
//! Actual two-controller TCP/IPC, CLI and MCP. Private Linux fixtures only.
use std::{path::Path, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use winbridge_controller::Controller;
use winbridge_core::{config::WorkerConfig, controller_config::ControllerConfig};
use winbridge_protocol::*;
use winbridge_worker::Worker;

struct Harness {
    worker: Arc<Worker>,
    controllers: [Arc<Controller>; 2],
    configs: [std::path::PathBuf; 2],
    servers: [JoinHandle<Result<(), String>>; 2],
    service: JoinHandle<Result<(), String>>,
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.worker.stop();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while !self.worker.is_idle() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.service.abort();
        for server in &self.servers {
            server.abort();
        }
    }
}
impl Harness {
    async fn new(root: &Path) -> Self {
        std::fs::create_dir(root.join("project")).unwrap();
        let mut addresses = vec![];
        for _ in 0..2 {
            let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            addresses.push(socket.local_addr().unwrap());
        }
        let instance = format!("diag-{}", uuid::Uuid::new_v4());
        let configs = [root.join("a.json"), root.join("b.json")];
        let controllers=[0,1].map(|i| {
            let value=serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"same-node","stateDirectory":root.join(format!("controller-{i}")),"controller":{"listenAddress":addresses[i],"workerNodeId":"worker"},"projects":[]});
            std::fs::write(&configs[i],serde_json::to_vec(&value).unwrap()).unwrap();Controller::new(serde_json::from_value(value).unwrap()).unwrap()
        });
        let servers = controllers
            .each_ref()
            .map(|c| tokio::spawn(winbridge_controller::serve(c.clone())));
        let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"worker","stateDirectory":root.join("worker"),"worker":{"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":"a","controllers":[{"id":"a","url":format!("ws://{}",addresses[0])},{"id":"b","url":format!("ws://{}",addresses[1])}]}},"projects":[{"id":"fixture","workspaceRoot":root.join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
        let worker = Worker::open(config).unwrap();
        let service = tokio::spawn(winbridge_worker::serve(worker.clone()));
        let result = Self {
            worker,
            controllers,
            configs,
            servers,
            service,
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(12);
        while !result
            .controllers
            .iter()
            .all(|c| c.status().unwrap().connected)
        {
            assert!(
                tokio::time::Instant::now() < until,
                "Two fixture controllers did not connect"
            );
            assert!(!result.service.is_finished());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        result
    }
    async fn close(&mut self) {
        self.worker.stop();
        (&mut self.service).await.unwrap().unwrap();
        for server in &mut self.servers {
            server.abort();
            let _ = server.await;
        }
    }
}
fn envelope(context: &RequestContext, request: LocalRequest) -> FencedRequest {
    FencedRequest {
        request_contract_version: 2,
        context: context.clone(),
        request,
    }
}
async fn cli(config: &Path, command: &str, input: &Path) -> (bool, serde_json::Value) {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_winbridge-controller"))
        .args([
            "--config",
            config.to_str().unwrap(),
            command,
            input.to_str().unwrap(),
        ])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr))),
    )
}
async fn mcp(
    config: &Path,
    context: &RequestContext,
    query: &OperationAttributionRequest,
) -> Vec<serde_json::Value> {
    use tokio::io::AsyncWriteExt;
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_winbridge-controller"))
        .args(["--config", config.to_str().unwrap(), "mcp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut bad = query.clone();
    bad.context.journal_incarnation = "foreign".into();
    let values = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"diagnostics","version":"1"}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"winbridge_controller_connections","arguments":{"context":context}}}),
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"winbridge_operation_attribution","arguments":query}}),
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"winbridge_operation_cancellation","arguments":query}}),
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"winbridge_operation_attribution","arguments":bad}}),
        serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"winbridge_operation_cancellation","arguments":{"context":context,"query":{"kind":"task","operationId":query.query.operation_id,"cancel":true}}}}),
    ];
    let input = values.iter().map(|v| format!("{v}\n")).collect::<String>();
    let mut stdin = process.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await.unwrap();
    drop(stdin);
    let output = process.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

async fn diagnostic_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let mut h = Harness::new(root.path()).await;
    let context = h.worker.request_context().unwrap();
    let reply = h.controllers[0]
        .call_fenced(envelope(
            &context,
            LocalRequest::Submit {
                request: TaskSubmission {
                    protocol_version: 1,
                    request_id: "fixture".into(),
                    project_id: "fixture".into(),
                    idempotency_key: "one".into(),
                    timeout_ms: 60000,
                    snapshot_id: None,
                    command: CommandSource::Direct {
                        executable: "/bin/sh".into(),
                        arguments: vec!["-c".into(), "exec sleep 60".into()],
                        working_directory: ".".into(),
                    },
                },
            },
        ))
        .await;
    let LocalResponse::Task { task } = reply.response else {
        panic!("{reply:?}")
    };
    let id = task.task_id.clone();
    loop {
        if matches!(h.worker.handle(LocalRequest::GetTask {task_id:id.clone()}),LocalResponse::Task {task} if task.process_id.is_some())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let query = OperationAttributionRequest {
        context: context.clone(),
        query: OperationAttributionQuery {
            kind: HistoryKind::Task,
            operation_id: id.clone(),
        },
    };
    let running_input = root.path().join("running.json");
    std::fs::write(&running_input, serde_json::to_vec(&query).unwrap()).unwrap();
    for config in &h.configs {
        let (success, value) = cli(config, "operation-cancellation", &running_input).await;
        assert!(success);
        assert!(value["result"]["observation"]["intent"].is_null());
        let messages = mcp(config, &context, &query).await;
        let observed = &messages.iter().find(|m| m["id"] == 5).unwrap()["result"];
        assert_eq!(observed["isError"], false);
        assert!(observed["structuredContent"]["result"]["observation"]["intent"].is_null());
        assert!(
            matches!(h.worker.handle(LocalRequest::GetTask {task_id:id.clone()}),LocalResponse::Task {task} if task.status==TaskStatus::Running&&!task.cancel_requested),
            "Readonly cancellation observation cancelled a running task"
        );
    }
    let cancel = h.controllers[1]
        .call_fenced(envelope(
            &context,
            LocalRequest::Cancel {
                task_id: id.clone(),
            },
        ))
        .await;
    assert_eq!(cancel.outcome, FencedReplyOutcome::Responded);
    loop {
        if h.worker.is_idle() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let inputs = [
        (
            "controller-connections",
            serde_json::json!({"context":context}),
            "controller_connections",
        ),
        (
            "operation-attribution",
            serde_json::to_value(&query).unwrap(),
            "operation_attribution",
        ),
        (
            "operation-cancellation",
            serde_json::to_value(&query).unwrap(),
            "operation_cancellation",
        ),
    ];
    for config in &h.configs {
        for (command, payload, kind) in &inputs {
            let path = root.path().join(format!("{command}.json"));
            std::fs::write(&path, serde_json::to_vec(payload).unwrap()).unwrap();
            let (success, value) = cli(config, command, &path).await;
            assert!(success, "{value}");
            assert_eq!(value["controllerDiagnosticsVersion"], 1);
            assert_eq!(value["result"]["kind"], *kind);
            let observation = &value["result"]["observation"];
            match *kind {
                "controller_connections" => {
                    assert_eq!(observation["connections"].as_array().unwrap().len(), 2);
                    assert_eq!(observation["primary"], "a");
                }
                "operation_attribution" => assert_eq!(observation["creator"]["controllerId"], "a"),
                _ => assert_eq!(observation["intent"]["actor"]["controllerId"], "b"),
            }
        }
        let messages = mcp(config, &context, &query).await;
        let tools = messages.iter().find(|m| m["id"] == 2).unwrap()["result"]["tools"]
            .as_array()
            .unwrap();
        for name in [
            "winbridge_controller_connections",
            "winbridge_operation_attribution",
            "winbridge_operation_cancellation",
        ] {
            let tool = tools.iter().find(|t| t["name"] == name).unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["annotations"]["destructiveHint"], false);
        }
        for id in 3..=5 {
            let result = &messages.iter().find(|m| m["id"] == id).unwrap()["result"];
            assert_eq!(result["isError"], false);
            assert_eq!(
                result["structuredContent"]["controllerDiagnosticsVersion"],
                1
            );
        }
        assert_eq!(
            messages.iter().find(|m| m["id"] == 4).unwrap()["result"]["structuredContent"]
                ["result"]["observation"]["creator"]["controllerId"],
            "a"
        );
        assert_eq!(
            messages.iter().find(|m| m["id"] == 5).unwrap()["result"]["structuredContent"]
                ["result"]["observation"]["intent"]["actor"]["controllerId"],
            "b"
        );
        assert_eq!(
            messages.iter().find(|m| m["id"] == 6).unwrap()["result"]["isError"],
            true
        );
        assert_eq!(
            messages.iter().find(|m| m["id"] == 7).unwrap()["error"]["code"],
            -32602
        );
    }
    // Even a deliberately mixed command envelope stays on the diagnostic route.
    let cfg = ControllerConfig::load(&h.configs[0]).unwrap();
    let endpoint = winbridge_controller::endpoint(&cfg);
    for value in [
        serde_json::json!({"controllerDiagnosticsVersion":1,"method":"cancel","taskId":id}),
        serde_json::json!({"controllerDiagnosticsVersion":1,"controllerContractVersion":1,"request":{"method":"verify_pair"}}),
        serde_json::json!({"controllerDiagnosticsVersion":2,"request":{"method":"controller_connections","request":{"context":context}}}),
    ] {
        let mut stream = tokio::net::UnixStream::connect(&endpoint).await.unwrap();
        winbridge_transport::write_frame(&mut stream, &serde_json::to_vec(&value).unwrap())
            .await
            .unwrap();
        let response: ControllerDiagnosticsResponse =
            serde_json::from_slice(&winbridge_transport::read_frame(&mut stream).await.unwrap())
                .unwrap();
        assert!(matches!(
            response.result,
            ControllerDiagnosticResult::Error { .. }
        ));
    }
    let mut stale = query;
    stale.context.journal_incarnation = "foreign".into();
    let path = root.path().join("stale.json");
    std::fs::write(&path, serde_json::to_vec(&stale).unwrap()).unwrap();
    let (success, response) = cli(&h.configs[0], "operation-attribution", &path).await;
    assert!(!success);
    assert_eq!(response["result"]["kind"], "error");
    assert!(
        matches!(h.worker.handle(LocalRequest::ListTasks),LocalResponse::Tasks {tasks} if tasks.len()==1&&tasks[0].status==TaskStatus::Cancelled)
    );
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn both_agents_read_the_same_connections_creator_and_cancel_actor_through_cli_and_mcp() {
    tokio::time::timeout(Duration::from_secs(40), diagnostic_roundtrip())
        .await
        .expect("Actual diagnostic CLI/MCP route deadline");
}
