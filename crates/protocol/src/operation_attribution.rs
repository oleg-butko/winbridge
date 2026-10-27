//! Historical creator attribution. Never authentication or cancellation authority.
use crate::*;
use serde::{Deserialize, Serialize};
pub const OPERATION_ATTRIBUTION_CONTRACT: &str = "winbridge.operation-attribution";
pub fn operation_attribution_contract() -> ContractRef {
    ContractRef {
        id: OPERATION_ATTRIBUTION_CONTRACT.into(),
        version: 1,
    }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OperationCreator {
    Local {
        worker_generation: String,
    },
    Controller {
        worker_generation: String,
        controller_id: String,
        controller_node_id: String,
        controller_generation: String,
        session_id: String,
    },
}
impl OperationCreator {
    pub fn valid(&self) -> bool {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 80
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        };
        match self {
            Self::Local { worker_generation } => id(worker_generation),
            Self::Controller {
                worker_generation,
                controller_id,
                controller_node_id,
                controller_generation,
                session_id,
            } => [
                worker_generation,
                controller_id,
                controller_node_id,
                controller_generation,
                session_id,
            ]
            .iter()
            .all(|s| id(s)),
        }
    }
}
macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all="camelCase", deny_unknown_fields)] $item
)*}; }
dto! {
    pub struct OperationAttributionQuery {pub kind:HistoryKind,pub operation_id:String}
    pub struct OperationAttributionRequest {pub context:RequestContext,pub query:OperationAttributionQuery}
    pub struct OperationAttribution {
        pub schema_version:u16,pub reference:OperationRef,pub creator:Option<OperationCreator>,
        pub inherited_from:Option<OperationRef>,pub provider:BuildIdentity,pub worker_generation:String,pub observed_at_ms:u64,
    }
}
#[cfg(feature = "jsonschema")]
pub fn operation_attribution_schema_sha256() -> String {
    use std::sync::OnceLock;
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":operation_attribution_contract(),"query":schemars::schema_for!(OperationAttributionQuery),"observation":schemars::schema_for!(OperationAttribution)})).expect("Attribution DTO schema"))).clone()
}
#[cfg(feature = "jsonschema")]
pub fn operation_attribution_envelope(
    info: &WorkerInfo,
    request: &OperationAttributionRequest,
) -> Result<ExtensionRequest, String> {
    if !matches!(interpret_catalog(info),CatalogObservation::Known(c) if c.supports(EXTENSION_TRANSPORT,1)&&c.supports(OPERATION_ATTRIBUTION_CONTRACT,1))
        || !interpret_contract_schemas(info).is_some_and(|s| {
            s.schemas.iter().any(|s| {
                s.contract == operation_attribution_contract()
                    && s.sha256 == operation_attribution_schema_sha256()
            })
        })
    {
        return Err("Worker does not support attribution contract/schema; no fallback".into());
    }
    if !info
        .fenced_requests
        .as_ref()
        .is_some_and(|f| f.context == request.context)
    {
        return Err("Attribution scope changed; no rebind".into());
    }
    if request.query.operation_id.is_empty() || request.query.operation_id.len() > 160 {
        return Err("Invalid attribution operation ID".into());
    }
    Ok(ExtensionRequest {
        extension_version: 1,
        contract: operation_attribution_contract(),
        context: request.context.clone(),
        payload: serde_json::to_value(&request.query).map_err(|e| e.to_string())?,
    })
}
pub fn decode_operation_attribution(
    response: ExtensionResponse,
    info: &WorkerInfo,
    request: &OperationAttributionRequest,
) -> Result<OperationAttribution, String> {
    response.validate_for(&ExtensionRequest {
        extension_version: 1,
        contract: operation_attribution_contract(),
        context: request.context.clone(),
        payload: serde_json::Value::Null,
    })?;
    let page: OperationAttribution = match response.result {
        ExtensionResult::Error { code, message } => {
            return Err(format!("Attribution query {code:?}: {message}"))
        }
        ExtensionResult::Data { payload } => {
            serde_json::from_value(payload).map_err(|_| "Unknown attribution observation format")?
        }
    };
    if page.schema_version != 1
        || page.reference.context != request.context
        || page.reference.kind != request.query.kind
        || page.reference.operation_id != request.query.operation_id
        || page.worker_generation != info.generation
        || info.build.as_ref() != Some(&page.provider)
        || page.creator.as_ref().is_some_and(|c| !c.valid())
        || page.inherited_from.as_ref().is_some_and(|p| {
            p.context != request.context
                || p.kind != HistoryKind::Workflow
                || p.operation_id.is_empty()
                || p.operation_id.len() > 160
                || request.query.kind == HistoryKind::Workflow
        })
        || page.observed_at_ms > 9_007_199_254_740_991
    {
        return Err("Stale/mismatched attribution observation; no rebind".into());
    }
    Ok(page)
}
