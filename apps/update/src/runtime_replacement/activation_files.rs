//! Whole-set bridge retains the original actor/exit/state/instance resources.
//! Read binding is not process creation, state handoff or a persisted permission.
use super::*;
use crate::{
    activation::ActivationPreparation,
    installed_set::{pin_installed_set, PinnedInstalledSet},
};
use std::collections::VecDeque;
use winbridge_platform::runtime_files::{RuntimeReadHandoffFailure, RuntimeReadPin};
#[path = "activation_process.rs"]
mod activation_process;
pub use activation_process::{
    ActivationControllerPreparation, ActivationFinalizationFailure, ActivationFinalizationGuard,
    ActivationFinalizationPreparation, ActivationLaunchFailure, ActivationLifetimePreparation,
    ActivationProcessGuard, ActivationServicePreparation,
};

/// Only a successful consuming factory can construct this object. Its installed
/// set denies writers/deleters; the original exited process and state stay held.
pub struct ActivationFilesGuard {
    preparation: ActivationPreparation,
    original: Box<RuntimeReplacementGuard>,
    installed: PinnedInstalledSet,
}
struct BindingWork {
    preparation: ActivationPreparation,
    original: Box<RuntimeReplacementGuard>,
    pending: VecDeque<OwnedRuntimeFile>,
    pins: Vec<RuntimeReadPin>,
    failed_file: Option<RuntimeReadHandoffFailure>,
}
/// Dropping this loses live resources, never permits automatic adoption/replay.
/// Keep the failure resident until explicit reconciliation/owner termination.
pub struct ActivationFilesFailure {
    pub reason: String,
    _custody: Box<BindingWork>,
}
impl std::fmt::Debug for ActivationFilesFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivationFilesFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ActivationFilesFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for ActivationFilesFailure {}
impl ActivationFilesFailure {
    pub fn preparation(&self) -> &crate::activation::ActivationRecord {
        self._custody.preparation.record()
    }
    pub fn held_files(&self) -> u16 {
        self._custody.pins.len() as u16
    }
}
impl BindingWork {
    fn require_fixed_custody(&self, r: &RuntimeReplacementRecord) -> Result<()> {
        if self.original.permit.intent != r.intent
            || !matches!(r.state, RuntimeReplacementState::FilesMoved { .. })
        {
            return Err("Activation files changed replacement/terminal scope".into());
        }
        self.original.require_exit_custody()?;
        self.original
            .anchors
            .as_ref()
            .ok_or("Original anchors missing")?
            .revalidate()?;
        self.original
            .retained
            .as_ref()
            .ok_or("Original retained directory missing")?
            .revalidate()
            .map_err(|e| e.to_string())
    }
    fn bind(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<PinnedInstalledSet> {
        let mut timing =
            crate::activation::ActivationTiming::new("bind_files", Some(self.preparation.deadline));
        timing.mark("initial_graph_before", None, None);
        let r = j
            .require_activation_prepared(a, s, &self.preparation)
            .map_err(|e| e.to_string())?;
        timing.mark("original_progress_before", None, None);
        self.original.require_progress(&r)?;
        timing.mark("fixed_custody_before", None, None);
        self.require_fixed_custody(&r)?;
        timing.mark("files_before", None, None);
        self.pending = std::mem::take(&mut self.original.prepared).into();
        while let Some(file) = self.pending.pop_front() {
            let index = self.pins.len();
            let mut checkpoint = 0;
            timing.mark("file_before", Some(index), None);
            // The file itself is now exclusively owned by this call. On any
            // failure its opaque error retains the residual kernel resource.
            let result = file.into_read_pin_checked(|| {
                checkpoint += 1;
                timing.mark("checkpoint_graph_before", Some(index), Some(checkpoint));
                j.require_activation_prepared(a, s, &self.preparation)
                    .map_err(std::io::Error::other)?;
                timing.mark("checkpoint_custody_before", Some(index), Some(checkpoint));
                self.require_fixed_custody(&r)
                    .map_err(std::io::Error::other)?;
                // Custody checks may themselves be slow. An entry check cannot
                // authorize a later handle transition after the same deadline.
                self.preparation
                    .require_live_window()
                    .map_err(std::io::Error::other)?;
                timing.mark("checkpoint_done", Some(index), Some(checkpoint));
                Ok(())
            });
            match result {
                Ok(pin) => self.pins.push(pin),
                Err(error) => {
                    let reason = error.to_string();
                    self.failed_file = Some(error);
                    return Err(reason);
                }
            }
            timing.mark("file_bound", Some(index), None);
        }
        if self.pins.len() != 5
            || !self.original.prepared.is_empty()
            || self.original.old.len() != 5
        {
            return Err("Activation file custody set incomplete".into());
        }
        for (i, file) in r.batch.files.iter().enumerate() {
            timing.mark("final_file_before", Some(i), None);
            let FileExchangeState::Installed {
                retained,
                installed,
            } = &file.state
            else {
                return Err("Activation file is not installed".into());
            };
            self.original.old[i]
                .revalidate()
                .map_err(|e| e.to_string())?;
            self.pins[i].revalidate().map_err(|e| e.to_string())?;
            if self.original.old[i].path() != Path::new(&retained.path)
                || self.original.old[i].identity() != retained.file_identity
                || self.pins[i].path() != Path::new(&installed.path)
                || self.pins[i].identity() != installed.file_identity
            {
                return Err("Activation read/retained identity differs".into());
            }
            timing.mark("final_file_after", Some(i), None);
        }
        exact_names(&r.intent.preparation.plan.directory(), &Default::default())?;
        exact_names(
            &r.intent.retained_directory(),
            &r.batch
                .files
                .iter()
                .map(|f| std::ffi::OsString::from(f.spec.file.name()))
                .collect(),
        )?;
        let source = &self.preparation.record().intent.source;
        timing.mark("installed_set_before", None, None);
        let set = pin_installed_set(
            Path::new(&source.runtime_directory),
            &source.installation_id,
            &source.scope_sha256,
        )?;
        timing.mark("installed_set_after", None, None);
        let observed = set.observation();
        let installed: Vec<_> = r
            .batch
            .files
            .iter()
            .map(|f| match &f.state {
                FileExchangeState::Installed { installed, .. } => Ok(installed.clone()),
                _ => Err("Activation installed evidence missing".to_string()),
            })
            .collect::<Result<_>>()?;
        if observed.runtime_identity != source.runtime_identity
            || observed.manifest != r.intent.preparation.plan.manifest
            || observed.manifest_file != installed[4]
            || observed.images != installed[..4]
        {
            return Err("Activation strict installed set differs from moved files/source".into());
        }
        self.require_fixed_custody(&r)?;
        j.require_activation_prepared(a, s, &self.preparation)
            .map_err(|e| e.to_string())?;
        timing.mark("bound", None, None);
        Ok(set)
    }
}
impl ActivationFilesGuard {
    pub fn preparation(&self) -> &crate::activation::ActivationRecord {
        self.preparation.record()
    }
    pub fn observation(&self) -> &crate::installed_set::InstalledSetObservation {
        self.installed.observation()
    }
    /// All conditions are rechecked by future effect admission; this method
    /// itself creates no process and never releases the original instance lock.
    pub fn require_current(
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        j.require_activation_prepared(a, s, &self.preparation)
            .map_err(|e| e.to_string())?;
        self.original.require_exit_custody()?;
        self.original
            .anchors
            .as_ref()
            .ok_or("Activation anchors missing")?
            .revalidate()?;
        self.installed.revalidate()?;
        self.preparation.require_live_window()
    }
}
impl UpdateJournal {
    /// Fresh in-memory preparation and the original native guard are consumed
    /// together. Neither serialized metadata nor an Existing admission suffices.
    pub fn bind_activation_files(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: ActivationPreparation,
        original: Box<RuntimeReplacementGuard>,
    ) -> std::result::Result<Box<ActivationFilesGuard>, ActivationFilesFailure> {
        let mut work = Box::new(BindingWork {
            preparation,
            original,
            pending: VecDeque::new(),
            pins: Vec::new(),
            failed_file: None,
        });
        match work.bind(self, a, s) {
            Ok(installed) => {
                let BindingWork {
                    preparation,
                    original,
                    pending: _,
                    pins: _,
                    failed_file: _,
                } = *work;
                Ok(Box::new(ActivationFilesGuard {
                    preparation,
                    original,
                    installed,
                }))
            }
            Err(reason) => Err(ActivationFilesFailure {
                reason,
                _custody: work,
            }),
        }
    }
}
