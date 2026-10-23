//! Internal v17/view9. Installed files are not activated by this contract.
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeReplacementPhase {
    NotAdmitted,
    Admitting,
    Acquiring,
    CreatingDirectory,
    MovingFiles,
    FilesMoved,
    Indeterminate,
    Blocked,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeReplacementMove {
    Retire,
    Place,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReplacementView {
    pub phase: RuntimeReplacementPhase,
    pub attempt_id: Option<String>,
    pub revision: Option<u64>,
    pub directory: Option<String>,
    pub retired_files: u16,
    pub installed_files: u16,
    pub total_files: u16,
    pub issued_file: Option<String>,
    pub issued_move: Option<RuntimeReplacementMove>,
    pub physical_custody_held: bool,
    pub reason: Option<String>,
}
