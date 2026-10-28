//! Internal26/view18: a saved session fact is not current network readiness.
use super::*;
use crate::{ControllerLinkObservation, ControllerReadinessProjection};
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationControllerPhase {
    NotStarted,
    Connecting,
    Observed,
    Indeterminate,
    NotResident,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationControllerDurablePhase {
    Issued,
    Confirmed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationControllerView {
    pub phase: ActivationControllerPhase,
    pub attempt_id: Option<String>,
    pub command_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationControllerDurablePhase>,
    pub saved_receipt: Option<WorkerControllerActivationReceipt>,
    pub resident_process: Option<SupervisorProcess>,
    pub process_alive: Option<bool>,
    /// Captured independently on normal IPC by the original parent. These are
    /// evidence of that observation, not a continuously current connection.
    pub observed_projection: Option<ControllerReadinessProjection>,
    pub observed_link: Option<ControllerLinkObservation>,
    pub reason: Option<String>,
}
