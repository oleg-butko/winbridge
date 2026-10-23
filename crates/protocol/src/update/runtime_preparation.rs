//! Internal v16/view8: saved preparation progress is separate from physical custody.
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePreparationPhase {
    NotAdmitted,
    Admitting,
    Admitted,
    CreatingDirectory,
    PreparingFiles,
    Prepared,
    Indeterminate,
    Blocked,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreparationView {
    pub phase: RuntimePreparationPhase,
    pub attempt_id: Option<String>,
    pub revision: Option<u64>,
    pub directory: Option<String>,
    pub prepared_files: u16,
    pub total_files: u16,
    pub issued_file: Option<String>,
    pub total_bytes: Option<u64>,
    pub reason: Option<String>,
}
