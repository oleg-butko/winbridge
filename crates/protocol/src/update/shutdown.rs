//! Data for the forthcoming internal shutdown contract. No wire action is
//! registered merely by declaring these DTOs; legacy control versions stay frozen.
use super::{SupervisorProcess, WorkerUpdateLease};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerShutdownCommand {
    pub command_id: String,
    pub expected_lease_revision: u64,
    pub intent_sha256: String,
    pub reason: String,
}

/// File identities, not a hash of a live SQLite main file (which may have WAL).
/// The worker computes these itself; a caller cannot choose another state root.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerShutdownState {
    pub state_directory: String,
    pub directory_identity: String,
    pub journal_identity: String,
    pub lock_identity: String,
}

/// Acceptance is durable BEFORE ACK/stop. It never claims process exit or
/// released runtime pins; those require the independent updater's OS proof.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerShutdownReceipt {
    pub schema_version: u16,
    pub update_id: String,
    pub launch_id: String,
    pub updater_owner_id: String,
    pub updater_epoch: u64,
    pub command: WorkerShutdownCommand,
    pub lease: WorkerUpdateLease,
    pub process: SupervisorProcess,
    pub state: WorkerShutdownState,
    pub accepted_at_ms: u64,
}
