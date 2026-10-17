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
