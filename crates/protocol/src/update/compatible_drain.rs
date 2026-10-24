//! Supervisor19/view11: a saved prerequisite decision, not an install permission.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompatibleDrainPhase {
    NotAdmitted,
    Admitting,
    Blocked,
    Draining,
    Quiesced,
    Indeterminate,
    Aborted,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibleDrainView {
    pub phase: CompatibleDrainPhase,
    pub attempt_id: Option<String>,
    pub support_attempt_id: Option<String>,
    pub support_revision: Option<u64>,
    pub drain_attempt_id: Option<String>,
    pub decided_at_ms: Option<u64>,
    pub assessment: Option<UpdaterCompatibilityAssessment>,
    pub reason: Option<String>,
}
