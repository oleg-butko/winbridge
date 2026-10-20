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
    Aborting,
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
    /// Internal baseline driver only; no source/runtime file effects.
    RuntimeBaselineAdmitted,
    RuntimeBaselineVerified,
    RuntimePreimagesAdmitted,
    RuntimePreimagesVerified,
    BeginPreparation,
    BeginSourceApply,
    SourceVerified,
    BuildSucceeded,
    /// Trusted health driver only, before its owned probe effects.
    CandidateHealthAdmitted,
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
    BeginAbort,
    AbortCompleted,
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

/// A readonly snapshot of logical blockers. It deliberately contains no
/// physical-owner count: only ConfirmQuiesced may claim a physical scan.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerDrainObservation {
    pub schema_version: u16,
    pub context: crate::RequestContext,
    pub worker_generation: String,
    pub lease: Option<Box<WorkerUpdateLease>>,
    pub accepted_operations: u64,
    pub logical_owners: u64,
    pub in_flight_effects: u64,
    pub queue_revision: String,
    pub queue_mode: crate::QueueMode,
    pub observed_at_ms: u64,
}

/// Internal trusted compiler driver contract; not registered as an agent command.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBuildSpec {
    pub attempt_id: String,
    pub update_id: String,
    pub request_key: String,
    pub platform: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub worker_source_spec: WorkerUpdateSpec,
    pub worker_lease_revision: u64,
    pub source_directory: String,
    pub runtime_directory: String,
    pub source_manifest_sha256: String,
    pub command: crate::ResolvedCommand,
    pub job_name: Option<String>,
    pub source_available_bytes_before: u64,
    pub runtime_available_bytes_before: u64,
    pub accepted_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateBuildPhase {
    Intent,
    Spawned,
    ResumeGranted,
    Running,
    Completed,
    Indeterminate,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateBuildOutcome {
    Succeeded,
    Failed,
    TimedOut,
    SpawnFailed,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBuildOutput {
    pub retained_bytes: u64,
    pub observed_bytes: u64,
    pub sha256: String,
    pub error: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBuildRecord {
    pub schema_version: u16,
    pub spec: UpdateBuildSpec,
    pub revision: u64,
    pub phase: UpdateBuildPhase,
    pub process: Option<SupervisorProcess>,
    pub outcome: Option<UpdateBuildOutcome>,
    pub exit_code: Option<i32>,
    pub cleanup_verified: bool,
    pub stdout: Option<UpdateBuildOutput>,
    pub stderr: Option<UpdateBuildOutput>,
    pub updated_at_ms: u64,
    pub reason: String,
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
    Aborted,
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
    /// Internal v2, authenticated preparation only; no runtime replacement.
    PrepareSource,
    ObservePreparation,
    /// Internal v3, command comes only from the frozen local build policy.
    BuildCandidate,
    /// Internal v4, release only after coordinated pre-cutover cleanup proof.
    AbortBeforeCutover,
    /// Internal v5, readonly file validation and atomic metadata receipt only.
    VerifyCandidateArtifacts,
    /// Internal v6, readonly identity of the exact bound live worker.
    ObserveWorkerRuntime,
    /// Internal v7: persist the old-runtime baseline before source effects.
    CaptureRuntimeBaseline,
    ObserveRuntimeBaseline,
    /// Internal v8: create-only previous-runtime generation and its saved status.
    ArchiveRuntimePreimages,
    ObserveRuntimePreimages,
    /// Internal v9: authenticated current-worker schemas/config; readonly.
    ObserveWorkerCompatibility,
    /// Internal v10: journal-bound owned config/rehearsal checks, never installation.
    ValidateCandidateHealth,
    ObserveCandidateHealth,
    /// Internal v11: read a saved worker-command receipt, never repeat a mutation.
    ObserveWorkerReceipt,
    /// Internal v12: logical drain blockers and user queue mode; readonly.
    ObserveWorkerDrain,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation: Option<UpdatePreparationView>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePreparationView {
    pub schema_version: u16,
    pub observed_at_ms: u64,
    pub update: UpdateRecord,
    pub source: Option<SourceTransactionRecord>,
    pub source_applied_files: u32,
    pub build: Option<UpdateBuildRecord>,
    /// Observation of this supervisor's blocking driver, separate from saved state.
    pub driver_active: bool,
    pub driver_error: Option<String>,
    /// V7+ replies include this projection; v1-v6 keep their JSON shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_baseline: Option<RuntimeBaselineView>,
    /// V8/view schema 3 only; older contracts omit the field entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_preimages: Option<RuntimePreimagesView>,
    /// V10/view schema 4; older contracts omit it entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_health: Option<CandidateHealthView>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateHealthPhase {
    NotAdmitted,
    Pending,
    Verified,
    Rejected,
    Cancelled,
    Indeterminate,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateHealthView {
    pub phase: CandidateHealthPhase,
    pub revision: Option<u64>,
    pub attempt_id: Option<String>,
    pub config_process: Option<SupervisorProcess>,
    pub rehearsal_process: Option<SupervisorProcess>,
    pub cleanup_verified: bool,
    pub reason: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimePreimagesView {
    NotAdmitted,
    Pending {
        #[serde(rename = "intentRevision")]
        intent_revision: u64,
        #[serde(rename = "admittedAtMs")]
        admitted_at_ms: u64,
        directory: String,
    },
    Verified {
        #[serde(rename = "intentRevision")]
        intent_revision: u64,
        #[serde(rename = "receiptRevision")]
        receipt_revision: u64,
        #[serde(rename = "completedAtMs")]
        completed_at_ms: u64,
        directory: String,
        #[serde(rename = "fileCount")]
        file_count: u16,
        #[serde(rename = "totalBytes")]
        total_bytes: u64,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeBaselineView {
    NotAdmitted,
    Pending {
        #[serde(rename = "intentRevision")]
        intent_revision: u64,
        #[serde(rename = "admittedAtMs")]
        admitted_at_ms: u64,
    },
    Verified {
        #[serde(rename = "intentRevision")]
        intent_revision: u64,
        #[serde(rename = "receiptRevision")]
        receipt_revision: u64,
        #[serde(rename = "observedAtMs")]
        observed_at_ms: u64,
        #[serde(rename = "setId")]
        set_id: String,
        #[serde(rename = "workerProcess")]
        worker_process: SupervisorProcess,
        #[serde(rename = "workerImageSha256")]
        worker_image_sha256: String,
    },
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
    /// Worker control v2 only; never a lease or mutation command.
    ObserveRuntime,
    /// Worker control v3 only; no caller-selected schema or config.
    ObserveCompatibility,
    /// Worker control v4 only. Missing receipt does not authorize a retry.
    ObserveReceipt {
        command: WorkerUpdateCommand,
    },
    /// Worker control v5 only; logical observation, never a quiescence receipt.
    ObserveDrain,
    BeginSource,
    Advance {
        command: WorkerUpdateCommand,
    },
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
    RuntimeIdentity {
        identity: Box<crate::LoadedWorkerIdentity>,
    },
    RuntimeCompatibility {
        compatibility: Box<crate::WorkerRuntimeCompatibility>,
    },
    CommandReceipt {
        command: WorkerUpdateCommand,
        receipt: Option<Box<WorkerUpdateLease>>,
        #[serde(rename = "observedAtMs")]
        observed_at_ms: u64,
    },
    DrainObservation {
        observation: Box<WorkerDrainObservation>,
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
        if self.control_version != 1 {
            return Err("Lease receipt requires worker control v1; no effect fallback".into());
        }
        match self.result {
            WorkerUpdateControlResult::Snapshot { lease } => Ok(lease.map(|l| *l)),
            WorkerUpdateControlResult::Error { message } => Err(message),
            WorkerUpdateControlResult::RuntimeIdentity { .. }
            | WorkerUpdateControlResult::RuntimeCompatibility { .. }
            | WorkerUpdateControlResult::CommandReceipt { .. }
            | WorkerUpdateControlResult::DrainObservation { .. } => {
                Err("Runtime identity is not a lease receipt".into())
            }
        }
    }
    pub fn into_runtime_identity(self) -> Result<crate::LoadedWorkerIdentity, String> {
        if self.control_version != 2 {
            return Err("Runtime identity requires worker control v2; no fallback".into());
        }
        match self.result {
            WorkerUpdateControlResult::RuntimeIdentity { identity } => Ok(*identity),
            WorkerUpdateControlResult::Error { message } => Err(message),
            WorkerUpdateControlResult::Snapshot { .. }
            | WorkerUpdateControlResult::RuntimeCompatibility { .. }
            | WorkerUpdateControlResult::CommandReceipt { .. }
            | WorkerUpdateControlResult::DrainObservation { .. } => {
                Err("Lease receipt is not a runtime identity".into())
            }
        }
    }
    pub fn into_runtime_compatibility(self) -> Result<crate::WorkerRuntimeCompatibility, String> {
        if self.control_version != 3 {
            return Err("Runtime compatibility requires worker control v3; no fallback".into());
        }
        match self.result {
            WorkerUpdateControlResult::RuntimeCompatibility { compatibility } => {
                compatibility.validate_binding(&self.binding)?;
                Ok(*compatibility)
            }
            WorkerUpdateControlResult::Error { message } => Err(message),
            _ => Err("Expected typed runtime compatibility; no fallback".into()),
        }
    }
    /// Historical evidence only. None means this read found no matching receipt,
    /// not that an in-flight command cannot commit after the observation.
    pub fn into_command_receipt(
        self,
        expected: &WorkerUpdateCommand,
    ) -> Result<Option<WorkerUpdateLease>, String> {
        if self.control_version != 4 {
            return Err("Command receipt requires worker control v4; no mutation fallback".into());
        }
        match self.result {
            WorkerUpdateControlResult::CommandReceipt {
                command,
                receipt,
                observed_at_ms,
            } if command == *expected && observed_at_ms > 0 => {
                let phase = match command.action {
                    WorkerUpdateAction::AcquireSource => WorkerUpdatePhase::SourceHeld,
                    WorkerUpdateAction::BeginDrain => WorkerUpdatePhase::Draining,
                    WorkerUpdateAction::CloseDispatch => WorkerUpdatePhase::Quiescing,
                    WorkerUpdateAction::ConfirmQuiesced => WorkerUpdatePhase::Quiesced,
                    WorkerUpdateAction::Release => WorkerUpdatePhase::Released,
                };
                if receipt.as_ref().is_some_and(|r| {
                    r.spec.update_id != self.update_id
                        || r.spec.instance_id != self.binding.context.instance_id
                        || r.spec.journal_incarnation != self.binding.context.journal_incarnation
                        || r.spec.worker_generation != self.binding.generation
                        || command.expected_revision.checked_add(1) != Some(r.revision)
                        || r.phase != phase
                        || r.reason != command.reason
                }) {
                    return Err("Command receipt scope/revision/phase differs".into());
                }
                Ok(receipt.map(|r| *r))
            }
            WorkerUpdateControlResult::Error { message } => Err(message),
            _ => Err("Expected exact typed command receipt; no fallback".into()),
        }
    }
    pub fn into_drain_observation(self) -> Result<WorkerDrainObservation, String> {
        if self.control_version != 5 {
            return Err("Drain observation requires worker control v5; no fallback".into());
        }
        match self.result {
            WorkerUpdateControlResult::DrainObservation { observation } => {
                let v = *observation;
                const MAX_COUNTER: u64 = 9_007_199_254_740_991;
                if v.schema_version != 1
                    || v.context != self.binding.context
                    || v.worker_generation != self.binding.generation
                    || v.observed_at_ms == 0
                    || v.observed_at_ms > MAX_COUNTER
                    || [v.accepted_operations, v.logical_owners, v.in_flight_effects]
                        .into_iter()
                        .any(|n| n > MAX_COUNTER)
                    || v.queue_revision.parse::<u64>().map_or(true, |n| {
                        n > MAX_COUNTER || n.to_string() != v.queue_revision
                    })
                    || v.lease.as_ref().is_some_and(|l| {
                        l.spec.update_id != self.update_id
                            || l.spec.instance_id != v.context.instance_id
                            || l.spec.journal_incarnation != v.context.journal_incarnation
                            || l.spec.worker_generation != v.worker_generation
                            || l.revision > MAX_COUNTER
                    })
                {
                    return Err("Drain observation context/counters/scope inconsistent".into());
                }
                Ok(v)
            }
            WorkerUpdateControlResult::Error { message } => Err(message),
            _ => Err("Expected logical drain observation, never a quiescence receipt".into()),
        }
    }
}

/// Readonly byte cursor for a single compiler attempt; no caller-selected path.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCompilerOutputRequest {
    pub attempt_id: String,
    pub stream: crate::OutputStream,
    pub offset: u64,
    pub limit_bytes: u32,
    pub file_identity: Option<String>,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCompilerOutputPage {
    pub schema_version: u16,
    pub kind: String,
    pub read_only: bool,
    pub update_id: String,
    pub attempt_id: String,
    pub build_revision: u64,
    pub observed_at_ms: u64,
    pub phase: UpdateBuildPhase,
    pub stream: crate::OutputStream,
    pub offset: u64,
    pub next_offset: u64,
    pub available_bytes: u64,
    pub file_identity: Option<String>,
    pub bytes: Vec<u8>,
    pub has_more: bool,
    pub capture_complete: bool,
    pub proof_verified: bool,
    pub observed_bytes: Option<u64>,
    pub quota_truncated: Option<bool>,
    pub output_error: Option<String>,
}
