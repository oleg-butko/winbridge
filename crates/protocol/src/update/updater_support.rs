//! Readonly query, never an installation permission or health receipt.
use super::SupervisorProcess;
use crate::ArtifactDescriptor;
use serde::{Deserialize, Serialize};

pub const UPDATER_RUNTIME_SUPPORT_MARKER: &[u8] = b"\0WinBridge.UpdaterRuntimeSupport.contract-1\0";

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdaterBuildFlavor {
    Standard,
    Validation,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdaterRuntimeSupport {
    pub schema_version: u16,
    pub kind: String,
    pub query_contract: u16,
    pub descriptor: ArtifactDescriptor,
    pub process: SupervisorProcess,
    pub updater_journal_schema: u32,
    pub build_flavor: UpdaterBuildFlavor,
    pub final_state_backup_contract: Option<u16>,
    pub runtime_preparation_contract: Option<u16>,
    pub runtime_replacement_contract: Option<u16>,
    pub activation_contract: Option<u16>,
    pub controller_confirmation_contract: Option<u16>,
    pub desktop_update_contract: Option<u16>,
}
