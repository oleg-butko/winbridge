//! Update preparation contract. Not registered in legacy RPC or remote discovery.
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceManifest {
    pub schema_version: u16,
    pub source_commit: String,
    pub dirty: bool,
    pub created_at: String,
    pub includes_built_ui: bool,
    pub files: Vec<SourceFile>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAction {
    Add,
    Replace,
    Remove,
    Keep,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceChange {
    pub path: String,
    pub action: SourceAction,
    pub previous: Option<SourceFile>,
    pub incoming: Option<SourceFile>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceBlockerCode {
    LocalModified,
    LocalMissing,
    UnmanagedFile,
    PathTypeConflict,
    CaseConflict,
    LayoutChange,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceBlocker {
    pub path: String,
    pub code: SourceBlockerCode,
    pub message: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePlan {
    pub changes: Vec<SourceChange>,
    pub blockers: Vec<SourceBlocker>,
    /// Source preconditions only. Never authorizes filesystem writes or shutdown.
    pub can_apply: bool,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Requested,
    Preparing,
    SourceApplying,
    Building,
    ValidatingCandidate,
    CandidateVerified,
    Draining,
    Quiesced,
    Stopping,
    Replacing,
    VerifyingLocal,
    AwaitingController,
    Committed,
    Deferred,
    FailedBeforeCutover,
    ManualIntervention,
}

/// Internal lifecycle event. Observations come from trusted adapters, not RPC input.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum UpdateEvent {
    BeginPreparation,
    BeginSourceApply,
    SourceVerified,
    BuildSucceeded,
    CandidateAccepted,
    BeginDrain,
    Quiesce {
        #[serde(rename = "dispatchClosed")]
        dispatch_closed: bool,
        #[serde(rename = "acceptedOperations")]
        accepted_operations: u64,
        #[serde(rename = "physicalOwners")]
        physical_owners: u64,
        #[serde(rename = "uncertainOwners")]
        uncertain_owners: u64,
    },
    BeginShutdown,
    ShutdownVerified,
    FilesVerified,
    LocalHealthVerified,
    ControllerConfirmed,
    Defer,
    Fail,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSourcePin {
    /// Filename only. The installed policy chooses the source delivery directory.
    pub archive_name: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub source_commit: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstallationPin {
    pub installation_id: String,
    pub scope_sha256: String,
    pub worker_build_id: String,
    pub worker_config_sha256: String,
    pub baseline_manifest_sha256: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRequest {
    pub contract_version: u16,
    pub idempotency_key: String,
    pub source: UpdateSourcePin,
    pub expected: UpdateInstallationPin,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRecord {
    pub update_id: String,
    pub revision: u64,
    pub phase: UpdatePhase,
    pub request: UpdateRequest,
    pub accepted_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: Option<String>,
}

/// Persisted receipt for a lifecycle command, separate from the original request.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCommand {
    pub idempotency_key: String,
    pub expected_revision: u64,
    pub event: UpdateEvent,
    pub reason: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTransition {
    /// Installation-wide order; a revision is scoped to one update.
    pub sequence: u64,
    pub update_id: String,
    pub revision: u64,
    pub previous_phase: Option<UpdatePhase>,
    pub phase: UpdatePhase,
    pub occurred_at_ms: u64,
    pub owner_epoch: u64,
    pub owner_id: String,
    pub command: Option<UpdateCommand>,
}

/// Worker-side coordination, independent of the updater journal/owner epoch.
/// Only a trusted local driver may create it; no mutation RPC is registered yet.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateSpec {
    pub update_id: String,
    pub instance_id: String,
    pub journal_incarnation: String,
    pub worker_generation: String,
    pub updater_owner_id: String,
    pub updater_epoch: u64,
    pub source_directory: String,
    pub baseline_manifest_sha256: String,
    pub target_manifest_sha256: String,
    /// Derived by worker from registered workspace overlap, never caller assertions.
    pub projects: Vec<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerUpdatePhase {
    SourcePending,
    SourceHeld,
    Draining,
    Quiescing,
    Quiesced,
    Released,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateLease {
    pub spec: WorkerUpdateSpec,
    pub revision: u64,
    pub phase: WorkerUpdatePhase,
    pub accepted_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerUpdateAction {
    AcquireSource,
    BeginDrain,
    CloseDispatch,
    ConfirmQuiesced,
    Release,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateCommand {
    pub command_id: String,
    pub expected_revision: u64,
    pub action: WorkerUpdateAction,
    pub source_manifest_sha256: Option<String>,
    pub reason: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateBlockers {
    pub accepted_operations: u64,
    pub logical_owners: u64,
    pub in_flight_effects: u64,
    pub physical_owners: u64,
    pub uncertain_owners: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceTransactionPhase {
    Prepared,
    Applying,
    Verified,
    Blocked,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileState {
    Planned,
    Intent,
    Applied,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTransactionRecord {
    pub schema_version: u16,
    pub update_id: String,
    pub source_directory: String,
    pub runtime_directory: String,
    pub stage_directory: String,
    pub transaction_directory: String,
    pub source_directory_identity: String,
    pub stage_directory_identity: String,
    pub transaction_directory_identity: String,
    pub worker_source_spec: WorkerUpdateSpec,
    pub worker_lease_revision: u64,
    pub plan_sha256: String,
    pub total_files: u32,
    pub revision: u64,
    pub phase: SourceTransactionPhase,
    pub accepted_at_ms: u64,
    pub updated_at_ms: u64,
    pub reason: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFileObservation {
    pub ordinal: u32,
    pub change: SourceChange,
    pub state: SourceFileState,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTransactionStatus {
    pub schema_version: u16,
    pub kind: String,
    pub read_only: bool,
    pub transaction: SourceTransactionRecord,
    pub applied_files: u32,
    pub files: Vec<SourceFileObservation>,
    pub has_more: bool,
    pub observed_at_ms: u64,
}

/// Exact OS identity; birth is a string to preserve Windows FILETIME in JS.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorProcess {
    pub process_id: u32,
    pub birth: String,
    pub host_id: String,
    pub session_id: u32,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorPhase {
    Intent,
    Spawned,
    Ready,
    Active,
    Stopped,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorSpec {
    pub launch_id: String,
    pub update_id: String,
    pub parent: SupervisorProcess,
    pub parent_owner_id: String,
    pub parent_epoch: u64,
    pub token_sha256: String,
    pub config_path: String,
    pub config_sha256: String,
    pub runtime_directory: String,
    pub image_path: String,
    pub image_sha256: String,
    pub image_identity: String,
    pub excluded_domain: Option<String>,
    pub endpoint: String,
    pub accepted_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<SupervisorWorkerBinding>,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorRecord {
    pub schema_version: u16,
    pub spec: SupervisorSpec,
    pub phase: SupervisorPhase,
    pub revision: u64,
    pub process: Option<SupervisorProcess>,
    pub owner_id: Option<String>,
    pub owner_epoch: Option<u64>,
    pub updated_at_ms: u64,
    pub reason: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorAction {
    Observe,
    Handoff,
    Stop,
    WorkerControl,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorRequest {
    pub contract_version: u16,
    pub launch_id: String,
    pub expected_revision: u64,
    pub action: SupervisorAction,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_command: Option<WorkerUpdateControlCommand>,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorResponse {
    pub contract_version: u16,
    pub launch_id: String,
    pub record: Option<SupervisorRecord>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_control: Option<WorkerUpdateControlResponse>,
}

/// Frozen by the worker launcher; request payloads cannot select another worker.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorWorkerBinding {
    pub context: crate::RequestContext,
    pub generation: String,
    pub build: crate::BuildIdentity,
    pub process: SupervisorProcess,
    pub config_sha256: String,
    pub endpoint: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerUpdateControlCommand {
    Observe,
    BeginSource,
    Advance { command: WorkerUpdateCommand },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateControlRequest {
    pub control_version: u16,
    pub binding: SupervisorWorkerBinding,
    pub update_id: String,
    pub launch_id: String,
    pub supervisor: SupervisorProcess,
    pub updater_owner_id: String,
    pub updater_epoch: u64,
    pub command: WorkerUpdateControlCommand,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerUpdateControlResponse {
    pub control_version: u16,
    pub binding: SupervisorWorkerBinding,
    pub update_id: String,
    pub launch_id: String,
    pub result: WorkerUpdateControlResult,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerUpdateControlResult {
    /// Current observation or original command receipt; never a fresh file grant.
    Snapshot {
        lease: Option<Box<WorkerUpdateLease>>,
    },
    /// An error does not prove absence of a committed effect. Observe the original ID.
    Error { message: String },
}
impl WorkerUpdateControlResult {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }
}
impl WorkerUpdateControlResponse {
    pub fn into_lease(self) -> Result<Option<WorkerUpdateLease>, String> {
        match self.result {
            WorkerUpdateControlResult::Snapshot { lease } => Ok(lease.map(|l| *l)),
            WorkerUpdateControlResult::Error { message } => Err(message),
        }
    }
}
