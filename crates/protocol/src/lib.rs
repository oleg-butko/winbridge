//! Shared contracts. Execution, transport and persistence are future milestones.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Controller,
    Worker,
    Desktop,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    TaskSubmit,
    TaskCancel,
    WorkspaceImport,
    LogRead,
    AttentionRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDescriptor {
    pub application: String,
    pub application_version: String,
    pub protocol_version: u16,
    pub component: Component,
    pub scaffold: bool,
    pub implemented_capabilities: Vec<Capability>,
    pub planned_capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSubmission {
    pub protocol_version: u16,
    pub request_id: String,
    pub project_id: String,
    pub idempotency_key: String,
    pub timeout_ms: u64,
    pub command: CommandSource,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandSource {
    Recipe {
        #[serde(rename = "recipeId")]
        recipe_id: String,
        arguments: Vec<String>,
    },
    Direct {
        executable: String,
        arguments: Vec<String>,
        /// Relative path inside the registered project. Validation belongs to core/worker.
        #[serde(rename = "workingDirectory")]
        working_directory: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    Indeterminate,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Minimal task read-model event. Full journal payloads are added with real capabilities.
pub struct TaskEvent {
    pub node_id: String,
    pub node_generation: String,
    pub sequence: u64,
    pub task_id: String,
    pub step_id: Option<String>,
    pub occurred_at: String,
    pub status: TaskStatus,
    pub summary: String,
}
