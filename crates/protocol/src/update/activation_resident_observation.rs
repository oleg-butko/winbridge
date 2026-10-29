//! Readonly V7 current-session observation. This advertises no release action.
use super::{WorkerLifetimeActivationCommand, WorkerLifetimeActivationReceipt};
use crate::{BuildIdentity, ControllerLinkObservation, RequestContext};
use serde::{Deserialize, Serialize};

pub const ACTIVATION_RESIDENT_OBSERVATION_CONTRACT: u16 = 7;
pub const ACTIVATION_RESIDENT_OBSERVATION_MARKER: &[u8] = b"WINBRIDGE_PRIVATE_RESIDENT_OBSERVE_V7";
/// The sender must not retain the receiver's verbatim marker in linked workers.
pub fn resident_observation_marker_pattern() -> Vec<u8> {
    format!(
        "WINBRIDGE_PRIVATE_RESIDENT_OBSERVE_V{}",
        std::hint::black_box(ACTIVATION_RESIDENT_OBSERVATION_CONTRACT)
    )
    .into_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationResidentQuery {
    pub resident_observation_version: u16,
    /// Fresh correlation for one observation, not an idempotency/execution key.
    pub request_id: String,
    pub command: WorkerLifetimeActivationCommand,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationResidentObservation {
    pub resident_observation_version: u16,
    pub request_id: Option<String>,
    pub result: ActivationResidentObservationResult,
}
/// Bounded header of the actually accepted frame. The full projection may carry
/// large project/workflow views; its canonical digest binds those omitted bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationResidentFrame {
    pub schema_version: u16,
    pub controller_build: BuildIdentity,
    pub controller_generation: String,
    pub session_id: String,
    pub provider_build: BuildIdentity,
    pub provider_generation: String,
    pub context: RequestContext,
    pub sequence: u64,
    pub projection_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivationResidentObservationResult {
    Observed {
        entry: Box<WorkerLifetimeActivationReceipt>,
        frame: Box<ActivationResidentFrame>,
        link: Box<ControllerLinkObservation>,
        observed_at_ms: u64,
    },
    Unavailable {
        message: String,
    },
    Rejected {
        message: String,
    },
}
