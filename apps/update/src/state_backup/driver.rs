//! Consumes a fresh DB permit while the caller retains original exit custody.
use super::*;
use crate::{
    artifact::{self, ProbeObserver},
    journal::{UpdateAuthority, UpdateJournal},
    shutdown::ShutdownExitGuard,
};
use std::{fs::OpenOptions, io::Write, path::Path};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn at(r: &StateBackupRecord) -> Result<u64> {
    Ok(crate::journal::now_ms()
        .map_err(error)?
        .max(r.updated_at_ms))
}
struct Observer<'a> {
    j: &'a mut UpdateJournal,
    a: &'a UpdateAuthority,
    s: &'a SupervisorRecord,
    guard: &'a ShutdownExitGuard,
    record: StateBackupRecord,
}
impl Observer<'_> {
    fn current(&self) -> Result<()> {
        self.j
            .require_state_backup_current(self.a, self.s, &self.record)
            .map_err(error)
    }
    fn save(&mut self, next: StateBackupRecord) -> Result<()> {
        self.record = self
            .j
            .update_state_backup(self.a, self.s, &self.record, next, self.guard)
            .map_err(error)?;
        Ok(())
    }
}
impl ProbeObserver for Observer<'_> {
    fn before_create(&mut self) -> Result<()> {
        self.guard
            .require(&self.record.intent.binding.shutdown.intent)?;
        self.current()
    }
    fn before_resume(&mut self, process: &SupervisorProcess, job: &str) -> Result<()> {
        if job != self.record.intent.job_name() {
            return Err("Backup probe Job differs".into());
        }
        self.save(
            self.record
                .grant_resume(process.clone(), at(&self.record)?)?,
        )
    }
    fn resumed(&mut self) -> Result<()> {
        self.save(self.record.resumed(at(&self.record)?)?)
    }
    fn poll(&mut self) -> Result<()> {
        self.current()
    }
    fn finished(&mut self, cleanup: &ProbeCleanup) -> Result<()> {
        self.save(self.record.finished(cleanup.clone(), at(&self.record)?)?)
    }
}
/// A saved Prepared row cannot call this API: only fresh admission supplies the
/// non-Serde permit, and the original physical exit/lock guard is also required.
/// No runtime files are replaced. The caller must retain custody after return.
pub fn capture_final_state(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: StateBackupPermit,
    guard: &ShutdownExitGuard,
) -> Result<StateBackupRecord> {
    let id = permit.intent.update_id().to_string();
    let mut record = j
        .state_backup(&id)
        .map_err(error)?
        .ok_or("Backup admission missing")?;
    if record.intent != permit.intent || record.phase != StateBackupPhase::Prepared {
        return Err("Only original fresh Prepared backup may dispatch; never replay".into());
    }
    guard.require(&record.intent.binding.shutdown.intent)?;
    let result = capture(j, a, s, guard, &mut record);
    match result {
        Ok(()) => Ok(record),
        Err(reason) => {
            // A failed receipt may leave an earlier revision. Observe that
            // revision; never recreate a process or overwrite partial files.
            if let Ok(Some(saved)) = j.state_backup(&id) {
                if saved.phase != StateBackupPhase::Verified
                    && saved.phase != StateBackupPhase::Indeterminate
                {
                    if let Ok(next) = saved.indeterminate(
                        reason
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(900)
                            .collect(),
                        at(&saved)?,
                    ) {
                        let _ = j.update_state_backup(a, s, &saved, next, guard);
                    }
                }
            }
            Err(reason)
        }
    }
}
fn capture(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    guard: &ShutdownExitGuard,
    record: &mut StateBackupRecord,
) -> Result<()> {
    j.require_state_backup_current(a, s, record)
        .map_err(error)?;
    let cfg = crate::preparation::configuration(s)?;
    let intent = &record.intent;
    let image = artifact::pin(Path::new(
        &intent
            .binding
            .shutdown
            .intent
            .binding
            .loaded_worker
            .image
            .path,
    ))?;
    let config = artifact::pin(&cfg.worker_config_path)?;
    if image.evidence != intent.binding.shutdown.intent.binding.loaded_worker.image
        || config.evidence != intent.binding.worker_config
        || !crate::worker_rehearsal::supports(&image, WORKER_STATE_BACKUP_MARKER)?
    {
        return Err("Exact supported old backup helper/config required; no process created".into());
    }
    let _parent = super::export::parent_pin(Path::new(&intent.binding.parent_directory))?;
    if winbridge_platform::file_identity(&_parent).map_err(error)? != intent.binding.parent_identity
    {
        return Err("Backup parent rebound before input creation".into());
    }
    j.require_state_backup_current(a, s, record)
        .map_err(error)?;
    let raw = serde_json::to_vec(intent).map_err(error)?;
    if raw.len() > 65536 {
        return Err("Backup input exceeds bound".into());
    }
    let input_path = intent.input_path();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = options.open(&input_path).map_err(error)?;
    writer.write_all(&raw).map_err(error)?;
    writer.sync_all().map_err(error)?;
    let input_identity = winbridge_platform::file_identity(&writer).map_err(error)?;
    drop(writer);
    winbridge_platform::source_files::sync_parent(&input_path).map_err(error)?;
    let input = artifact::pin(&input_path)?;
    if input.evidence.sha256 != crate::sha256(&raw)
        || input.evidence.file_identity != input_identity
    {
        return Err("Backup input changed".into());
    }
    let evidence = StateBackupFileEvidence {
        path: input.evidence.path.clone(),
        bytes: input.evidence.bytes,
        sha256: input.evidence.sha256.clone(),
        file_identity: input.evidence.file_identity.clone(),
    };
    let next = record.issue(evidence, at(record)?)?;
    *record = j
        .update_state_backup(a, s, record, next, guard)
        .map_err(error)?;
    let arguments = vec![
        "--export-update-state".into(),
        input.evidence.path.clone(),
        input.evidence.sha256.clone(),
    ];
    let job = record.intent.job_name();
    let mut observer = Observer {
        j,
        a,
        s,
        guard,
        record: record.clone(),
    };
    let outcome = artifact::run_probe_observed(&image, &arguments, 120000, &job, &mut observer);
    *record = observer.record;
    let (stdout, stderr, process) = outcome?;
    if !stderr.is_empty() || record.process.as_ref() != Some(&process) {
        return Err("Backup helper diagnostics/process mismatch".into());
    }
    let report: WorkerStateBackupReport =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid final backup report/fields")?;
    let backup = pin_backup_file(&record.intent.backup_path())?;
    if backup.evidence() != &report.backup {
        return Err("Backup actual file/SHA differs from report".into());
    }
    guard.require(&record.intent.binding.shutdown.intent)?;
    if artifact::inspect_image(&cfg.worker_config_path)? != config.evidence
        || artifact::inspect_image(&input_path)? != input.evidence
    {
        return Err("Backup input/config changed before receipt".into());
    }
    backup.revalidate()?;
    let next = record.verify(report, at(record)?)?;
    *record = j
        .update_state_backup(a, s, record, next, guard)
        .map_err(error)?;
    Ok(())
}
