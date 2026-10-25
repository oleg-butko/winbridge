//! Internal24/view16. No worker service, controller or lifetime readiness promise.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationWriterPhase {
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
pub enum ActivationWriterDurablePhase {
    Issued,
    Confirmed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationWriterView {
    pub phase: ActivationWriterPhase,
    pub attempt_id: Option<String>,
    /// Resident dispatch key may precede durable Issued; not an admission grant.
    pub command_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationWriterDurablePhase>,
    pub saved_receipt: Option<WorkerJournalActivationReceipt>,
    pub resident_process: Option<SupervisorProcess>,
    pub process_alive: Option<bool>,
    pub reason: Option<String>,
}
