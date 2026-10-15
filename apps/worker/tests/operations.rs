use std::{fs, sync::Arc, time::Duration};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn actual_local_ipc_reads_completed_task_and_rejects_foreign_or_future_contracts() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let instance = format!("operations-{}", uuid::Uuid::new_v4());
        let config: WorkerConfig = serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"worker","stateDirectory":temp.path().join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
        let address = winbridge_transport::endpoint(&config.state_directory, &instance).unwrap();
        let worker = winbridge_worker::Worker::open(config).unwrap();
        let context = worker.request_context().unwrap();
        let server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
        loop {
            if winbridge_transport::call(&address, &LocalRequest::Info).await.is_ok() { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let submit = FencedRequest { request_contract_version:2, context:context.clone(), request:LocalRequest::Submit { request:TaskSubmission {protocol_version:1,request_id:"page-task".into(),idempotency_key:"page-task".into(),project_id:"fixture".into(),timeout_ms:5000,snapshot_id:None,command:CommandSource::Direct {executable:env!("CARGO_BIN_EXE_winbridge-fixture").into(),arguments:vec!["success".into()],working_directory:".".into()}}}};
        let accepted = winbridge_transport::call_fenced(&address, &submit).await.unwrap();
        let LocalResponse::Task { task } = accepted.response else { panic!("task not accepted"); };
        let request=OperationPageRequest { context:context.clone(),query:OperationQuery {project_id:Some("fixture".into()),kind:Some(HistoryKind::Task),search:task.task_id.clone(),limit:1,cursor:None}};
        let terminal = loop {
            let page=winbridge_transport::operation_page(&address,&request).await.unwrap();
            assert_eq!(page.records.len(),1);
            let record=page.records.into_iter().next().unwrap();
            if matches!(record.state, OperationState::Execution {status:TaskStatus::Succeeded}) {break record;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(terminal.reference.context,context);
        assert!(terminal.finished_at_ms.is_some());
        assert!(terminal.resources.is_empty());
        let LocalResponse::Output { chunks } = winbridge_transport::call_fenced(&address,&FencedRequest { request:LocalRequest::ReadOutput {task_id:task.task_id.clone(),after_sequence:0},..submit.clone() }).await.unwrap().response else {panic!("output");};
        let stdout=chunks.iter().filter(|c|matches!(c.stream,OutputStream::Stdout)).flat_map(|c|c.bytes.iter().copied()).collect::<Vec<_>>();
        assert!(String::from_utf8_lossy(&stdout).contains("Привет"));
        let base=ExtensionRequest {extension_version:1,contract:operation_contract(),context:context.clone(),payload:serde_json::to_value(&request.query).unwrap()};
        for bad in [ExtensionRequest {contract:ContractRef {version:2,..operation_contract()},..base.clone()},ExtensionRequest {context:RequestContext {journal_incarnation:"foreign".into(),..context.clone()},..base.clone()},ExtensionRequest {payload:serde_json::json!({"method":"submit"}),..base.clone()}] {
            let response=winbridge_transport::call_extension(&address,&bad).await.unwrap();
            assert!(matches!(response.result,ExtensionResult::Error {..}));
            assert!(response.context.is_none());
        }
        let LocalResponse::Tasks { tasks }=worker.handle(LocalRequest::ListTasks) else {panic!("tasks");};
        assert_eq!(tasks.len(),1,"readonly queries may not admit any tasks");
        assert_eq!(tasks[0].task_id,task.task_id);
        worker.stop(); server.await.unwrap().unwrap();
    }).await.expect("isolated IPC operation gate timed out");
}
