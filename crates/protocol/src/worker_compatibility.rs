//! Readonly compatibility observations. Neither type permits journal migration or install.
use crate::{
    update::SupervisorProcess, ArtifactDescriptor, LoadedWorkerIdentity, RequestContext,
    RuntimeImageEvidence,
};
use serde::{Deserialize, Serialize};

pub const WORKER_CONFIG_CHECK_MARKER: &[u8] = b"\0WinBridge.WorkerConfigCheck.contract-1\0";

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerRuntimeCompatibility {
    pub schema_version: u16,
    pub kind: String,
    pub identity: LoadedWorkerIdentity,
    pub context: RequestContext,
    pub generation: String,
    pub worker_config_sha256: String,
    pub worker_config_schema: u16,
    pub worker_journal_schema: u32,
    pub updater_journal_schema: u32,
    pub protocol_version: u16,
    pub observed_at_ms: u64,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerConfigCheckReport {
    pub schema_version: u16,
    pub kind: String,
    pub descriptor: ArtifactDescriptor,
    pub process: SupervisorProcess,
    pub config: RuntimeImageEvidence,
    pub worker_config_schema: u16,
    pub instance_id: String,
    pub worker_journal_opened: bool,
    pub scheduler_started: bool,
    pub candidate_verified: bool,
    pub runtime_applied: bool,
}

impl WorkerRuntimeCompatibility {
    /// Structure and frozen scope only. Caller must authenticate the process
    /// and separately compare candidate schemas; this is never a file grant.
    pub fn validate_binding(
        &self,
        binding: &crate::update::SupervisorWorkerBinding,
    ) -> Result<(), String> {
        if self.schema_version != 1
            || self.kind != "worker_runtime_compatibility"
            || self.identity.schema_version != 1
            || self.identity.kind != "loaded_worker_identity"
            || self.identity.process != binding.process
            || self.identity.descriptor.build != binding.build
            || self.context != binding.context
            || self.generation != binding.generation
            || self.worker_config_sha256 != binding.config_sha256
            || self.worker_config_schema != 1
            || self.worker_journal_schema == 0
            || self.updater_journal_schema == 0
            || self.protocol_version != crate::PROTOCOL_VERSION
            || self.observed_at_ms == 0
        {
            return Err(
                "Runtime compatibility contract/context/build/config changed; no rebind".into(),
            );
        }
        Ok(())
    }
}
