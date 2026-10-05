//! Snapshot metadata contract. Import is not implemented/advertised by these DTOs.
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SnapshotSource {
    GitCommit { commit: String },
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotFile {
    /// Canonical UTF-8 path with '/' separators, relative to the payload root.
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub executable: bool,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotManifest {
    pub schema_version: u16,
    pub project_id: String,
    pub snapshot_id: String,
    pub source: SnapshotSource,
    /// payload.zip is fixed by the format; no caller-supplied filesystem path.
    pub payload_bytes: u64,
    pub payload_sha256: String,
    pub total_bytes: u64,
    pub files: Vec<SnapshotFile>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotReady {
    pub schema_version: u16,
    pub project_id: String,
    pub snapshot_id: String,
    pub manifest_sha256: String,
    pub payload_sha256: String,
    pub payload_bytes: u64,
}
