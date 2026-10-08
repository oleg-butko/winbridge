//! Durable startup workflows, owned applications, and byte-oriented log cursors.
use crate::{CommandSource, JournalEvent, TaskStatus};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadinessProbe {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout_contains: String,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub netwatch: Option<NetwatchProbe>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetwatchProbe {
    pub instance_id: String,
    pub runtime_directory: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactSpec {
    pub name: String,
    /// Relative to the project root, outside immutable snapshots.
    pub path: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuildArtifact {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub source_commit: String,
    pub manifest_sha256: String,
    pub build_task_id: String,
    pub recorded_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowSubmission {
    pub protocol_version: u16,
    pub request_id: String,
    pub idempotency_key: String,
    pub name: String,
    pub project_id: String,
    pub snapshot_id: String,
    pub manifest_sha256: String,
    pub timeout_ms: u64,
    pub build: CommandSource,
    pub start: CommandSource,
    pub readiness: ReadinessProbe,
    /// UTF-8 application log, relative to the registered project root, outside snapshots.
    pub log_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactSpec>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Indeterminate,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub step_id: String,
    pub name: String,
    pub status: WorkflowStepStatus,
    pub operation_id: Option<String>,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub reason: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecord {
    pub workflow_id: String,
    pub submission: WorkflowSubmission,
    pub status: TaskStatus,
    pub accepted_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub cancel_requested: bool,
    pub reason: Option<String>,
    pub application_id: Option<String>,
    pub steps: Vec<WorkflowStep>,
    pub events: Vec<JournalEvent>,
    #[serde(default)]
    pub artifacts: Vec<BuildArtifact>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatus {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Exited,
    Failed,
    Indeterminate,
}
impl ApplicationStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Exited | Self::Failed | Self::Indeterminate
        )
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationRecord {
    pub application_id: String,
    pub workflow_id: String,
    pub project_id: String,
    pub task_id: String,
    pub status: ApplicationStatus,
    pub process_id: Option<u32>,
    pub created_at_ms: u64,
    pub ready_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub log_path: String,
    pub reason: Option<String>,
    pub failure_reason: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogCursor {
    pub file_identity: String,
    pub offset: u64,
    pub prefix_bytes: u16,
    pub prefix_sha256: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub application_id: String,
    pub path: String,
    pub encoding: String,
    pub bytes: Vec<u8>,
    pub cursor: LogCursor,
    pub reset_reason: Option<String>,
    pub end_of_file: bool,
}
