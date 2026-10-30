//! Version-neutral local keeper control; no executable, path or saved permit.
use super::{SupervisorProcess, WorkerFinalizationReceipt};
use serde::{Deserialize, Serialize};
pub const KEEPER_CONTROL_CONTRACT: u16 = 1;
/// Read-only current-worker handshake, independent of updater journal formats.
pub const KEEPER_PROBE_CONTRACT: u16 = 1;
pub const KEEPER_PROBE_MARKER: &[u8] = b"WINBRIDGE_KEEPER_PROBE_V1";
pub const KEEPER_CONTROL_MARKER: &[u8] = b"WINBRIDGE_KEEPER_CONTROL_V1";
pub fn keeper_control_marker_pattern() -> Vec<u8> {
    format!(
        "WINBRIDGE_KEEPER_CONTROL_V{}",
        std::hint::black_box(KEEPER_CONTROL_CONTRACT)
    )
    .into_bytes()
}
macro_rules! dto {($($item:item)*)=>{$(
 #[cfg_attr(feature="jsonschema",derive(schemars::JsonSchema))]
 #[cfg_attr(feature="typescript",derive(ts_rs::TS))]
 #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
 #[serde(rename_all="camelCase",deny_unknown_fields)]
 $item
)*};}
dto! {
 pub struct KeeperProbeRequest {
    pub keeper_probe_version:u16,
    pub request_id:String,
    pub binding:KeeperBinding,
 }
 pub struct KeeperProbeResponse {
    pub keeper_probe_version:u16,
    pub request_id:Option<String>,
    pub result:KeeperProbeResult,
 }
 pub struct KeeperControlRequest {
    pub keeper_control_version:u16,
    pub update_id:String,
    pub launch_id:String,
    pub request_id:String,
    pub expected_coordinator_revision:u64,
    pub token:String,
    pub command:KeeperControlCommand,
 }
 pub struct KeeperControlResponse {
    pub keeper_control_version:u16,
    pub update_id:String,
    pub launch_id:String,
    pub request_id:Option<String>,
    pub result:KeeperControlResult,
 }
 /// Immutable identity data, never a new process/Job/bootstrap permission.
 pub struct KeeperBinding {
    pub update_id:String,
    pub launch_id:String,
    pub coordinator:SupervisorProcess,
    pub worker:SupervisorProcess,
    pub job_name:String,
    pub generation:String,
    pub context:crate::RequestContext,
    pub finalization_command_id:String,
    pub finalization_command_sha256:String,
    pub selection_sha256:Option<String>,
 }
 pub struct KeeperStatus {
    pub phase:KeeperPhase,
    pub binding:Option<KeeperBinding>,
    pub role_selection_known:bool,
    pub writer_closed:bool,
    pub authority_released:bool,
    pub original_custody_held:Option<bool>,
    pub worker_alive:Option<bool>,
    pub job_tree_empty:Option<bool>,
    pub stdout_tail:String,
    pub stderr_tail:String,
    pub output_incomplete:Option<bool>,
    pub readers_finished:Option<bool>,
    pub reason:Option<String>,
 }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum KeeperProbeResult {
    Observed {
        binding: Box<KeeperBinding>,
        observed_at_ms: u64,
    },
    Rejected {
        message: String,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeeperControlCommand {
    RetireCoordinator,
    Observe,
    RetryWriterClose,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeeperPhase {
    NotSelected,
    Preparing,
    Unconfirmed,
    Keeper,
    WorkerExited,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeeperControlResult {
    Status {
        status: Box<KeeperStatus>,
    },
    /// This request did not start another retirement operation.
    Busy {
        message: String,
    },
    Rejected {
        message: String,
    },
}
/// One immutable Active fact can be referred to by a keeper; never deserialize
/// this function's output as an execution permit.
pub fn keeper_binding_from_receipt(
    update_id: String,
    launch_id: String,
    coordinator: SupervisorProcess,
    job_name: String,
    receipt: &WorkerFinalizationReceipt,
    selection_sha256: Option<String>,
) -> KeeperBinding {
    KeeperBinding {
        update_id,
        launch_id,
        coordinator,
        worker: receipt.compatibility.identity.process.clone(),
        job_name,
        generation: receipt.compatibility.generation.clone(),
        context: receipt.compatibility.context.clone(),
        finalization_command_id: receipt.command_id.clone(),
        finalization_command_sha256: receipt.command_sha256.clone(),
        selection_sha256,
    }
}
