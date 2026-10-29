//! Resident quarantine is a typed transition, never a disabled deadline or
//! ownership reconstructed from a journal/IPC query. Dispatch remains frozen.
use super::*;
use winbridge_protocol::update::*;
use winbridge_update::activation::lifetime;
#[path = "resident_observation.rs"]
mod resident_observation;
pub(crate) use resident_observation::rejected_resident;
#[path = "finalization_observation.rs"]
mod finalization_observation;
pub(crate) use finalization_observation::rejected_finalization_observation;

pub(crate) enum ServiceLifetime {
    Verification,
    #[cfg_attr(not(windows), allow(dead_code))]
    Supervised(Arc<ResidentLifetime>),
    #[cfg_attr(not(windows), allow(dead_code))]
    ReleasePending(Arc<PendingLifetime>),
    #[cfg_attr(not(windows), allow(dead_code))]
    Active(Arc<ActiveLifetime>),
}
pub(crate) struct ActiveLifetime {
    resident: Arc<ResidentLifetime>,
    receipt: WorkerFinalizationReceipt,
}
pub(crate) struct PendingLifetime {
    resident: Arc<ResidentLifetime>,
    command: WorkerFinalizationCommand,
}
#[path = "finalization_service.rs"]
mod finalization_service;
pub(crate) struct ResidentLifetime {
    command: WorkerLifetimeActivationCommand,
    receipt: WorkerLifetimeActivationReceipt,
    keeper: LifetimeKeeper,
}
enum LifetimeKeeper {
    #[cfg_attr(not(windows), allow(dead_code))]
    Native(winbridge_platform::infrastructure::PinnedProcess),
    #[cfg(test)]
    Fixture(Arc<AtomicBool>),
}
impl LifetimeKeeper {
    fn require_live(&self) -> Result<()> {
        match self {
            Self::Native(pin) => pin.require_live().map_err(|e| e.to_string()),
            #[cfg(test)]
            Self::Fixture(live) => {
                if live.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err("Fixture keeper lost".into())
                }
            }
        }
    }
}

impl Worker {
    /// Origin is immutable; permissions follow the sealed runtime mode. This
    /// short lock is released before callers acquire Store (Info already owns
    /// Store). Poisoning must never accidentally open command admission.
    pub(crate) fn activation_read_only(&self) -> bool {
        self.local_activation.as_ref().is_some_and(|scope| {
            scope.lifetime.lock().map(|mode| !matches!(&*mode, ServiceLifetime::Active(v) if v.receipt.compatibility.generation == self.generation)).unwrap_or(true)
        })
    }
    pub(crate) fn check_local_service_scope(&self) -> Result<()> {
        self.check_local_service_scope_at(Instant::now())
    }

