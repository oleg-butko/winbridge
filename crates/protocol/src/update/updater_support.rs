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
impl UpdaterRuntimeSupport {
    /// Data snapshot; the trusted caller must bind it to the executing owner.
    pub fn driver_contracts(&self) -> super::UpdaterDriverContracts {
        super::UpdaterDriverContracts {
            schema_version: 1,
            build_flavor: self.build_flavor,
            final_state_backup: self.final_state_backup_contract,
            runtime_preparation: self.runtime_preparation_contract,
            runtime_replacement: self.runtime_replacement_contract,
            activation: self.activation_contract,
            controller_confirmation: self.controller_confirmation_contract,
            desktop_update: self.desktop_update_contract,
        }
    }
}
