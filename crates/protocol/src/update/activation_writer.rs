//! Proposed journal-writer data only. No advertised RPC, startup or write grant.
use super::{ActivationStateOwned, StateBackupAuthority};
use serde::{Deserialize, Serialize};

/// Must be joined to the current activation and verified final backup by the
/// trusted journal/driver. Deserializing this object never creates an owner.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerJournalActivationBinding {
    pub update_id: String,
    pub attempt_id: String,
    pub launch_id: String,
    pub updater_owner_id: String,
    pub updater_epoch: u64,
    pub activation_revision: u64,
    pub activation_record_sha256: String,
    pub backup_record_sha256: String,
    pub physical: ActivationStateOwned,
    pub expected_authority: StateBackupAuthority,
    pub worker_journal_schema: u32,
    pub updater_journal_schema: u32,
    pub worker_config_sha256: String,
}
/// The stable command ID is both correlation and update-scoped dedup key.
/// Same ID with different payload is a conflict, never a second activation.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerJournalActivationCommand {
    pub schema_version: u16,
    pub command_id: String,
    pub binding: WorkerJournalActivationBinding,
    pub issued_at_ms: u64,
    pub deadline_at_ms: u64,
}
/// A committed fact, not a token for rebuilding a live writable Store.
/// The private authority ID remains only in the worker journal/owner memory.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerJournalActivationReceipt {
    pub schema_version: u16,
    pub command_id: String,
    pub update_id: String,
    pub attempt_id: String,
    pub command_sha256: String,
    pub authority: StateBackupAuthority,
    pub committed_at_ms: u64,
}
