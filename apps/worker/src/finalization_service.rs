//! Internal operations consumed by the original normal-IPC V8 coordinator.
//! Keep SQL release separate from Active and retain every owner on failure.
use super::*;
#[cfg(any(windows, test))]
#[path = "active_service.rs"]
pub(super) mod active;

#[cfg(any(windows, test))]
pub(super) fn require_quiet_registries(worker: &Worker) -> Result<()> {
    // These callbacks also run while the Store is held. Other paths acquire
    // registries before Store: never wait on them in the reverse order.
    if worker.active_jobs.load(Ordering::Acquire) != 0
        || worker.maintenance_running.load(Ordering::Acquire)
        || !worker
            .active_projects
            .try_lock()
            .map_err(|_| "Process registry busy/poisoned")?
            .is_empty()
        || !worker
            .checks
            .try_lock()
            .map_err(|_| "Readiness registry busy/poisoned")?
            .is_empty()
        || !worker
            .unreconciled_tasks
            .try_lock()
            .map_err(|_| "Recovery registry busy/poisoned")?
            .is_empty()
        || worker
            .maintenance
            .try_lock()
            .map_err(|_| "Maintenance registry busy/poisoned")?
            .1
            .is_some()
    {
        return Err("Finalization refuses live, uncertain or reserved worker effects".into());
    }
    Ok(())
}

impl RunningLocalService {
    /// Testable in-memory boundary. Only the native owning coordinator below
    /// calls this after its OS peer/current committed-graph checks.
    #[cfg(any(windows, test))]
    pub(super) fn seal_finalization_pending(
        &mut self,
        command: WorkerFinalizationCommand,
        expected: &Arc<ResidentLifetime>,
    ) -> Result<()> {
        let worker = &self.service.worker;
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Local scope missing")?;
        let state = worker.store()?;
        let mut mode = scope
            .lifetime
            .lock()
            .map_err(|_| "Lifetime mode lock poisoned")?;
        let ServiceLifetime::Supervised(resident) = &*mode else {
            return Err("Finalization requires the original Supervised mode; no replay".into());
        };
        if !Arc::ptr_eq(resident, expected) || command.binding.lifetime_command != resident.command
        {
            return Err("Finalization changed the original resident owner".into());
        }
        resident.keeper.require_live()?;
        if state
            .observe_service_finalization_release(&scope.command, &scope.shutdown, &command)?
            .is_some()
        {
            return Err("Saved SQL release cannot create a pending owner or Active permit".into());
        }
        resident.keeper.require_live()?;
        *mode = ServiceLifetime::ReleasePending(Arc::new(PendingLifetime {
            resident: Arc::clone(resident),
            command,
        }));
        Ok(())
    }

    /// The mailbox receiver supplies the sealed OS peer, never a JSON PID.
    /// A durable command alone cannot enter this mode on a fresh process.
    #[cfg(windows)]
    pub(super) fn enter_finalization_pending(
        &mut self,
        command: WorkerFinalizationCommand,
        peer: &winbridge_transport::local_peer::LocalPeer,
    ) -> Result<()> {
        let worker = &self.service.worker;
        if worker.stop.load(Ordering::Acquire)
            || self.exit.is_some()
            || self.task.as_ref().is_none_or(|t| t.is_finished())
            || !matches!(self.controller, super::super::ControllerStartup::Running(_))
        {
            return Err("Original resident/controller is stopping or not running".into());
        }
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Local scope missing")?;
        let resident = {
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::Supervised(value) => Arc::clone(value),
                _ => return Err("Finalization requires original Supervised mode".into()),
            }
        };
        require_pending_runtime(worker, &resident, &command, peer)?;
        self.seal_finalization_pending(command.clone(), &resident)?;
        // Failure retains ReleasePending and its original keeper; no SQL
        // release, second launch, deadline extension or Active seal follows.
        require_pending_runtime(&self.service.worker, &resident, &command, peer).map(|_| ())
    }

    #[cfg(windows)]
    pub(super) fn release_pending_finalization(
        &mut self,
        peer: &winbridge_transport::local_peer::LocalPeer,
    ) -> Result<store::activation_finalization::ActivatedReleaseAdmission> {
        let worker = &self.service.worker;
        if worker.stop.load(Ordering::Acquire)
            || self.exit.is_some()
            || self.task.as_ref().is_none_or(|t| t.is_finished())
        {
            return Err("Original pending resident is stopping".into());
        }
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Local scope missing")?;
        // Store -> mode is the common order for watchdog/seal/release.
        let mut state = worker.store()?;
        let pending = {
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::ReleasePending(value) => Arc::clone(value),
                _ => return Err("SQL release requires the already sealed pending owner".into()),
            }
        };
        state.release_activated_service(&scope.command, &scope.shutdown, &pending.command, || {
            require_pending_runtime(worker, &pending.resident, &pending.command, peer).map(|_| ())
        })
    }
}

