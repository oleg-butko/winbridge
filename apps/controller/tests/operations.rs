#![cfg(unix)]
use std::{fs, sync::Arc, time::Duration};
use winbridge_core::{config::WorkerConfig, controller_config::ControllerConfig};
use winbridge_protocol::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_ws_controller_api_cli_and_mcp_observe_the_same_scoped_operation() {
    tokio::time::timeout(Duration::from_secs(25),async {
        let temp=tempfile::tempdir().unwrap();let root=temp.path();
        let project=root.join("project");fs::create_dir(&project).unwrap();
        let port=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=port.local_addr().unwrap();drop(port);
        let config_value=serde_json::json!({"schemaVersion":1,"instanceId":"operations","nodeId":"controller","stateDirectory":root.join("controller"),"controller":{"listenAddress":address.to_string(),"workerNodeId":"worker"},"projects":[]});
        let config:ControllerConfig=serde_json::from_value(config_value.clone()).unwrap();
        let controller=winbridge_controller::Controller::new(config.clone()).unwrap();
        let server=tokio::spawn(winbridge_controller::serve(Arc::clone(&controller)));
        let worker_config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"operations","nodeId":"worker","stateDirectory":root.join("worker"),"worker":{"keepRunningOnUiExit":true,"controllerUrl":format!("ws://{address}")},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
        let worker=winbridge_worker::Worker::open(worker_config).unwrap();
        let context=worker.request_context().unwrap();
        let response=worker.handle_fenced(FencedRequest {request_contract_version:2,context:context.clone(),request:LocalRequest::Submit {request:TaskSubmission {protocol_version:1,request_id:"real-route".into(),idempotency_key:"real-route".into(),project_id:"fixture".into(),timeout_ms:5000,snapshot_id:None,command:CommandSource::Direct {executable:"/bin/sh".into(),arguments:vec!["-c".into(),"printf 'operation route ✓'".into()],working_directory:".".into()}}}});
        let LocalResponse::Task { task }=response.response else {panic!("accepted task");};
        let worker_server=tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
        while !controller.status().unwrap().connected {tokio::time::sleep(Duration::from_millis(10)).await;}
        let request=OperationPageRequest {context:context.clone(),query:OperationQuery {project_id:Some("fixture".into()),kind:Some(HistoryKind::Task),search:task.task_id.clone(),limit:10,cursor:None}};
        loop {
            let page=controller.operation_page(request.clone()).await.unwrap();
            assert_eq!(page.records.len(),1);
            if matches!(page.records[0].state,OperationState::Execution {status:TaskStatus::Succeeded}) {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let response=winbridge_controller::readiness::call(&winbridge_controller::endpoint(&config),&ControllerApiRequest {controller_contract_version:1,adapter:build_identity(),adapter_kind:ControllerAdapter::Cli,request:ControllerOperation::OperationPage {request:Box::new(request.clone())}}).await.unwrap();
        assert!(matches!(response.result,ControllerApiResult::OperationPage {page} if page.context==context && page.records[0].reference.operation_id==task.task_id));
        let cfg=root.join("controller.local.json");fs::write(&cfg,serde_json::to_vec(&config_value).unwrap()).unwrap();
        let request_path=root.join("request.json");fs::write(&request_path,serde_json::to_vec(&request).unwrap()).unwrap();
        let exe=env!("CARGO_BIN_EXE_winbridge-controller");
        let output=tokio::process::Command::new(exe).args(["--config",cfg.to_str().unwrap(),"operations",request_path.to_str().unwrap()]).kill_on_drop(true).output().await.unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let cli:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cli["result"]["page"]["records"][0]["reference"]["operationId"],task.task_id);
        use tokio::io::AsyncWriteExt;
        let mut process=tokio::process::Command::new(exe).args(["--config",cfg.to_str().unwrap(),"mcp"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true).spawn().unwrap();
        let input=format!("{}\n{}\n{}\n",serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"operation-page-test","version":"1"}}}),serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"winbridge_operations","arguments":request}}));
        let mut stdin=process.stdin.take().unwrap();stdin.write_all(input.as_bytes()).await.unwrap();drop(stdin);
        let output=process.wait_with_output().await.unwrap();assert!(output.status.success());
        let values=String::from_utf8(output.stdout).unwrap().lines().map(|s|serde_json::from_str::<serde_json::Value>(s).unwrap()).collect::<Vec<_>>();
        let result=&values[1]["result"];assert_eq!(result["isError"],false);
        assert_eq!(result["structuredContent"]["result"]["page"]["records"][0]["reference"]["operationId"],task.task_id);
        let mut foreign=request.clone();foreign.context.journal_incarnation="foreign".into();
        assert!(controller.operation_page(foreign).await.unwrap_err().contains("scope changed"));
        worker.stop();worker_server.await.unwrap().unwrap();server.abort();let _=server.await;
    }).await.expect("actual operation route timed out");
}
