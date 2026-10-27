//! First effective cancellation intent; separate from immutable creator and outcome.
use crate::*;
use serde::{Deserialize, Serialize};
pub const OPERATION_CANCELLATION_CONTRACT: &str = "winbridge.operation-cancellation";
pub fn operation_cancellation_contract() -> ContractRef {
    ContractRef {
        id: OPERATION_CANCELLATION_CONTRACT.into(),
        version: 1,
    }
}
macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript",derive(ts_rs::TS))]
    #[derive(Clone,Debug,Deserialize,Serialize)]
    #[serde(rename_all="camelCase",deny_unknown_fields)] $item
)*}; }
dto! {
    pub struct CancellationIntent {pub requested_at_ms:u64,pub actor:Option<OperationCreator>,pub inherited_from:Option<OperationRef>}
    pub struct OperationCancellation {pub schema_version:u16,pub reference:OperationRef,pub intent:Option<CancellationIntent>,pub provider:BuildIdentity,pub worker_generation:String,pub observed_at_ms:u64}
}
#[cfg(feature = "jsonschema")]
pub fn operation_cancellation_schema_sha256() -> String {
    use std::sync::OnceLock;
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":operation_cancellation_contract(),"query":schemars::schema_for!(OperationAttributionQuery),"observation":schemars::schema_for!(OperationCancellation)})).expect("Cancellation schema"))).clone()
}
#[cfg(feature = "jsonschema")]
pub fn operation_cancellation_envelope(
    info: &WorkerInfo,
    request: &OperationAttributionRequest,
) -> Result<ExtensionRequest, String> {
    if !matches!(interpret_catalog(info),CatalogObservation::Known(c) if c.supports(EXTENSION_TRANSPORT,1)&&c.supports(OPERATION_CANCELLATION_CONTRACT,1))
        || !interpret_contract_schemas(info).is_some_and(|s| {
            s.schemas.iter().any(|s| {
                s.contract == operation_cancellation_contract()
                    && s.sha256 == operation_cancellation_schema_sha256()
            })
        })
    {
        return Err(
            "Worker does not support cancellation observation contract/schema; no fallback".into(),
        );
    }
    if !info
        .fenced_requests
        .as_ref()
        .is_some_and(|f| f.context == request.context)
        || request.query.operation_id.is_empty()
        || request.query.operation_id.len() > 160
    {
        return Err("Cancellation query scope/ID differs; no rebind".into());
    }
    Ok(ExtensionRequest {
        extension_version: 1,
        contract: operation_cancellation_contract(),
        context: request.context.clone(),
        payload: serde_json::to_value(&request.query).map_err(|e| e.to_string())?,
    })
}
pub fn decode_operation_cancellation(
    response: ExtensionResponse,
    info: &WorkerInfo,
    request: &OperationAttributionRequest,
) -> Result<OperationCancellation, String> {
    response.validate_for(&ExtensionRequest {
        extension_version: 1,
        contract: operation_cancellation_contract(),
        context: request.context.clone(),
        payload: serde_json::Value::Null,
    })?;
    let page: OperationCancellation = match response.result {
        ExtensionResult::Error { code, message } => {
            return Err(format!("Cancellation observation {code:?}: {message}"))
        }
        ExtensionResult::Data { payload } => serde_json::from_value(payload)
            .map_err(|_| "Unknown cancellation observation format")?,
    };
    let valid_parent = |p: &OperationRef| {
        p.context == request.context
            && matches!(p.kind, HistoryKind::Workflow | HistoryKind::Application)
            && !p.operation_id.is_empty()
            && p.operation_id.len() <= 160
            && !(p.kind == request.query.kind && p.operation_id == request.query.operation_id)
    };
    if page.schema_version != 1
        || page.reference.context != request.context
        || page.reference.kind != request.query.kind
        || page.reference.operation_id != request.query.operation_id
        || page.worker_generation != info.generation
        || info.build.as_ref() != Some(&page.provider)
        || page.observed_at_ms > 9_007_199_254_740_991
        || page.intent.as_ref().is_some_and(|i| {
            i.requested_at_ms > 9_007_199_254_740_991
                || i.actor.as_ref().is_some_and(|a| !a.valid())
                || i.inherited_from.as_ref().is_some_and(|p| !valid_parent(p))
        })
    {
        return Err("Stale/malformed cancellation observation; no rebind".into());
    }
    Ok(page)
}
