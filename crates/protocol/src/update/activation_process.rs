//! Readonly internal22/view14. None means unknown, not permission to retry.
use super::*;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationProcessPhase {
    NotStarted,
    Launching,
    ReadyForState,
    Indeterminate,
    NotResident,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationDurablePhase {
    Prepared,
    Creating,
    Created,
    ResumeGranted,
    ReadyForState,
    HandoffIssued,
    StateOwned,
    LocalVerified,
    CreationFailed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationProcessView {
    pub phase: ActivationProcessPhase,
    pub attempt_id: Option<String>,
    pub revision: Option<u64>,
    pub durable_phase: Option<ActivationDurablePhase>,
    pub saved_process: Option<SupervisorProcess>,
    pub job_name: Option<String>,
    pub saved_ready: Option<crate::LoadedWorkerIdentity>,
    /// Actual retained child identity, never reconstructed from saved_process.
    pub resident_process: Option<SupervisorProcess>,
    /// None while a blocking owner is not observed; false on a cold reader.
    pub process_custody_held: Option<bool>,
    pub process_alive: Option<bool>,
    pub original_state_held: bool,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
    pub output_incomplete: Option<bool>,
    pub capture_complete: Option<bool>,
    pub reason: Option<String>,
}
