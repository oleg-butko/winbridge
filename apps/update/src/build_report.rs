//! Compiler-owned report receipts. Reading a report never approves installation.
use crate::{journal::UpdateJournal, UpdateConfig};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
};
use winbridge_platform::source_files::directory_identity;
use winbridge_protocol::update::{
    UpdateBuildOutcome, UpdateBuildPhase, UpdateBuildRecord, UpdateBuildSpec,
};
type Result<T> = std::result::Result<T, String>;
pub const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportIntent {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub build_spec_sha256: String,
    pub runtime_identity: String,
    pub update_directory_identity: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportEvidence {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub build_spec_sha256: String,
    pub candidate_directory_identity: String,
    pub file_identity: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSnapshot {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub journal: crate::journal::UpdateJournalInfo,
    pub build: UpdateBuildRecord,
    pub intent: Option<ReportIntent>,
    pub evidence: Option<ReportEvidence>,
}
pub(crate) fn spec_sha256(spec: &UpdateBuildSpec) -> Result<String> {
    Ok(crate::sha256(&serde_json::to_vec(spec).map_err(err)?))
}
fn update_directory(spec: &UpdateBuildSpec) -> PathBuf {
    Path::new(&spec.runtime_directory)
        .join("update")
        .join(&spec.update_id)
}
pub(crate) fn admit(spec: &UpdateBuildSpec) -> Result<ReportIntent> {
    let root = update_directory(spec);
    let candidate = root.join("candidate");
    winbridge_platform::require_safe_path(&candidate).map_err(err)?;
    if candidate.try_exists().map_err(err)? {
        return Err("Candidate report staging already exists; no compiler replay/overwrite".into());
    }
    Ok(ReportIntent {
        schema_version: 1,
        update_id: spec.update_id.clone(),
        attempt_id: spec.attempt_id.clone(),
        build_spec_sha256: spec_sha256(spec)?,
        runtime_identity: directory_identity(Path::new(&spec.runtime_directory)).map_err(err)?,
        update_directory_identity: directory_identity(&root).map_err(err)?,
    })
}
pub(crate) fn guard(spec: &UpdateBuildSpec, intent: &ReportIntent) -> Result<()> {
    if intent.schema_version != 1
        || intent.update_id != spec.update_id
        || intent.attempt_id != spec.attempt_id
        || intent.build_spec_sha256 != spec_sha256(spec)?
        || directory_identity(Path::new(&spec.runtime_directory)).map_err(err)?
            != intent.runtime_identity
        || directory_identity(&update_directory(spec)).map_err(err)?
            != intent.update_directory_identity
    {
        return Err("Compiler report roots/attempt identity changed; preserve barrier".into());
    }
    Ok(())
}
/// Capture bytes and evidence in one bounded read. No JSON semantics are trusted here.
pub(crate) fn capture(
    spec: &UpdateBuildSpec,
    intent: &ReportIntent,
) -> Result<(ReportEvidence, Vec<u8>)> {
    guard(spec, intent)?;
    let candidate = update_directory(spec).join("candidate");
    let identity = directory_identity(&candidate).map_err(err)?;
    let path = candidate.join("candidate-report.json");
    winbridge_platform::require_safe_path(&path).map_err(err)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options.open(&path).map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_REPORT_BYTES {
        return Err("Compiler report must be a nonempty bounded regular file".into());
    }
    let file_identity = winbridge_platform::file_identity(&file).map_err(err)?;
    let mut bytes = vec![];
    (&mut file)
        .take(MAX_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    guard(spec, intent)?;
    winbridge_platform::require_safe_path(&path).map_err(err)?;
    if bytes.len() as u64 != meta.len()
        || file.metadata().map_err(err)?.len() != meta.len()
        || directory_identity(&candidate).map_err(err)? != identity
        || winbridge_platform::file_identity(&options.open(&path).map_err(err)?).map_err(err)?
            != file_identity
    {
        return Err("Compiler report changed during capture".into());
    }
    Ok((
        ReportEvidence {
            schema_version: 1,
            update_id: spec.update_id.clone(),
            attempt_id: spec.attempt_id.clone(),
            build_spec_sha256: intent.build_spec_sha256.clone(),
            candidate_directory_identity: identity,
            file_identity,
            bytes: bytes.len() as u64,
            sha256: crate::sha256(&bytes),
        },
        bytes,
    ))
}
/// Return the exact bytes verified against the durable compiler completion receipt.
/// A successful compiler/report is not candidate verification or worker health.
pub fn read_verified(cfg: &UpdateConfig, id: &str) -> Result<(ReportEvidence, Vec<u8>)> {
    let directory = cfg.runtime_directory.join("update-state");
    let ReportSnapshot {
        journal: info,
        build,
        intent,
        evidence: saved,
        ..
    } = UpdateJournal::inspect_build_report(&directory, id).map_err(err)?;
    if info.scope_sha256 != crate::installation_scope(cfg)?
        || cfg.runtime_directory.canonicalize().map_err(err)?
            != Path::new(&build.spec.runtime_directory)
        || cfg.source_directory.canonicalize().map_err(err)?
            != Path::new(&build.spec.source_directory)
        || build.phase != UpdateBuildPhase::Completed
        || build.outcome != Some(UpdateBuildOutcome::Succeeded)
        || !build.cleanup_verified
    {
        return Err("Completed successful compiler in this installation required".into());
    }
    let intent = intent.ok_or(
        "Compiler did not admit a report; legacy/generic build cannot approve a candidate",
    )?;
    let saved = saved.ok_or("Compiler report receipt missing; no adoption/replay")?;
    let (actual, bytes) = capture(&build.spec, &intent)?;
    if actual != saved {
        return Err("Compiler report differs from durable receipt; no candidate approval".into());
    }
    Ok((saved, bytes))
}
pub(crate) fn successful(build: &UpdateBuildRecord) -> bool {
    build.phase == UpdateBuildPhase::Completed
        && build.outcome == Some(UpdateBuildOutcome::Succeeded)
}
