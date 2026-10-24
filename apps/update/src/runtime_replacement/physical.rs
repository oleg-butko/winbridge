//! Native custody conversion, never reconstructed from a saved record.
//! Each native effect requires the exact already-committed intent and one owner.
use super::*;
use crate::{
    journal::{AdmissionPins, UpdateAuthority, UpdateJournal},
    shutdown::{ExitedStateCustody, ShutdownExitGuard},
};
use std::path::Path;
use winbridge_platform::{runtime_files::OwnedRuntimeFile, source_files::PinnedDirectory};
use winbridge_protocol::update::SupervisorRecord;
#[path = "activation_files.rs"]
mod activation_files;
pub use activation_files::{
    ActivationFilesFailure, ActivationFilesGuard, ActivationLaunchFailure, ActivationProcessGuard,
};
fn exact_names(
    path: &Path,
    expected: &std::collections::HashSet<std::ffi::OsString>,
) -> Result<()> {
    let actual = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.file_name()).map_err(|e| e.to_string()))
        .collect::<Result<std::collections::HashSet<_>>>()?;
    if actual != *expected {
        return Err(
            "Replacement directory contains unexpected/missing entries; preserve all".into(),
        );
    }
    Ok(())
}

pub struct RuntimeReplacementGuard {
    permit: RuntimeReplacementPermit,
    original: Option<ShutdownExitGuard>,
    exited: Option<ExitedStateCustody>,
    anchors: Option<AdmissionPins>,
    old: Vec<OwnedRuntimeFile>,
    prepared: Vec<OwnedRuntimeFile>,
    retained: Option<PinnedDirectory>,
    directory_attempted: bool,
    error: Option<String>,
    signal: CustodySignal,
}
/// Failure keeps the very same kernel resources and any partial acquisitions.
/// It cannot be converted back to a fresh permit or retried as a new acquisition.
pub struct RuntimeReplacementFailure {
    pub reason: String,
    pub custody: Box<RuntimeReplacementGuard>,
}
impl std::fmt::Debug for RuntimeReplacementFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeReplacementFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for RuntimeReplacementFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for RuntimeReplacementFailure {}
impl RuntimeReplacementGuard {
    /// Consume a freshly admitted permit and the unique ORIGINAL exit guard.
    /// Prepared handles and fixed anchors are acquired before old read pins drop.
    pub fn acquire(
        original: ShutdownExitGuard,
        permit: RuntimeReplacementPermit,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        signal: CustodySignal,
    ) -> std::result::Result<Self, RuntimeReplacementFailure> {
        let mut guard = Self {
            permit,
            original: Some(original),
            exited: None,
            anchors: None,
            old: Vec::new(),
            prepared: Vec::new(),
            retained: None,
            directory_attempted: false,
            error: None,
            signal,
        };
        match guard.acquire_inner(j, a, s) {
            Ok(()) => Ok(guard),
            Err(reason) => {
                guard.stop(reason);
                let reason = guard.error.as_ref().expect("Failure retained").clone();
                Err(RuntimeReplacementFailure {
                    reason,
                    custody: Box::new(guard),
                })
            }
        }
    }
    fn acquire_inner(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        let record = RuntimeReplacementRecord::planned(self.permit.intent.clone())?;
        j.require_runtime_replacement_current(a, s, &record, &self.permit)
            .map_err(|e| e.to_string())?;
        let original = self
            .original
            .as_ref()
            .ok_or("Original custody missing before conversion")?;
        original.require(self.permit.shutdown_intent())?;
        self.signal
            .0
            .store(true, std::sync::atomic::Ordering::Release);
        let retained = self.permit.intent.retained_directory();
        if retained.try_exists().map_err(|e| e.to_string())? {
            return Err("Displaced runtime directory already exists; preserve, never adopt".into());
        }
        self.anchors = Some(
            j.replacement_anchors(a, s, &record, &self.permit, original)
                .map_err(|e| e.to_string())?,
        );
        for file in &record.batch.files {
            j.require_runtime_replacement_current(a, s, &record, &self.permit)
                .map_err(|e| e.to_string())?;
            let image = &file.spec.prepared;
            self.prepared.push(
                OwnedRuntimeFile::acquire(
                    Path::new(&image.path),
                    &image.file_identity,
                    image.bytes,
                    &image.sha256,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        self.original
            .as_ref()
            .ok_or("Original custody missing")?
            .require(self.permit.shutdown_intent())?;
        self.anchors
            .as_ref()
            .ok_or("Replacement anchors missing")?
            .revalidate()?;
        j.require_runtime_replacement_current(a, s, &record, &self.permit)
            .map_err(|e| e.to_string())?;
        // From this point no failure drops the original process/state/instance
        // locks: either success or the Failure object owns the same resources.
        let original = self
            .original
            .take()
            .ok_or("Original custody already consumed")?;
        let (exited, old_read_pins) = original.into_replacement_parts();
        self.exited = Some(exited);
        drop(old_read_pins);
        for file in &record.batch.files {
            j.require_runtime_replacement_current(a, s, &record, &self.permit)
                .map_err(|e| e.to_string())?;
            let image = &file.spec.old;
            self.old.push(
                OwnedRuntimeFile::acquire(
                    Path::new(&image.path),
                    &image.file_identity,
                    image.bytes,
                    &image.sha256,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        self.require_progress(&record)?;
        j.require_runtime_replacement_current(a, s, &record, &self.permit)
            .map_err(|e| e.to_string())
    }
    pub fn permit(&self) -> &RuntimeReplacementPermit {
        &self.permit
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn stop(&mut self, reason: String) {
        if self.error.is_none() {
            let reason: String = reason
                .chars()
                .filter(|c| !c.is_control())
                .take(900)
                .collect();
            self.error = Some(if reason.is_empty() {
                "Replacement outcome unknown; inspect original attempt".into()
            } else {
                reason
            });
        }
    }
    /// Create only after DirectoryIssued is committed. Even collision consumes
    /// this guard's single directory attempt; never adopt, remove or retry.
    pub fn create_retained_directory(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        record: &RuntimeReplacementRecord,
    ) -> Result<RuntimeReplacementRecord> {
        let result: Result<RuntimeReplacementRecord> = (|| {
            if !matches!(record.state, RuntimeReplacementState::DirectoryIssued {})
                || self.directory_attempted
            {
                return Err("Retained directory needs original unexecuted saved issue".into());
            }
            self.require_progress(record)?;
            j.require_runtime_replacement_current(a, s, record, &self.permit)
                .map_err(|e| e.to_string())?;
            let path = self.permit.intent.retained_directory();
            self.directory_attempted = true;
            std::fs::DirBuilder::new()
                .recursive(false)
                .create(&path)
                .map_err(|e| e.to_string())?;
            winbridge_platform::source_files::sync_parent(&path).map_err(|e| e.to_string())?;
            let retained = PinnedDirectory::acquire(&path, None).map_err(|e| e.to_string())?;
            let identity = retained.identity().to_owned();
            self.retained = Some(retained);
            let next = record.directory_created(identity)?;
            self.require_progress(&next)?;
            j.require_runtime_replacement_current(a, s, record, &self.permit)
                .map_err(|e| e.to_string())?;
            Ok(next)
        })();
        if let Err(reason) = &result {
            self.stop(reason.clone());
        }
        result
    }
    /// Only the first incomplete file and its saved RetireIssued/PlaceIssued
    /// decide the native operation. No caller-selected path/index/receipt.
    pub fn execute_next_move(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        record: &RuntimeReplacementRecord,
    ) -> Result<RuntimeReplacementRecord> {
        let result = self.move_inner(j, a, s, record);
        if let Err(reason) = &result {
            self.stop(reason.clone());
        }
        result
    }
    fn move_inner(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        record: &RuntimeReplacementRecord,
    ) -> Result<RuntimeReplacementRecord> {
        self.require_progress(record)?;
        j.require_runtime_replacement_current(a, s, record, &self.permit)
            .map_err(|e| e.to_string())?;
        if !matches!(record.state, RuntimeReplacementState::Replacing { .. }) {
            return Err("File move requires retained-directory receipt".into());
        }
        let i = record
            .batch
            .next_file()?
            .ok_or("Replacement already complete")?;
        let file = &record.batch.files[i];
        let next = match &file.state {
            FileExchangeState::RetireIssued {} => {
                let parent = self
                    .retained
                    .as_ref()
                    .ok_or("Retained directory not owned")?
                    .identity()
                    .to_owned();
                self.old[i]
                    .move_new(&file.spec.retained_path(), &parent)
                    .map_err(|e| e.to_string())?;
                let mut image = file.spec.old.clone();
                image.path = self.old[i]
                    .path()
                    .to_str()
                    .ok_or("Moved old path requires UTF-8")?
                    .into();
                file.retired(image)?
            }
            FileExchangeState::PlaceIssued { .. } => {
                self.prepared[i]
                    .move_new(
                        &file.spec.installed_path(),
                        &self.permit.intent.preparation.plan.runtime_identity,
                    )
                    .map_err(|e| e.to_string())?;
                let mut image = file.spec.prepared.clone();
                image.path = self.prepared[i]
                    .path()
                    .to_str()
                    .ok_or("Moved new path requires UTF-8")?
                    .into();
                file.installed(image)?
            }
            _ => return Err("No original saved move intent; observe without replay".into()),
        };
        let next = record.advance_file(i, next)?;
        self.require_progress(&next)?;
        j.require_runtime_replacement_current(a, s, record, &self.permit)
            .map_err(|e| e.to_string())?;
        #[cfg(feature = "update-validation")]
        super::validation::hold_before_receipt(record, &next, || {
            self.require_exit_custody()?;
            j.check_source_authority(a).map_err(|e| e.to_string())
        })?;
        Ok(next)
    }
    /// Custody is independent of success: failed acquisition still keeps locks.
    pub fn require_exit_custody(&self) -> Result<()> {
        match (&self.original, &self.exited) {
            (Some(original), None) => original.require_exit_state(self.permit.shutdown_intent()),
            (None, Some(exited)) => exited.require(self.permit.shutdown_intent()),
            _ => Err("Original exit custody was lost or duplicated".into()),
        }
    }
    /// Check current locations against an exact logical prefix, rather than
    /// requiring the whole old runtime to stay installed after its first move.
    pub fn require_progress(&mut self, record: &RuntimeReplacementRecord) -> Result<()> {
        record.validate()?;
        if record.intent != self.permit.intent
            || self.error.is_some()
            || matches!(record.state, RuntimeReplacementState::Indeterminate { .. })
            || self.old.len() != 5
            || self.prepared.len() != 5
        {
            return Err("Replacement scope/terminal outcome or owned set differs".into());
        }
        self.require_exit_custody()?;
        self.anchors
            .as_ref()
            .ok_or("Replacement anchors missing")?
            .revalidate()?;
        if let RuntimeReplacementState::Replacing { directory_identity }
        | RuntimeReplacementState::FilesMoved { directory_identity } = &record.state
        {
            let retained = self
                .retained
                .as_ref()
                .ok_or("Retained directory not physically owned")?;
            if retained.identity() != directory_identity {
                return Err("Retained directory receipt differs from original pin".into());
            }
            retained.revalidate().map_err(|e| e.to_string())?;
        }
        for (i, file) in record.batch.files.iter().enumerate() {
            let (old, new) = match &file.state {
                FileExchangeState::Prepared {} | FileExchangeState::RetireIssued {} => (
                    Path::new(&file.spec.old.path),
                    Path::new(&file.spec.prepared.path),
                ),
                FileExchangeState::Retired { retained }
                | FileExchangeState::PlaceIssued { retained } => (
                    Path::new(&retained.path),
                    Path::new(&file.spec.prepared.path),
                ),
                FileExchangeState::Installed {
                    retained,
                    installed,
                } => (Path::new(&retained.path), Path::new(&installed.path)),
                FileExchangeState::Indeterminate { .. } => {
                    return Err("File outcome indeterminate; no physical effects".into())
                }
            };
            self.old[i].revalidate().map_err(|e| e.to_string())?;
            self.prepared[i].revalidate().map_err(|e| e.to_string())?;
            if self.old[i].path() != old
                || self.prepared[i].path() != new
                || self.old[i].identity() != file.spec.old.file_identity
                || self.prepared[i].identity() != file.spec.prepared.file_identity
            {
                return Err(
                    "Owned replacement file path/identity differs from saved prefix".into(),
                );
            }
        }
        let remaining = record
            .batch
            .files
            .iter()
            .filter(|f| !matches!(f.state, FileExchangeState::Installed { .. }))
            .map(|f| std::ffi::OsString::from(f.spec.file.name()))
            .collect();
        exact_names(&self.permit.intent.preparation.plan.directory(), &remaining)?;
        let retained = record
            .batch
            .files
            .iter()
            .filter(|f| {
                matches!(
                    f.state,
                    FileExchangeState::Retired { .. }
                        | FileExchangeState::PlaceIssued { .. }
                        | FileExchangeState::Installed { .. }
                )
            })
            .map(|f| std::ffi::OsString::from(f.spec.file.name()))
            .collect();
        if self.retained.is_some() {
            exact_names(&self.permit.intent.retained_directory(), &retained)?;
        } else if self
            .permit
            .intent
            .retained_directory()
            .try_exists()
            .map_err(|e| e.to_string())?
        {
            return Err(
                "Retained directory appeared outside original creation; never adopt".into(),
            );
        }
        Ok(())
    }
}
impl Drop for RuntimeReplacementGuard {
    fn drop(&mut self) {
        self.signal
            .0
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_directory_check_rejects_unknown_entries_without_deleting_them() {
        let temp = tempfile::tempdir().unwrap();
        let empty = std::collections::HashSet::new();
        exact_names(temp.path(), &empty).unwrap();
        let sentinel = temp.path().join("sentinel");
        std::fs::write(&sentinel, b"preserve unknown file").unwrap();
        assert!(exact_names(temp.path(), &empty).is_err());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserve unknown file");
        let expected = [std::ffi::OsString::from("sentinel")].into_iter().collect();
        exact_names(temp.path(), &expected).unwrap();
        std::fs::create_dir(temp.path().join("unexpected-directory")).unwrap();
        assert!(exact_names(temp.path(), &expected).is_err());
        assert!(temp.path().join("unexpected-directory").is_dir());
    }
}
