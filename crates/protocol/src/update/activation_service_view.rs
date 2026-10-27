//! Internal25/view17: saved service facts and resident verification are distinct.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationServicePhase {
    NotStarted,
    Activating,
    Confirmed,
    Indeterminate,
    NotResident,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationServiceDurablePhase {
    Issued,
    Confirmed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationServiceView {
    pub phase: ActivationServicePhase,
    pub attempt_id: Option<String>,
    pub command_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationServiceDurablePhase>,
    pub saved_receipt: Option<WorkerServiceActivationReceipt>,
    pub resident_process: Option<SupervisorProcess>,
    pub process_alive: Option<bool>,
    pub reason: Option<String>,
}
