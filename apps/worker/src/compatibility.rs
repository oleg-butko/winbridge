//! Read exactly the pinned config bytes and validate them without Worker::open.
use crate::Result;
use std::{io::Read, path::Path};
use winbridge_protocol::{artifact_descriptor, ArtifactRole, WorkerConfigCheckReport};

pub fn check_config(path: &Path, expected_sha256: &str) -> Result<WorkerConfigCheckReport> {
    load_checked_config(path, expected_sha256).map(|(_, report)| report)
}
pub(crate) fn load_checked_config(
    path: &Path,
    expected_sha256: &str,
) -> Result<(
    winbridge_core::config::WorkerConfig,
    WorkerConfigCheckReport,
)> {
    std::hint::black_box(winbridge_protocol::WORKER_CONFIG_CHECK_MARKER);
    if !winbridge_core::update::valid_hash(expected_sha256, &[64]) {
        return Err("Expected lowercase worker config SHA-256".into());
    }
    let before = winbridge_update::artifact::inspect_image(path)?;
    if before.bytes > 64 * 1024 || before.sha256 != expected_sha256 {
        return Err("Worker config size/SHA mismatch; no journal opened".into());
    }
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 || winbridge_update::sha256(&bytes) != expected_sha256 {
        return Err("Worker config bytes changed; no journal opened".into());
    }
    let config: winbridge_core::config::WorkerConfig = serde_json::from_slice(&bytes)
        .map_err(|_| "Worker config format/fields rejected; no journal opened")?;
    config
        .validate()
        .map_err(|_| "Worker config schema/project policy rejected; no journal opened")?;
    if winbridge_update::artifact::inspect_image(path)? != before {
        return Err("Worker config identity changed; no journal opened".into());
    }
    let report = WorkerConfigCheckReport {
        schema_version: 1,
        kind: "worker_config_check".into(),
        descriptor: artifact_descriptor(ArtifactRole::Worker),
        process: winbridge_platform::infrastructure::stamp(std::process::id())
            .map_err(|e| e.to_string())?,
        config: before,
        worker_config_schema: config.schema_version,
        instance_id: config.instance_id.clone(),
        worker_journal_opened: false,
        scheduler_started: false,
        candidate_verified: false,
        runtime_applied: false,
    };
    Ok((config, report))
}
