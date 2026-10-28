//! V6 lifetime transition and independent readonly normal-IPC observation.
use super::{StateBackupAuthority, SupervisorProcess};
use crate::WorkerRuntimeCompatibility;
use serde::{Deserialize, Serialize};

pub const ACTIVATION_LIFETIME_CONTRACT: u16 = 6;
pub const ACTIVATION_LIFETIME_MARKER: &[u8] = b"WINBRIDGE_PRIVATE_LIFETIME_V6";

/// A sender must not embed the receiver marker merely by checking support:
/// the updater library is also linked into worker binaries. Only the actual
/// implemented receiver should retain ACTIVATION_LIFETIME_MARKER verbatim.
pub fn activation_lifetime_marker_pattern() -> Vec<u8> {
    format!(
        "WINBRIDGE_PRIVATE_LIFETIME_V{}",
        std::hint::black_box(ACTIVATION_LIFETIME_CONTRACT)
    )
    .into_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverLifetimeCommand {
    pub contract_version: u16,
    pub command: WorkerLifetimeActivationCommand,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverLifetimeReady {
    pub contract_version: u16,
    pub receipt: WorkerLifetimeActivationReceipt,
}

/// The implemented mode keeps command dispatch frozen. Normal service and
/// maintenance release require separate, independently checked contracts.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLifetimeActivationMode {
    SupervisedQuarantine,
}

macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    $item
)*}; }

dto! {
    pub struct ActivationLifetimeQuery {
        pub lifetime_observation_version: u16,
        pub command: WorkerLifetimeActivationCommand,
    }
    pub struct ActivationLifetimeObservation {
        pub lifetime_observation_version: u16,
        pub result: ActivationLifetimeObservationResult,
    }
    /// Flat references to exact retained records, not another recursive copy
    /// of the service/controller command graph. Data never reconstructs custody.
    pub struct WorkerLifetimeActivationBinding {
        pub update_id: String,
        pub attempt_id: String,
        pub launch_id: String,
        pub updater_owner_id: String,
        pub updater_epoch: u64,
        pub root_revision: u64,
        pub controller_command_id: String,
        pub controller_record_sha256: String,
        pub local_record_sha256: String,
        pub keeper: SupervisorProcess,
        pub job_name: String,
        pub compatibility: WorkerRuntimeCompatibility,
        pub authority: StateBackupAuthority,
    }
    pub struct WorkerLifetimeActivationCommand {
        pub schema_version: u16,
        pub command_id: String,
        pub binding: WorkerLifetimeActivationBinding,
        pub mode: WorkerLifetimeActivationMode,
        pub issued_at_ms: u64,
        pub deadline_at_ms: u64,
    }
    /// Historical acceptance of supervised quarantine on the same resident
    /// runtime. It grants no ownership, normal dispatch, root commit or release.
    pub struct WorkerLifetimeActivationReceipt {
        pub schema_version: u16,
        pub command_id: String,
        pub update_id: String,
        pub attempt_id: String,
        pub command_sha256: String,
        pub keeper: SupervisorProcess,
        pub job_name: String,
        pub compatibility: WorkerRuntimeCompatibility,
        pub authority: StateBackupAuthority,
        pub mode: WorkerLifetimeActivationMode,
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
pub enum ActivationLifetimeObservationResult {
    Observed {
        receipt: Box<WorkerLifetimeActivationReceipt>,
        observed_at_ms: u64,
    },
    Rejected {
        message: String,
    },
}
