//! The live binding actor owns exactly one phase; data never reconstructs it.
use super::*;
#[path = "activation_service.rs"]
mod service;
enum State {
    Idle,
    #[cfg(windows)]
    Binding(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Bound(Box<crate::activation::ActivationFilesGuard>),
    #[cfg(windows)]
    Failed(Box<crate::activation::ActivationFilesFailure>),
    #[cfg(windows)]
    Launching(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Ready(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    LaunchFailed(Box<crate::activation::ActivationLaunchFailure>),
    #[cfg(windows)]
    HandingOff(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Owned(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    HandoffFailed(Box<crate::activation::ActivationLaunchFailure>),
    #[cfg(windows)]
    Writing(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Written(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    WriterUnconfirmed(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    WriterFailed(Box<crate::activation::ActivationLaunchFailure>),
    #[cfg(windows)]
    StartingService(Box<crate::activation::ActivationRecord>),
    #[cfg(windows)]
    Serviced(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    ServiceUnconfirmed(Box<crate::activation::ActivationProcessGuard>),
    #[cfg(windows)]
    ServiceFailed(Box<crate::activation::ActivationLaunchFailure>),
}
pub(super) struct FilesActor {
    state: State,
    writer_key: Option<String>,
    service_key: Option<String>,
}
impl Default for FilesActor {
    fn default() -> Self {
        Self {
            state: State::Idle,
            writer_key: None,
            service_key: None,
        }
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
            #[cfg(windows)]
            State::Launching(r) => Some(r),
            #[cfg(windows)]
            State::Ready(g) => Some(g.record()),
            #[cfg(windows)]
            State::LaunchFailed(e) => Some(e.record()),
            #[cfg(windows)]
            State::HandingOff(r) => Some(r),
            #[cfg(windows)]
            State::Owned(g) => Some(g.record()),
            #[cfg(windows)]
            State::HandoffFailed(e) => Some(e.record()),
            #[cfg(windows)]
            State::Writing(r) | State::StartingService(r) => Some(r),
            #[cfg(windows)]
            State::Written(g)
            | State::WriterUnconfirmed(g)
            | State::Serviced(g)
            | State::ServiceUnconfirmed(g) => Some(g.record()),
            #[cfg(windows)]
            State::WriterFailed(e) | State::ServiceFailed(e) => Some(e.record()),
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
        self.patch_files(&mut view, signal)?;
        Ok(view)
    }
    pub(super) fn process_started(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(
                self.state,
                State::Launching(_)
                    | State::Ready(_)
                    | State::LaunchFailed(_)
                    | State::HandingOff(_)
                    | State::Owned(_)
                    | State::HandoffFailed(_)
                    | State::Writing(_)
                    | State::Written(_)
                    | State::WriterUnconfirmed(_)
                    | State::WriterFailed(_)
                    | State::StartingService(_)
                    | State::Serviced(_)
                    | State::ServiceUnconfirmed(_)
                    | State::ServiceFailed(_)
            )
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(super) fn can_launch(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Bound(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(super) fn handoff_started(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(
                self.state,
                State::HandingOff(_)
                    | State::Owned(_)
                    | State::HandoffFailed(_)
                    | State::Writing(_)
                    | State::Written(_)
                    | State::WriterUnconfirmed(_)
                    | State::WriterFailed(_)
                    | State::StartingService(_)
                    | State::Serviced(_)
                    | State::ServiceUnconfirmed(_)
                    | State::ServiceFailed(_)
            )
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(super) fn can_handoff(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Ready(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(super) fn writer_started(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(
                self.state,
                State::Writing(_)
                    | State::Written(_)
                    | State::WriterUnconfirmed(_)
                    | State::WriterFailed(_)
                    | State::StartingService(_)
                    | State::Serviced(_)
                    | State::ServiceUnconfirmed(_)
                    | State::ServiceFailed(_)
            )
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(super) fn can_activate_writer(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Owned(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(super) fn take_for_writer(
        &mut self,
    ) -> Result<(Box<crate::activation::ActivationProcessGuard>, String)> {
        let record = match &self.state {
            State::Owned(g) => Box::new(g.record().clone()),
            _ => {
                return Err("Writer requires the one original StateOwned owner; no adoption".into())
            }
        };
        let key = uuid::Uuid::new_v4().to_string();
        let State::Owned(guard) = std::mem::replace(&mut self.state, State::Writing(record)) else {
            unreachable!()
        };
        self.writer_key = Some(key.clone());
        Ok((guard, key))
    }
    #[cfg(windows)]
    pub(super) fn complete_writer(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
        saved: Result<Option<crate::activation::ActivationWriterRecord>>,
    ) -> Result<()> {
        let expected = match &self.state {
            State::Writing(r) => Some(r.intent.clone()),
            _ => None,
        };
        let (state, outcome) = match result {
            Ok(guard) => {
                let check = (|| {
                    if expected.as_ref() != Some(&guard.record().intent) {
                        return Err("Writer completion changed scope or was not pending; resources retained".into());
                    }
                    let saved = saved?.ok_or("Writer returned without a durable confirmation")?;
                    validate_saved_writer(
                        &saved,
                        guard.record(),
                        self.writer_key
                            .as_deref()
                            .ok_or("Resident writer key missing")?,
                    )?;
                    Ok(())
                })();
                if check.is_ok() {
                    (State::Written(guard), check)
                } else {
                    (State::WriterUnconfirmed(guard), check)
                }
            }
            Err(failure) => {
                let reason = failure.reason.clone();
                (State::WriterFailed(Box::new(failure)), Err(reason))
            }
        };
        self.state = state; // Retain every outcome, including unexpected completion.
        outcome
    }
    #[cfg(windows)]
    pub(super) fn take_for_handoff(
        &mut self,
    ) -> Result<Box<crate::activation::ActivationProcessGuard>> {
        let record = match &self.state {
            State::Ready(g) => Box::new(g.record().clone()),
            _ => return Err("State handoff needs the same resident Ready owner; no replay".into()),
        };
        let State::Ready(guard) = std::mem::replace(&mut self.state, State::HandingOff(record))
        else {
            unreachable!()
        };
        Ok(guard)
    }
    #[cfg(windows)]
    pub(super) fn complete_handoff(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
    ) -> Result<()> {
        let expected = match &self.state {
            State::HandingOff(r) => &r.intent,
            _ => return Err("Unexpected state handoff completion; no state reset".into()),
        };
        let (state, same, error) = match result {
            Ok(guard) => {
                let same = &guard.record().intent == expected
                    && matches!(
                        guard.record().state,
                        crate::activation::ActivationState::StateOwned {}
                    );
                (State::Owned(guard), same, None)
            }
            Err(failure) => {
                let same = &failure.record().intent == expected;
                let error = failure.reason.clone();
                (State::HandoffFailed(Box::new(failure)), same, Some(error))
            }
        };
        self.state = state;
        if !same {
            return Err(
                "State handoff completion changed original scope; resources retained".into(),
            );
        }
        error.map_or(Ok(()), Err)
    }
    #[cfg(windows)]
    pub(super) fn take_for_launch(
        &mut self,
    ) -> Result<Box<crate::activation::ActivationFilesGuard>> {
        let record =
            match &self.state {
                State::Bound(g) => Box::new(g.preparation().clone()),
                _ => return Err(
                    "Activation receiver needs the one resident Bound guard; no adoption/replay"
                        .into(),
                ),
            };
        let State::Bound(guard) = std::mem::replace(&mut self.state, State::Launching(record))
        else {
            unreachable!()
        };
        Ok(guard)
    }
    #[cfg(windows)]
    pub(super) fn complete_launch(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
    ) -> Result<()> {
        let expected = match &self.state {
            State::Launching(r) => &r.intent,
            _ => return Err("Unexpected activation process completion; no state reset".into()),
        };
        let (state, same, error) = match result {
            Ok(guard) => {
                let same = &guard.record().intent == expected;
                (State::Ready(guard), same, None)
            }
            Err(failure) => {
                let same = &failure.record().intent == expected;
                let error = failure.reason.clone();
                (State::LaunchFailed(Box::new(failure)), same, Some(error))
            }
        };
        self.state = state;
        if !same {
            return Err(
                "Activation process completion changed frozen scope; resources retained".into(),
            );
        }
        error.map_or(Ok(()), Err)
    }
    pub(super) fn process_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_process_snapshot(directory, id, active, error)
            .map_err(super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        Ok(view)
    }
    fn patch_process(
        &self,
        view: &mut UpdatePreparationView,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<()> {
        let process = view
            .activation_process
            .as_mut()
            .ok_or("Activation process view missing")?;
        process.original_state_held = signal.held();
        #[cfg(windows)]
        match &self.state {
            State::Launching(_) => {
                process.phase = ActivationProcessPhase::Launching;
                process.process_custody_held = None;
            }
            State::Ready(guard) => {
                diagnostics(process, guard.diagnostics());
                process.resident_process = guard.resident_process();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.process_alive = guard.process_alive();
                if process.process_alive == Some(true)
                    && signal.held()
                    && view.driver_error.is_none()
                {
                    process.phase = ActivationProcessPhase::ReadyForState;
                } else {
                    process.phase = ActivationProcessPhase::Indeterminate;
                    process.reason=Some("Saved Ready retained, but live process/original custody is unavailable; no state handoff".into());
                }
            }
            State::LaunchFailed(failure) => {
                diagnostics(process, failure.diagnostics());
                process.phase = ActivationProcessPhase::Indeterminate;
                process.resident_process = failure.resident_process();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.process_alive = failure.process_alive();
                process.reason = Some(failure.reason.chars().take(1000).collect());
            }
            State::HandingOff(_) => {
                process.phase = ActivationProcessPhase::Indeterminate;
                process.process_custody_held = None;
                process.reason = Some("State handoff in progress; inspect internal23".into());
            }
            State::Owned(guard) => {
                diagnostics(process, guard.diagnostics());
                process.resident_process = guard.resident_process();
                process.process_alive = guard.process_alive();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.phase = ActivationProcessPhase::Indeterminate;
                process.reason =
                    Some("Physical state handoff progressed; inspect internal23".into());
            }
            State::HandoffFailed(failure) => {
                diagnostics(process, failure.diagnostics());
                process.resident_process = failure.resident_process();
                process.process_alive = failure.process_alive();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.phase = ActivationProcessPhase::Indeterminate;
                process.reason = Some(failure.reason.chars().take(1000).collect());
            }
            State::Writing(_) | State::StartingService(_) => {
                process.phase = ActivationProcessPhase::Indeterminate;
                process.process_custody_held = None;
                process.reason = Some("Journal activation in progress; inspect internal24".into());
            }
            State::Written(g)
            | State::WriterUnconfirmed(g)
            | State::Serviced(g)
            | State::ServiceUnconfirmed(g) => {
                diagnostics(process, g.diagnostics());
                process.resident_process = g.resident_process();
                process.process_alive = g.process_alive();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.phase = ActivationProcessPhase::Indeterminate;
                process.reason = Some(
                    if self.service_started() {
                        "Local service progressed; inspect internal25"
                    } else {
                        "Journal activation progressed; inspect internal24"
                    }
                    .into(),
                );
            }
            State::WriterFailed(f) | State::ServiceFailed(f) => {
                diagnostics(process, f.diagnostics());
                process.resident_process = f.resident_process();
                process.process_alive = f.process_alive();
                process.process_custody_held = Some(process.resident_process.is_some());
                process.phase = ActivationProcessPhase::Indeterminate;
                process.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn state_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_state_snapshot(directory, id, active, error)
            .map_err(super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        self.patch_state(&mut view, signal)?;
        Ok(view)
    }
    fn patch_state(
        &self,
        view: &mut UpdatePreparationView,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<()> {
        let state = view
            .activation_state
            .as_mut()
            .ok_or("State handoff view missing")?;
        state.original_lock_held = Some(signal.held());
        #[cfg(windows)]
        match &self.state {
            State::HandingOff(_) => {
                state.phase = ActivationStatePhase::HandingOff;
                state.original_lock_held = None;
            }
            State::Ready(guard) | State::Owned(guard) => {
                state.resident_process = guard.resident_process();
                state.process_alive = guard.process_alive();
                state.original_lock_held = guard.original_lock_held();
                if matches!(self.state, State::Owned(_)) {
                    if state.process_alive == Some(true) && view.driver_error.is_none() {
                        state.phase = ActivationStatePhase::StateOwned;
                    } else {
                        state.phase = ActivationStatePhase::Indeterminate;
                        state.reason = Some(
                            "Physical StateOwned retained; current child liveness unavailable"
                                .into(),
                        );
                    }
                }
            }
            State::HandoffFailed(failure) => {
                state.phase = ActivationStatePhase::Indeterminate;
                state.original_lock_held = failure.original_lock_held();
                state.resident_process = failure.resident_process();
                state.process_alive = failure.process_alive();
                state.reason = Some(failure.reason.chars().take(1000).collect());
            }
            State::Writing(_) | State::StartingService(_) => {
                state.phase = ActivationStatePhase::Indeterminate;
                state.original_lock_held = None;
                state.reason = Some("Journal activation in progress; inspect internal24".into());
            }
            State::Written(g)
            | State::WriterUnconfirmed(g)
            | State::Serviced(g)
            | State::ServiceUnconfirmed(g) => {
                state.phase = ActivationStatePhase::Indeterminate;
                state.original_lock_held = g.original_lock_held();
                state.resident_process = g.resident_process();
                state.process_alive = g.process_alive();
                state.reason = Some(
                    if self.service_started() {
                        "Local service progressed; inspect internal25"
                    } else {
                        "Journal activation progressed; inspect internal24"
                    }
                    .into(),
                );
            }
            State::WriterFailed(f) | State::ServiceFailed(f) => {
                state.phase = ActivationStatePhase::Indeterminate;
                state.original_lock_held = f.original_lock_held();
                state.resident_process = f.resident_process();
                state.process_alive = f.process_alive();
                state.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn writer_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_writer_snapshot(directory, id, active, error)
            .map_err(super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        self.patch_state(&mut view, signal)?;
        self.patch_writer(&mut view, active)?;
        Ok(view)
    }
    fn patch_writer(&self, view: &mut UpdatePreparationView, _active: bool) -> Result<()> {
        let writer = view
            .activation_writer
            .as_mut()
            .ok_or("Writer view missing")?;
        if writer.command_id.is_none() {
            writer.command_id = self.writer_key.clone();
        }
        #[cfg(windows)]
        match &self.state {
            State::Owned(g) => {
                writer.resident_process = g.resident_process();
                writer.process_alive = g.process_alive();
                if writer.durable_phase.is_some()
                    || writer.process_alive != Some(true)
                    || view.driver_error.is_some()
                {
                    writer.phase = ActivationWriterPhase::Indeterminate;
                    writer.reason = Some(
                        "Original physical owner is not a confirmed writer; no adoption".into(),
                    );
                }
            }
            State::Writing(_) | State::StartingService(_) => {
                writer.phase = if _active && view.driver_error.is_none() {
                    ActivationWriterPhase::Activating
                } else {
                    ActivationWriterPhase::Indeterminate
                };
                writer.reason = Some(
                    "Original writer dispatch staged; durable admission/commit not inferred".into(),
                );
            }
            State::Written(g)
            | State::WriterUnconfirmed(g)
            | State::Serviced(g)
            | State::ServiceUnconfirmed(g) => {
                writer.resident_process = g.resident_process();
                writer.process_alive = g.process_alive();
                if matches!(self.state, State::Written(_) | State::Serviced(_))
                    && writer.durable_phase == Some(ActivationWriterDurablePhase::Confirmed)
                    && writer.process_alive == Some(true)
                    && view.driver_error.is_none()
                {
                    writer.phase = ActivationWriterPhase::Confirmed;
                    writer.reason=Some("Journal commit confirmed; service progress belongs to internal25 and does not imply controller/lifetime readiness".into());
                } else {
                    writer.phase = ActivationWriterPhase::Indeterminate;
                    writer.reason=view.driver_error.clone().or_else(||Some("Saved commit is history; current writer residency/liveness unconfirmed".into()));
                }
            }
            State::WriterFailed(f) | State::ServiceFailed(f) => {
                writer.phase = ActivationWriterPhase::Indeterminate;
                writer.resident_process = f.resident_process();
                writer.process_alive = f.process_alive();
                writer.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        if writer.phase == ActivationWriterPhase::Confirmed {
            if let Some(state) = &mut view.activation_state {
                state.phase = ActivationStatePhase::StateOwned;
                state.reason = Some(
                    "Same physical state retained by the confirmed resident journal writer".into(),
                );
            }
        }
        Ok(())
    }
    fn patch_files(
        &self,
        view: &mut UpdatePreparationView,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<()> {
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
            State::Launching(record) => held_source(files, record),
            #[cfg(windows)]
            State::Ready(guard) => held_source(files, guard.record()),
            #[cfg(windows)]
            State::LaunchFailed(failure) => held_source(files, failure.record()),
            #[cfg(windows)]
            State::HandingOff(record) => held_source(files, record),
            #[cfg(windows)]
            State::Owned(guard) => held_source(files, guard.record()),
            #[cfg(windows)]
            State::HandoffFailed(failure) => held_source(files, failure.record()),
            #[cfg(windows)]
            State::Writing(record) | State::StartingService(record) => held_source(files, record),
            #[cfg(windows)]
            State::Written(g)
            | State::WriterUnconfirmed(g)
            | State::Serviced(g)
            | State::ServiceUnconfirmed(g) => held_source(files, g.record()),
            #[cfg(windows)]
            State::WriterFailed(f) | State::ServiceFailed(f) => held_source(files, f.record()),
            #[cfg(windows)]
            State::Failed(failure) => {
                files.phase = ActivationFilesPhase::Indeterminate;
                files.held_files = failure.held_files();
                files.reason = Some(failure.reason.chars().take(1000).collect());
            }
        }
        Ok(())
    }
}

#[cfg(any(windows, test))]
fn validate_saved_writer(
    saved: &crate::activation::ActivationWriterRecord,
    record: &crate::activation::ActivationRecord,
    key: &str,
) -> Result<()> {
    saved.validate()?;
    if saved.command.command_id != key
        || !matches!(
            saved.state,
            crate::activation::ActivationWriterState::Confirmed { .. }
        )
        || saved.command.binding.physical
            != *record
                .state_owned
                .as_ref()
                .ok_or("StateOwned receipt missing")?
    {
        return Err("Writer completion lacks exact confirmed command/physical receipt".into());
    }
    crate::activation::writer::validate_command(
        &saved.command,
        record,
        &saved.command.binding.expected_authority,
    )
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    #[test]
    fn completion_checks_resident_key_physical_scope_and_confirmed_fact() {
        let (physical, saved) = crate::activation::writer_view_fixture();
        validate_saved_writer(&saved, &physical, &saved.command.command_id).unwrap();
        assert!(
            validate_saved_writer(&saved, &physical, &uuid::Uuid::new_v4().to_string()).is_err()
        );
        let issued =
            crate::activation::ActivationWriterRecord::issued(saved.command.clone()).unwrap();
        assert!(validate_saved_writer(&issued, &physical, &saved.command.command_id).is_err());
        let mut foreign = physical.clone();
        foreign
            .state_owned
            .as_mut()
            .unwrap()
            .process
            .birth
            .push('0');
        assert!(validate_saved_writer(&saved, &foreign, &saved.command.command_id).is_err());
        let mut bad = saved.clone();
        if let crate::activation::ActivationWriterState::Confirmed { receipt } = &mut bad.state {
            receipt.authority.epoch += 1;
        }
        assert!(validate_saved_writer(&bad, &physical, &saved.command.command_id).is_err());
    }
}

#[cfg(windows)]
fn held_source(files: &mut ActivationFilesView, record: &crate::activation::ActivationRecord) {
    files.phase = ActivationFilesPhase::Bound;
    files.held_files = 5;
    files.worker = Some(record.intent.source.worker.clone());
    files.manifest_sha256 = Some(record.intent.source.manifest.sha256.clone());
}
#[cfg(windows)]
fn diagnostics(
    view: &mut ActivationProcessView,
    (stdout, stderr, incomplete, complete): (String, String, bool, bool),
) {
    view.stdout_tail = Some(stdout);
    view.stderr_tail = Some(stderr);
    view.output_incomplete = Some(incomplete);
    view.capture_complete = Some(complete);
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn launch_actor_never_recreates_bound_custody_from_saved_or_pending_data() {
        let (_, record) = crate::activation::activation_fixture();
        let mut actor = FilesActor::default();
        assert!(!actor.can_launch() && !actor.process_started());
        assert!(actor.take_for_launch().is_err());
        actor.state = State::Launching(Box::new(record));
        assert!(actor.process_started() && !actor.can_launch());
        assert!(actor.take_for_launch().is_err());
    }
    #[test]
    fn state_actor_never_reconstructs_a_ready_owner_or_retries_a_pending_handoff() {
        let (r, initial) = crate::activation::activation_fixture();
        let mut actor = FilesActor::default();
        assert!(!actor.can_handoff() && !actor.handoff_started());
        assert!(actor.take_for_handoff().is_err());
        actor.state = State::HandingOff(Box::new(
            crate::activation::activation_stages(&initial, &r)[4].clone(),
        ));
        assert!(actor.handoff_started() && !actor.can_handoff());
        assert!(actor.take_for_handoff().is_err());
        assert!(!actor.can_launch() && actor.process_started());
    }
}
