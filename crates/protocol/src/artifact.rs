//! Explicit readonly executable identity. Legacy BuildIdentity remains unchanged.
use crate::BuildIdentity;
use serde::{Deserialize, Serialize};

/// Capability marker checked in the file BEFORE querying an executable. In
/// particular, an older desktop must never be launched with an unknown flag.
pub const ARTIFACT_QUERY_MARKER: &[u8] = b"\0WinBridge.ArtifactQuery.contract-1\0";
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRole {
    Worker,
    Fixture,
    Desktop,
    Updater,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDescriptor {
    pub schema_version: u16,
    pub kind: String,
    pub query_contract: u16,
    pub role: ArtifactRole,
    pub build: BuildIdentity,
    pub target_os: String,
    pub target_arch: String,
}
pub fn artifact_descriptor(role: ArtifactRole) -> ArtifactDescriptor {
    // Keep the marker in each binary that actually wires this query. It is
    // capability detection, not a signature or permission to trust the payload.
    std::hint::black_box(ARTIFACT_QUERY_MARKER);
    ArtifactDescriptor {
        schema_version: 1,
        kind: "winbridge_artifact".into(),
        query_contract: 1,
        role,
        build: crate::build_identity(),
        target_os: std::env::consts::OS.into(),
        target_arch: std::env::consts::ARCH.into(),
    }
}
