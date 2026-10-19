//! Readonly candidate config probe. No live journal, migration or acceptance is performed.
use crate::artifact::{self, ImageEvidence};
use serde::Serialize;
use std::path::Path;
use winbridge_protocol::{ArtifactRole, WorkerConfigCheckReport, WORKER_CONFIG_CHECK_MARKER};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerConfigInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub image: ImageEvidence,
    pub check: WorkerConfigCheckReport,
    pub cleanup_verified: bool,
    pub journal_bound: bool,
    pub candidate_verified: bool,
    pub runtime_applied: bool,
}
pub fn inspect_config(
    image: &Path,
    image_sha256: &str,
    config: &Path,
    config_sha256: &str,
) -> Result<WorkerConfigInspection> {
    if ![image_sha256, config_sha256]
        .iter()
        .all(|s| winbridge_core::update::valid_hash(s, &[64]))
    {
        return Err("Pinned worker image and config SHA-256 required".into());
    }
    let pin = artifact::pin(image)?;
    if pin.evidence.sha256 != image_sha256 {
        return Err("Candidate worker SHA mismatch; no process started".into());
    }
    if !crate::worker_rehearsal::supports(&pin, WORKER_CONFIG_CHECK_MARKER)? {
        return Err("UnsupportedWorkerConfigCheck: marker absent; executable not started".into());
    }
    let config_pin = artifact::pin(config)?;
    if config_pin.evidence.bytes > 64 * 1024 || config_pin.evidence.sha256 != config_sha256 {
        return Err("Pinned config size/SHA mismatch; no process started".into());
    }
    let identity = artifact::query_image(image, ArtifactRole::Worker)?;
    if identity.image != pin.evidence {
        return Err("Candidate worker changed during readonly query".into());
    }
    let (stdout, stderr, process) = artifact::run_probe(
        &pin,
        &[
            "--check-update-config".into(),
            config_pin.evidence.path.clone(),
            config_sha256.into(),
        ],
        10000,
    )?;
    let check: WorkerConfigCheckReport =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid worker config check report/fields")?;
    if !stderr.is_empty()
        || check.schema_version != 1
        || check.kind != "worker_config_check"
        || check.descriptor != identity.descriptor
        || check.process != process
        || check.config != config_pin.evidence
        || check.worker_config_schema != 1
        || !winbridge_core::config::valid_id(&check.instance_id)
        || check.worker_journal_opened
        || check.scheduler_started
        || check.candidate_verified
        || check.runtime_applied
        || artifact::inspect_image(image)? != pin.evidence
        || artifact::inspect_image(config)? != config_pin.evidence
    {
        return Err("Worker config check image/process/bytes/read-only proof rejected".into());
    }
    Ok(WorkerConfigInspection {
        schema_version: 1,
        kind: "worker_config_inspection",
        read_only: true,
        image: pin.evidence,
        check,
        cleanup_verified: true,
        journal_bound: false,
        candidate_verified: false,
        runtime_applied: false,
    })
}
