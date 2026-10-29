//! Internal27/view19: historical entry cannot reconstruct original custody.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationLifetimePhase {
    NotStarted,
    Entering,
    Observing,
    Observed,
    Indeterminate,
    NotResident,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationLifetimeDurablePhase {
    Issued,
    Confirmed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationLifetimeView {
    pub phase: ActivationLifetimePhase,
    pub attempt_id: Option<String>,
    pub command_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationLifetimeDurablePhase>,
    pub saved_receipt: Option<WorkerLifetimeActivationReceipt>,
    pub resident_process: Option<SupervisorProcess>,
    pub process_alive: Option<bool>,
    /// Only the resident actor may assert this; a journal read leaves it unknown.
    pub original_custody_held: Option<bool>,
    /// Captured on actual normal IPC. Its timestamp is not a continuous promise.
    pub observed_receipt: Option<WorkerLifetimeActivationReceipt>,
    pub observed_at_ms: Option<u64>,
    pub reason: Option<String>,
}
