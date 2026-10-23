//! Consuming preparation driver. It never renames installed files or activates a worker.
use super::*;
use crate::{
    artifact::{self, Pinned},
    journal::{UpdateAuthority, UpdateJournal},
    shutdown::ShutdownExitGuard,
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use winbridge_platform::source_files::{directory_identity, sync_parent, PinnedDirectory};
use winbridge_protocol::update::SupervisorRecord;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
trait Observer {
    fn checkpoint(&mut self, record: &RuntimePreparationRecord) -> Result<()>;
    fn pulse(&mut self, record: &RuntimePreparationRecord) -> Result<()>;
    fn save(
        &mut self,
        old: &RuntimePreparationRecord,
        next: RuntimePreparationRecord,
    ) -> Result<RuntimePreparationRecord>;
}
struct JournalObserver<'a> {
    journal: &'a mut UpdateJournal,
    owner: &'a UpdateAuthority,
    supervisor: &'a SupervisorRecord,
    permit: &'a RuntimePreparationPermit,
    guard: &'a ShutdownExitGuard,
}
impl Observer for JournalObserver<'_> {
    fn checkpoint(&mut self, r: &RuntimePreparationRecord) -> Result<()> {
        self.journal
            .require_runtime_preparation_current(self.owner, self.supervisor, r)
            .map_err(error)?;
        let saved = self
            .journal
            .state_backup(&r.plan.update_id)
            .map_err(error)?
            .ok_or("Final backup missing")?;
        self.guard.require(&saved.intent.binding.shutdown.intent)?;
        crate::preparation::configuration(self.supervisor)?;
        Ok(())
    }
    fn pulse(&mut self, r: &RuntimePreparationRecord) -> Result<()> {
        self.journal
            .preparation_stream_fence(self.owner, self.supervisor, &r.plan)
            .map_err(error)
    }
    fn save(
        &mut self,
        old: &RuntimePreparationRecord,
        next: RuntimePreparationRecord,
    ) -> Result<RuntimePreparationRecord> {
        self.journal
            .update_runtime_preparation(
                self.owner,
                self.supervisor,
                old,
                next,
                self.permit,
                self.guard,
            )
            .map_err(error)
    }
}
/// Sources, exact final backup and local policy remain pinned through the final
/// receipt. The original guard is borrowed; caller keeps custody after retirement.
pub fn prepare_runtime_files(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: RuntimePreparationPermit,
    guard: &ShutdownExitGuard,
) -> Result<RuntimePreparationRecord> {
    let mut record = j
        .runtime_preparation(&permit.plan.update_id)
        .map_err(error)?
        .ok_or("Preparation admission missing")?;
    if record.plan != permit.plan || record.phase != PreparationPhase::Planned {
        return Err(
            "Only original fresh Planned preparation may dispatch; no resume/replay".into(),
        );
    }
    let result = (|| {
        j.require_runtime_preparation_current(a, s, &record)
            .map_err(error)?;
        let cfg = crate::preparation::configuration(s)?;
        if cfg.runtime_directory.canonicalize().map_err(error)?
            != Path::new(&record.plan.runtime_directory)
        {
            return Err("Preparation policy runtime differs".into());
        }
        let config = artifact::pin(Path::new(&s.spec.config_path))?;
        if config.evidence.sha256 != s.spec.config_sha256 {
            return Err("Preparation policy bytes changed".into());
        }
        let saved = j
            .state_backup(&record.plan.update_id)
            .map_err(error)?
            .ok_or("Verified backup missing")?;
        guard.require(&saved.intent.binding.shutdown.intent)?;
        let report = saved
            .report
            .as_ref()
            .ok_or("Verified backup report missing")?;
        let backup = crate::state_backup::pin_backup_file(Path::new(&report.backup.path))?;
        if backup.evidence() != &report.backup {
            return Err("Final backup changed before preparation".into());
        }
        let worker_config = artifact::pin(&cfg.worker_config_path)?;
        if worker_config.evidence != saved.intent.binding.worker_config {
            return Err("Approved worker config changed".into());
        }
        let previous = j.runtime_preimages(&record.plan.update_id).map_err(error)?;
        let previous_intent = previous.intent.ok_or("Previous runtime intent missing")?;
        let previous = previous.receipt.ok_or("Previous runtime copy missing")?;
        let _previous_directory = PinnedDirectory::acquire(
            Path::new(&previous_intent.directory),
            Some(&previous.directory_identity),
        )
        .map_err(error)?;
        let _previous = previous
            .files
            .iter()
            .map(|file| {
                let pin = artifact::pin(Path::new(&file.path))?;
                if pin.evidence != *file {
                    return Err("Previous runtime copy changed".into());
                }
                Ok(pin)
            })
            .collect::<Result<Vec<_>>>()?;
        let artifacts = j
            .candidate_artifacts(&record.plan.update_id)
            .map_err(error)?;
        let payload = Path::new(&record.plan.runtime_directory)
            .join("update")
            .join(&record.plan.update_id)
            .join("candidate")
            .join("payload");
        let _payload = PinnedDirectory::acquire(
            &payload,
            Some(&artifacts.inspection.payload_directory_identity),
        )
        .map_err(error)?;
        let mut observer = JournalObserver {
            journal: j,
            owner: a,
            supervisor: s,
            permit: &permit,
            guard,
        };
        let result = copy_set(&mut observer, &mut record);
        // Keep all pins alive across the final SQLite commit, then validate once
        // more for development adapters which cannot deny external writes.
        if result.is_ok() {
            backup.revalidate()?;
            config.file.metadata().map_err(error)?;
        }
        result
    })();
    if let Err(reason) = result {
        if let Ok(Some(saved)) = j.runtime_preparation(&permit.plan.update_id) {
            if !matches!(
                saved.phase,
                PreparationPhase::Prepared | PreparationPhase::Indeterminate
            ) {
                let reason: String = reason
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(900)
                    .collect();
                if let Ok(next) = saved.indeterminate(if reason.is_empty() {
                    "Preparation failed; inspect original attempt".into()
                } else {
                    reason.clone()
                }) {
                    let _ = j.update_runtime_preparation(a, s, &saved, next, &permit, guard);
                }
            }
        }
        return Err(reason);
    }
    Ok(record)
}
fn exact_names(record: &RuntimePreparationRecord) -> Result<()> {
    let mut actual = fs::read_dir(record.plan.directory())
        .map_err(error)?
        .map(|entry| entry.map(|e| e.file_name()).map_err(error))
        .collect::<Result<Vec<_>>>()?;
    let mut expected = record
        .plan
        .files
        .iter()
        .zip(&record.files)
        .filter(|(_, state)| matches!(state, PreparationFileState::Verified { .. }))
        .map(|(file, _)| std::ffi::OsString::from(file.file.name()))
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err("Prepared directory contains unexpected or missing files; preserve all".into());
    }
    Ok(())
}
fn manifest_new(record: &RuntimePreparationRecord) -> Result<Pinned> {
    let destination = record.plan.directory().join(RuntimeFile::Manifest.name());
    let raw = record.plan.manifest_bytes()?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut writer = options.open(&destination).map_err(error)?;
    let id = winbridge_platform::file_identity(&writer).map_err(error)?;
    writer.write_all(&raw).map_err(error)?;
    writer.sync_all().map_err(error)?;
    drop(writer);
    sync_parent(&destination).map_err(error)?;
    let pin = artifact::pin(&destination)?;
    if pin.evidence.file_identity != id {
        return Err("Prepared manifest replaced before pinning".into());
    }
    Ok(pin)
}
fn copy_set(observer: &mut impl Observer, record: &mut RuntimePreparationRecord) -> Result<()> {
    record.validate()?;
    if record.phase != PreparationPhase::Planned {
        return Err("Saved preparation cannot resume file effects".into());
    }
    let root = PinnedDirectory::acquire(
        Path::new(&record.plan.runtime_directory),
        Some(&record.plan.runtime_identity),
    )
    .map_err(error)?;
    let directory = record.plan.directory();
    let parent = PinnedDirectory::acquire(
        directory.parent().ok_or("Preparation parent missing")?,
        Some(&record.plan.parent_identity),
    )
    .map_err(error)?;
    let sources = record
        .plan
        .files
        .iter()
        .filter_map(|f| f.source.as_ref())
        .map(|source| {
            let pin = artifact::pin(Path::new(&source.path))?;
            if pin.evidence != *source {
                return Err("Candidate source changed before preparation".into());
            }
            Ok(pin)
        })
        .collect::<Result<Vec<_>>>()?;
    if fs2::available_space(directory.parent().ok_or("Preparation parent missing")?)
        .map_err(error)?
        < 6 * 1024 * 1024 * 1024 + record.plan.required_bytes()?
    {
        return Err("Preparation needs 6 GiB reserve plus copies; no cleanup".into());
    }
    if directory.try_exists().map_err(error)? {
        return Err("Prepared destination already exists; never adopt/overwrite".into());
    }
    observer.checkpoint(record)?;
    *record = observer.save(record, record.issue_directory()?)?;
    observer.pulse(record)?;
    parent.revalidate().map_err(error)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).map_err(error)?;
    sync_parent(&directory).map_err(error)?;
    let target = PinnedDirectory::acquire(&directory, None).map_err(error)?;
    *record = observer.save(record, record.directory_created(target.identity().into())?)?;
    let mut copies = Vec::new();
    for (index, source) in sources
        .iter()
        .map(Some)
        .chain(std::iter::once(None))
        .enumerate()
    {
        root.revalidate().map_err(error)?;
        parent.revalidate().map_err(error)?;
        target.revalidate().map_err(error)?;
        observer.checkpoint(record)?;
        exact_names(record)?;
        *record = observer.save(record, record.issue_file(index)?)?;
        observer.pulse(record)?;
        let copy = if let Some(source) = source {
            source.copy_new_checked(Path::new(&record.plan.files[index].destination), || {
                observer.pulse(record)
            })?
        } else {
            manifest_new(record)?
        };
        #[cfg(feature = "update-validation")]
        super::validation::hold_before_receipt(record, index, &copy, || observer.pulse(record))?;
        target.revalidate().map_err(error)?;
        parent.revalidate().map_err(error)?;
        observer.checkpoint(record)?;
        let next = record.file_created(index, copy.evidence.clone())?;
        copies.push(copy);
        exact_names(&next)?;
        // Each original create/copy identity is pinned through receipt commit.
        *record = observer.save(record, next)?;
    }
    exact_names(record)?;
    root.revalidate().map_err(error)?;
    parent.revalidate().map_err(error)?;
    target.revalidate().map_err(error)?;
    if directory_identity(&directory).map_err(error)? != target.identity() {
        return Err("Prepared destination rebound".into());
    }
    observer.checkpoint(record)?;
    record.exchange_batch()?;
    Ok(())
}

#[cfg(test)]
mod tests;
