use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Task,
    Import,
    Workflow,
    Application,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryCursor {
    pub created_at_ms: u64,
    pub kind: HistoryKind,
    pub operation_id: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryQuery {
    pub project_id: Option<String>,
    pub kind: Option<HistoryKind>,
    pub status: Option<String>,
    pub search: String,
    pub before: Option<HistoryCursor>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub operation_id: String,
    pub kind: HistoryKind,
    pub project_id: String,
    pub label: String,
    pub status: String,
    pub created_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub workflow_id: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub records: Vec<HistoryEntry>,
    pub next_cursor: Option<HistoryCursor>,
    pub has_more: bool,
    pub observed_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticRequest {
    pub task_id: String,
    pub include_command: bool,
    pub include_output: bool,
    pub redact_values: Vec<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPreview {
    pub task_id: String,
    pub observed_at_ms: u64,
    pub json: String,
    pub command_included: bool,
    pub output_included: bool,
    pub output_truncated: bool,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub task_count: u64,
    pub import_count: u64,
    pub workflow_count: u64,
    pub application_count: u64,
    pub attention_count: u64,
    pub pending_attention_count: u64,
    pub stored_output_bytes: u64,
    pub journal_bytes: u64,
    pub wal_bytes: u64,
    pub output_limit_bytes_per_task: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDiagnostic {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
