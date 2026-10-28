//! Controller-session activation data. Neither a network nor a commit grant.
use super::{StateBackupAuthority, WorkerServiceActivationCommand, WorkerServiceActivationReceipt};
use crate::{BuildIdentity, RequestContext, WorkerRuntimeCompatibility};
use serde::{Deserialize, Serialize};

/// Recognized only after the private receiver is implemented; defining this
/// marker does not advertise it from the current worker binary.
pub const ACTIVATION_CONTROLLER_MARKER: &[u8] = b"WINBRIDGE_PRIVATE_CONTROLLER_SESSION_V5";
pub const ACTIVATION_CONTROLLER_CONTRACT: u16 = 5;

/// Private delivery on the original authenticated parent/child channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverControllerCommand {
    pub contract_version: u16,
    pub command: WorkerControllerActivationCommand,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverControllerReady {
    pub contract_version: u16,
    pub receipt: WorkerControllerActivationReceipt,
}
/// Readonly normal IPC; recognition requires the resident private binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationControllerQuery {
    pub controller_observation_version: u16,
    pub command: WorkerControllerActivationCommand,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationControllerObservation {
    pub controller_observation_version: u16,
    pub result: ActivationControllerObservationResult,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivationControllerObservationResult {
    Observed {
        receipt: Box<WorkerControllerActivationReceipt>,
        projection: Box<crate::ControllerReadinessProjection>,
        link: Box<crate::ControllerLinkObservation>,
    },
    Waiting {
        message: String,
    },
    Rejected {
        message: String,
    },
}

macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    $item
)*}; }

dto! {
    pub struct WorkerControllerActivationEndpoint {
        pub id: String,
        pub url: String,
    }
    /// Exact frozen config plus a separate trusted policy selection. A proposal
    /// does not authorize adding addresses or changing the configured primary.
    pub struct WorkerControllerActivationTopology {
        pub schema_version: u16,
        pub primary: String,
        pub confirmation_endpoint_id: String,
        pub controllers: Vec<WorkerControllerActivationEndpoint>,
    }
    pub struct WorkerControllerActivationBinding {
        pub service_command: WorkerServiceActivationCommand,
        pub service_receipt: WorkerServiceActivationReceipt,
        pub service_record_sha256: String,
        pub local_record_sha256: String,
        pub topology: WorkerControllerActivationTopology,
    }
    pub struct WorkerControllerActivationCommand {
        pub schema_version: u16,
        pub command_id: String,
        pub binding: WorkerControllerActivationBinding,
        pub issued_at_ms: u64,
        pub deadline_at_ms: u64,
    }
    /// A summary of an actually accepted current-ticket projection. The full
    /// frame is retained by validation separately; this data cannot adopt a link.
    pub struct WorkerControllerSessionEvidence {
        pub controller_id: String,
        pub controller_url: String,
        pub controller_node_id: String,
        pub controller_generation: String,
        pub session_id: String,
        pub controller_build: BuildIdentity,
        pub worker_context: RequestContext,
        pub worker_generation: String,
        pub worker_build: BuildIdentity,
        pub connection_attempt: u64,
        pub projection_sequence: u64,
        pub projection_sha256: String,
        pub connected_at_ms: u64,
    }
    /// A fact about the same resident child and selected controller. It does not
    /// transfer Job lifetime, release maintenance or commit the whole update.
    pub struct WorkerControllerActivationReceipt {
        pub schema_version: u16,
        pub command_id: String,
        pub update_id: String,
        pub attempt_id: String,
        pub command_sha256: String,
        pub authority: StateBackupAuthority,
        pub compatibility: WorkerRuntimeCompatibility,
        pub session: WorkerControllerSessionEvidence,
    }
}
