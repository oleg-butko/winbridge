//! Lifetime proposals only. No receiver marker, RPC action or advertised support.
use super::{StateBackupAuthority, SupervisorProcess};
use crate::WorkerRuntimeCompatibility;
use serde::{Deserialize, Serialize};

/// The only proposed mode keeps command dispatch frozen. Normal service and
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
