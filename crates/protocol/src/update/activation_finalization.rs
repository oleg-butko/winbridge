//! Separate mutable V8 delivery and readonly observation. DTO data never
//! reconstructs the original process/Job or authorizes a cold activation.
use super::{StateBackupAuthority, WorkerLifetimeActivationCommand, WorkerUpdateLease};
use crate::WorkerRuntimeCompatibility;
use serde::{Deserialize, Serialize};

pub const ACTIVATION_FINALIZATION_CONTRACT: u16 = 8;
/// Compatibility name for code that only manipulates the data-only proposal.
pub const PROPOSED_ACTIVATION_FINALIZATION_CONTRACT: u16 = ACTIVATION_FINALIZATION_CONTRACT;
pub const ACTIVATION_FINALIZATION_MARKER: &[u8] = b"WINBRIDGE_ACTIVATION_FINALIZATION_V8";
pub fn activation_finalization_marker_pattern() -> Vec<u8> {
    format!(
        "WINBRIDGE_ACTIVATION_FINALIZATION_V{}",
        std::hint::black_box(ACTIVATION_FINALIZATION_CONTRACT)
    )
    .into_bytes()
}
pub const ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT: u16 = 1;
pub const ACTIVATION_FINALIZATION_OBSERVATION_MARKER: &[u8] = b"WINBRIDGE_FINALIZATION_OBSERVE_V1";
/// Sender pattern must not advertise a receiver in linked worker images.
pub fn finalization_observation_marker_pattern() -> Vec<u8> {
    format!(
        "WINBRIDGE_FINALIZATION_OBSERVE_V{}",
        std::hint::black_box(ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT)
    )
    .into_bytes()
}

macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    $item
)*}; }

dto! {
    pub struct ActivationFinalizationRequest {
        pub finalization_version: u16,
        pub command: WorkerFinalizationCommand,
    }
    pub struct ActivationFinalizationResponse {
        pub finalization_version: u16,
        pub command_id: Option<String>,
        pub result: ActivationFinalizationResult,
    }
    /// Readonly correlation, never a delivery key or an Active permit.
    pub struct ActivationFinalizationQuery {
        pub finalization_observation_version: u16,
        pub request_id: String,
        pub command: WorkerFinalizationCommand,
    }
    pub struct ActivationFinalizationObservation {
        pub finalization_observation_version: u16,
        pub request_id: Option<String>,
        pub result: ActivationFinalizationObservationResult,
    }
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
    /// A fact the resident owner may produce only after SQL release AND the
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivationFinalizationResult {
    Active {
        receipt: Box<WorkerFinalizationReceipt>,
    },
    Indeterminate {
        message: String,
    },
    Rejected {
        message: String,
    },
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
pub enum ActivationFinalizationObservationResult {
    Observed {
        receipt: Box<WorkerFinalizationReceipt>,
        observed_at_ms: u64,
    },
    Pending {
        command_id: String,
        message: String,
    },
    Unavailable {
        message: String,
    },
    Rejected {
        message: String,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerFinalizationMode {
    Active,
}