#[cfg(windows)]
fn require_pending_runtime(
    worker: &Worker,
    resident: &ResidentLifetime,
    command: &WorkerFinalizationCommand,
    peer: &winbridge_transport::local_peer::LocalPeer,
) -> Result<winbridge_update::journal::RetainedFinalizationSnapshot> {
    if peer.process_id() != resident.command.binding.keeper.process_id
        || command.binding.lifetime_command != resident.command
        || worker.stop.load(Ordering::Acquire)
    {
        return Err(
            "Finalization requires the actual original live keeper peer and command".into(),
        );
    }
    resident.keeper.require_live()?;
    require_quiet_registries(worker)?;
    worker._instance_lock.revalidate()?;
    let scope = worker
        .local_activation
        .as_ref()
        .ok_or("Local scope missing")?;
    let guard = worker
        .runtime_guard
        .lock()
        .map_err(|_| "Runtime guard lock poisoned")?
        .clone()
        .ok_or("No actual loaded runtime guard for finalization")?;
    guard.revalidate()?;
    guard.require_installation(
        &command.binding.installation_id,
        &command.binding.scope_sha256,
    )?;
    if guard.identity() != &resident.command.binding.compatibility.identity {
        return Err("Finalization loaded runtime differs".into());
    }
    let path = worker
        .config_path
        .lock()
        .map_err(|_| "Config lock poisoned")?
        .clone()
        .ok_or("Finalization config path missing")?;
    let config = compatibility::check_config(
        &path,
        &resident.command.binding.compatibility.worker_config_sha256,
    )?;
    if config.instance_id != worker.config.instance_id
        || config.worker_config_schema != worker.config.schema_version
    {
        return Err("Finalization loaded config differs".into());
    }
    let child = winbridge_platform::infrastructure::PinnedProcess::live(&guard.identity().process)
        .map_err(|e| e.to_string())?;
    child
        .require_job_membership(&resident.command.binding.job_name)
        .map_err(|e| e.to_string())?;
    {
        use winbridge_platform::process_identity::ProcessObservation;
        let domain = worker
            .execution_domain
            .try_lock()
            .map_err(|_| "Execution domain busy/poisoned")?;
        if !domain.as_ref().is_some_and(|d| {
            matches!(
                winbridge_platform::execution_domain::observe_domain(d.identity()),
                ProcessObservation::Empty(_)
            )
        }) {
            return Err("Finalization execution domain is occupied or unknown".into());
        }
    }
    let directory = Path::new(&guard.identity().image.path)
        .parent()
        .ok_or("Runtime parent missing")?
        .join("update-state");
    let graph = winbridge_update::journal::UpdateJournal::inspect_pending_finalization_receiver(
        &directory,
        &guard.identity().process,
        &resident.command,
        &resident.receipt,
        command,
    )
    .map_err(|e| e.to_string())?;
    let winbridge_update::shutdown::ShutdownDeliveryState::Exited { receipt, .. } =
        &graph.shutdown.delivery
    else {
        return Err("Finalization old worker exit missing".into());
    };
    if graph.lifetime.controller.service.command != scope.command
        || receipt.as_ref() != &scope.shutdown
    {
        return Err("Finalization changed original service/shutdown history".into());
    }
    child
        .require_job_membership(&resident.command.binding.job_name)
        .map_err(|e| e.to_string())?;
    worker._instance_lock.revalidate()?;
    require_quiet_registries(worker)?;
    resident.keeper.require_live()?;
    Ok(graph)
}
