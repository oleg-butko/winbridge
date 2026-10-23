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
    InvalidDescriptor {},
    UncommittedBuild {},
    ValidationBuild {},
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
