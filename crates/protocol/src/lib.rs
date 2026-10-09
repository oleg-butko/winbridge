//! Shared wire contracts and the local worker read model.

use serde::{Deserialize, Serialize};

mod attention;
pub use attention::*;
mod history;
pub use history::*;
mod maintenance;
pub use maintenance::*;

mod session;
pub use session::*;
mod snapshot;
pub use snapshot::*;
mod workflow;
pub use workflow::*;

pub const PROTOCOL_VERSION: u16 = 1;

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Controller,
    Worker,
    Desktop,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    TaskSubmit,
    TaskCancel,
    WorkspaceImport,
    LogRead,
    AttentionRequest,
    AttentionTargets,
    HistoryRead,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
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
    #[serde(default)]
    pub build: Option<BuildIdentity>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuildIdentity {
    pub version: String,
    pub source_commit: Option<String>,
    pub source_dirty: bool,
    pub source_manifest_sha256: Option<String>,
    pub build_id: String,
}
pub fn build_identity() -> BuildIdentity {
    let optional = |v: &str| {
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    };
    BuildIdentity {
        version: env!("CARGO_PKG_VERSION").into(),
        source_commit: optional(env!("WINBRIDGE_SOURCE_COMMIT")),
        source_dirty: env!("WINBRIDGE_SOURCE_DIRTY") == "true",
        source_manifest_sha256: optional(env!("WINBRIDGE_SOURCE_MANIFEST_SHA256")),
        build_id: env!("WINBRIDGE_BUILD_ID").into(),
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSubmission {
    pub protocol_version: u16,
    pub request_id: String,
    pub project_id: String,
    pub idempotency_key: String,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub snapshot_id: Option<String>,
    pub command: CommandSource,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
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

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
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

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
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

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedCommand {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub timeout_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEvent {
    pub sequence: u64,
    pub occurred_at_ms: u64,
    pub status: TaskStatus,
    pub summary: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub task_id: String,
    pub submission: TaskSubmission,
    pub resolved_command: ResolvedCommand,
    #[serde(default)]
    pub snapshot: Option<VerifiedSnapshotRef>,
    pub status: TaskStatus,
    pub accepted_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub process_id: Option<u32>,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub cancel_requested: bool,
    pub output_bytes: u64,
    pub output_truncated: bool,
    #[serde(default)]
    pub output_removed_at_ms: Option<u64>,
    #[serde(default)]
    pub removed_output_bytes: u64,
    pub events: Vec<JournalEvent>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub task_id: String,
    pub project_id: String,
    pub status: TaskStatus,
    pub accepted_at_ms: u64,
}
impl From<TaskRecord> for TaskSummary {
    fn from(task: TaskRecord) -> Self {
        Self {
            task_id: task.task_id,
            project_id: task.submission.project_id,
            status: task.status,
            accepted_at_ms: task.accepted_at_ms,
        }
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub sequence: u64,
    pub stream: OutputStream,
    pub offset: u64,
    /// Raw bytes: UI decodes each stream incrementally, without splitting UTF-8 characters.
    pub bytes: Vec<u8>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeInfo {
    pub id: String,
    pub command: ResolvedCommand,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub id: String,
    pub workspace_root: String,
    pub allow_direct_commands: bool,
    pub recipes: Vec<RecipeInfo>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerInfo {
    pub protocol_version: u16,
    pub instance_id: String,
    pub node_id: String,
    pub generation: String,
    pub platform: String,
    pub runtime_validation: String,
    pub projects: Vec<ProjectInfo>,
    #[serde(default)]
    pub controller_connection: Option<ControllerConnection>,
    #[serde(default)]
    pub snapshot_import_available: bool,
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

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalRequest {
    GetRuntimeSettings,
    SaveRuntimeSettings {
        request: SaveRuntimeSettings,
    },
    PreviewOutputCleanup {
        query: OutputCleanupQuery,
    },
    ApplyOutputCleanup {
        request: ApplyOutputCleanup,
    },
    History {
        query: HistoryQuery,
    },
    PrepareTaskDiagnostic {
        request: DiagnosticRequest,
    },
    StorageUsage,
    ListAttentionEvents {
        #[serde(rename = "afterSequence")]
        after_sequence: u64,
        #[serde(rename = "pendingOnly")]
        pending_only: bool,
    },
    AcknowledgeAttentionEvent {
        #[serde(rename = "attentionId")]
        attention_id: String,
    },
    RequestAttention {
        request: AttentionSubmission,
    },
    ListAttention {
        #[serde(rename = "afterSequence")]
        after_sequence: u64,
        #[serde(rename = "pendingOnly")]
        pending_only: bool,
    },
    AcknowledgeAttention {
        #[serde(rename = "attentionId")]
        attention_id: String,
    },
    SubmitWorkflow {
        request: Box<WorkflowSubmission>,
    },
    ListWorkflows,
    GetWorkflow {
        #[serde(rename = "workflowId")]
        workflow_id: String,
    },
    CancelWorkflow {
        #[serde(rename = "workflowId")]
        workflow_id: String,
    },
    ListApplications,
    GetApplication {
        #[serde(rename = "applicationId")]
        application_id: String,
    },
    StopApplication {
        #[serde(rename = "applicationId")]
        application_id: String,
    },
    ReadApplicationLog {
        #[serde(rename = "applicationId")]
        application_id: String,
        cursor: Option<LogCursor>,
        #[serde(rename = "maxBytes")]
        max_bytes: u32,
    },
    ListSnapshots {
        #[serde(rename = "projectId")]
        project_id: String,
        offset: u64,
    },
    ImportSnapshot {
        request: ImportSubmission,
    },
    ListImports,
    GetImport {
        #[serde(rename = "importId")]
        import_id: String,
    },
    CancelImport {
        #[serde(rename = "importId")]
        import_id: String,
    },
    PreviewSnapshot {
        #[serde(rename = "projectId")]
        project_id: String,
        #[serde(rename = "snapshotId")]
        snapshot_id: String,
        offset: u64,
    },
    Info,
    ControllerStatus,
    Submit {
        request: TaskSubmission,
    },
    ListTasks,
    GetTask {
        #[serde(rename = "taskId")]
        task_id: String,
    },
    ReadOutput {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "afterSequence")]
        after_sequence: u64,
    },
    Cancel {
        #[serde(rename = "taskId")]
        task_id: String,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalResponse {
    RuntimeSettings {
        settings: RuntimeSettings,
    },
    SettingsSaved {
        saved: SettingsSaved,
    },
    OutputCleanupPreview {
        preview: OutputCleanupPreview,
    },
    OutputCleanupResult {
        result: OutputCleanupResult,
    },
    History {
        page: HistoryPage,
    },
    Diagnostic {
        preview: DiagnosticPreview,
    },
    StorageUsage {
        usage: StorageUsage,
    },
    AttentionEvent {
        event: AttentionEvent,
    },
    AttentionEvents {
        page: AttentionEventPage,
    },
    Attention {
        attention: AttentionRecord,
    },
    AttentionList {
        page: AttentionPage,
    },
    Workflow {
        workflow: Box<WorkflowRecord>,
    },
    Workflows {
        workflows: Vec<WorkflowRecord>,
    },
    Application {
        application: ApplicationRecord,
    },
    Applications {
        applications: Vec<ApplicationRecord>,
    },
    Log {
        page: LogPage,
    },
    Snapshots {
        page: PublishedSnapshotPage,
    },
    Import {
        import: Box<ImportRecord>,
    },
    Imports {
        imports: Vec<ImportRecord>,
    },
    SnapshotPreview {
        preview: SnapshotPreview,
    },
    Info {
        info: Box<WorkerInfo>,
    },
    Task {
        task: Box<TaskRecord>,
    },
    Tasks {
        tasks: Vec<TaskSummary>,
    },
    Output {
        chunks: Vec<OutputChunk>,
    },
    Error {
        message: String,
    },
    ControllerStatus {
        status: ControllerStatus,
    },
}

impl LocalRequest {
    pub fn local_maintenance_only(&self) -> bool {
        matches!(
            self,
            Self::GetRuntimeSettings
                | Self::SaveRuntimeSettings { .. }
                | Self::PreviewOutputCleanup { .. }
                | Self::ApplyOutputCleanup { .. }
        )
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReport {
    pub schema_version: u16,
    pub platform: String,
    pub task: TaskRecord,
    pub output: Vec<OutputChunk>,
    pub output_complete: bool,
}

/// Desktop form state only: restoring a draft never submits a worker task.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandDraft {
    pub worker_instance_id: String,
    pub project_id: String,
    pub workspace_root: String,
    pub recipe_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub snapshot_id: Option<String>,
    pub executable: String,
    /// Preserve the text exactly, including an incomplete JSON edit.
    pub arguments_json: String,
    pub working_directory: String,
    pub timeout_seconds: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopDraftState {
    pub schema_version: u16,
    /// Oldest first; the last matching worker entry restores its project choice.
    pub drafts: Vec<CommandDraft>,
    #[serde(default)]
    pub delivery_drafts: Vec<DeliveryDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflow_profiles: Vec<WorkflowProfile>,
    #[serde(default)]
    pub notification_preferences: NotificationPreferences,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notification_cursors: Vec<NotificationCursor>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationPreferences {
    pub popup_enabled: bool,
    pub sound_enabled: bool,
    pub volume_percent: u8,
    pub minimum_severity: AttentionSeverity,
    pub minimum_interval_seconds: u32,
    pub quiet_hours_enabled: bool,
    pub quiet_start_minute: u16,
    pub quiet_end_minute: u16,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            popup_enabled: true,
            sound_enabled: true,
            volume_percent: 25,
            minimum_severity: AttentionSeverity::Warning,
            minimum_interval_seconds: 30,
            quiet_hours_enabled: false,
            quiet_start_minute: 22 * 60,
            quiet_end_minute: 8 * 60,
        }
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationCursor {
    pub worker_instance_id: String,
    pub node_id: String,
    pub after_sequence: u64,
}

/// Native shell observations, never substituted for worker task acceptance.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionStatus {
    pub tray_ready: bool,
    pub worker_connected: bool,
    pub last_confirmed_at_ms: Option<u64>,
    pub pending_count: Option<u64>,
    pub active_task_count: Option<u64>,
    pub error: Option<String>,
    pub latest_attention: Option<AttentionEvent>,
    pub open_task_id: Option<String>,
    #[serde(default)]
    pub open_target: Option<AttentionTarget>,
    pub open_task_sequence: u64,
    pub quit_request_sequence: u64,
    pub preferences: NotificationPreferences,
}

/// Visible progress of the explicitly enabled isolated desktop validation.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopValidationStatus {
    pub enabled: bool,
    pub phase: String,
    pub step: String,
    pub completed: bool,
    pub error: Option<String>,
}

/// Named editable desktop profiles; an empty name is the unfinished draft.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowProfile {
    pub worker_instance_id: String,
    pub project_id: String,
    pub workspace_root: String,
    pub name: String,
    pub parameters_json: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryDraft {
    pub worker_instance_id: String,
    pub project_id: String,
    pub workspace_root: String,
    pub snapshot_id: Option<String>,
}
