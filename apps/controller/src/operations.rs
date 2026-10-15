use crate::Controller;
use winbridge_protocol::*;
impl Controller {
    pub async fn operation_page(
        &self,
        request: OperationPageRequest,
    ) -> Result<OperationPage, String> {
        winbridge_core::operation_query::validate(&request.context, &request.query)?;
        if request.context.instance_id != self.config.instance_id {
            return Err("Readonly query instance differs; no transmission".into());
        }
        let sender = {
            let state = self.state.lock().map_err(|_| "Controller state poisoned")?;
            state
                .active
                .as_ref()
                .filter(|s| s.ready)
                .ok_or("Worker offline; operation query not transmitted")?
                .sender
                .clone()
        };
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Missing readonly provider metadata".into());
        };
        if !matches!(interpret_catalog(&info),CatalogObservation::Known(c) if c.supports(EXTENSION_TRANSPORT,1)&&c.supports(OPERATION_READ_MODEL,1))
        {
            return Err(
                "Worker does not advertise operation-read-model 1; no dispatch or legacy fallback"
                    .into(),
            );
        }
        if !info
            .fenced_requests
            .as_ref()
            .is_some_and(|f| f.context == request.context)
        {
            return Err("Readonly query scope changed; start a new view explicitly".into());
        }
        if !interpret_contract_schemas(&info).is_some_and(|schemas| {
            schemas.schemas.iter().any(|s| {
                s.contract == operation_contract() && s.sha256 == operation_schema_sha256()
            })
        }) {
            return Err("Readonly query schema differs; no dispatch".into());
        }
        let envelope = ExtensionRequest {
            extension_version: 1,
            contract: operation_contract(),
            context: request.context.clone(),
            payload: serde_json::to_value(&request.query).map_err(|e| e.to_string())?,
        };
        let response = sender.call_extension(envelope).await?;
        decode_operation_page(response, &request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};
    #[tokio::test]
    async fn missing_contract_or_changed_schema_is_refused_before_extension_or_legacy_dispatch() {
        for missing in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let config=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"pages","nodeId":"controller","stateDirectory":temp.path().join("controller"),"controller":{"listenAddress":"127.0.0.1:1143","workerNodeId":"worker"},"projects":[]})).unwrap();
            let controller = Controller::new(config).unwrap();
            let (sender, mut calls) = winbridge_transport::websocket::rpc_channel();
            controller.state.lock().unwrap().active = Some(crate::Session {
                sender,
                id: "session".into(),
                ready: true,
            });
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
                context: RequestContext {
                    instance_id: "pages".into(),
                    journal_incarnation: "journal".into(),
                },
                legacy_mutations_allowed: false,
            });
            info.contract_catalog =
                Some(serde_json::to_value(ContractCatalog::implemented(build_identity())).unwrap());
            info.contract_schemas =
                Some(serde_json::to_value(implemented_contract_schemas(build_identity())).unwrap());
            if missing {
                info.contract_catalog = None;
            } else {
                info.contract_schemas.as_mut().unwrap()["schemas"][1]["sha256"] =
                    "0".repeat(64).into();
            }
            let request = OperationPageRequest {
                context: info.fenced_requests.as_ref().unwrap().context.clone(),
                query: OperationQuery {
                    project_id: None,
                    kind: None,
                    search: String::new(),
                    limit: 10,
                    cursor: None,
                },
            };
            let client = Arc::clone(&controller);
            let pending = tokio::spawn(async move { client.operation_page(request).await });
            let call = calls.recv().await.unwrap();
            assert!(matches!(
                call.request,
                WireRequest::Legacy(LocalRequest::Info)
            ));
            call.reply
                .send(Ok(WireResponse::Legacy(Box::new(LocalResponse::Info {
                    info: Box::new(info),
                }))))
                .unwrap();
            let error = tokio::time::timeout(Duration::from_millis(500), pending)
                .await
                .expect("unsupported negotiation must return without a second request")
                .unwrap()
                .unwrap_err();
            assert!(
                error.contains(if missing {
                    "does not advertise"
                } else {
                    "schema differs"
                }),
                "{error}"
            );
            assert!(matches!(
                calls.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ));
        }
    }
}
