//! Proposed finalization data. No decoder, marker, discovery or mutation route
//! is registered by declaring these types. V7 remains strictly readonly.
use super::{StateBackupAuthority, WorkerLifetimeActivationCommand, WorkerUpdateLease};
use crate::WorkerRuntimeCompatibility;
use serde::{Deserialize, Serialize};

/// Reserved for the future authenticated normal-IPC effect, not advertised.
pub const PROPOSED_ACTIVATION_FINALIZATION_CONTRACT: u16 = 8;

macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    $item
)*}; }

dto! {
    /// Flat causal references plus the original bounded V6 command. No nested
    /// service/controller history, serialized OS handles or execution permit.
    pub struct ActivationFinalizationBinding {
        pub lifetime_command: WorkerLifetimeActivationCommand,
        pub lifetime_record_sha256: String,
        pub shutdown_record_sha256: String,
        pub installation_id: String,
        pub scope_sha256: String,
        pub committed_root_revision: u64,
        pub expected_lease_revision: u64,
        pub source_manifest_sha256: String,
        pub resident_request_id: String,
        pub resident_observation_sha256: String,
    }
    /// One durable decision and exact release key. The old V6 deadline is
    /// historical; this command neither renews it nor grants another launch.
    pub struct WorkerFinalizationCommand {
        pub schema_version: u16,
        pub command_id: String,
        pub binding: ActivationFinalizationBinding,
        pub issued_at_ms: u64,
    }
    /// A fact the future receiver may produce only after SQL release AND the
    /// in-memory Active seal on the same Store. It never reconstructs custody.
    pub struct WorkerFinalizationReceipt {
        pub schema_version: u16,
        pub command_id: String,
        pub command_sha256: String,
        pub compatibility: WorkerRuntimeCompatibility,
        pub authority: StateBackupAuthority,
        pub released_lease: WorkerUpdateLease,
        pub mode: WorkerFinalizationMode,
        pub released_at_ms: u64,
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerFinalizationMode {
    Active,
}