    fn check_local_service_scope_at(&self, now: Instant) -> Result<()> {
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("Resident service binding missing")?;
        // Serialize the mode snapshot with the lease check. Otherwise a
        // watchdog that copied Supervised before a release could wait for the
        // Store, then incorrectly reject the now legitimately Released lease.
        let state = self.store()?;
        let (resident, pending, active) = {
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::Verification => (None, None, false),
                ServiceLifetime::Supervised(resident) => (Some(Arc::clone(resident)), None, false),
                ServiceLifetime::ReleasePending(pending) => (
                    Some(Arc::clone(&pending.resident)),
                    Some(Arc::clone(pending)),
                    false,
                ),
                ServiceLifetime::Active(value) => (Some(Arc::clone(&value.resident)), None, true),
            }
        };
        if let Some(resident) = resident {
            resident.keeper.require_live()?;
            if active {
                // The same Store/instance/keeper remain authoritative. A later
                // update or saved config must not revalidate the old release.
                state.check_authority()?;
            } else if let Some(pending) = pending {
                // The SQL/mode gap accepts ONLY the original Quiesced lease
                // or this exact release receipt. A query does not seal Active.
                state.observe_service_finalization_release(
                    &scope.command,
                    &scope.shutdown,
                    &pending.command,
                )?;
            } else {
                state.check_supervised_service_identity(&scope.command, &scope.shutdown)?;
            }
            resident.keeper.require_live()
        } else {
            if now >= scope.deadline {
                return Err("Original verification window expired".into());
            }
            state.check_local_service_identity(&scope.command, &scope.shutdown, scope.deadline)
        }
    }

    fn observe_lifetime(
        &self,
        query: ActivationLifetimeQuery,
    ) -> Result<ActivationLifetimeObservationResult> {
        if query.lifetime_observation_version != 1 || self.stop.load(Ordering::Acquire) {
            return Err("Unsupported lifetime observation or service stopping".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No resident local service")?;
        let resident = {
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::Verification => {
                    return Err(
                        "No resident supervised lifetime decision; a query cannot enter it".into(),
                    )
                }
                ServiceLifetime::Supervised(resident) => Arc::clone(resident),
                ServiceLifetime::Active(_) => {
                    return Err("Worker is Active; V6 cannot re-enter quarantine".into())
                }
                ServiceLifetime::ReleasePending(_) => {
                    return Err(
                        "Finalization pending; old lifetime query cannot confirm Active".into(),
                    );
                }
            }
        };
        if query.command != resident.command {
            return Err("Lifetime query changed resident command".into());
        }
        lifetime::validate_receipt(&resident.receipt, &resident.command)?;
        self.check_local_service_scope()?;
        self._instance_lock.revalidate()?;
        let guard = self
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .clone()
            .ok_or("No actual loaded runtime guard; fixture data is not lifetime evidence")?;
        guard.revalidate()?;
        if guard.identity() != &resident.command.binding.compatibility.identity {
            return Err("Lifetime loaded identity changed".into());
        }
        let path = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?
            .clone()
            .ok_or("Lifetime config path missing")?;
        let config = compatibility::check_config(
            &path,
            &resident.command.binding.compatibility.worker_config_sha256,
        )?;
        if config.instance_id != self.config.instance_id
            || config.worker_config_schema != self.config.schema_version
        {
            return Err("Lifetime config binding changed".into());
        }
        let directory = Path::new(&guard.identity().image.path)
            .parent()
            .ok_or("Runtime parent missing")?
            .join("update-state");
        winbridge_update::journal::UpdateJournal::inspect_activation_lifetime_continuation(
            &directory,
            &resident.command.binding.update_id,
            &resident.command.binding.attempt_id,
            &guard.identity().process,
            &resident.command,
            &resident.receipt,
        )
        .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        winbridge_platform::infrastructure::PinnedProcess::live(&guard.identity().process)
            .and_then(|p| p.require_job_membership(&resident.command.binding.job_name))
            .map_err(|e| e.to_string())?;
        self.check_local_service_scope()?;
        self._instance_lock.revalidate()?;
        let observed_at_ms = store::now_ms();
        if observed_at_ms < resident.receipt.compatibility.observed_at_ms
            || observed_at_ms > 9_007_199_254_740_991
        {
            return Err("Lifetime observation clock moved backwards/outside contract".into());
        }
        Ok(ActivationLifetimeObservationResult::Observed {
            receipt: Box::new(resident.receipt.clone()),
            observed_at_ms,
        })
    }
    pub(crate) fn handle_lifetime_observation(
        &self,
        query: ActivationLifetimeQuery,
    ) -> ActivationLifetimeObservation {
        match self.observe_lifetime(query) {
            Ok(result) => ActivationLifetimeObservation {
                lifetime_observation_version: 1,
                result,
            },
            Err(message) => rejected_lifetime(message),
        }
    }
}

pub(crate) fn rejected_lifetime(message: String) -> ActivationLifetimeObservation {
    ActivationLifetimeObservation {
        lifetime_observation_version: 1,
        result: ActivationLifetimeObservationResult::Rejected { message },
    }
}

