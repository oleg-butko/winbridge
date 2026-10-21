//! Pure final-backup intent/attempt FSM. Rows and JSON cannot mint OS custody,
//! helper-launch permission, runtime replacement or restore authority.
use crate::{
    artifact::ProbeCleanup,
    shutdown::{ShutdownDeliveryState, ShutdownRecord},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::{update::*, RuntimeImageEvidence};
type Result<T> = std::result::Result<T, String>;
mod data_file;
mod export;
pub use data_file::{pin_backup_file, StateBackupFilePin};
mod driver;
pub use driver::capture_final_state;
pub use export::{prepare_export, StateBackupExportGuard};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateBackupBinding {
    pub shutdown: Box<ShutdownRecord>,
    pub worker_config: RuntimeImageEvidence,
    pub worker_journal_schema: u32,
    /// Existing update run directory, supplied/verified by local policy.
    pub parent_directory: String,
    pub parent_identity: String,
    pub admitted_at_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateBackupIntent {
    pub schema_version: u16,
    pub attempt_id: String,
    pub binding: StateBackupBinding,
    pub binding_sha256: String,
}
/// Fresh, non-Serde database admission. Driver must also retain its OS guard.
pub struct StateBackupPermit {
    pub(crate) intent: StateBackupIntent,
}
impl StateBackupPermit {
    pub fn intent(&self) -> &StateBackupIntent {
        &self.intent
    }
}
pub enum StateBackupAdmission {
    Fresh(Box<StateBackupPermit>),
    Existing(Box<StateBackupRecord>),
}

fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn time(at: u64) -> Result<()> {
    if at == 0 || at > MAX_UPDATE_COUNTER {
        Err("Invalid backup timestamp".into())
    } else {
        Ok(())
    }
}
fn digest(binding: &StateBackupBinding, attempt: &str) -> Result<String> {
    let v = serde_json::to_value((binding, attempt)).map_err(|e| e.to_string())?;
    let mut raw = b"winbridge.final-state-backup.intent.v1\n".to_vec();
    raw.extend(winbridge_protocol::canonical_evidence_json(&v).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&raw))
}
impl StateBackupIntent {
    /// Data only. Admission must still join live custody, authority, policy and
    /// original shutdown/root records before returning a one-use driver permit.
    pub fn from_exited(binding: StateBackupBinding, attempt_id: String) -> Result<Self> {
        let binding_sha256 = digest(&binding, &attempt_id)?;
        let v = Self {
            schema_version: 1,
            attempt_id,
            binding,
            binding_sha256,
        };
        v.validate()?;
        Ok(v)
    }
    pub fn validate(&self) -> Result<()> {
        let b = &self.binding;
        b.shutdown.validate()?;
        time(b.admitted_at_ms)?;
        let id =
            uuid::Uuid::parse_str(&self.attempt_id).map_err(|_| "Invalid backup attempt ID")?;
        let ShutdownDeliveryState::Exited {
            at_ms, physical, ..
        } = &b.shutdown.delivery
        else {
            return Err("Backup requires retained exact shutdown exit history".into());
        };
        let s = &b.shutdown.intent.binding;
        crate::shutdown::ShutdownInspectionRequest {
            schema_version: 1,
            intent: b.shutdown.intent.clone(),
            worker_config: b.worker_config.clone(),
            helper_image: s.loaded_worker.image.clone(),
            worker_journal_schema: b.worker_journal_schema,
        }
        .validate()?;
        if self.schema_version != 1
            || id.hyphenated().to_string() != self.attempt_id
            || self.binding_sha256 != digest(b, &self.attempt_id)?
            || b.admitted_at_ms < *at_ms
            || b.admitted_at_ms < physical.observed_at_ms
            || !text(&b.parent_directory, 4096)
            || !text(&b.parent_identity, 128)
            || [
                &s.worker_state.directory_identity,
                &s.worker_state.journal_identity,
                &s.worker_state.lock_identity,
                &s.loaded_worker.image.file_identity,
                &b.worker_config.file_identity,
            ]
            .into_iter()
            .any(|v| v == &b.parent_identity)
            || s.parent_revision
                .checked_add(2)
                .is_none_or(|v| v > MAX_UPDATE_COUNTER)
        {
            return Err("Backup immutable intent/time/directory binding differs".into());
        }
        Ok(())
    }
    pub fn update_id(&self) -> &str {
        &self.binding.shutdown.intent.binding.update_id
    }
    pub fn parent_revision(&self) -> Result<u64> {
        self.validate()?;
        self.binding
            .shutdown
            .intent
            .binding
            .parent_revision
            .checked_add(2)
            .ok_or("Backup parent revision overflow".into())
    }
    pub fn input_path(&self) -> PathBuf {
        PathBuf::from(&self.binding.parent_directory)
            .join(format!("state-backup-{}.json", self.attempt_id))
    }
    pub fn backup_path(&self) -> PathBuf {
        PathBuf::from(&self.binding.parent_directory)
            .join(format!("state-backup-{}.sqlite", self.attempt_id))
    }
    pub fn job_name(&self) -> String {
        format!("Global\\WinBridge-StateBackup-{}", self.binding_sha256)
    }
    fn receipt(&self) -> &WorkerShutdownReceipt {
        let ShutdownDeliveryState::Exited { receipt, .. } = &self.binding.shutdown.delivery else {
            unreachable!("validated exit intent");
        };
        receipt
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateBackupPhase {
    Prepared,
    Issued,
    ResumeGranted,
    Running,
    Cleaned,
    Verified,
    Indeterminate,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateBackupRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub intent: StateBackupIntent,
    pub phase: StateBackupPhase,
    pub updated_at_ms: u64,
    pub input_file: Option<StateBackupFileEvidence>,
    pub process: Option<SupervisorProcess>,
    pub resume_granted: bool,
    pub resume_acknowledged: bool,
    pub cleanup: Option<ProbeCleanup>,
    pub report: Option<Box<WorkerStateBackupReport>>,
    pub reason: Option<String>,
}
impl StateBackupRecord {
    /// Causal data check only, not launch/custody authority. The export helper
    /// must supply its own OS identity and freshly pinned input; the journal
    /// additionally checks the current owner/root/supervisor in one snapshot.
    pub fn require_export_process(
        &self,
        process: &SupervisorProcess,
        input: &StateBackupFileEvidence,
    ) -> Result<()> {
        self.validate()?;
        if !matches!(
            self.phase,
            StateBackupPhase::ResumeGranted | StateBackupPhase::Running
        ) || self.process.as_ref() != Some(process)
            || self.input_file.as_ref() != Some(input)
            || !self.resume_granted
        {
            return Err("Export requires the exact saved live attempt PID/input and resume grant; no replay".into());
        }
        Ok(())
    }
    pub fn prepare(intent: StateBackupIntent) -> Result<Self> {
        intent.validate()?;
        Ok(Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: intent.binding.admitted_at_ms,
            intent,
            phase: StateBackupPhase::Prepared,
            input_file: None,
            process: None,
            resume_granted: false,
            resume_acknowledged: false,
            cleanup: None,
            report: None,
            reason: None,
        })
    }
    fn base_revision(&self) -> u64 {
        if self.resume_acknowledged {
            3
        } else if self.resume_granted {
            2
        } else if self.input_file.is_some() {
            1
        } else {
            0
        }
    }
    fn safe_cleanup(&self) -> bool {
        self.resume_granted
            && self.resume_acknowledged
            && self.cleanup.as_ref().is_some_and(|c| {
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
    fn validate_report(&self, r: &WorkerStateBackupReport) -> Result<()> {
        r.validate_envelope()?;
        let b = &self.intent.binding;
        let s = &b.shutdown.intent.binding;
        if !self.safe_cleanup()
            || r.attempt_id != self.intent.attempt_id
            || r.update_id != self.intent.update_id()
            || Some(&r.helper_process) != self.process.as_ref()
            || r.helper_descriptor != s.loaded_worker.descriptor
            || r.helper_image != s.loaded_worker.image
            || r.worker_config != b.worker_config
            || r.state != s.worker_state
            || r.worker_journal_schema != b.worker_journal_schema
            || r.shutdown_receipt != *self.intent.receipt()
            || r.backup.path
                != self
                    .intent
                    .backup_path()
                    .to_str()
                    .ok_or("Backup path must be UTF-8")?
            || self.input_file.as_ref().is_none_or(|f| {
                f.sha256 != r.input_sha256 || f.file_identity == r.backup.file_identity
            })
            || r.backup.file_identity == self.intent.binding.parent_identity
            || r.completed_at_ms < b.admitted_at_ms
            || r.completed_at_ms > self.updated_at_ms
        {
            return Err(
                "Backup report differs from saved helper/input/exit scope or successful cleanup"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        time(self.updated_at_ms)?;
        if self.schema_version != 1
            || self.revision > 6
            || self.updated_at_ms < self.intent.binding.admitted_at_ms
            || self.resume_acknowledged && !self.resume_granted
            || self.resume_granted && self.process.is_none()
        {
            return Err("Backup record version/revision/flags differ".into());
        }
        if let Some(f) = &self.input_file {
            if f.path
                != self
                    .intent
                    .input_path()
                    .to_str()
                    .ok_or("Backup input path UTF-8")?
                || f.bytes == 0
                || f.bytes > 65536
                || !valid_hash(&f.sha256, &[64])
                || !text(&f.file_identity, 128)
                || [
                    &self.intent.binding.parent_identity,
                    &self.intent.binding.worker_config.file_identity,
                    &self
                        .intent
                        .binding
                        .shutdown
                        .intent
                        .binding
                        .loaded_worker
                        .image
                        .file_identity,
                    &self
                        .intent
                        .binding
                        .shutdown
                        .intent
                        .binding
                        .worker_state
                        .directory_identity,
                    &self
                        .intent
                        .binding
                        .shutdown
                        .intent
                        .binding
                        .worker_state
                        .journal_identity,
                    &self
                        .intent
                        .binding
                        .shutdown
                        .intent
                        .binding
                        .worker_state
                        .lock_identity,
                ]
                .into_iter()
                .any(|id| id == &f.file_identity)
            {
                return Err("Backup exact input file differs".into());
            }
        }
        if let Some(p) = &self.process {
            let old = &self.intent.binding.shutdown.intent.binding.worker.process;
            if self.input_file.is_none()
                || p.process_id == 0
                || !text(&p.birth, 128)
                || p.host_id != old.host_id
                || p.session_id != old.session_id
                || p == old
            {
                return Err("Backup helper identity/scope differs".into());
            }
        }
        if let Some(c) = &self.cleanup {
            if c.job_name != self.intent.job_name()
                || c.process != self.process
                || c.resume_requested && !self.resume_granted
                || c.creation_confirmed && self.process.is_none()
                || c.no_process_created
                    && (self.process.is_some() || c.creation_confirmed || c.resume_requested)
            {
                return Err("Backup cleanup differs from exact saved process/grant".into());
            }
        }
        let base = self.base_revision();
        let shape = match self.phase {
            StateBackupPhase::Prepared => {
                self.revision == 0 && base == 0 && self.process.is_none() && self.cleanup.is_none()
            }
            StateBackupPhase::Issued => {
                self.revision == 1 && base == 1 && self.process.is_none() && self.cleanup.is_none()
            }
            StateBackupPhase::ResumeGranted => {
                self.revision == 2 && base == 2 && self.cleanup.is_none()
            }
            StateBackupPhase::Running => self.revision == 3 && base == 3 && self.cleanup.is_none(),
            StateBackupPhase::Cleaned => self.revision == base + 1 && self.cleanup.is_some(),
            StateBackupPhase::Verified => {
                self.revision == 5 && self.safe_cleanup() && self.report.is_some()
            }
            StateBackupPhase::Indeterminate => {
                self.revision == base + u64::from(self.cleanup.is_some()) + 1
                    && self.reason.as_ref().is_some_and(|s| text(s, 4096))
            }
        };
        if !shape
            || (self.phase == StateBackupPhase::Verified) != self.report.is_some()
            || (self.phase == StateBackupPhase::Indeterminate) != self.reason.is_some()
        {
            return Err("Backup FSM phase/evidence shape differs".into());
        }
        if let Some(r) = &self.report {
            self.validate_report(r)?;
        }
        Ok(())
    }
    fn next(&self, at: u64) -> Result<Self> {
        self.validate()?;
        time(at)?;
        if at < self.updated_at_ms {
            return Err("Backup time moved backwards".into());
        }
        let mut n = self.clone();
        n.revision += 1;
        n.updated_at_ms = at;
        Ok(n)
    }
    pub fn issue(&self, input: StateBackupFileEvidence, at: u64) -> Result<Self> {
        if self.phase != StateBackupPhase::Prepared {
            return Err("Backup attempt already issued; never recreate a launch".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::Issued;
        n.input_file = Some(input);
        n.validate()?;
        Ok(n)
    }
    pub fn grant_resume(&self, process: SupervisorProcess, at: u64) -> Result<Self> {
        if self.phase != StateBackupPhase::Issued {
            return Err("Backup PID/resume requires original issued attempt".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::ResumeGranted;
        n.process = Some(process);
        n.resume_granted = true;
        n.validate()?;
        Ok(n)
    }
    pub fn resumed(&self, at: u64) -> Result<Self> {
        if self.phase != StateBackupPhase::ResumeGranted {
            return Err("Backup resume acknowledgment out of order".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::Running;
        n.resume_acknowledged = true;
        n.validate()?;
        Ok(n)
    }
    pub fn finished(&self, cleanup: ProbeCleanup, at: u64) -> Result<Self> {
        if !matches!(
            self.phase,
            StateBackupPhase::Prepared
                | StateBackupPhase::Issued
                | StateBackupPhase::ResumeGranted
                | StateBackupPhase::Running
        ) {
            return Err("Backup cleanup already saved; observe only".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::Cleaned;
        if n.process.is_none() {
            n.process = cleanup.process.clone();
        }
        n.cleanup = Some(cleanup);
        n.validate()?;
        Ok(n)
    }
    pub fn verify(&self, report: WorkerStateBackupReport, at: u64) -> Result<Self> {
        self.validate()?;
        if self.phase == StateBackupPhase::Verified {
            return if self.report.as_deref() == Some(&report) {
                Ok(self.clone())
            } else {
                Err("Backup receipt already belongs to another result".into())
            };
        }
        if self.phase != StateBackupPhase::Cleaned {
            return Err("Backup result requires saved process cleanup".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::Verified;
        n.report = Some(Box::new(report));
        n.validate()?;
        Ok(n)
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        if matches!(
            self.phase,
            StateBackupPhase::Verified | StateBackupPhase::Indeterminate
        ) {
            return Err("Backup terminal outcome immutable; observe only".into());
        }
        let mut n = self.next(at)?;
        n.phase = StateBackupPhase::Indeterminate;
        n.reason = Some(reason);
        n.validate()?;
        Ok(n)
    }
}

#[cfg(test)]
#[path = "state_backup_tests.rs"]
pub(crate) mod tests;

pub(crate) fn view(
    saved: Option<&StateBackupRecord>,
    active: bool,
    failed: bool,
) -> CandidateStateBackupView {
    let phase = match saved {
        None if failed => CandidateStateBackupPhase::Blocked,
        None => CandidateStateBackupPhase::NotAdmitted,
        Some(r) if r.phase == StateBackupPhase::Verified => CandidateStateBackupPhase::Verified,
        Some(r) if r.phase == StateBackupPhase::Indeterminate || !active || failed => {
            CandidateStateBackupPhase::Indeterminate
        }
        Some(r) => match r.phase {
            StateBackupPhase::Prepared => CandidateStateBackupPhase::Prepared,
            StateBackupPhase::Issued | StateBackupPhase::ResumeGranted => {
                CandidateStateBackupPhase::StartingHelper
            }
            StateBackupPhase::Running => CandidateStateBackupPhase::CopyingState,
            StateBackupPhase::Cleaned => CandidateStateBackupPhase::VerifyingBackup,
            _ => CandidateStateBackupPhase::Indeterminate,
        },
    };
    let report = saved.and_then(|r| r.report.as_deref());
    CandidateStateBackupView {
        phase,
        attempt_id: saved.map(|r| r.intent.attempt_id.clone()),
        revision: saved.map(|r| r.revision),
        helper_process: saved.and_then(|r| r.process.clone()),
        completed_at_ms: report.map(|r| r.completed_at_ms),
        backup_bytes: report.map(|r| r.backup.bytes),
        backup_sha256: report.map(|r| r.backup.sha256.clone()),
        reason: saved.and_then(|r| r.reason.clone()),
    }
}
