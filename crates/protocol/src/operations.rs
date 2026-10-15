//! Readonly operation pages; independent of frozen worker RPC 1/2.
use crate::*;
use serde::{Deserialize, Serialize};
pub const OPERATION_READ_MODEL: &str = "winbridge.operation-read-model";
pub const EXTENSION_TRANSPORT: &str = "winbridge.extensions";
pub fn operation_contract() -> ContractRef {
    ContractRef {
        id: OPERATION_READ_MODEL.into(),
        version: 1,
    }
}

macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all="camelCase", deny_unknown_fields)]
    $item
)*}; }
dto! {
    pub struct OperationRef { pub context: RequestContext, pub kind: HistoryKind, pub operation_id: String }
    #[derive(PartialEq, Eq)]
    pub struct AdmissionWatermarks { pub tasks: String, pub imports: String, pub workflows: String, pub applications: String }
    pub struct OperationCursor { pub context: RequestContext, pub query_sha256: String, pub watermarks: AdmissionWatermarks, pub before: HistoryCursor }
    pub struct OperationQuery { pub project_id: Option<String>, pub kind: Option<HistoryKind>, pub search: String, pub limit: u16, pub cursor: Option<OperationCursor> }
    pub struct OperationPageRequest { pub context: RequestContext, pub query: OperationQuery }
    pub struct OperationResource { pub lease_id: String, pub owner: OperationRef, pub state: ObservedLeaseState, pub scope: ObservedLeaseScope, pub revision: String, pub reason: Option<String> }
    pub struct OperationObservation {
        pub reference: OperationRef, pub project_id: String, pub state: OperationState,
        pub revision: String, pub label: String, pub reason: Option<String>,
        pub text_truncated: bool, pub accepted_at_ms: u64, pub finished_at_ms: Option<u64>,
        pub last_event_sequence: Option<String>, pub related: Vec<OperationRef>,
        pub resources: Vec<OperationResource>, pub resources_truncated: bool,
    }
    pub struct OperationPage {
        pub schema_version: u16, pub context: RequestContext, pub observation_id: String,
        pub observed_at_ms: u64, pub records: Vec<OperationObservation>,
        pub next_cursor: Option<OperationCursor>, pub has_more: bool, pub read_only: bool,
    }
    pub struct ExtensionRequest {
        pub extension_version: u16, pub contract: ContractRef, pub context: RequestContext,
        #[cfg_attr(feature="typescript", ts(type="unknown"))]
        pub payload: serde_json::Value,
    }
    pub struct ExtensionResponse { pub extension_version: u16, pub contract: ContractRef, pub context: Option<RequestContext>, pub result: ExtensionResult }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "family", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationState {
    Execution { status: TaskStatus },
    Application { status: ApplicationStatus },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedLeaseState {
    Held,
    Uncertain,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedLeaseScope {
    ProjectMutation,
    ApplicationLifetime,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionResult {
    Data {
        #[cfg_attr(feature = "typescript", ts(type = "unknown"))]
        payload: serde_json::Value,
    },
    Error {
        code: ExtensionError,
        message: String,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionError {
    UnsupportedContract,
    InvalidRequest,
    ScopeMismatch,
    QueryFailed,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionSessionMessage {
    ExtensionRequest {
        id: String,
        request: ExtensionRequest,
    },
    ExtensionResponse {
        id: String,
        response: ExtensionResponse,
    },
}
impl ExtensionResponse {
    pub fn rejected(contract: ContractRef, code: ExtensionError, message: String) -> Self {
        Self {
            extension_version: 1,
            contract,
            context: None,
            result: ExtensionResult::Error { code, message },
        }
    }
    pub fn validate_for(&self, request: &ExtensionRequest) -> Result<(), String> {
        if self.extension_version != 1
            || self.contract != request.contract
            || self
                .context
                .as_ref()
                .is_some_and(|context| context != &request.context)
            || (matches!(self.result, ExtensionResult::Data { .. })
                && self.context.as_ref() != Some(&request.context))
        {
            return Err("Extension reply contract/scope mismatch; no fallback or rebind".into());
        }
        Ok(())
    }
}
pub fn decode_operation_page(
    response: ExtensionResponse,
    request: &OperationPageRequest,
) -> Result<OperationPage, String> {
    response.validate_for(&ExtensionRequest {
        extension_version: 1,
        contract: operation_contract(),
        context: request.context.clone(),
        payload: serde_json::Value::Null,
    })?;
    match response.result {
        ExtensionResult::Error { code, message } => {
            Err(format!("Readonly query {code:?}: {message}"))
        }
        ExtensionResult::Data { payload } => {
            let page: OperationPage =
                serde_json::from_value(payload).map_err(|_| "Unsupported operation page format")?;
            fn decimal(value: &str) -> bool {
                !value.is_empty()
                    && (value.len() == 1 || !value.starts_with('0'))
                    && value.bytes().all(|b| b.is_ascii_digit())
                    && value.parse::<i64>().is_ok()
            }
            fn key(record: &OperationObservation) -> (u64, &str, &str) {
                (
                    record.accepted_at_ms,
                    kind_key(record.reference.kind),
                    &record.reference.operation_id,
                )
            }
            fn kind_key(kind: HistoryKind) -> &'static str {
                match kind {
                    HistoryKind::Task => "task",
                    HistoryKind::Import => "import",
                    HistoryKind::Workflow => "workflow",
                    HistoryKind::Application => "application",
                }
            }
            if page.schema_version != 1
                || page.records.len() > 50
                || page.context != request.context
                || page.records.len() > usize::from(request.query.limit)
                || page.has_more != page.next_cursor.is_some()
                || page
                    .records
                    .windows(2)
                    .any(|rows| key(&rows[0]) <= key(&rows[1]))
                || request.query.cursor.as_ref().is_some_and(|cursor| {
                    page.records.first().is_some_and(|r| {
                        key(r)
                            >= (
                                cursor.before.created_at_ms,
                                kind_key(cursor.before.kind),
                                cursor.before.operation_id.as_str(),
                            )
                    })
                })
                || page.next_cursor.as_ref().is_some_and(|c| {
                    c.context != request.context
                        || request
                            .query
                            .cursor
                            .as_ref()
                            .is_some_and(|previous| previous.watermarks != c.watermarks)
                        || operation_query_sha256(&request.query).ok().as_ref()
                            != Some(&c.query_sha256)
                        || [
                            &c.watermarks.tasks,
                            &c.watermarks.imports,
                            &c.watermarks.workflows,
                            &c.watermarks.applications,
                        ]
                        .iter()
                        .any(|s| !decimal(s))
                        || !page.records.last().is_some_and(|r| {
                            r.reference.kind == c.before.kind
                                && r.reference.operation_id == c.before.operation_id
                                && r.accepted_at_ms == c.before.created_at_ms
                        })
                })
                || page.records.iter().any(|r| {
                    r.reference.context != request.context
                        || r.reference.operation_id.is_empty()
                        || r.label.chars().count() > 256
                        || r.reason.as_ref().is_some_and(|s| s.chars().count() > 512)
                        || r.resources.len() > 5
                        || !decimal(&r.revision)
                        || r.last_event_sequence.as_ref().is_some_and(|s| !decimal(s))
                        || request
                            .query
                            .project_id
                            .as_ref()
                            .is_some_and(|p| p != &r.project_id)
                        || request.query.kind.is_some_and(|k| k != r.reference.kind)
                        || r.related.iter().any(|link| link.context != request.context)
                        || r.resources.iter().any(|res| {
                            res.owner.context != request.context
                                || !decimal(&res.revision)
                                || res.reason.as_ref().is_some_and(|s| s.chars().count() > 256)
                        })
                        || !matches!(
                            (&r.reference.kind, &r.state),
                            (HistoryKind::Application, OperationState::Application { .. })
                                | (
                                    HistoryKind::Task | HistoryKind::Import | HistoryKind::Workflow,
                                    OperationState::Execution { .. }
                                )
                        )
                })
            {
                return Err("Inconsistent/foreign operation page; no cursor rebind".into());
            }
            Ok(page)
        }
    }
}
pub fn operation_query_sha256(query: &OperationQuery) -> Result<String, String> {
    canonical_evidence_json(&serde_json::json!({"version":1,"projectId":query.project_id,"kind":query.kind,"search":query.search})).map(|bytes|evidence_sha256(&bytes)).map_err(|e|e.to_string())
}
#[cfg(feature = "jsonschema")]
pub fn operation_schema_sha256() -> String {
    static DIGEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIGEST.get_or_init(|| evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":operation_contract(),"query":schemars::schema_for!(OperationQuery),"page":schemars::schema_for!(OperationPage)})).expect("DTO schema"))).clone()
}
