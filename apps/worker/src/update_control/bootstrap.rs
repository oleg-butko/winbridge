//! Resident original bootstrap driver. Public entry is trusted Rust only: a
//! verified managed-build result, never a command-supplied path or argv.
use super::*;

/// The slot survives cancelled futures and retained errors, until the original
/// attempt is explicitly retired. It must never be minted from a saved launch.
pub(super) struct RegistrationLease {
    guard: LaunchGuard,
}
impl RegistrationLease {
    pub(super) fn capture(worker: &Worker) -> Result<Self> {
        let reg = worker
            .update_control
            .try_lock()
            .map_err(|_| "Bootstrap registration busy/poisoned")?
            .clone()
            .ok_or("Bootstrap update control not configured")?;
        if !reg.ready.load(Ordering::Acquire) {
            return Err("Bootstrap update-control listener not ready".into());
        }
        reg.launching
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Bootstrap launch slot busy; observe original attempt")?;
        let value = Self {
            guard: LaunchGuard(reg),
        };
        value.require_current(worker)?;
        Ok(value)
    }
    pub(super) fn require_current(&self, worker: &Worker) -> Result<()> {
        let reg = &self.guard.0;
        let current = worker
            .update_control
            .try_lock()
            .map_err(|_| "Bootstrap registration busy/poisoned")?;
        if !current.as_ref().is_some_and(|v| Arc::ptr_eq(v, reg))
            || !reg.ready.load(Ordering::Acquire)
            || !reg.launching.load(Ordering::Acquire)
            || worker.inspection
            || worker.stop.load(Ordering::Acquire)
        {
            return Err("Bootstrap original registration/readiness lost".into());
        }
        if worker.config.resource_scheduling.is_some() {
            return Err("Named-resource update reservations are not implemented".into());
        }
        let settings = worker.try_runtime_settings()?;
        if settings.restart_required
            || settings.saved_sha256 != reg.binding.config_sha256
            || hash_config(&reg.config_path)? != reg.config_sha256
        {
            return Err("ConfigRestartPending: bootstrap configuration changed".into());
        }
        Ok(())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::ActiveWorkerBootstrap;
    use winbridge_update::{
        bootstrap::*,
        journal::{BootstrapReservation, UpdateAuthority},
    };

    struct Context {
        worker: Arc<Worker>,
        active: ActiveWorkerBootstrap,
        registration: RegistrationLease,
    }
    impl Context {
        fn require_current(&self) -> Result<()> {
            let started = std::time::Instant::now();
            let active = self.active.require_current();
            let active_ms = started.elapsed().as_millis();
            let result = active.and_then(|()| {
                let reg = &self.registration.guard.0;
                self.active.require_worker_binding(
                    &reg.binding,
                    &reg.config_path,
                    &reg.directory,
                    &reg.scope,
                )?;
                self.registration.require_current(&self.worker)
            });
            // On-demand phase diagnostics, never an idle timer. Existing
            // updater bootstrap traces also account for process-wide hash I/O.
            eprintln!(
                "WinBridge worker bootstrap {}",
                serde_json::json!({
                    "event":"worker_bootstrap_checkpoint", "generation":self.worker.generation,
                    "outcome":if result.is_ok() {"ok"} else {"failed"},
                    "activeMs":active_ms,"bindingAndConfigMs":started.elapsed().as_millis().saturating_sub(active_ms),
                    "elapsedMs":started.elapsed().as_millis()
                })
            );
            result
        }
    }
    enum Stage {
        Unavailable,
        Files {
            files: Box<PreparedBootstrapFiles>,
            probe_attempted: bool,
        },
        Attempt(Box<BootstrapLaunchAttempt>),
        Closed(Box<BootstrapLaunchAttempt>),
        Session(Box<BootstrapSession>),
        AdmissionFailed(Box<BootstrapPreparationFailure>),
        LaunchFailed(Box<BootstrapLaunchFailure>),
        Historical(Box<BootstrapReservation>),
        Finished,
    }
    /// Keep the original object after cancellation/error. Methods borrow it,
    /// so dropping an I/O future never releases its owner, slot or file pins.
    #[must_use = "Retain the original bootstrap after an unknown outcome; never recreate it from history"]
    pub struct WorkerBootstrap {
        journal: Option<UpdateJournal>,
        owner: Option<UpdateAuthority>,
        request: UpdateRequest,
        update: Option<UpdateRecord>,
        stage: Stage,
        reason: Option<String>,
        // Release the launch slot only after the journal/owner/attempt fields.
        context: Context,
    }
    pub enum WorkerBootstrapPreparation {
        Prepared(Box<WorkerBootstrap>),
        Existing {
            update: Box<UpdateRecord>,
            supervisor: Option<Box<SupervisorRecord>>,
        },
    }
    #[must_use = "An error may retain the original owner and pins; inspect/keep it instead of replaying preparation"]
    pub struct WorkerBootstrapFailure {
        reason: String,
        original: Option<Box<WorkerBootstrap>>,
    }
    impl std::fmt::Debug for WorkerBootstrapFailure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WorkerBootstrapFailure")
                .field("reason", &self.reason)
                .field("retains_original", &self.original.is_some())
                .finish_non_exhaustive()
        }
    }
    impl std::fmt::Display for WorkerBootstrapFailure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.reason.fmt(f)
        }
    }
    impl std::error::Error for WorkerBootstrapFailure {}
    impl WorkerBootstrapFailure {
        pub fn reason(&self) -> &str {
            &self.reason
        }
        pub fn into_original(self) -> Option<Box<WorkerBootstrap>> {
            self.original
        }
        fn early(reason: String) -> Box<Self> {
            Box::new(Self {
                reason,
                original: None,
            })
        }
    }
    fn existing(
        directory: &Path,
        request: &UpdateRequest,
    ) -> Result<Option<WorkerBootstrapPreparation>> {
        let Some(update) =
            UpdateJournal::find_request(directory, request).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let supervisor = match UpdateJournal::inspect_supervisor(directory, &update.update_id) {
            Ok(r) => Some(Box::new(r)),
            Err(e) if e.code == JournalErrorCode::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        Ok(Some(WorkerBootstrapPreparation::Existing {
            update: Box::new(update),
            supervisor,
        }))
    }
    impl Worker {
        /// Fixed installed config, intrinsic Active scope and a clean queried
        /// updater from a trusted managed build. The legacy launcher is intact.
        pub fn prepare_next_update_supervisor(
            self: &Arc<Self>,
            request: &UpdateRequest,
            fresh: FreshUpdater,
        ) -> std::result::Result<WorkerBootstrapPreparation, Box<WorkerBootstrapFailure>> {
            let reg = self
                .control_registration()
                .map_err(WorkerBootstrapFailure::early)?;
            if let Some(data) =
                existing(&reg.directory, request).map_err(WorkerBootstrapFailure::early)?
            {
                return Ok(data);
            }
            let active = self
                .active_worker_bootstrap()
                .map_err(WorkerBootstrapFailure::early)?;
            let registration =
                RegistrationLease::capture(self).map_err(WorkerBootstrapFailure::early)?;
            if !Arc::ptr_eq(&reg, &registration.guard.0) {
                return Err(WorkerBootstrapFailure::early(
                    "Bootstrap registration changed during preparation".into(),
                ));
            }
            let mut original = Box::new(WorkerBootstrap {
                context: Context {
                    worker: Arc::clone(self),
                    active,
                    registration,
                },
                journal: None,
                owner: None,
                request: request.clone(),
                update: None,
                stage: Stage::Unavailable,
                reason: None,
            });
            let result = (|| -> Result<Option<WorkerBootstrapPreparation>> {
                original.context.require_current()?;
                original.journal = Some(
                    UpdateJournal::open(&reg.directory, &reg.scope).map_err(|e| e.to_string())?,
                );
                original.owner = Some(
                    original
                        .journal
                        .as_mut()
                        .expect("Opened journal")
                        .acquire_owner()
                        .map_err(|e| e.to_string())?,
                );
                // Duplicate-first lookup before the lock is advisory. Under
                // the actual owner, recheck before accepting a fresh root.
                if let Some(data) = existing(&reg.directory, request)? {
                    return Ok(Some(data));
                }
                original.context.require_current()?;
                let journal = original.journal.as_mut().expect("Opened journal");
                let owner = original.owner.as_ref().expect("Original owner");
                let info = journal.info().map_err(|e| e.to_string())?;
                let observed =
                    UpdateInstallationPin {
                        installation_id: info.installation_id,
                        scope_sha256: reg.scope.clone(),
                        worker_build_id: reg.binding.build.build_id.clone(),
                        worker_config_sha256: reg.binding.config_sha256.clone(),
                        baseline_manifest_sha256:
                            winbridge_workspace_sync::source_tree::read_manifest(&reg.source)?.1,
                    };
                original.update = Some(
                    journal
                        .accept(owner, request, &observed)
                        .map_err(|e| e.to_string())?,
                );
                let domain = self
                    .execution_domain
                    .try_lock()
                    .map_err(|_| "Bootstrap domain busy/poisoned")?;
                let files = PreparedBootstrapFiles::prepare(
                    journal,
                    owner,
                    &original.update.as_ref().expect("Accepted root").update_id,
                    &reg.config_path,
                    reg.binding.clone(),
                    domain.as_ref().ok_or("Bootstrap actual domain missing")?,
                    fresh,
                )?;
                drop(domain); // Own-state callbacks must be able to try-lock it.
                original.stage = Stage::Files {
                    files: Box::new(files),
                    probe_attempted: false,
                };
                original.context.require_current()?;
                Ok(None)
            })();
            match result {
                Ok(Some(data)) => Ok(data),
                Ok(None) => Ok(WorkerBootstrapPreparation::Prepared(original)),
                Err(reason) => {
                    original.reason = Some(reason.clone());
                    Err(Box::new(WorkerBootstrapFailure {
                        reason,
                        original: Some(original),
                    }))
                }
            }
        }
    }
    impl WorkerBootstrap {
        pub fn phase(&self) -> &'static str {
            match &self.stage {
                Stage::Unavailable => "preparation_unconfirmed",
                Stage::Files {
                    probe_attempted: true,
                    ..
                } => "probe_unconfirmed",
                Stage::Files { .. } => "files_prepared",
                Stage::Attempt(_) => "original_attempt",
                Stage::Closed(_) => "writer_closed_unconfirmed",
                Stage::Session(_) => "original_session",
                Stage::AdmissionFailed(_retained) => "admission_unconfirmed",
                Stage::LaunchFailed(_retained) => "launch_preflight_failed",
                Stage::Historical(_retained) => "historical_only",
                Stage::Finished => "finished",
            }
        }
        pub fn reason(&self) -> Option<&str> {
            self.reason.as_deref()
        }
        /// Original accepted root snapshot; observe_root reads current history.
        pub fn accepted_update(&self) -> Option<&UpdateRecord> {
            self.update.as_ref()
        }
        /// Resolve an unknown accept COMMIT as data, never adopt a launch grant.
        pub fn observe_root(&self) -> Result<Option<UpdateRecord>> {
            UpdateJournal::find_request(&self.context.registration.guard.0.directory, &self.request)
                .map_err(|e| e.to_string())
        }
        fn retain<T>(&mut self, result: Result<T>) -> Result<T> {
            self.reason = result
                .as_ref()
                .err()
                .map(|e| e.chars().take(1000).collect());
            result
        }
        pub async fn admit(&mut self) -> Result<()> {
            self.context.require_current()?;
            match &mut self.stage {
                Stage::Files { probe_attempted, .. } if !*probe_attempted => *probe_attempted = true,
                _ => return Err("Original bootstrap admission/probe already attempted or unavailable; no fresh capture/replay".into()),
            }
            self.reason = Some(
                "Original keeper probe unconfirmed; retain files/owner, do not recreate admission"
                    .into(),
            );
            let capture = match self.context.active.observe_keeper().await {
                Ok(capture) => capture,
                Err(reason) => return self.retain(Err(reason)),
            };
            let Stage::Files { files, .. } = std::mem::replace(&mut self.stage, Stage::Unavailable)
            else {
                unreachable!("Checked Files stage");
            };
            let context = &self.context;
            let journal = self.journal.as_mut().expect("Original journal");
            let owner = self.owner.as_ref().expect("Original owner");
            let result = match (*files).admit(journal, owner, capture, || context.require_current())
            {
                Ok(BootstrapPreparation::Prepared(prepared)) => {
                    match prepared.begin_launch(journal, owner, || context.require_current()) {
                        Ok(attempt) => {
                            self.stage = Stage::Attempt(attempt);
                            Ok(())
                        }
                        Err(failure) => {
                            let reason = failure.reason.clone();
                            self.stage = Stage::LaunchFailed(failure);
                            Err(reason)
                        }
                    }
                }
                Ok(BootstrapPreparation::Existing(data)) => {
                    self.stage = Stage::Historical(data);
                    Err("Existing reservation is historical data, not an original attempt".into())
                }
                Err(failure) => {
                    let reason = failure.reason.clone();
                    self.stage = Stage::AdmissionFailed(failure);
                    Err(reason)
                }
            };
            self.retain(result)
        }
        pub async fn send_start(&mut self) -> Result<KeeperLaunchStatus> {
            let Stage::Attempt(attempt) = &mut self.stage else {
                return Err("No original bootstrap attempt".into());
            };
            self.reason = Some("Keeper Start unconfirmed; observe the same attempt".into());
            let context = &self.context;
            let result = attempt
                .send_start(
                    self.journal.as_mut().expect("Original journal"),
                    self.owner.as_ref().expect("Original owner"),
                    || context.require_current(),
                )
                .await;
            self.retain(result)
        }
        pub async fn observe_launch(&mut self) -> Result<KeeperLaunchStatus> {
            let result = match &mut self.stage {
                Stage::Attempt(attempt) | Stage::Closed(attempt) => attempt.observe().await,
                Stage::Session(session) => session.observe_keeper().await,
                _ => Err("No original attempted keeper launch to observe".into()),
            };
            self.retain(result)
        }
        pub fn publish_spawned(&mut self) -> Result<SupervisorRecord> {
            let Stage::Attempt(attempt) = &mut self.stage else {
                return Err("No original bootstrap attempt".into());
            };
            let context = &self.context;
            let result = attempt.publish_spawned(
                self.journal.as_mut().expect("Original journal"),
                self.owner.as_ref().expect("Original owner"),
                || context.require_current(),
            );
            self.retain(result)
        }
        pub fn release_owner(&mut self) -> Result<()> {
            if !matches!(self.stage, Stage::Attempt(_)) {
                return Err("No original bootstrap attempt/owner to release".into());
            }
            let Stage::Attempt(attempt) = std::mem::replace(&mut self.stage, Stage::Unavailable)
            else {
                unreachable!();
            };
            let journal = self.journal.take().expect("Original journal");
            let owner = self.owner.take().expect("Original owner");
            let context = &self.context;
            match attempt.release_owner(journal, owner, || context.require_current()) {
                Ok(session) => {
                    self.stage = Stage::Session(session);
                    self.reason = None;
                    Ok(())
                }
                Err(failure) => {
                    let (attempt, journal, owner, reason) = failure.into_parts();
                    self.stage = if journal.is_some() {
                        Stage::Attempt(attempt)
                    } else {
                        Stage::Closed(attempt)
                    };
                    self.journal = journal;
                    self.owner = Some(owner);
                    self.retain(Err(reason))
                }
            }
        }
        pub async fn wait_ready_and_confirm(&mut self) -> Result<SupervisorRecord> {
            let Stage::Session(session) = &mut self.stage else {
                return Err("Original owner has not been released".into());
            };
            self.reason =
                Some("Original Ready/confirmation unconfirmed; observe the same session".into());
            let result = session.wait_ready_and_confirm().await;
            self.retain(result)
        }
        pub async fn handoff(&mut self) -> Result<SupervisorRecord> {
            let Stage::Session(session) = &mut self.stage else {
                return Err("No original bootstrap session".into());
            };
            let context = &self.context;
            self.reason = Some("Original Handoff unconfirmed; observe the same session".into());
            let result = session.handoff(|| context.require_current()).await;
            self.retain(result)
        }
        pub async fn observe_handoff(&mut self) -> Result<SupervisorRecord> {
            let Stage::Session(session) = &mut self.stage else {
                return Err("No original bootstrap session".into());
            };
            let result = session.observe_handoff().await;
            self.retain(result)
        }
        pub fn finish(self: Box<Self>) -> std::result::Result<SupervisorHandle, Box<Self>> {
            let mut this = self;
            if !matches!(this.stage, Stage::Session(_)) {
                return Err(this);
            }
            let Stage::Session(session) = std::mem::replace(&mut this.stage, Stage::Finished)
            else {
                unreachable!();
            };
            match session.finish() {
                Ok(handle) => Ok(handle),
                Err(session) => {
                    this.reason = session.reason().map(str::to_owned);
                    this.stage = Stage::Session(session);
                    Err(this)
                }
            }
        }
    }
}
#[cfg(windows)]
pub use native::{WorkerBootstrap, WorkerBootstrapFailure, WorkerBootstrapPreparation};
