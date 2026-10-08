use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttentionSeverity {
    Info,
    Warning,
    Error,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttentionSubmission {
    pub request_id: String,
    pub idempotency_key: String,
    pub task_id: String,
    pub severity: AttentionSeverity,
    pub message: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionRecord {
    pub attention_id: String,
    pub sequence: u64,
    pub project_id: String,
    pub task_id: String,
    pub severity: AttentionSeverity,
    pub message: String,
    /// "agent_request" or "task_outcome"; never claims authenticated identity.
    pub origin: String,
    pub created_at_ms: u64,
    pub acknowledged_at_ms: Option<u64>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionPage {
    pub records: Vec<AttentionRecord>,
    pub next_sequence: u64,
    pub has_more: bool,
    pub pending_count: u64,
}

/// Additive feed: legacy task DTOs remain unchanged for older clients.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttentionTarget {
    Task {
        #[serde(rename = "taskId")]
        task_id: String,
    },
    Workflow {
        #[serde(rename = "workflowId")]
        workflow_id: String,
    },
    Application {
        #[serde(rename = "applicationId")]
        application_id: String,
        #[serde(rename = "workflowId")]
        workflow_id: String,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionEvent {
    pub attention_id: String,
    pub sequence: u64,
    pub project_id: String,
    pub target: AttentionTarget,
    pub severity: AttentionSeverity,
    pub message: String,
    pub origin: String,
    pub created_at_ms: u64,
    pub acknowledged_at_ms: Option<u64>,
}
impl From<AttentionRecord> for AttentionEvent {
    fn from(r: AttentionRecord) -> Self {
        Self {
            attention_id: r.attention_id,
            sequence: r.sequence,
            project_id: r.project_id,
            target: AttentionTarget::Task { task_id: r.task_id },
            severity: r.severity,
            message: r.message,
            origin: r.origin,
            created_at_ms: r.created_at_ms,
            acknowledged_at_ms: r.acknowledged_at_ms,
        }
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionEventPage {
    pub records: Vec<AttentionEvent>,
    pub next_sequence: u64,
    pub has_more: bool,
    pub pending_count: u64,
}
