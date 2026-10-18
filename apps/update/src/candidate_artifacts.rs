//! Verify exact compiler-report bytes against payload files. No processes or install effects.
use crate::{
    artifact::{ImageEvidence, Pinned},
    build_report::ReportEvidence,
    UpdateConfig,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winbridge_platform::source_files::directory_identity;
use winbridge_protocol::{
    update::SupervisorProcess, ArtifactDescriptor, ArtifactRole, BuildIdentity,
};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
const ARTIFACTS: [(&str, ArtifactRole); 4] = [
    ("winbridge-worker.exe", ArtifactRole::Worker),
    ("winbridge-fixture.exe", ArtifactRole::Fixture),
    ("winbridge-desktop.exe", ArtifactRole::Desktop),
    ("winbridge-update.exe", ArtifactRole::Updater),
];
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BuildReport {
    schema_version: u16,
    kind: String,
    status: String,
    runtime_applied: bool,
    candidate_verified: bool,
    source_directory: PathBuf,
    manifest_sha256: String,
    #[serde(rename = "storage")]
    _storage: serde_json::Value,
    commands: Vec<Command>,
    artifacts: Vec<Summary>,
    error: Option<String>,
    #[serde(default, rename = "validationCacheDirectory")]
    _validation_cache_directory: Option<PathBuf>,
    source: SourceProof,
    worker_build: BuildIdentity,
    artifact_identities: Vec<NamedQuery>,
    artifact_identity_checked: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Command {
    name: String,
    executable: String,
    arguments: Vec<String>,
    log: String,
    exit_code: i32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceProof {
    schema_version: u16,
    kind: String,
    read_only: bool,
    source_commit: String,
    manifest_sha256: String,
    includes_built_ui: bool,
    file_count: usize,
    bytes: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Summary {
    name: String,
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NamedQuery {
    name: String,
    query: Query,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Query {
    schema_version: u16,
    kind: String,
    read_only: bool,
    candidate_verified: bool,
    image: ImageEvidence,
    descriptor: ArtifactDescriptor,
    process: SupervisorProcess,
    cleanup_verified: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateArtifact {
    pub name: String,
    pub image: ImageEvidence,
    pub descriptor: ArtifactDescriptor,
    pub query_process: SupervisorProcess,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateArtifactInspection {
    pub schema_version: u16,
    pub kind: String,
    pub read_only: bool,
    pub runtime_applied: bool,
    pub candidate_verified: bool,
    pub report: ReportEvidence,
    pub payload_directory_identity: String,
    pub artifacts: Vec<CandidateArtifact>,
    pub set_sha256: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateFileInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub journal_bound: bool,
    pub candidate_verified: bool,
    pub runtime_applied: bool,
    pub source: crate::VerifiedSource,
    pub stage_directory_identity: String,
    pub payload_directory_identity: String,
    pub report: ImageEvidence,
    pub artifacts: Vec<CandidateArtifact>,
}
/// Standalone diagnostics for build-only staging, explicitly lacking journal authority.
/// Fixed filenames only; source/report hashes are required, no code is executed.
pub fn inspect_candidate_files(
    source: &Path,
    manifest_sha256: &str,
    stage: &Path,
    report_sha256: &str,
) -> Result<CandidateFileInspection> {
    if !winbridge_core::update::valid_hash(report_sha256, &[64]) {
        return Err("Expected lowercase compiler report SHA-256".into());
    }
    let stage_identity = directory_identity(stage).map_err(err)?;
    let verified = crate::verify_source(source, manifest_sha256)?;
    let source = source.canonicalize().map_err(err)?;
    let stage = stage.canonicalize().map_err(err)?;
    if source.starts_with(&stage) || stage.starts_with(&source) {
        return Err("Source and candidate staging must not overlap".into());
    }
    let report_path = stage.join("candidate-report.json");
    let report_pin = crate::artifact::pin(&report_path)?;
    if report_pin.evidence.bytes > crate::build_report::MAX_REPORT_BYTES
        || report_pin.evidence.sha256 != report_sha256
    {
        return Err("Standalone compiler report size/hash mismatch".into());
    }
    let bytes = crate::read(&report_path, crate::build_report::MAX_REPORT_BYTES)?;
    if crate::sha256(&bytes) != report_sha256 {
        return Err("Compiler report changed during standalone capture".into());
    }
    let payload = stage.join("payload");
    let payload_identity = directory_identity(&payload).map_err(|e| {
        format!(
            "Candidate payload directory unavailable: {}: {e}",
            payload.display()
        )
    })?;
    let (artifacts, _pins) = check_report(&verified, &source, &payload, &bytes)?;
    for a in &artifacts {
        if crate::artifact::inspect_image(Path::new(&a.image.path))? != a.image {
            return Err("Candidate image changed during standalone inspection".into());
        }
    }
    check_payload_entries(&payload)?;
    if directory_identity(&stage).map_err(err)? != stage_identity
        || directory_identity(&payload).map_err(err)? != payload_identity
        || crate::artifact::inspect_image(&report_path)? != report_pin.evidence
    {
        return Err("Candidate/report root identity changed".into());
    }
    crate::verify_source(&source, manifest_sha256)?;
    Ok(CandidateFileInspection {
        schema_version: 1,
        kind: "candidate_file_inspection",
        read_only: true,
        journal_bound: false,
        candidate_verified: false,
        runtime_applied: false,
        source: verified,
        stage_directory_identity: stage_identity,
        payload_directory_identity: payload_identity,
        report: report_pin.evidence.clone(),
        artifacts,
    })
}
pub(crate) fn fingerprint(value: &CandidateArtifactInspection) -> Result<String> {
    Ok(crate::sha256(
        &serde_json::to_vec(&(
            &value.report,
            &value.payload_directory_identity,
            &value.artifacts,
        ))
        .map_err(err)?,
    ))
}
pub(crate) fn validate_saved(
    value: &CandidateArtifactInspection,
    build: &winbridge_protocol::update::UpdateBuildSpec,
    target: &winbridge_protocol::update::UpdateSourcePin,
) -> Result<()> {
    if value.schema_version != 1
        || value.kind != "candidate_artifact_inspection"
        || !value.read_only
        || value.runtime_applied
        || value.candidate_verified
        || value.report.update_id != build.update_id
        || value.report.attempt_id != build.attempt_id
        || value.report.build_spec_sha256 != crate::build_report::spec_sha256(build)?
        || value.payload_directory_identity.is_empty()
        || value.payload_directory_identity.len() > 128
        || value.artifacts.len() != 4
        || fingerprint(value)? != value.set_sha256
    {
        return Err("Candidate artifact-set shape/fingerprint rejected".into());
    }
    let payload = Path::new(&build.runtime_directory)
        .join("update")
        .join(&build.update_id)
        .join("candidate/payload");
    for (a, (name, role)) in value.artifacts.iter().zip(ARTIFACTS) {
        let d = &a.descriptor;
        if a.name != name
            || d.role != role
            || !same_path(Path::new(&a.image.path), &payload.join(name))?
            || !a.image.query_supported
            || a.image.bytes == 0
            || a.image.bytes > 256 * 1024 * 1024
            || !winbridge_core::update::valid_hash(&a.image.sha256, &[64])
            || a.image.file_identity.is_empty()
            || a.image.file_identity.len() > 128
            || d.schema_version != 1
            || d.kind != "winbridge_artifact"
            || d.query_contract != 1
            || d.target_os != std::env::consts::OS
            || d.target_arch != std::env::consts::ARCH
            || d.build.source_dirty
            || d.build.source_commit.as_deref() != Some(target.source_commit.as_str())
            || d.build.source_manifest_sha256.as_deref() != Some(target.manifest_sha256.as_str())
            || d.build.version.is_empty()
            || d.build.version.len() > 128
            || d.build.version != value.artifacts[0].descriptor.build.version
            || d.build.build_id.is_empty()
            || d.build.build_id.len() > 128
            || a.query_process.process_id == 0
            || a.query_process.birth.is_empty()
            || a.query_process.birth.len() > 128
            || !a.query_process.birth.bytes().all(|v| v.is_ascii_digit())
            || !winbridge_core::update::valid_hash(&a.query_process.host_id, &[64])
        {
            return Err("Candidate artifact-set role/path/build evidence rejected".into());
        }
    }
    Ok(())
}
fn same_path(reported: &Path, actual: &Path) -> Result<bool> {
    // Compare spellings without resolving any report-controlled path on disk.
    let a = crate::preparation::tool_path(reported)?;
    let b = crate::preparation::tool_path(actual)?;
    Ok(if cfg!(windows) {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    })
}
fn build_matches(build: &BuildIdentity, source: &crate::VerifiedSource) -> bool {
    !build.source_dirty
        && build.source_commit.as_deref() == Some(source.source_commit.as_str())
        && build.source_manifest_sha256.as_deref() == Some(source.manifest_sha256.as_str())
        && !build.version.is_empty()
        && build.version.len() <= 128
        && !build.build_id.is_empty()
        && build.build_id.len() <= 128
}
fn check_payload_entries(payload: &Path) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(payload)
        .map_err(err)?
        .take(5)
        .collect::<std::io::Result<_>>()
        .map_err(err)?;
    if entries.len() != 4
        || entries
            .iter()
            .any(|e| !ARTIFACTS.iter().any(|(name, _)| e.file_name() == *name))
    {
        return Err(format!("Candidate payload must contain only the four fixed artifact files; observed entries (up to 5): {}",entries.iter().map(|e|e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ")));
    }
    Ok(())
}
fn check_report(
    source: &crate::VerifiedSource,
    source_directory: &Path,
    payload: &Path,
    bytes: &[u8],
) -> Result<(Vec<CandidateArtifact>, Vec<Pinned>)> {
    if bytes.len() as u64 > crate::build_report::MAX_REPORT_BYTES {
        return Err("Candidate report exceeds byte limit".into());
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let report: BuildReport = serde_json::from_slice(bytes)
        .map_err(|e| format!("Candidate report contract/JSON rejected: {e}"))?;
    check_payload_entries(payload)?;
    if report.schema_version != 1
        || report.kind != "winbridge-update-candidate"
        || report.status != "built"
        || report.runtime_applied
        || report.candidate_verified
        || report.error.is_some()
        || !report.artifact_identity_checked
        || report.manifest_sha256 != source.manifest_sha256
        || !same_path(&report.source_directory, source_directory)?
        || report.source.schema_version != 1
        || report.source.kind != "source_verified"
        || !report.source.read_only
        || report.source.source_commit != source.source_commit
        || report.source.manifest_sha256 != source.manifest_sha256
        || report.source.includes_built_ui != source.includes_built_ui
        || report.source.file_count != source.file_count
        || report.source.bytes != source.bytes
        || !build_matches(&report.worker_build, source)
        || report.artifacts.len() != 4
        || report.artifact_identities.len() != 4
        || report.commands.is_empty()
        || report.commands.len() > 32
        || report.commands.iter().any(|c| {
            c.exit_code != 0
                || c.name.is_empty()
                || c.name.len() > 128
                || c.executable.is_empty()
                || c.executable.len() > 4096
                || c.arguments.len() > 256
                || c.arguments.iter().any(|s| s.len() > 8192)
                || c.log.is_empty()
                || c.log.len() > 128
        })
    {
        return Err(
            "Candidate report source/build/roles/command completion differs from pinned compiler"
                .into(),
        );
    }
    let mut artifacts = vec![];
    let mut pins = vec![];
    for (name, role) in ARTIFACTS {
        let summaries: Vec<_> = report.artifacts.iter().filter(|f| f.name == name).collect();
        let queries: Vec<_> = report
            .artifact_identities
            .iter()
            .filter(|f| f.name == name)
            .collect();
        if summaries.len() != 1 || queries.len() != 1 {
            return Err(
                "Candidate requires exactly one entry for every fixed artifact name".into(),
            );
        }
        let summary = summaries[0];
        let q = &queries[0].query;
        let pin = crate::artifact::pin(&payload.join(name))?;
        let image = &pin.evidence;
        let d = &q.descriptor;
        if q.schema_version != 1
            || q.kind != "artifact_query"
            || !q.read_only
            || q.candidate_verified
            || !q.cleanup_verified
            || !q.image.query_supported
            || !image.query_supported
            || !same_path(Path::new(&q.image.path), Path::new(&image.path))?
            || q.image.bytes != image.bytes
            || q.image.sha256 != image.sha256
            || q.image.file_identity != image.file_identity
            || summary.bytes != image.bytes
            || summary.sha256 != image.sha256
            || d.schema_version != 1
            || d.kind != "winbridge_artifact"
            || d.query_contract != 1
            || d.role != role
            || d.target_os != std::env::consts::OS
            || d.target_arch != std::env::consts::ARCH
            || !build_matches(&d.build, source)
            || d.build.version != report.worker_build.version
            || (role == ArtifactRole::Worker && d.build != report.worker_build)
            || q.process.process_id == 0
            || q.process.birth.is_empty()
            || !q.process.birth.bytes().all(|v| v.is_ascii_digit())
            || !winbridge_core::update::valid_hash(&q.process.host_id, &[64])
        {
            return Err(format!(
                "Candidate artifact identity/hash/query proof rejected: {name}"
            ));
        }
        artifacts.push(CandidateArtifact {
            name: name.into(),
            image: image.clone(),
            descriptor: d.clone(),
            query_process: q.process.clone(),
        });
        pins.push(pin);
    }
    Ok((artifacts, pins))
}
/// A readonly observation. It grants no candidate/install transition and launches nothing.
pub fn inspect_candidate(cfg: &UpdateConfig, id: &str) -> Result<CandidateArtifactInspection> {
    Ok(inspect_pinned(cfg, id)?.0)
}
pub(crate) fn inspect_pinned(
    cfg: &UpdateConfig,
    id: &str,
) -> Result<(CandidateArtifactInspection, Vec<Pinned>)> {
    let (report, bytes) = crate::build_report::read_verified(cfg, id)?;
    let view = crate::journal::UpdateJournal::inspect_build_report(
        &cfg.runtime_directory.join("update-state"),
        id,
    )
    .map_err(err)?;
    if view.evidence.as_ref() != Some(&report) {
        return Err("Compiler report snapshot changed; no adoption".into());
    }
    let source = cfg.source_directory.canonicalize().map_err(err)?;
    let verified = crate::verify_source(&source, &view.build.spec.source_manifest_sha256)?;
    let candidate = cfg
        .runtime_directory
        .join("update")
        .join(&report.update_id)
        .join("candidate");
    let payload = candidate.join("payload");
    let payload_identity = directory_identity(&payload).map_err(|e| {
        format!(
            "Candidate payload directory unavailable: {}: {e}",
            payload.display()
        )
    })?;
    let (artifacts, pins) = check_report(&verified, &source, &payload, &bytes)?;
    // Recheck every retained image and root while Windows pins still deny write/delete.
    for a in &artifacts {
        if crate::artifact::inspect_image(Path::new(&a.image.path))? != a.image {
            return Err("Candidate changed while inspecting artifact set".into());
        }
    }
    check_payload_entries(&payload)?;
    if directory_identity(&payload).map_err(err)? != payload_identity
        || crate::build_report::read_verified(cfg, id)?.0 != report
    {
        return Err("Candidate report/payload directory changed".into());
    }
    crate::verify_source(&source, &view.build.spec.source_manifest_sha256)?;
    let set_sha256 =
        crate::sha256(&serde_json::to_vec(&(&report, &payload_identity, &artifacts)).map_err(err)?);
    Ok((
        CandidateArtifactInspection {
            schema_version: 1,
            kind: "candidate_artifact_inspection".into(),
            read_only: true,
            runtime_applied: false,
            candidate_verified: false,
            report,
            payload_directory_identity: payload_identity,
            artifacts,
            set_sha256,
        },
        pins,
    ))
}
#[cfg(test)]
#[path = "candidate_artifacts_tests.rs"]
mod tests;
