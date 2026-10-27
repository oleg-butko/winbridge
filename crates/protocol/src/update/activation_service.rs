//! Local-service data and readonly normal-IPC observations, never a writer grant.
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

/// Only a staged local service recognizes this envelope on its normal IPC.
/// Re-reading is safe; the command must equal its resident sealed binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationServiceQuery {
    pub service_observation_version: u16,
    pub command: WorkerServiceActivationCommand,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationServiceObservation {
    pub service_observation_version: u16,
    pub result: ActivationServiceObservationResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivationServiceObservationResult {
    Observed {
        receipt: Box<WorkerServiceActivationReceipt>,
    },
    Rejected {
        message: String,
    },
}
