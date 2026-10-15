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
        if request.extension_version != 1 || request.contract != operation_contract() {
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
            Ok::<_, String>(ExtensionResponse {
                extension_version: 1,
                contract: request.contract.clone(),
                context: Some(context),
                result: ExtensionResult::Data {
                    payload: serde_json::to_value(page).map_err(|e| e.to_string())?,
                },
            })
        })();
        result.unwrap_or_else(|message| reject(ExtensionError::QueryFailed, message))
    }
}
