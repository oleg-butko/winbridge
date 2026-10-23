//! Passive prerequisite assessment. Never an update permit or installation receipt.
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdaterExecutionContract {
    FinalStateBackup,
    RuntimePreparation,
    RuntimeReplacement,
    Activation,
    ControllerConfirmation,
    DesktopUpdate,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdaterCompatibilityPolicy {
    SameSchemaInstallV1,
}

/// Persist the executing updater's capabilities with the assessment. Historical
/// decisions must not be recomputed against a later executable's feature set.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdaterDriverContracts {
    pub schema_version: u16,
    pub build_flavor: super::UpdaterBuildFlavor,
    pub final_state_backup: Option<u16>,
    pub runtime_preparation: Option<u16>,
    pub runtime_replacement: Option<u16>,
    pub activation: Option<u16>,
    pub controller_confirmation: Option<u16>,
    pub desktop_update: Option<u16>,
}
impl UpdaterDriverContracts {
    pub fn implemented_windows() -> Self {
        Self {
            schema_version: 1,
            build_flavor: super::UpdaterBuildFlavor::Standard,
            final_state_backup: Some(15),
            runtime_preparation: Some(16),
            runtime_replacement: Some(17),
            activation: None,
            controller_confirmation: None,
            desktop_update: None,
        }
    }
    pub fn unavailable() -> Self {
        Self {
            schema_version: 1,
            build_flavor: super::UpdaterBuildFlavor::Standard,
            final_state_backup: None,
            runtime_preparation: None,
            runtime_replacement: None,
            activation: None,
            controller_confirmation: None,
            desktop_update: None,
        }
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum UpdaterCompatibilityBlocker {
    UnsupportedQuery {},
    InvalidDriverContext {},
    InvalidDescriptor {},
    UncommittedBuild {},
    ValidationBuild {},
    ValidationExecutor {},
    JournalSchemaMismatch {
        expected: u32,
        actual: u32,
    },
    MissingContract {
        contract: UpdaterExecutionContract,
    },
    ContractVersionMismatch {
        contract: UpdaterExecutionContract,
        expected: u16,
        actual: u16,
    },
    DriverNotImplemented {
        contract: UpdaterExecutionContract,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdaterCompatibilityAssessment {
    pub schema_version: u16,
    /// Frozen meaning: full same-schema set, including desktop coordination.
    pub policy: UpdaterCompatibilityPolicy,
    pub expected_updater_journal_schema: u32,
    pub blockers: Vec<UpdaterCompatibilityBlocker>,
}
