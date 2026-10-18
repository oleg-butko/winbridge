//! Readonly observations, never install/shutdown authority or a file handle.
use crate::{update::SupervisorProcess, ArtifactDescriptor};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeImageEvidence {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub file_identity: String,
    pub query_supported: bool,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadedWorkerIdentity {
    pub schema_version: u16,
    pub kind: String,
    pub process: SupervisorProcess,
    pub runtime_identity: String,
    pub set_id: String,
    pub manifest_sha256: String,
    pub descriptor: ArtifactDescriptor,
    pub image: RuntimeImageEvidence,
}
