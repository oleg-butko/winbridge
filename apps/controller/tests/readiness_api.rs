#![cfg(unix)]
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::{config::WorkerConfig, controller_config::ControllerConfig};
use winbridge_protocol::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_readiness_ipc_mcp_observe_new_metadata_without_enabling_missing_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    let port = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = port.local_addr().unwrap();
    drop(port);
    let config:ControllerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"readiness","nodeId":"controller","stateDirectory":root.join("controller"),"controller":{"listenAddress":address.to_string(),"workerNodeId":"worker"},"projects":[]})).unwrap();
    let controller = winbridge_controller::Controller::new(config.clone()).unwrap();
    let server = tokio::spawn(winbridge_controller::serve(Arc::clone(&controller)));
    let worker_config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"readiness","nodeId":"worker","stateDirectory":root.join("worker"),"worker":{"keepRunningOnUiExit":true,"controllerUrl":format!("ws://{address}")},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    let worker = winbridge_worker::Worker::open(worker_config).unwrap();
    let worker_server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !controller.status().unwrap().connected {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let query = ContractReadinessQuery {
        project_id: "fixture".into(),
        contract: ContractRef {
            id: WORKER_RPC_CONTRACT.into(),
            version: 2,
        },
        receipt_id: None,
    };
    let request = ControllerApiRequest {
        controller_contract_version: 1,
        adapter: build_identity(),
        adapter_kind: ControllerAdapter::Mcp,
        request: ControllerOperation::ContractReadiness {
            query: query.clone(),
        },
    };
    let response =
        winbridge_controller::readiness::call(&winbridge_controller::endpoint(&config), &request)
            .await
            .unwrap();
    let ControllerApiResult::ContractReadiness { view } = response.result else {
        panic!("expected readiness")
    };
    assert!(view.readiness.supported && !view.readiness.enabled && !view.readiness.pair_tested);
    assert!(view.session_id.is_some());
    let identity = view.identity.unwrap();
    assert_eq!(identity.context, worker.request_context().unwrap());
    assert_eq!(identity.contract_schema_sha256, fenced_rpc_schema_sha256());
    assert_eq!(identity.route[1].build, build_identity());
    assert_eq!(identity.provider, build_identity());
    let input = format!(
        "{}\n{}\n{}\n",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"readiness-test","version":"1"}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"winbridge_contract_readiness","arguments":query}})
    );
    use std::io::{Read, Write};
    let executable = env!("CARGO_BIN_EXE_winbridge-controller");
    let config_path = root.join("controller.local.json");
    fs::write(&config_path,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"instanceId":config.instance_id,"nodeId":config.node_id,"stateDirectory":config.state_directory,"controller":{"listenAddress":config.controller.listen_address.to_string(),"workerNodeId":config.controller.worker_node_id},"projects":[]})).unwrap()).unwrap();
    let descriptor = std::process::Command::new(executable)
        .arg("--describe")
        .output()
        .unwrap();
    assert!(descriptor.status.success());
    let descriptor: serde_json::Value = serde_json::from_slice(&descriptor.stdout).unwrap();
    assert_eq!(
        descriptor["build"],
        serde_json::to_value(build_identity()).unwrap()
    );
    let mut process = std::process::Command::new(executable)
        .args(["--config", config_path.to_str().unwrap(), "mcp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = process.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    let out = process.stdout.take().unwrap();
    let err = process.stderr.take().unwrap();
    let drain = |mut stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            bytes
        })
    };
    let out = drain(Box::new(out));
    let err = drain(Box::new(err));
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = process.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            process.kill().unwrap();
            process.wait().unwrap();
            panic!("Actual MCP process timeout");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(
        status.success(),
        "{:?}",
        String::from_utf8_lossy(&err.join().unwrap())
    );
    let output = out.join().unwrap();
    let values: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(values[1]["result"]["isError"], false);
    assert_eq!(
        values[1]["result"]["structuredContent"]["result"]["view"]["readiness"]["enabled"],
        false
    );
    assert_eq!(
        values[1]["result"]["structuredContent"]["result"]["view"]["identity"]["context"]
            ["journalIncarnation"],
        worker.request_context().unwrap().journal_incarnation
    );
    let unknown = ContractReadinessQuery {
        contract: ContractRef {
            id: "unknown.execution".into(),
            version: 3,
        },
        ..query.clone()
    };
    let view = controller
        .observe_contract(&build_identity(), &ControllerAdapter::Mcp, unknown)
        .await
        .unwrap();
    assert!(!view.readiness.supported && !view.readiness.enabled && view.identity.is_none());
    assert!(
        matches!(worker.handle(LocalRequest::ListTasks),LocalResponse::Tasks{tasks} if tasks.is_empty())
    );
    worker.stop();
    worker_server.await.unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.status().unwrap().connected {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let offline = controller
        .observe_contract(&build_identity(), &ControllerAdapter::Mcp, query)
        .await
        .unwrap();
    assert!(!offline.readiness.enabled && offline.identity.is_none());
    assert!(offline
        .readiness
        .reasons
        .contains(&ReadinessReason::RouteNotFresh));
    if let Some(directory) = std::env::var_os("WINBRIDGE_CONSUMER_PROOF_DIRECTORY") {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let directory = std::path::Path::new(&directory);
        let artifact = fs::read(executable).unwrap();
        let proof = serde_json::json!({"schemaVersion":1,"profile":"controller-consumer-rpc2-v1","passed":true,"platform":"linux","build":build_identity(),"binarySha256":evidence_sha256(&artifact),"checks":["actual-cli-identity","actual-mcp-stdio","actual-ws-ipc-route","schema-policy-scope-preserved","unknown-contract-no-dispatch","disconnect-no-enable"]});
        let bytes = serde_json::to_vec_pretty(&proof).unwrap();
        let id = evidence_sha256(&serde_json::to_vec(&build_identity()).unwrap());
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .unwrap();
        let path = directory.join(format!("consumer-{id}.json"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        fs::File::open(directory).unwrap().sync_all().unwrap();
    }
    server.abort();
    let _ = server.await;
}
