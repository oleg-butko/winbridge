//! Only known readonly handlers are registered. No generic command dispatcher.
use crate::Worker;
use winbridge_protocol::*;
impl Worker {
    pub fn handle_extension(&self, request: ExtensionRequest) -> ExtensionResponse {
        if request.contract.id.len() > 128 || request.contract.id.is_empty() {
            return ExtensionResponse::rejected(
                operation_contract(),
                ExtensionError::InvalidRequest,
                "Invalid contract reference".into(),
            );
        }
        let reject =
            |code, message| ExtensionResponse::rejected(request.contract.clone(), code, message);
        if request.extension_version != 1
            || (request.contract != operation_contract()
                && request.contract != controller_connections_contract()
                && request.contract != operation_attribution_contract()
                && request.contract != operation_cancellation_contract())
        {
            return reject(
                ExtensionError::UnsupportedContract,
                "Unknown readonly contract/version; no handler or fallback".into(),
            );
        }
        let result = (|| {
            let store = self.store()?;
            let context = RequestContext {
                instance_id: self.config.instance_id.clone(),
                journal_incarnation: store.journal_authority().incarnation.clone(),
            };
            if context != request.context {
                return Ok(reject(
                    ExtensionError::ScopeMismatch,
                    "Readonly query belongs to another journal scope; cursor not rebound".into(),
                ));
            }
            let payload = if request.contract == operation_cancellation_contract() {
                if !request.payload.is_object() {
                    return Ok(reject(
                        ExtensionError::InvalidRequest,
                        "Cancellation query must be an object".into(),
                    ));
                }
                let query: OperationAttributionQuery =
                    match serde_json::from_value(request.payload.clone()) {
                        Ok(q) => q,
                        Err(_) => {
                            return Ok(reject(
                                ExtensionError::InvalidRequest,
                                "Invalid cancellation query".into(),
                            ))
                        }
                    };
                serde_json::to_value(store.operation_cancellation(
                    &context,
                    &query,
                    &self.generation,
                )?)
                .map_err(|e| e.to_string())?
            } else if request.contract == operation_attribution_contract() {
                if !request.payload.is_object() {
                    return Ok(reject(
                        ExtensionError::InvalidRequest,
                        "Attribution query must be an object".into(),
                    ));
                }
                let query: OperationAttributionQuery =
                    match serde_json::from_value(request.payload.clone()) {
                        Ok(q) => q,
                        Err(_) => {
                            return Ok(reject(
                                ExtensionError::InvalidRequest,
                                "Invalid attribution query".into(),
                            ))
                        }
                    };
                serde_json::to_value(store.operation_attribution(
                    &context,
                    &query,
                    &self.generation,
                )?)
                .map_err(|e| e.to_string())?
            } else if request.contract == controller_connections_contract() {
                if !request.payload.is_object()
                    || serde_json::from_value::<ControllerConnectionsQuery>(request.payload.clone())
                        .is_err()
                {
                    return Ok(reject(
                        ExtensionError::InvalidRequest,
                        "Connection query must be an empty object; no command dispatch".into(),
                    ));
                }
                serde_json::to_value(self.controller_connections_snapshot(context.clone())?)
                    .map_err(|e| e.to_string())?
            } else {
                let query: OperationQuery = match serde_json::from_value(request.payload.clone()) {
                    Ok(query) => query,
                    Err(_) => {
                        return Ok(reject(
                            ExtensionError::InvalidRequest,
                            "Invalid typed operation query; no handler dispatch".into(),
                        ))
                    }
                };
                let page = store.operation_page(&context, &query)?;
                serde_json::to_value(page).map_err(|e| e.to_string())?
            };
            Ok::<_, String>(ExtensionResponse {
                extension_version: 1,
                contract: request.contract.clone(),
                context: Some(context),
                result: ExtensionResult::Data { payload },
            })
        })();
        result.unwrap_or_else(|message| reject(ExtensionError::QueryFailed, message))
    }
}