impl RunningLocalService {
    #[cfg(windows)]
    pub(crate) fn enter_supervised_lifetime(
        &mut self,
        command: WorkerLifetimeActivationCommand,
        parent: &winbridge_platform::infrastructure::PinnedProcess,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<WorkerLifetimeActivationReceipt> {
        if parent.identity() != &command.binding.keeper {
            return Err("Lifetime keeper differs from authenticated original parent".into());
        }
        parent.require_live().map_err(|e| e.to_string())?;
        self.service.check_ready()?;
        let scope = self
            .service
            .worker
            .local_activation
            .as_ref()
            .ok_or("Local scope missing")?;
        let controller = scope
            .controller
            .lock()
            .map_err(|_| "Controller binding lock poisoned")?
            .clone()
            .ok_or("Controller has not been started on this resident service")?;
        if !matches!(self.controller, super::ControllerStartup::Running(_))
            || self.exit.is_some()
            || self.task.as_ref().is_none_or(|task| task.is_finished())
            || controller.command_id != command.binding.controller_command_id
            || controller.binding.service_command != scope.command
        {
            return Err("Lifetime changed original running controller/local service".into());
        }
        lifetime::validate_command_envelope(&command)?;
        let actual = self
            .service
            .worker
            .observe_service(ActivationServiceQuery {
                service_observation_version: 1,
                command: scope.command.clone(),
            })?;
        let mut expected = command.binding.compatibility.clone();
        expected.observed_at_ms = actual.compatibility.observed_at_ms;
        if actual.compatibility != expected || actual.authority != command.binding.authority {
            return Err("Lifetime changed resident loaded state/authority/config".into());
        }
        let child = winbridge_platform::infrastructure::PinnedProcess::live(
            &actual.compatibility.identity.process,
        )
        .map_err(|e| e.to_string())?;
        child
            .require_job_membership(&command.binding.job_name)
            .map_err(|e| e.to_string())?;
        let keeper = winbridge_platform::infrastructure::PinnedProcess::live(parent.identity())
            .map_err(|e| e.to_string())?;
        checkpoint()?;
        self.service.check_ready()?;
        child
            .require_job_membership(&command.binding.job_name)
            .map_err(|e| e.to_string())?;
        keeper.require_live().map_err(|e| e.to_string())?;
        let receipt = WorkerLifetimeActivationReceipt {
            schema_version: 1,
            command_id: command.command_id.clone(),
            update_id: command.binding.update_id.clone(),
            attempt_id: command.binding.attempt_id.clone(),
            command_sha256: lifetime::command_sha256(&command)?,
            keeper: command.binding.keeper.clone(),
            job_name: command.binding.job_name.clone(),
            compatibility: actual.compatibility,
            authority: actual.authority,
            mode: command.mode,
        };
        self.seal_supervised_lifetime(command, receipt, LifetimeKeeper::Native(keeper))
    }

    #[cfg(any(windows, test))]
    fn seal_supervised_lifetime(
        &mut self,
        command: WorkerLifetimeActivationCommand,
        receipt: WorkerLifetimeActivationReceipt,
        keeper: LifetimeKeeper,
    ) -> Result<WorkerLifetimeActivationReceipt> {
        self.seal_supervised_lifetime_with_clock(command, receipt, keeper, Instant::now)
    }

    #[cfg(any(windows, test))]
    fn seal_supervised_lifetime_with_clock(
        &mut self,
        command: WorkerLifetimeActivationCommand,
        mut receipt: WorkerLifetimeActivationReceipt,
        keeper: LifetimeKeeper,
        now: impl FnOnce() -> Instant,
    ) -> Result<WorkerLifetimeActivationReceipt> {
        let scope = self
            .service
            .worker
            .local_activation
            .as_ref()
            .ok_or("Local scope missing")?;
        let writer = &scope.command.binding.writer_command;
        let compatibility = &command.binding.compatibility;
        if compatibility.context != writer.binding.physical.context
            || compatibility.generation != self.service.worker.generation
            || compatibility.identity != scope.command.binding.expected_worker
            || compatibility.worker_config_sha256 != writer.binding.worker_config_sha256
            || compatibility.worker_config_schema != self.service.worker.config.schema_version
            || command.binding.authority != scope.command.binding.writer_receipt.authority
            || command.deadline_at_ms != scope.command.deadline_at_ms
            || command.issued_at_ms < scope.command.issued_at_ms
        {
            return Err("Lifetime entry differs from resident service binding".into());
        }
        self.service.worker._instance_lock.revalidate()?;
        // Keep the same Store mutex through the in-memory decision. No local
        // authority/lease operation may slip between this check and sealing.
        let state = self.service.worker.store()?;
        state.check_supervised_service_identity(&scope.command, &scope.shutdown)?;
        let mut mode = scope
            .lifetime
            .lock()
            .map_err(|_| "Lifetime mode lock poisoned")?;
        if !matches!(*mode, ServiceLifetime::Verification) {
            return Err("Lifetime already entered; no replay".into());
        }
        keeper.require_live()?;
        receipt.compatibility.observed_at_ms = store::now_ms();
        lifetime::validate_receipt(&receipt, &command)?;
        if now() >= scope.deadline {
            return Err("Original monotonic lifetime entry window expired".into());
        }
        *mode = ServiceLifetime::Supervised(Arc::new(ResidentLifetime {
            command,
            receipt: receipt.clone(),
            keeper,
        }));
        Ok(receipt)
    }

    #[cfg(any(windows, test))]
    pub(crate) async fn hold_supervised_lifetime(&mut self) -> Result<()> {
        // The keeper's kernel Job still controls process death. This watchdog
        // adds a cheap parent/authority check; it never rehashes the image at idle.
        let worker = Arc::clone(&self.service.worker);
        if !matches!(
            *worker
                .local_activation
                .as_ref()
                .ok_or("Local scope missing")?
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?,
            ServiceLifetime::Supervised(_)
                | ServiceLifetime::ReleasePending(_)
                | ServiceLifetime::Active(_)
        ) {
            return Err("Supervised hold requires an actual resident entry".into());
        }
        loop {
            tokio::select! {
                result = self.wait_until_stopped() => return Err(format!("Supervised local IPC stopped: {result:?}")),
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    worker.check_local_service_scope()?;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "lifetime_service/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifetime_service/active_tests.rs"]
mod active_tests;

#[cfg(test)]
#[path = "lifetime_service/finalization_observation_tests.rs"]
mod finalization_observation_tests;
