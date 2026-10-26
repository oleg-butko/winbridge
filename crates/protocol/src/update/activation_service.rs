//! Local-service proposals only. No advertised RPC, receiver marker or grant.
use super::{StateBackupAuthority, WorkerJournalActivationCommand, WorkerJournalActivationReceipt};
use crate::{LoadedWorkerIdentity, WorkerRuntimeCompatibility};
use serde::{Deserialize, Serialize};

/// Trusted admission must join the confirmed writer and current physical graph.
/// The old command/receipt remain immutable; service startup never adds an epoch.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerServiceActivationBinding {
    pub writer_command: WorkerJournalActivationCommand,
    pub writer_receipt: WorkerJournalActivationReceipt,
    pub writer_record_sha256: String,
    pub expected_worker: LoadedWorkerIdentity,
    pub worker_config_schema: u16,
    pub protocol_version: u16,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerServiceActivationCommand {
    pub schema_version: u16,
    pub command_id: String,
    pub binding: WorkerServiceActivationBinding,
    pub issued_at_ms: u64,
    pub deadline_at_ms: u64,
}

/// An observed normal-IPC fact. It grants no controller connection, Job lifetime
/// transfer, maintenance release, cold Store opening or automatic retry.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerServiceActivationReceipt {
    pub schema_version: u16,
    pub command_id: String,
    pub update_id: String,
    pub attempt_id: String,
    pub command_sha256: String,
    pub authority: StateBackupAuthority,
    pub compatibility: WorkerRuntimeCompatibility,
}
