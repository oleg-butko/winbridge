//! Passive v18/view10 projection; no caller-supplied report or install authority.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSupportViewPhase {
    NotAdmitted,
    Admitting,
    Admitted,
    Creating,
    ResumeGranted,
    Running,
    Captured,
    Checked,
    Unsupported,
    Rejected,
    Cancelled,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSupportView {
    pub phase: CandidateSupportViewPhase,
    pub revision: Option<u64>,
    pub attempt_id: Option<String>,
    pub command: Option<crate::ResolvedCommand>,
    pub process: Option<SupervisorProcess>,
    pub report: Option<UpdaterRuntimeSupport>,
    pub assessment: Option<UpdaterCompatibilityAssessment>,
    /// Saved cleanup evidence; not an observation of current physical custody.
    pub cleanup_verified: bool,
    pub reason: Option<String>,
}
