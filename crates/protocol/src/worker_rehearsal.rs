//! Isolated candidate smoke result; no install permission or production migration proof.
use crate::{update::SupervisorProcess, ArtifactDescriptor};
use serde::{Deserialize, Serialize};

pub const WORKER_REHEARSAL_MARKER: &[u8] = b"\0WinBridge.WorkerRehearsal.contract-1\0";

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerRehearsalReport {
    pub schema_version: u16,
    pub kind: String,
    pub contract_version: u16,
    pub descriptor: ArtifactDescriptor,
    pub process: SupervisorProcess,
    pub directory: String,
    pub directory_identity: String,
    pub instance_id: String,
    pub generation: String,
    pub journal_incarnation: String,
    pub worker_journal_schema: u32,
    pub updater_journal_schema: u32,
    pub worker_config_schema: u16,
    pub protocol_version: u16,
    pub fresh_journal: bool,
    pub ipc_ready: bool,
    pub server_stopped: bool,
    pub controller_enabled: bool,
    pub direct_commands_enabled: bool,
    pub production_state_opened: bool,
    pub candidate_verified: bool,
    pub runtime_applied: bool,
}
