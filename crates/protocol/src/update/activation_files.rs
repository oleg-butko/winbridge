//! Internal21/view13: live resource observations, never permission to launch.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationFilesPhase {
    NotBound,
    Binding,
    Bound,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationFilesView {
    pub phase: ActivationFilesPhase,
    pub attempt_id: Option<String>,
    pub set_id: Option<String>,
    pub held_files: u16,
    pub worker: Option<crate::RuntimeImageEvidence>,
    pub manifest_sha256: Option<String>,
    pub original_state_held: bool,
    pub reason: Option<String>,
}
