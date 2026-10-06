//! Immutable source delivery and durable import contracts.
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

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportSubmission {
    pub protocol_version: u16,
    pub request_id: String,
    pub project_id: String,
    pub snapshot_id: String,
    pub manifest_sha256: String,
    pub idempotency_key: String,
    pub timeout_ms: u64,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub import_id: String,
    pub submission: ImportSubmission,
    pub status: crate::TaskStatus,
    pub phase: String,
    pub accepted_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub cancel_requested: bool,
    pub completed_files: u64,
    pub completed_bytes: u64,
    pub total_files: Option<u64>,
    pub total_bytes: Option<u64>,
    pub commit: Option<String>,
    pub shared_path: String,
    pub workspace_path: String,
    pub reason: Option<String>,
    pub events: Vec<crate::JournalEvent>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPreview {
    pub project_id: String,
    pub snapshot_id: String,
    pub manifest_sha256: String,
    pub source: SnapshotSource,
    pub total_files: u64,
    pub total_bytes: u64,
    pub payload_bytes: u64,
    pub shared_path: String,
    pub workspace_path: String,
    pub files: Vec<SnapshotFile>,
    pub next_offset: Option<u64>,
    pub payload_verified: bool,
    pub peer_shared_path: Option<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedSnapshotRef {
    pub snapshot_id: String,
    pub manifest_sha256: String,
    pub source: SnapshotSource,
}

/// Metadata discovery only; payload bytes are verified during import.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedSnapshot {
    pub snapshot_id: String,
    pub source: Option<SnapshotSource>,
    pub total_files: Option<u64>,
    pub total_bytes: Option<u64>,
    pub payload_bytes: Option<u64>,
    pub metadata_error: Option<String>,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedSnapshotPage {
    pub project_id: String,
    pub shared_path: String,
    pub snapshots: Vec<PublishedSnapshot>,
    pub next_offset: Option<u64>,
}
