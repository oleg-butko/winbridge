use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettings {
    pub generation: String,
    pub path: String,
    pub active_json: String,
    pub saved_json: String,
    pub saved_sha256: String,
    pub restart_required: bool,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRuntimeSettings {
    pub generation: String,
    pub request_id: String,
    pub expected_sha256: String,
    pub json: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSaved {
    pub settings: RuntimeSettings,
    pub backup_path: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputCleanupQuery {
    pub older_than_days: u16,
    pub project_id: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputCleanupEntry {
    pub task_id: String,
    pub project_id: String,
    pub bytes: u64,
    pub finished_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputCleanupPlan {
    pub before_ms: u64,
    pub project_id: Option<String>,
    pub task_ids: Vec<String>,
    pub sha256: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputCleanupPreview {
    pub plan: OutputCleanupPlan,
    pub entries: Vec<OutputCleanupEntry>,
    pub bytes: u64,
    pub has_more: bool,
    pub observed_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyOutputCleanup {
    pub idempotency_key: String,
    pub plan: OutputCleanupPlan,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputCleanupResult {
    pub idempotency_key: String,
    pub removed_tasks: u64,
    pub removed_bytes: u64,
    pub occurred_at_ms: u64,
}
