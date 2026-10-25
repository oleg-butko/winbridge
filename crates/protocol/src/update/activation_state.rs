//! Internal23/view15: physical handoff receipts, never journal writer authority.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationStatePhase {
    NotStarted,
    HandingOff,
    StateOwned,
    Indeterminate,
    NotResident,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationStateView {
    pub phase: ActivationStatePhase,
    pub attempt_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationDurablePhase>,
    pub saved_ownership: Option<ActivationStateOwned>,
    /// Resident original custody, not an assertion that an arbitrary lock is
    /// free. None while the sole blocking owner cannot be observed; false cold.
    pub original_lock_held: Option<bool>,
    pub resident_process: Option<SupervisorProcess>,
    pub process_alive: Option<bool>,
    pub reason: Option<String>,
}
