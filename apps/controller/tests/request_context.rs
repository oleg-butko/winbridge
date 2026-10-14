#![cfg(unix)]
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::{config::WorkerConfig, controller_config::ControllerConfig};
use winbridge_protocol::*;
use winbridge_worker::{store::Store, Worker};

async fn until_ready(controller: &winbridge_controller::Controller) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !controller.status().unwrap().connected {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_controller_worker_and_mcp_preserve_caller_scope_across_restart() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let project = root.path().join("project");
    fs::create_dir(&project).unwrap();
    let reserve = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let cfg:ControllerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"pair","nodeId":"controller","stateDirectory":root.path().join("controller"),"controller":{"listenAddress":address.to_string(),"workerNodeId":"worker"},"projects":[]})).unwrap();
    let controller = winbridge_controller::Controller::new(cfg.clone()).unwrap();
    let server = tokio::spawn(winbridge_controller::serve(Arc::clone(&controller)));
    let worker_cfg:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"pair","nodeId":"worker","stateDirectory":root.path().join("worker"),"maxOutputBytesPerTask":10000,"worker":{"keepRunningOnUiExit":true,"controllerUrl":format!("ws://{address}")},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    fs::create_dir(&worker_cfg.state_directory).unwrap();
    fs::set_permissions(
        &worker_cfg.state_directory,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let mut store = Store::open(&worker_cfg.state_directory.join("journal.sqlite")).unwrap();
    store.require_fenced_requests().unwrap();
    drop(store);
    let worker = Worker::open(worker_cfg.clone()).unwrap();
    let context = worker.request_context().unwrap();
    let worker_server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
    until_ready(&controller).await;
    let LocalResponse::Info { info } = controller.call(LocalRequest::Info).await else {
        panic!("route info")
    };
    let advertised = info.contract_catalog.clone();
    let policies = info.project_policies.clone();
    let schemas = info.contract_schemas.clone();
    assert!(interpret_contract_schemas(&info).is_some());
    assert!(interpret_project_policies(&info).is_some());
    let CatalogObservation::Known(catalog) = interpret_catalog(&info) else {
        panic!("route lost catalog")
    };
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 2));
    let legacy:LocalRequest=serde_json::from_value(serde_json::json!({"method":"submit","request":{"protocolVersion":1,"requestId":"r","projectId":"fixture","idempotencyKey":"logical-key","timeoutMs":5000,"command":{"kind":"direct","executable":"sh","arguments":["-c","printf 'ran\\n' >> runs.log; printf 'paired ✓\\n'"],"workingDirectory":"."}}})).unwrap();
    let envelope = FencedRequest {
        request_contract_version: 2,
        context: context.clone(),
        request: legacy.clone(),
    };
    assert!(
        matches!(controller.call(legacy).await,LocalResponse::Error{message} if message.contains("contract 2"))
    );
    let response =
        winbridge_transport::call_fenced(&winbridge_controller::endpoint(&cfg), &envelope)
            .await
            .unwrap();
    let LocalResponse::Task { task } = response.response else {
        panic!("{response:?}")
    };
    let id = task.task_id;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let LocalResponse::Task { task } = controller
            .call(LocalRequest::GetTask {
                task_id: id.clone(),
            })
            .await
        else {
            panic!("task missing")
        };
        if task.status.is_terminal() && worker.is_idle() {
            assert_eq!(task.status, TaskStatus::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let input = format!(
        "{}\n{}\n{}\n{}\n",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"scope-test","version":"1"}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"winbridge_invoke_fenced","arguments":envelope}}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"winbridge_info","arguments":{}}})
    );
    let mut output = Vec::new();
    winbridge_controller::mcp::run(cfg.clone(), input.as_bytes(), &mut output)
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages[1]["result"]["isError"], false);
    assert_eq!(messages[2]["result"]["isError"], false);
    // Support metadata is immutable; a readonly session projection may advance
    // between two Info calls. Scope/policy/schema forwarding remains exact.
    let mut forwarded_catalog =
        messages[2]["result"]["structuredContent"]["info"]["contractCatalog"].clone();
    let mut advertised = advertised.unwrap();
    forwarded_catalog
        .as_object_mut()
        .unwrap()
        .remove("controllerProjection");
    advertised
        .as_object_mut()
        .unwrap()
        .remove("controllerProjection");
    assert_eq!(forwarded_catalog, advertised);
    assert_eq!(
        messages[2]["result"]["structuredContent"]["info"]["projectPolicies"],
        policies.unwrap()
    );
    assert_eq!(
        messages[2]["result"]["structuredContent"]["info"]["contractSchemas"],
        schemas.unwrap()
    );
    assert_eq!(
        messages[1]["result"]["structuredContent"]["context"]["journalIncarnation"],
        context.journal_incarnation
    );
    assert_eq!(
        messages[1]["result"]["structuredContent"]["response"]["task"]["taskId"],
        id
    );
    let mut stale = envelope.clone();
    stale.context.journal_incarnation = "different-journal".into();
    let response = controller.call_fenced(stale).await;
    assert!(
        matches!(response.response,LocalResponse::Error{message} if message.contains("Caller-pinned"))
    );
    worker.stop();
    worker_server.await.unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.status().unwrap().connected {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(worker);
    let worker = Worker::open(worker_cfg).unwrap();
    assert_eq!(worker.request_context().unwrap(), context);
    let worker_server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
    until_ready(&controller).await;
    let response = controller.call_fenced(envelope).await;
    let LocalResponse::Task { task } = response.response else {
        panic!("{response:?}")
    };
    assert_eq!(task.task_id, id);
    assert_eq!(
        fs::read_to_string(project.join("runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    worker.stop();
    worker_server.await.unwrap().unwrap();
    server.abort();
    let _ = server.await;
}
