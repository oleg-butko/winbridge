//! Durable health orchestration. Standalone diagnostic JSON is never accepted as authority.
use crate::{artifact::ProbeCleanup, candidate::ArtifactSetRecord};
use serde::{Deserialize, Serialize};
use winbridge_protocol::{
    update::*, RuntimeImageEvidence, WorkerConfigCheckReport, WorkerRehearsalReport,
    WorkerRuntimeCompatibility,
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthIntent {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub launch_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub parent_revision: u64,
    pub admitted_at_ms: u64,
    pub artifacts_sha256: String,
    pub preimages_sha256: String,
    pub directory: String,
    pub worker_config_path: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthProbeKind {
    Config,
    Rehearsal,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthContext {
    pub compatibility: WorkerRuntimeCompatibility,
    pub worker_config: RuntimeImageEvidence,
    pub artifacts: ArtifactSetRecord,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthProbe {
    pub kind: HealthProbeKind,
    pub job_name: String,
    pub command: winbridge_protocol::ResolvedCommand,
    pub process: Option<SupervisorProcess>,
    pub resume_granted: bool,
    pub resume_acknowledged: bool,
    pub cleanup: Option<ProbeCleanup>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthOutcome {
    Verified,
    Rejected,
    Cancelled,
    Indeterminate,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthResult {
    pub outcome: HealthOutcome,
    pub completed_at_ms: u64,
    pub reason: String,
    pub config: Option<WorkerConfigCheckReport>,
    pub rehearsal: Option<WorkerRehearsalReport>,
    pub final_compatibility: Option<WorkerRuntimeCompatibility>,
    pub parent_revision: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub intent: HealthIntent,
    pub context: Option<HealthContext>,
    pub probes: Vec<HealthProbe>,
    pub result: Option<HealthResult>,
}
/// One-use live admission; pending records never reconstruct this permit.
pub struct HealthPermit {
    pub(crate) intent: HealthIntent,
}
pub enum HealthAdmission {
    Fresh(Box<HealthPermit>),
    Existing(Box<HealthRecord>),
}
pub(crate) fn digest(v: &impl Serialize) -> Result<String, String> {
    Ok(crate::sha256(
        &serde_json::to_vec(v).map_err(|e| e.to_string())?,
    ))
}
pub(crate) fn directory(runtime: &str, id: &str) -> std::path::PathBuf {
    std::path::Path::new(runtime)
        .join("update")
        .join(id)
        .join("health")
}
pub(crate) fn job(intent: &HealthIntent, kind: HealthProbeKind) -> String {
    format!(
        "Global\\WinBridge-Health-{}",
        digest(&(intent.attempt_id.as_str(), kind)).expect("serializable health job")
    )
}

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(error)?
        .as_millis()
        .try_into()
        .map_err(error)
}
pub(crate) fn command(
    intent: &HealthIntent,
    ctx: &HealthContext,
    kind: HealthProbeKind,
) -> winbridge_protocol::ResolvedCommand {
    let image = &ctx.artifacts.inspection.artifacts[0].image.path;
    let (arguments, timeout_ms) = match kind {
        HealthProbeKind::Config => (
            vec![
                "--check-update-config".into(),
                intent.worker_config_path.clone(),
                ctx.worker_config.sha256.clone(),
            ],
            10000,
        ),
        HealthProbeKind::Rehearsal => (
            vec![
                "--rehearsal".into(),
                std::path::Path::new(&intent.directory)
                    .join("worker")
                    .to_string_lossy()
                    .into(),
            ],
            30000,
        ),
    };
    winbridge_protocol::ResolvedCommand {
        executable: image.clone(),
        arguments,
        working_directory: std::path::Path::new(image)
            .parent()
            .unwrap_or(std::path::Path::new(""))
            .to_string_lossy()
            .into(),
        timeout_ms,
    }
}
fn same_compatibility(a: &WorkerRuntimeCompatibility, b: &WorkerRuntimeCompatibility) -> bool {
    let mut b = b.clone();
    b.observed_at_ms = a.observed_at_ms;
    *a == b
}
pub(crate) fn completed(p: &HealthProbe) -> bool {
    p.resume_granted
        && p.resume_acknowledged
        && p.cleanup.as_ref().is_some_and(|c| {
            c.creation_confirmed
                && !c.no_process_created
                && c.process == p.process
                && c.resume_requested
                && c.tree_empty
                && c.capture_complete
                && !c.output_exceeded
                && c.exit_code == Some(0)
                && c.failure.is_none()
        })
}
/// Pure validation of typed, journal-bound evidence. Physical guards are also required at acceptance.
pub(crate) fn validate_success(h: &HealthRecord) -> Result<(), String> {
    let ctx = h.context.as_ref().ok_or("Health context missing")?;
    let r = h.result.as_ref().ok_or("Health result missing")?;
    let config = r.config.as_ref().ok_or("Health config report missing")?;
    let rehearsal = r.rehearsal.as_ref().ok_or("Health rehearsal missing")?;
    let final_compat = r
        .final_compatibility
        .as_ref()
        .ok_or("Final live compatibility missing")?;
    let artifact = ctx
        .artifacts
        .inspection
        .artifacts
        .first()
        .ok_or("Worker artifact missing")?;
    if r.outcome != HealthOutcome::Verified
        || h.probes.len() != 2
        || !h.probes.iter().all(completed)
        || h.probes[0].kind != HealthProbeKind::Config
        || h.probes[1].kind != HealthProbeKind::Rehearsal
        || !same_compatibility(&ctx.compatibility, final_compat)
        || final_compat.observed_at_ms < ctx.compatibility.observed_at_ms
        || final_compat.observed_at_ms > r.completed_at_ms
        || artifact.descriptor.role != winbridge_protocol::ArtifactRole::Worker
        || config.schema_version != 1
        || config.kind != "worker_config_check"
        || config.descriptor != artifact.descriptor
        || Some(&config.process) != h.probes[0].process.as_ref()
        || config.config != ctx.worker_config
        || config.worker_config_schema != ctx.compatibility.worker_config_schema
        || config.instance_id != ctx.compatibility.context.instance_id
        || config.worker_journal_opened
        || config.scheduler_started
        || config.candidate_verified
        || config.runtime_applied
        || rehearsal.schema_version != 1
        || rehearsal.kind != "worker_rehearsal"
        || rehearsal.contract_version != 1
        || rehearsal.descriptor != artifact.descriptor
        || Some(&rehearsal.process) != h.probes[1].process.as_ref()
        || rehearsal.directory
            != std::path::Path::new(&h.intent.directory)
                .join("worker")
                .to_string_lossy()
        || rehearsal.directory_identity.is_empty()
        || rehearsal.directory_identity.len() > 128
        || !rehearsal
            .instance_id
            .strip_prefix("rehearsal-")
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
        || rehearsal.instance_id == ctx.compatibility.context.instance_id
        || uuid::Uuid::parse_str(&rehearsal.generation).is_err()
        || rehearsal.generation == ctx.compatibility.generation
        || uuid::Uuid::parse_str(&rehearsal.journal_incarnation).is_err()
        || rehearsal.journal_incarnation == ctx.compatibility.context.journal_incarnation
        || rehearsal.worker_journal_schema != ctx.compatibility.worker_journal_schema
        || rehearsal.updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
        || ctx.compatibility.updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
        || rehearsal.worker_config_schema != ctx.compatibility.worker_config_schema
        || rehearsal.protocol_version != ctx.compatibility.protocol_version
        || !rehearsal.fresh_journal
        || !rehearsal.ipc_ready
        || !rehearsal.server_stopped
        || rehearsal.controller_enabled
        || rehearsal.direct_commands_enabled
        || rehearsal.production_state_opened
        || rehearsal.candidate_verified
        || rehearsal.runtime_applied
    {
        return Err("Health config/rehearsal/process/schema/isolation evidence rejected".into());
    }
    Ok(())
}
/// A live driver may only append evidence. Old PID, context and cleanup cannot be rewritten.
pub(crate) fn validate_step(old: &HealthRecord, next: &HealthRecord) -> Result<(), String> {
    if old.intent != next.intent
        || old.schema_version != next.schema_version
        || old.result.is_some()
        || old
            .context
            .as_ref()
            .is_some_and(|c| Some(c) != next.context.as_ref())
        || next.probes.len() < old.probes.len()
        || next.probes.len() > old.probes.len() + 1
        || (next.probes.len() > old.probes.len()
            && old.probes.last().is_some_and(|p| !completed(p)))
    {
        return Err("Health evidence is immutable/append-only; no replay".into());
    }
    for (p, n) in old.probes.iter().zip(&next.probes) {
        if p.kind != n.kind
            || p.job_name != n.job_name
            || p.command != n.command
            || p.process
                .as_ref()
                .is_some_and(|v| Some(v) != n.process.as_ref())
            || (p.resume_granted && !n.resume_granted)
            || (p.resume_acknowledged && !n.resume_acknowledged)
            || p.cleanup
                .as_ref()
                .is_some_and(|v| Some(v) != n.cleanup.as_ref())
            || (p.cleanup.is_some() && p != n)
        {
            return Err("Saved health PID/resume/cleanup cannot change".into());
        }
    }
    Ok(())
}
/// Cleanup is conservative: a pending/no-result run never releases the worker barrier.
pub(crate) fn quiescent(h: &HealthRecord) -> Result<(), String> {
    if h.result
        .as_ref()
        .is_none_or(|r| r.outcome == HealthOutcome::Indeterminate)
        || h.probes.iter().any(|p| {
            p.cleanup.as_ref().is_none_or(|c| {
                !c.tree_empty
                    || !c.capture_complete
                    || (!c.creation_confirmed && !c.no_process_created)
            })
        })
    {
        return Err(
            "Health process/capture outcome is pending or unknown; preserve source barrier".into(),
        );
    }
    Ok(())
}
/// Before releasing a source lease, re-observe exact process births and Jobs.
pub(crate) fn physically_quiescent(h: &HealthRecord) -> Result<(), String> {
    quiescent(h)?;
    for p in &h.probes {
        let Some(process) = &p.process else { continue };
        match winbridge_platform::infrastructure::matches(process) {
            Ok(false) => {}
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    || cfg!(windows) && e.raw_os_error() == Some(87) => {}
            _ => return Err("Health process is alive or its exit cannot be confirmed".into()),
        }
        #[cfg(windows)]
        {
            use winbridge_platform::process_identity::{
                observe_process, ProcessIdentity, ProcessObservation,
            };
            if !matches!(
                observe_process(&ProcessIdentity {
                    job_name: p.job_name.clone(),
                    host_id: process.host_id.clone(),
                    session_id: process.session_id,
                    process_id: process.process_id,
                    creation_time: process.birth.parse().map_err(error)?,
                }),
                ProcessObservation::Empty(_)
            ) {
                return Err("Health Job is occupied or unknown; preserve barrier".into());
            }
        }
    }
    Ok(())
}
/// A trusted local implementation obtains compatibility through the bound v3 peer.
/// Wire callers cannot supply compatibility evidence or reconstruct a permit.
pub trait HealthControl: crate::source::SourceControl {
    fn compatibility(&mut self) -> Result<WorkerRuntimeCompatibility, String>;
}
pub fn admit(
    j: &mut crate::journal::UpdateJournal,
    a: &crate::journal::UpdateAuthority,
    s: &SupervisorRecord,
) -> Result<HealthAdmission, String> {
    j.check_source_authority(a).map_err(error)?;
    if let Some(h) = j.health(&s.spec.update_id).map_err(error)? {
        return Ok(HealthAdmission::Existing(Box::new(h)));
    }
    let cfg = crate::preparation::configuration(s)?;
    let id = &s.spec.update_id;
    let source =
        crate::build::saved_source_guard_for(j, a, &cfg, id, UpdatePhase::ValidatingCandidate)?;
    let artifacts = j.candidate_artifacts(id).map_err(error)?;
    let preimages = j
        .runtime_preimages(id)
        .map_err(error)?
        .receipt
        .ok_or("Verified preimages required")?;
    if artifacts.owner_id != a.owner_id()
        || artifacts.owner_epoch != a.epoch()
        || artifacts.source_digest != digest(&source)?
    {
        return Err("Health requires current source/artifact authority".into());
    }
    let directory = directory(&s.spec.runtime_directory, id);
    winbridge_platform::require_safe_path(&directory).map_err(error)?;
    if directory.try_exists().map_err(error)? {
        return Err("Existing health directory is never adopted/overwritten".into());
    }
    let intent = HealthIntent {
        schema_version: 1,
        update_id: id.clone(),
        attempt_id: uuid::Uuid::new_v4().to_string(),
        launch_id: s.spec.launch_id.clone(),
        owner_id: a.owner_id().into(),
        owner_epoch: a.epoch(),
        parent_revision: j.get(id).map_err(error)?.revision + 1,
        admitted_at_ms: now()?,
        artifacts_sha256: digest(&artifacts)?,
        preimages_sha256: digest(&preimages)?,
        directory: directory.to_str().ok_or("Health directory UTF-8")?.into(),
        worker_config_path: cfg
            .worker_config_path
            .canonicalize()
            .map_err(error)?
            .to_str()
            .ok_or("Health config UTF-8")?
            .into(),
    };
    j.admit_health(a, s, intent.clone()).map_err(error)?;
    Ok(HealthAdmission::Fresh(Box::new(HealthPermit { intent })))
}
mod driver;
pub use driver::execute;

pub(crate) fn view(h: Option<&HealthRecord>) -> CandidateHealthView {
    let phase = match h.and_then(|h| h.result.as_ref()).map(|r| &r.outcome) {
        Some(HealthOutcome::Verified) => CandidateHealthPhase::Verified,
        Some(HealthOutcome::Rejected) => CandidateHealthPhase::Rejected,
        Some(HealthOutcome::Cancelled) => CandidateHealthPhase::Cancelled,
        Some(HealthOutcome::Indeterminate) => CandidateHealthPhase::Indeterminate,
        None if h.is_some() => CandidateHealthPhase::Pending,
        None => CandidateHealthPhase::NotAdmitted,
    };
    CandidateHealthView {
        phase,
        revision: h.map(|h| h.revision),
        attempt_id: h.map(|h| h.intent.attempt_id.clone()),
        config_process: h
            .and_then(|h| h.probes.first())
            .and_then(|p| p.process.clone()),
        rehearsal_process: h
            .and_then(|h| h.probes.get(1))
            .and_then(|p| p.process.clone()),
        cleanup_verified: h.is_some_and(|h| quiescent(h).is_ok()),
        reason: h.and_then(|h| h.result.as_ref()).map(|r| r.reason.clone()),
    }
}

#[cfg(test)]
mod tests;
