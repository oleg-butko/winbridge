//! Supervisor20/view12. Passive preparation facts, never startup permissions.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPreparationPhase {
    NotPrepared,
    Preparing,
    Prepared,
    /// A later execution contract has progressed beyond this preparation view.
    Progressed,
    Indeterminate,
    Blocked,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationPreparationView {
    pub phase: ActivationPreparationPhase,
    pub attempt_id: Option<String>,
    pub revision: Option<u64>,
    pub set_id: Option<String>,
    pub replacement_sha256: Option<String>,
    pub worker: Option<crate::RuntimeImageEvidence>,
    pub worker_config_sha256: Option<String>,
    pub prepared_at_ms: Option<u64>,
    /// Live owner observation only; never deserialized into execution authority.
    pub physical_custody_held: bool,
    pub reason: Option<String>,
}
