use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttentionSeverity {
    Info,
    Warning,
    Error,
}

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

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionPage {
    pub records: Vec<AttentionRecord>,
    pub next_sequence: u64,
    pub has_more: bool,
    pub pending_count: u64,
}
