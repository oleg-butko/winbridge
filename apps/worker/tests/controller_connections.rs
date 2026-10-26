use winbridge_protocol::*;
use winbridge_worker::Worker;

#[test]
fn readonly_diagnostics_validate_contract_payload_and_scope_without_accepting_work() {
    let root = tempfile::tempdir().unwrap();
    let worker=Worker::open(serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"diagnostics","nodeId":"worker","stateDirectory":root.path().join("state"),
        "worker":{"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":"b","controllers":[
            {"id":"a","url":"ws://127.0.0.1:1143"},{"id":"b","url":"ws://127.0.0.1:1144"}]}},
        "projects":[{"id":"fixture","workspaceRoot":root.path(),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap()).unwrap();
    // No serve: configured endpoints are observations only, never contacted.
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("info")
    };
    let request = ControllerConnectionsRequest {
        context: worker.request_context().unwrap(),
    };
    let envelope = controller_connections_envelope(&info, &request).unwrap();
    let observed =
        decode_controller_connections(worker.handle_extension(envelope.clone()), &info, &request)
            .unwrap();
    assert_eq!(observed.primary.as_deref(), Some("b"));
    assert_eq!(observed.connections.len(), 2);
    assert!(observed.connections.iter().all(|c| c.attempts == "0"));
    for payload in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!({"method":"cancel","taskId":"foreign"}),
    ] {
        let mut invalid = envelope.clone();
        invalid.payload = payload;
        assert!(matches!(
            worker.handle_extension(invalid).result,
            ExtensionResult::Error {
                code: ExtensionError::InvalidRequest,
                ..
            }
        ));
    }
    let mut stale = envelope.clone();
    stale.context.journal_incarnation = "foreign".into();
    assert!(matches!(
        worker.handle_extension(stale).result,
        ExtensionResult::Error {
            code: ExtensionError::ScopeMismatch,
            ..
        }
    ));
    let mut future = envelope;
    future.contract.version = 2;
    assert!(matches!(
        worker.handle_extension(future).result,
        ExtensionResult::Error {
            code: ExtensionError::UnsupportedContract,
            ..
        }
    ));
    assert!(
        matches!(worker.handle(LocalRequest::ListTasks),LocalResponse::Tasks {tasks} if tasks.is_empty())
    );
    assert_eq!(observed.rpc.admitted, "0");
}

#[cfg(unix)]
#[tokio::test]
async fn local_only_worker_exposes_an_empty_scoped_observation_through_real_ipc() {
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let worker=Worker::open(serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"local-diagnostics","nodeId":"worker","stateDirectory":root.path().join("state"),
        "worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root.path(),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap()).unwrap();
    let address =
        winbridge_transport::endpoint(&worker.config.state_directory, &worker.config.instance_id)
            .unwrap();
    let service = tokio::spawn(winbridge_worker::serve(worker.clone()));
    let request = ControllerConnectionsRequest {
        context: worker.request_context().unwrap(),
    };
    let page = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(page) = winbridge_transport::controller_connections(&address, &request).await
            {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.stop();
    service.await.unwrap().unwrap();
    assert!(page.primary.is_none() && page.connections.is_empty());
    assert_eq!(
        (
            page.rpc.admitted.as_str(),
            page.rpc.completed.as_str(),
            page.rpc.in_flight
        ),
        ("0", "0", 0)
    );
}
