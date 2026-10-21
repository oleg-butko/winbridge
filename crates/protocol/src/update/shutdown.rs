//! Data for the forthcoming internal shutdown contract. No wire action is
//! registered merely by declaring these DTOs; legacy control versions stay frozen.
use super::{
    SupervisorProcess, WorkerUpdateControlCommand, WorkerUpdateControlResponse,
    WorkerUpdateControlResult, WorkerUpdateLease, WorkerUpdatePhase,
};
use serde::{Deserialize, Serialize};
pub const WORKER_SHUTDOWN_INSPECTION_MARKER: &[u8] =
    b"\0WinBridge.WorkerShutdownInspection.contract-1\0";

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

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerShutdownInspectionReport {
    pub schema_version: u16,
    pub kind: String,
    pub request_sha256: String,
    pub descriptor: crate::ArtifactDescriptor,
    pub helper_process: SupervisorProcess,
    pub helper_image: crate::RuntimeImageEvidence,
    pub worker_config: crate::RuntimeImageEvidence,
    pub state: WorkerShutdownState,
    pub receipt: Option<Box<WorkerShutdownReceipt>>,
    pub worker_journal_schema: u32,
    pub journal_opened_read_only: bool,
    pub worker_constructed: bool,
    pub scheduler_started: bool,
    pub recovery_marker_changed: bool,
    pub runtime_applied: bool,
}

impl WorkerUpdateControlCommand {
    /// Single source for both routing and strict version/action admission.
    /// Legacy versions do not gain new effects merely by sharing an envelope.
    pub fn required_version(&self) -> u16 {
        match self {
            Self::Observe | Self::BeginSource | Self::Advance { .. } => 1,
            Self::ObserveRuntime => 2,
            Self::ObserveCompatibility => 3,
            Self::ObserveReceipt { .. } => 4,
            Self::ObserveDrain => 5,
            Self::ObserveShutdownState
            | Self::RequestShutdown { .. }
            | Self::ObserveShutdownReceipt { .. } => 6,
        }
    }
}
impl WorkerShutdownState {
    pub fn validate(&self) -> Result<(), String> {
        let text = |s: &str, n| !s.is_empty() && s.len() <= n && !s.chars().any(char::is_control);
        if !text(&self.state_directory, 4096)
            || [
                &self.directory_identity,
                &self.journal_identity,
                &self.lock_identity,
            ]
            .into_iter()
            .any(|s| !text(s, 128))
            || self.directory_identity == self.journal_identity
            || self.directory_identity == self.lock_identity
            || self.journal_identity == self.lock_identity
        {
            Err("Invalid shutdown state path/file identities".into())
        } else {
            Ok(())
        }
    }
}
impl WorkerUpdateControlResponse {
    pub fn into_shutdown_state(self) -> Result<WorkerShutdownState, String> {
        if self.control_version != 6 {
            return Err("Shutdown state requires worker control v6".into());
        }
        match self.result {
            WorkerUpdateControlResult::ShutdownState { state } => {
                state.validate()?;
                Ok(state)
            }
            WorkerUpdateControlResult::Error { message } => Err(message),
            _ => Err("Expected worker-computed shutdown state; this is not an exit receipt".into()),
        }
    }
    pub fn into_shutdown_receipt(
        self,
        expected: &WorkerShutdownCommand,
    ) -> Result<Option<WorkerShutdownReceipt>, String> {
        if self.control_version != 6 {
            return Err("Shutdown receipt requires worker control v6".into());
        }
        const MAX: u64 = 9_007_199_254_740_991;
        match self.result {
            WorkerUpdateControlResult::ShutdownReceipt {
                command,
                receipt,
                observed_at_ms,
            } => {
                if command != *expected || observed_at_ms == 0 || observed_at_ms > MAX {
                    return Err("Shutdown receipt command/observation differs".into());
                }
                if let Some(r) = &receipt {
                    r.state.validate()?;
                    let spec = &r.lease.spec;
                    if r.schema_version != 1
                        || r.command != *expected
                        || r.update_id != self.update_id
                        || r.launch_id != self.launch_id
                        || r.lease.phase != WorkerUpdatePhase::Quiesced
                        || r.lease.revision != expected.expected_lease_revision
                        || r.lease.revision == 0
                        || r.lease.revision >= MAX
                        || r.process != self.binding.process
                        || spec.update_id != self.update_id
                        || spec.instance_id != self.binding.context.instance_id
                        || spec.journal_incarnation != self.binding.context.journal_incarnation
                        || spec.worker_generation != self.binding.generation
                        || spec.updater_owner_id != r.updater_owner_id
                        || spec.updater_epoch != r.updater_epoch
                        || r.updater_epoch == 0
                        || r.updater_epoch > MAX
                        || r.accepted_at_ms == 0
                        || r.accepted_at_ms > MAX
                    {
                        return Err("Shutdown receipt context/lease/owner/process differs".into());
                    }
                }
                Ok(receipt.map(|r| *r))
            }
            WorkerUpdateControlResult::Error { message } => Err(message),
            _ => Err("Expected exact shutdown acceptance, never process exit".into()),
        }
    }
}
