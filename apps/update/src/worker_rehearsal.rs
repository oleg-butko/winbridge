//! Standalone diagnostic, not journal-bound candidate approval. No production DB/config is read.
use crate::artifact::{self, ImageEvidence, Pinned};
use serde::Serialize;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use winbridge_protocol::{ArtifactRole, WorkerRehearsalReport, WORKER_REHEARSAL_MARKER};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRehearsalInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub image: ImageEvidence,
    pub rehearsal: WorkerRehearsalReport,
    pub expected_worker_journal_schema: u32,
    pub expected_updater_journal_schema: u32,
    pub same_schema: bool,
    pub cleanup_verified: bool,
    pub journal_bound: bool,
    pub candidate_verified: bool,
    pub runtime_applied: bool,
}
fn supports(pin: &Pinned) -> Result<bool> {
    // Inspect the retained handle, with bounded memory, before executing any
    // switch that an older binary might interpret as a normal start.
    let mut file = pin.file.try_clone().map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut tail = Vec::new();
    let mut buffer = [0; 65536];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(false);
        }
        total += n as u64;
        if total > pin.evidence.bytes {
            return Err("Rehearsal image grew".into());
        }
        tail.extend_from_slice(&buffer[..n]);
        if tail
            .windows(WORKER_REHEARSAL_MARKER.len())
            .any(|w| w == WORKER_REHEARSAL_MARKER)
        {
            return Ok(true);
        }
        let retain = WORKER_REHEARSAL_MARKER.len() - 1;
        if tail.len() > retain {
            tail.drain(..tail.len() - retain);
        }
    }
}
/// The expected image SHA must come from the trusted candidate artifact set.
/// Explicit schema numbers are diagnostic expectations, not live-worker evidence.
/// This function cannot save a health receipt or advance an update FSM.
pub fn inspect(
    image: &Path,
    expected_sha256: &str,
    directory: &Path,
    worker_schema: u32,
    updater_schema: u32,
) -> Result<WorkerRehearsalInspection> {
    if !winbridge_core::update::valid_hash(expected_sha256, &[64])
        || worker_schema == 0
        || updater_schema == 0
    {
        return Err("Pinned image SHA and nonzero expected schemas required".into());
    }
    if !directory.is_absolute()
        || directory
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Rehearsal requires an absolute new directory without traversal".into());
    }
    winbridge_platform::require_local_path_prefix(directory).map_err(|e| e.to_string())?;
    winbridge_platform::require_safe_path(directory).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(directory).map_err(|e| e.to_string())?;
    if directory.try_exists().map_err(|e| e.to_string())? {
        return Err("Existing rehearsal directory is preserved; never replay or adopt it".into());
    }
    let pin = artifact::pin(image)?;
    if pin.evidence.sha256 != expected_sha256 {
        return Err("Rehearsal image SHA mismatch; executable not started".into());
    }
    if !supports(&pin)? {
        return Err("UnsupportedWorkerRehearsal: marker absent; executable not started".into());
    }
    let identity = artifact::query_image(image, ArtifactRole::Worker)?;
    if identity.image != pin.evidence {
        return Err("Rehearsal image changed during descriptor query".into());
    }
    let (stdout, _stderr, process) = artifact::run_probe(
        &pin,
        &[
            "--rehearsal".into(),
            directory
                .to_str()
                .ok_or("Rehearsal path must be UTF-8")?
                .into(),
        ],
        30000,
    )?;
    let rehearsal: WorkerRehearsalReport =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid worker rehearsal report/fields")?;
    let actual = directory.canonicalize().map_err(|e| e.to_string())?;
    let reported = Path::new(&rehearsal.directory);
    let same_path = if cfg!(windows) {
        reported
            .to_str()
            .zip(actual.to_str())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    } else {
        reported == actual
    };
    if rehearsal.schema_version != 1
        || rehearsal.kind != "worker_rehearsal"
        || rehearsal.contract_version != 1
        || rehearsal.descriptor != identity.descriptor
        || rehearsal.process != process
        || !same_path
        || rehearsal.directory_identity
            != winbridge_platform::source_files::directory_identity(&actual)
                .map_err(|e| e.to_string())?
        || !rehearsal.instance_id.starts_with("rehearsal-")
        || uuid::Uuid::parse_str(rehearsal.instance_id.trim_start_matches("rehearsal-")).is_err()
        || uuid::Uuid::parse_str(&rehearsal.generation).is_err()
        || uuid::Uuid::parse_str(&rehearsal.journal_incarnation).is_err()
        || rehearsal.worker_journal_schema != worker_schema
        || rehearsal.updater_journal_schema != updater_schema
        || rehearsal.worker_config_schema != 1
        || rehearsal.protocol_version != winbridge_protocol::PROTOCOL_VERSION
        || !rehearsal.fresh_journal
        || !rehearsal.ipc_ready
        || !rehearsal.server_stopped
        || rehearsal.controller_enabled
        || rehearsal.direct_commands_enabled
        || rehearsal.production_state_opened
        || rehearsal.candidate_verified
        || rehearsal.runtime_applied
        || artifact::inspect_image(image)? != pin.evidence
    {
        return Err(
            "Worker rehearsal identity/isolation/schema proof rejected; no candidate approval"
                .into(),
        );
    }
    Ok(WorkerRehearsalInspection {
        schema_version: 1,
        kind: "worker_rehearsal_inspection",
        image: pin.evidence,
        rehearsal,
        expected_worker_journal_schema: worker_schema,
        expected_updater_journal_schema: updater_schema,
        same_schema: true,
        cleanup_verified: true,
        journal_bound: false,
        candidate_verified: false,
        runtime_applied: false,
    })
}
