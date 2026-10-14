// Frozen WorkerInfo reader from a42cd7e; do not add current fields.
use serde::{Deserialize, Serialize};
use winbridge_protocol::*;
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenV1WorkerInfo {
    pub protocol_version: u16,
    pub instance_id: String,
    pub node_id: String,
    pub generation: String,
    pub platform: String,
    pub runtime_validation: String,
    pub projects: Vec<ProjectInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_handlers: Option<Vec<CommandHandlerInfo>>,
    #[serde(default)]
    pub controller_connection: Option<ControllerConnection>,
    #[serde(default)]
    pub snapshot_import_available: bool,
    #[serde(default)]
    pub snapshot_delta_available: bool,
    #[serde(default)]
    pub snapshot_resume_available: bool,
    #[serde(default)]
    pub workflow_available: bool,
    #[serde(default)]
    pub workflow_features: Vec<String>,
    #[serde(default)]
    pub attention_available: bool,
    #[serde(default)]
    pub attention_targets_available: bool,
    #[serde(default)]
    pub history_available: bool,
    #[serde(default)]
    pub maintenance_available: bool,
    #[serde(default)]
    pub maintenance_error: Option<String>,
    #[serde(default)]
    pub journal_recovery_required: Option<String>,
    /// Queued/running tasks across the entire journal; None when unavailable or stale.
    #[serde(default)]
    pub active_task_count: Option<u64>,
    #[serde(default)]
    pub build: Option<BuildIdentity>,
}
