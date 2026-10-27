//! Explicit readonly IPC. No path from this route to command/verification dispatch.
use crate::Controller;
use winbridge_protocol::*;

pub fn request(
    method: &str,
    payload: serde_json::Value,
) -> Result<ControllerDiagnosticsRequest, String> {
    if !payload.is_object() {
        return Err("Diagnostic arguments must be an object; no dispatch".into());
    }
    let request = serde_json::from_value(serde_json::json!({"method":method,"request":payload}))
        .map_err(|_| "Unknown diagnostic method or invalid pinned query; no dispatch")?;
    Ok(ControllerDiagnosticsRequest {
        controller_diagnostics_version: CONTROLLER_DIAGNOSTICS_VERSION,
        request,
    })
}
impl Controller {
    pub async fn call_diagnostics(
        &self,
        request: ControllerDiagnosticsRequest,
    ) -> ControllerDiagnosticsResponse {
        if request.controller_diagnostics_version != CONTROLLER_DIAGNOSTICS_VERSION {
            return ControllerDiagnosticsResponse::error("Unsupported readonly controller diagnostics version; no worker request transmitted".into());
        }
        let method = request.request.method();
        let result = match request.request {
            ControllerDiagnosticQuery::ControllerConnections { request } => self
                .controller_connections(request)
                .await
                .map(
                    |observation| ControllerDiagnosticResult::ControllerConnections {
                        observation: Box::new(observation),
                    },
                ),
            ControllerDiagnosticQuery::OperationAttribution { request } => self
                .operation_attribution(request)
                .await
                .map(
                    |observation| ControllerDiagnosticResult::OperationAttribution {
                        observation: Box::new(observation),
                    },
                ),
            ControllerDiagnosticQuery::OperationCancellation { request } => self
                .operation_cancellation(request)
                .await
                .map(
                    |observation| ControllerDiagnosticResult::OperationCancellation {
                        observation: Box::new(observation),
                    },
                ),
        };
        self.activity.record(serde_json::json!({"event":"diagnostic_query","origin":"local_ipc","method":method,"outcome":if result.is_ok(){"observed"}else{"error"}}));
        ControllerDiagnosticsResponse {
            controller_diagnostics_version: CONTROLLER_DIAGNOSTICS_VERSION,
            result: result.unwrap_or_else(|message| ControllerDiagnosticResult::Error { message }),
        }
    }
}
#[cfg(unix)]
pub async fn call(
    address: &str,
    request: &ControllerDiagnosticsRequest,
) -> Result<ControllerDiagnosticsResponse, String> {
    if request.controller_diagnostics_version != CONTROLLER_DIAGNOSTICS_VERSION {
        return Err("Unsupported controller diagnostics request version; no transmission".into());
    }
    tokio::time::timeout(winbridge_transport::IPC_TIMEOUT, async {
        let mut stream = tokio::net::UnixStream::connect(address)
            .await
            .map_err(|e| e.to_string())?;
        winbridge_transport::write_frame(
            &mut stream,
            &serde_json::to_vec(request).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
        let bytes = winbridge_transport::read_frame(&mut stream)
            .await
            .map_err(|e| e.to_string())?;
        let response: ControllerDiagnosticsResponse = serde_json::from_slice(&bytes)
            .map_err(|_| "Unsupported controller diagnostics reply; no legacy fallback")?;
        response.validate_for(request)?;
        Ok(response)
    })
    .await
    .map_err(|_| "Controller diagnostics IPC timeout; no command submitted")?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unsupported_schema_scope_and_version_stop_before_extension_or_command() {
        for method in [
            "controller_connections",
            "operation_attribution",
            "operation_cancellation",
        ] {
            for fault in 0..5 {
                let root = tempfile::tempdir().unwrap();
                let c=Controller::new(serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"diagnostics","nodeId":"controller","stateDirectory":root.path().join("state"),"controller":{"listenAddress":"127.0.0.1:1143","workerNodeId":"worker"},"projects":[]})).unwrap()).unwrap();
                let (sender, mut calls) = winbridge_transport::websocket::rpc_channel();
                c.state.lock().unwrap().active = Some(crate::Session {
                    sender,
                    id: "session".into(),
                    ready: true,
                });
                let context = RequestContext {
                    instance_id: "diagnostics".into(),
                    journal_incarnation: "journal".into(),
                };
                let mut payload = if method == "controller_connections" {
                    serde_json::json!({"context":context})
                } else {
                    serde_json::json!({"context":context,"query":{"kind":"task","operationId":"task"}})
                };
                if fault == 4 {
                    payload["context"]["instanceId"] = "another-pair".into();
                }
                let mut request = super::request(method, payload).unwrap();
                if fault == 3 {
                    request.controller_diagnostics_version = 2;
                }
                let cloned = c.clone();
                let pending = tokio::spawn(async move { cloned.call_diagnostics(request).await });
                if !matches!(fault, 3 | 4) {
                    let call = calls.recv().await.unwrap();
                    assert!(matches!(
                        call.request,
                        WireRequest::Legacy(LocalRequest::Info)
                    ));
                    let mut info: WorkerInfo = serde_json::from_value(
                        serde_json::from_str::<serde_json::Value>(include_str!(
                            "../../../tests/fixtures/legacy-v1/worker-info.json"
                        ))
                        .unwrap()["info"]
                            .clone(),
                    )
                    .unwrap();
                    info.build = Some(build_identity());
                    info.fenced_requests = Some(FencedRequestInfo {
                        request_contract_version: 2,
                        context: context.clone(),
                        legacy_mutations_allowed: false,
                    });
                    info.contract_catalog = Some(
                        serde_json::to_value(ContractCatalog::implemented(build_identity()))
                            .unwrap(),
                    );
                    info.contract_schemas = Some(
                        serde_json::to_value(implemented_contract_schemas(build_identity()))
                            .unwrap(),
                    );
                    match fault {
                        0 => info.contract_catalog = None,
                        1 => {
                            let contract = match method {
                                "controller_connections" => CONTROLLER_CONNECTIONS_CONTRACT,
                                "operation_attribution" => OPERATION_ATTRIBUTION_CONTRACT,
                                _ => OPERATION_CANCELLATION_CONTRACT,
                            };
                            let schemas = info.contract_schemas.as_mut().unwrap()["schemas"]
                                .as_array_mut()
                                .unwrap();
                            schemas
                                .iter_mut()
                                .find(|s| s["contract"]["id"] == contract)
                                .unwrap()["sha256"] = "0".repeat(64).into();
                        }
                        _ => {
                            info.fenced_requests
                                .as_mut()
                                .unwrap()
                                .context
                                .journal_incarnation = "new".into()
                        }
                    }
                    call.reply
                        .send(Ok(WireResponse::Legacy(Box::new(LocalResponse::Info {
                            info: Box::new(info),
                        }))))
                        .unwrap();
                }
                let response = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(matches!(
                    response.result,
                    ControllerDiagnosticResult::Error { .. }
                ));
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(10), calls.recv())
                        .await
                        .is_err(),
                    "Unexpected dispatch after {method} fault {fault}"
                );
            }
        }
    }
    #[test]
    fn parsing_rejects_commands_arrays_and_extra_nested_fields() {
        let value = serde_json::json!({"context":{"instanceId":"pair","journalIncarnation":"journal"},"query":{"kind":"task","operationId":"task"}});
        assert!(request("operation_attribution", value.clone()).is_ok());
        assert!(request("cancel", value.clone()).is_err());
        assert!(request("operation_attribution", serde_json::json!([])).is_err());
        let mut extra = value;
        extra["query"]["cancel"] = true.into();
        assert!(request("operation_cancellation", extra).is_err());
    }
    #[tokio::test]
    async fn old_controller_reply_is_not_reinterpreted_as_a_diagnostic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ipc");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = winbridge_transport::read_frame(&mut stream).await.unwrap();
            winbridge_transport::write_frame(
                &mut stream,
                br#"{"kind":"error","message":"old controller"}"#,
            )
            .await
            .unwrap();
        });
        let request = request(
            "controller_connections",
            serde_json::json!({"context":{"instanceId":"pair","journalIncarnation":"journal"}}),
        )
        .unwrap();
        assert!(call(path.to_str().unwrap(), &request)
            .await
            .unwrap_err()
            .contains("no legacy fallback"));
        server.await.unwrap();
    }
}
