//! Journal-bound candidate updater discovery. Data never reconstructs a permit.
use crate::artifact::ProbeCleanup;
use serde::{Deserialize, Serialize};
use winbridge_core::update::{
    assess_updater_compatibility_with_drivers, valid_hash, MAX_UPDATE_COUNTER,
};
use winbridge_protocol::{
    update::*, ArtifactDescriptor, ArtifactRole, ResolvedCommand, RuntimeImageEvidence,
    WorkerRuntimeCompatibility,
};

mod driver;
pub use driver::execute;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSupportIntent {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub installation_id: String,
    pub scope_sha256: String,
    pub launch_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub parent_revision: u64,
    pub admitted_at_ms: u64,
    pub config_sha256: String,
    pub supervisor_image_sha256: String,
    pub executor: ArtifactDescriptor,
    pub drivers: UpdaterDriverContracts,
    pub source_sha256: String,
    pub health_sha256: String,
    pub artifacts_sha256: String,
    pub worker_config: RuntimeImageEvidence,
    pub image: RuntimeImageEvidence,
    pub descriptor: ArtifactDescriptor,
    pub expected_updater_journal_schema: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSupportPhase {
    Admitted,
    Creating,
    ResumeGranted,
    Running,
    Captured,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSupportProbe {
    pub job_name: String,
    pub command: ResolvedCommand,
    pub process: Option<SupervisorProcess>,
    pub resume_granted: bool,
    pub resume_acknowledged: bool,
    pub cleanup: Option<ProbeCleanup>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSupportOutcome {
    Checked,
    Unsupported,
    Rejected,
    Cancelled,
    Indeterminate,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSupportResult {
    pub outcome: CandidateSupportOutcome,
    pub completed_at_ms: u64,
    pub reason: String,
    pub report: Option<UpdaterRuntimeSupport>,
    pub assessment: Option<UpdaterCompatibilityAssessment>,
    pub final_worker: Option<WorkerRuntimeCompatibility>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSupportRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub intent: CandidateSupportIntent,
    pub phase: CandidateSupportPhase,
    pub probe: Option<CandidateSupportProbe>,
    pub result: Option<CandidateSupportResult>,
}

/// Only fresh committed admission constructs this value; not Clone/Deserialize.
pub struct CandidateSupportPermit {
    pub(crate) intent: CandidateSupportIntent,
}
pub enum CandidateSupportAdmission {
    Fresh(Box<CandidateSupportPermit>),
    Existing(Box<CandidateSupportRecord>),
}

pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(crate::sha256(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
fn process_valid(p: &SupervisorProcess) -> bool {
    p.process_id > 0
        && !p.birth.is_empty()
        && p.birth.len() <= 32
        && p.birth.bytes().all(|b| b.is_ascii_digit())
        && valid_hash(&p.host_id, &[64])
}
fn clean_descriptor(d: &ArtifactDescriptor) -> bool {
    d.schema_version == 1
        && d.kind == "winbridge_artifact"
        && d.query_contract == 1
        && d.role == ArtifactRole::Updater
        && !d.build.source_dirty
        && d.build
            .source_commit
            .as_ref()
            .is_some_and(|s| valid_hash(s, &[40, 64]))
        && d.build
            .source_manifest_sha256
            .as_ref()
            .is_some_and(|s| valid_hash(s, &[64]))
        && !d.build.build_id.is_empty()
}
fn image_valid(i: &RuntimeImageEvidence, maximum: u64) -> bool {
    std::path::Path::new(&i.path).is_absolute()
        && i.path.len() <= 4096
        && !std::path::Path::new(&i.path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        && i.bytes > 0
        && i.bytes <= maximum
        && valid_hash(&i.sha256, &[64])
        && !i.file_identity.is_empty()
        && i.file_identity.len() <= 256
}
impl CandidateSupportIntent {
    pub fn validate(&self) -> Result<()> {
        for id in [
            &self.update_id,
            &self.attempt_id,
            &self.installation_id,
            &self.launch_id,
            &self.owner_id,
        ] {
            uuid::Uuid::parse_str(id).map_err(|_| "Invalid support intent identity")?;
        }
        if self.schema_version != 1
            || self.attempt_id == self.update_id
            || self.attempt_id == self.launch_id
            || self.attempt_id == self.owner_id
            || self.expected_updater_journal_schema == 0
            || self.owner_epoch == 0
            || self.owner_epoch > MAX_UPDATE_COUNTER
            || self.parent_revision > MAX_UPDATE_COUNTER
            || self.admitted_at_ms == 0
            || self.admitted_at_ms > MAX_UPDATE_COUNTER
            || self.drivers.schema_version != 1
            || !clean_descriptor(&self.executor)
            || !clean_descriptor(&self.descriptor)
            || !image_valid(&self.image, 256 * 1024 * 1024)
            || !self.image.query_supported
            || !image_valid(&self.worker_config, 65536)
            || [
                &self.scope_sha256,
                &self.config_sha256,
                &self.supervisor_image_sha256,
                &self.source_sha256,
                &self.health_sha256,
                &self.artifacts_sha256,
            ]
            .into_iter()
            .any(|h| !valid_hash(h, &[64]))
            || [
                self.drivers.final_state_backup,
                self.drivers.runtime_preparation,
                self.drivers.runtime_replacement,
                self.drivers.activation,
                self.drivers.controller_confirmation,
                self.drivers.desktop_update,
            ]
            .into_iter()
            .flatten()
            .any(|v| v == 0)
        {
            return Err("Invalid support intent scope/counters/image/driver snapshot".into());
        }
        Ok(())
    }
    pub fn command(&self) -> Result<ResolvedCommand> {
        Ok(ResolvedCommand {
            executable: self.image.path.clone(),
            arguments: vec!["--runtime-support".into()],
            working_directory: std::path::Path::new(&self.image.path)
                .parent()
                .and_then(|p| p.to_str())
                .ok_or("Support cwd missing")?
                .into(),
            timeout_ms: 10000,
        })
    }
    pub fn job_name(&self) -> Result<String> {
        Ok(format!(
            "Global\\WinBridge-UpdaterSupport-{}",
            digest(self)?
        ))
    }
}

pub(crate) fn certain(probe: &CandidateSupportProbe) -> bool {
    probe.cleanup.as_ref().is_some_and(|c| {
        c.tree_empty
            && c.capture_complete
            && (c.creation_confirmed || c.no_process_created)
            && (!c.creation_confirmed || c.process.is_some())
    })
}
fn completed(probe: &CandidateSupportProbe) -> bool {
    probe.resume_granted
        && probe.resume_acknowledged
        && probe.cleanup.as_ref().is_some_and(|c| {
            c.creation_confirmed
                && !c.no_process_created
                && c.resume_requested
                && c.tree_empty
                && c.capture_complete
                && !c.output_exceeded
                && c.exit_code == Some(0)
                && c.failure.is_none()
        })
}
impl CandidateSupportRecord {
    pub fn admitted(intent: CandidateSupportIntent) -> Result<Self> {
        let r = Self {
            schema_version: 1,
            revision: 0,
            intent,
            phase: CandidateSupportPhase::Admitted,
            probe: None,
            result: None,
        };
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        if self.schema_version != 1
            || self.revision > 5
            || (self.revision == 0) != (self.phase == CandidateSupportPhase::Admitted)
            || (self.phase == CandidateSupportPhase::Finished) != self.result.is_some()
        {
            return Err("Support phase/revision/result inconsistent".into());
        }
        if let Some(p) = &self.probe {
            if p.command != self.intent.command()?
                || p.job_name != self.intent.job_name()?
                || p.process.as_ref().is_some_and(|p| !process_valid(p))
                || p.resume_granted && p.process.is_none()
                || p.resume_acknowledged && !p.resume_granted
            {
                return Err("Support probe command/PID/grant inconsistent".into());
            }
            if let Some(c) = &p.cleanup {
                if c.job_name != p.job_name
                    || c.process != p.process
                    || c.resume_requested && !p.resume_granted
                    || c.no_process_created
                        && (c.creation_confirmed || c.process.is_some() || c.resume_requested)
                    || c.failure.as_ref().is_some_and(|r| r.len() > 4096)
                    || c.process.is_some() && !c.creation_confirmed
                    || c.creation_confirmed
                        && c.process.is_none()
                        && (c.tree_empty || c.capture_complete)
                    || c.failure.is_none()
                        && (!c.creation_confirmed
                            || !c.resume_requested
                            || !c.tree_empty
                            || !c.capture_complete
                            || c.output_exceeded
                            || c.exit_code != Some(0))
                {
                    return Err("Support cleanup evidence inconsistent".into());
                }
            }
        }
        use CandidateSupportPhase as P;
        let shape = match self.phase {
            P::Admitted => self.probe.is_none(),
            P::Creating => {
                self.revision == 1
                    && self.probe.as_ref().is_some_and(|p| {
                        p.process.is_none()
                            && !p.resume_granted
                            && !p.resume_acknowledged
                            && p.cleanup.is_none()
                    })
            }
            P::ResumeGranted => {
                self.revision == 2
                    && self.probe.as_ref().is_some_and(|p| {
                        p.resume_granted && !p.resume_acknowledged && p.cleanup.is_none()
                    })
            }
            P::Running => {
                self.revision == 3
                    && self.probe.as_ref().is_some_and(|p| {
                        p.resume_granted && p.resume_acknowledged && p.cleanup.is_none()
                    })
            }
            P::Captured => {
                (2..=4).contains(&self.revision)
                    && self.probe.as_ref().is_some_and(|p| p.cleanup.is_some())
            }
            P::Finished => self.result.is_some(),
        };
        if !shape {
            return Err("Support probe phase does not match saved evidence".into());
        }
        if let Some(r) = &self.result {
            if r.completed_at_ms < self.intent.admitted_at_ms
                || r.completed_at_ms > MAX_UPDATE_COUNTER
                || r.reason.is_empty()
                || r.reason.chars().count() > 1000
                || r.reason.chars().any(char::is_control)
            {
                return Err("Support result time/reason inconsistent".into());
            }
            if r.outcome == CandidateSupportOutcome::Checked {
                let p = self
                    .probe
                    .as_ref()
                    .filter(|p| completed(p))
                    .ok_or("Support success lacks exact probe cleanup")?;
                let report = r.report.as_ref().ok_or("Support report missing")?;
                crate::updater_support::require_report_binding(
                    report,
                    &self.intent.descriptor,
                    p.process.as_ref().ok_or("Support PID missing")?,
                )?;
                if r.final_worker.as_ref().is_none_or(|w| {
                    w.observed_at_ms < self.intent.admitted_at_ms
                        || w.observed_at_ms > r.completed_at_ms
                }) || r.assessment.as_ref()
                    != Some(&assess_updater_compatibility_with_drivers(
                        report,
                        self.intent.expected_updater_journal_schema,
                        &self.intent.drivers,
                    ))
                {
                    return Err("Support assessment differs from frozen driver context".into());
                }
            } else if r.report.is_some()
                || r.assessment.is_some()
                || r.final_worker.is_some()
                || r.outcome == CandidateSupportOutcome::Unsupported && self.probe.is_some()
                || r.outcome != CandidateSupportOutcome::Indeterminate
                    && self.probe.as_ref().is_some_and(|p| !certain(p))
            {
                return Err("Support outcome invented report or confirmed unknown process".into());
            }
        }
        Ok(())
    }
    pub fn advance(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        if self.intent != next.intent || next.revision != self.revision + 1 || self.result.is_some()
        {
            return Err("Support identity/revision/terminal state changed".into());
        }
        use CandidateSupportPhase as P;
        let permitted = match (self.phase, next.phase) {
            (P::Admitted, P::Creating) => next.result.is_none(),
            (P::Creating, P::ResumeGranted) | (P::ResumeGranted, P::Running) => {
                let mut old = self.probe.clone().ok_or("Support probe missing")?;
                if self.phase == P::Creating {
                    old.process = next.probe.as_ref().and_then(|p| p.process.clone());
                    old.resume_granted = true;
                } else {
                    old.resume_acknowledged = true;
                }
                next.probe.as_ref() == Some(&old)
            }
            (P::Creating | P::ResumeGranted | P::Running, P::Captured) => {
                let mut old = self.probe.clone().ok_or("Support probe missing")?;
                let n = next.probe.as_ref().ok_or("Support cleanup missing")?;
                if old.process.is_none() {
                    old.process = n.process.clone();
                }
                old.cleanup = n.cleanup.clone();
                next.probe.as_ref() == Some(&old)
            }
            (P::Admitted | P::Captured, P::Finished) => self.probe == next.probe,
            // Unknown creation/cleanup can be preserved without inventing a
            // successful Captured callback or a second launch permission.
            (P::Creating | P::ResumeGranted | P::Running, P::Finished) => {
                self.probe == next.probe
                    && next
                        .result
                        .as_ref()
                        .is_some_and(|r| r.outcome == CandidateSupportOutcome::Indeterminate)
            }
            _ => false,
        };
        if !permitted {
            return Err("Support history skipped or rewrote process evidence".into());
        }
        Ok(())
    }
}

pub fn admit(
    j: &mut crate::journal::UpdateJournal,
    a: &crate::journal::UpdateAuthority,
    s: &SupervisorRecord,
    lease: &WorkerUpdateLease,
) -> Result<CandidateSupportAdmission> {
    j.admit_candidate_support(a, s, lease)
        .map_err(|e| e.to_string())
}

pub(crate) fn quiescent(r: &CandidateSupportRecord) -> Result<()> {
    r.validate()?;
    if r.result
        .as_ref()
        .is_none_or(|r| r.outcome == CandidateSupportOutcome::Indeterminate)
        || r.probe.as_ref().is_some_and(|p| !certain(p))
    {
        return Err("Support probe pending/unknown; preserve source barrier".into());
    }
    Ok(())
}
pub(crate) fn physically_quiescent(r: &CandidateSupportRecord) -> Result<()> {
    quiescent(r)?;
    let Some(p) = r.probe.as_ref() else {
        return Ok(());
    };
    let Some(process) = p.process.as_ref() else {
        return Ok(());
    };
    match winbridge_platform::infrastructure::matches(process) {
        Ok(false) => {}
        Err(e)
            if e.kind() == std::io::ErrorKind::NotFound
                || cfg!(windows) && e.raw_os_error() == Some(87) => {}
        _ => return Err("Support process exit cannot be confirmed".into()),
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
                creation_time: process
                    .birth
                    .parse()
                    .map_err(|_| "Support process birth invalid")?
            }),
            ProcessObservation::Empty(_)
        ) {
            return Err("Support Job occupied or unknown; preserve barrier".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
