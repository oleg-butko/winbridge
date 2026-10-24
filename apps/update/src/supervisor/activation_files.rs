//! The live binding actor owns exactly one phase; data never reconstructs it.
use super::*;
enum State {
    Idle,
    #[cfg(windows)]
    Binding(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Bound(Box<crate::activation::ActivationFilesGuard>),
    #[cfg(windows)]
    Failed(Box<crate::activation::ActivationFilesFailure>),
}
pub(super) struct FilesActor {
    state: State,
}
impl Default for FilesActor {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}
impl FilesActor {
    pub(super) fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }
    #[cfg(windows)]
    pub(super) fn begin(&mut self, record: &crate::activation::ActivationRecord) -> Result<()> {
        if !self.is_idle() {
            return Err("Activation files already admitted; no replay".into());
        }
        self.state = State::Binding(Box::new(record.clone()));
        Ok(())
    }
    #[cfg(windows)]
    pub(super) fn complete(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationFilesGuard>,
            crate::activation::ActivationFilesFailure,
        >,
    ) -> Result<()> {
        let expected = match &self.state {
            State::Binding(record) => record.as_ref(),
            _ => return Err("Unexpected activation files completion; no state reset".into()),
        };
        let (state, actual, reason) = match result {
            Ok(guard) => {
                let actual = guard.preparation().clone();
                (State::Bound(guard), actual, None)
            }
            Err(failure) => {
                let actual = failure.preparation().clone();
                let reason = failure.reason.clone();
                (State::Failed(Box::new(failure)), actual, Some(reason))
            }
        };
        let same = actual == *expected;
        self.state = state;
        if !same {
            return Err("Activation files completion changed frozen scope".into());
        }
        reason.map_or(Ok(()), Err)
    }
    pub(super) fn require_scope(&self, _s: &SupervisorRecord) -> Result<()> {
        let record: Option<&crate::activation::ActivationRecord> = match &self.state {
            State::Idle => None,
            #[cfg(windows)]
            State::Binding(r) => Some(r.as_ref()),
            #[cfg(windows)]
            State::Bound(g) => Some(g.preparation()),
            #[cfg(windows)]
            State::Failed(e) => Some(e.preparation()),
        };
        if let Some(record) = record {
            if record.intent.source.update_id != _s.spec.update_id
                || Some(record.intent.source.owner_epoch) != _s.owner_epoch
            {
                return Err("Resident activation files changed scope".into());
            }
        }
        Ok(())
    }
    pub(super) fn view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_files_snapshot(directory, id, active, error)
            .map_err(super::error)?;
        if let Some(replacement) = &mut view.runtime_replacement {
            replacement.physical_custody_held = signal.held();
        }
        if let Some(preparation) = &mut view.activation_preparation {
            preparation.physical_custody_held = signal.held();
        }
        let files = view
            .activation_files
            .as_mut()
            .ok_or("Activation files view missing")?;
        files.original_state_held = signal.held();
        match &self.state {
            State::Idle => {}
            #[cfg(windows)]
            State::Binding(_) => {
                files.phase = ActivationFilesPhase::Binding;
            }
            #[cfg(windows)]
            State::Bound(guard) => {
                let observed = guard.observation();
                files.phase = ActivationFilesPhase::Bound;
                files.held_files = 5;
                files.worker = observed.images.first().cloned();
                files.manifest_sha256 = Some(observed.manifest_file.sha256.clone());
            }
            #[cfg(windows)]
            State::Failed(failure) => {
                files.phase = ActivationFilesPhase::Indeterminate;
                files.held_files = failure.held_files();
                files.reason = Some(failure.reason.chars().take(1000).collect());
            }
        }
        Ok(view)
    }
}
