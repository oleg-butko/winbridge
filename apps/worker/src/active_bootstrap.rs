//! Intrinsic Active-worker scope for the next updater. Saved Active/keeper rows
//! cannot construct this owner. Checkpoints never wait for Worker mutexes while
//! an updater SQL transaction may be holding its own locks.
use super::*;

struct ActiveModeWitness {
    active: Arc<ActiveLifetime>,
    authority: store::JournalAuthority,
}
impl ActiveModeWitness {
    fn capture(worker: &Worker) -> Result<Self> {
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Bootstrap requires the original resident worker; no cold Active adoption")?;
        // Keep the established Store -> mode order. Contention is a refusal,
        // never a wait under the caller's updater-journal transaction.
        let state = worker
            .store
            .try_lock()
            .map_err(|_| "Bootstrap Store busy/poisoned")?;
        let mode = scope
            .lifetime
            .try_lock()
            .map_err(|_| "Bootstrap mode busy/poisoned")?;
        let ServiceLifetime::Active(active) = &*mode else {
            return Err("Bootstrap requires the sealed in-memory Active mode".into());
        };
        let value = Self {
            active: Arc::clone(active),
            authority: state.journal_authority().clone(),
        };
        value.require_locked(worker, &state, &mode)?;
        Ok(value)
    }
    fn require_locked(&self, worker: &Worker, state: &Store, mode: &ServiceLifetime) -> Result<()> {
        if worker.inspection || worker.stop.load(Ordering::Acquire) {
            return Err("Bootstrap unavailable during inspection/stop".into());
        }
        if !matches!(mode, ServiceLifetime::Active(v) if Arc::ptr_eq(v, &self.active)) {
            return Err("Bootstrap lost the original sealed Active owner".into());
        }
        let recovery = worker
            .unreconciled_tasks
            .try_lock()
            .map_err(|_| "Bootstrap recovery registry busy/poisoned")?;
        if !recovery.is_empty() {
            return Err("Bootstrap unavailable during journal recovery".into());
        }
        let c = &self.active.receipt.compatibility;
        if state.journal_authority() != &self.authority
            || state.state_backup_authority()? != self.active.receipt.authority
            || c.generation != worker.generation
            || c.context.instance_id != worker.config.instance_id
            || c.context.journal_incarnation != self.authority.incarnation
        {
            return Err("Bootstrap Active/Store authority or worker context changed".into());
        }
        // Running tasks and a nonempty domain are legitimate here. Drain is a
        // later operation, not an admission prerequisite.
        worker._instance_lock.revalidate()?;
        self.active.resident.keeper.require_live()
    }
    fn require_current(&self, worker: &Worker) -> Result<()> {
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Original resident scope missing")?;
        let state = worker
            .store
            .try_lock()
            .map_err(|_| "Bootstrap Store busy/poisoned")?;
        let mode = scope
            .lifetime
            .try_lock()
            .map_err(|_| "Bootstrap mode busy/poisoned")?;
        self.require_locked(worker, &state, &mode)
    }
}

/// Retains the actual worker, its sealed Active owner and loaded-runtime pins.
/// This is preparation scope, NOT a keeper capture, SQL commit or spawn permit.
/// There is deliberately no constructor from a receipt, JSON or journal row.
#[cfg(windows)]
pub struct ActiveWorkerBootstrap {
    worker: Arc<Worker>,
    mode: ActiveModeWitness,
    runtime: Arc<RuntimeGuard>,
    domain: winbridge_platform::execution_domain::DomainIdentity,
    process: winbridge_platform::infrastructure::PinnedProcess,
}
#[cfg(windows)]
impl Worker {
    /// Trusted Rust driver only; no CLI/MCP/network route. Ordinary workers
    /// keep the existing refusal to launch from an unverified external Job.
    pub fn active_worker_bootstrap(self: &Arc<Self>) -> Result<ActiveWorkerBootstrap> {
        let mode = ActiveModeWitness::capture(self)?;
        let runtime = self
            .runtime_guard
            .try_lock()
            .map_err(|_| "Bootstrap runtime guard busy/poisoned")?
            .clone()
            .ok_or("Actual loaded runtime guard missing")?;
        let domain = self
            .execution_domain
            .try_lock()
            .map_err(|_| "Bootstrap execution domain busy/poisoned")?
            .as_ref()
            .ok_or("Actual execution domain missing")?
            .identity()
            .clone();
        let process =
            winbridge_platform::infrastructure::PinnedProcess::live(&runtime.identity().process)
                .map_err(|e| e.to_string())?;
        let value = ActiveWorkerBootstrap {
            worker: Arc::clone(self),
            mode,
            runtime,
            domain,
            process,
        };
        value.require_current()?;
        Ok(value)
    }
}
#[cfg(windows)]
impl ActiveWorkerBootstrap {
    /// Nonblocking Worker-lock checkpoint, safe to use in updater SQL callbacks.
    /// Physical file/kernel checks remain necessary; this is not a time budget.
    pub fn require_current(&self) -> Result<()> {
        self.mode.require_current(&self.worker)?;
        let runtime = self
            .worker
            .runtime_guard
            .try_lock()
            .map_err(|_| "Bootstrap runtime guard busy/poisoned")?;
        if !runtime
            .as_ref()
            .is_some_and(|r| Arc::ptr_eq(r, &self.runtime))
        {
            return Err("Bootstrap loaded runtime owner changed".into());
        }
        let domain = self
            .worker
            .execution_domain
            .try_lock()
            .map_err(|_| "Bootstrap execution domain busy/poisoned")?;
        if domain.as_ref().map(|d| d.identity()) != Some(&self.domain) {
            return Err("Bootstrap execution domain changed".into());
        }
        let original = &self.mode.active.resident.command.binding;
        let identity = self.runtime.identity();
        if identity != &original.compatibility.identity
            || identity.process.process_id != std::process::id()
            || identity.descriptor.build != winbridge_protocol::build_identity()
            || self.domain.host_id != identity.process.host_id
            || self.domain.session_id != identity.process.session_id
            || self.domain.job_name == original.job_name
        {
            return Err("Bootstrap loaded worker/domain differs from original Active".into());
        }
        match &self.mode.active.resident.keeper {
            LifetimeKeeper::Native(keeper) => {
                if keeper.identity() != &original.keeper {
                    return Err("Bootstrap keeper pin identity changed".into());
                }
                keeper.require_live().map_err(|e| e.to_string())?;
                keeper
                    .require_outside_job(&original.job_name)
                    .map_err(|e| e.to_string())?;
            }
            #[cfg(test)]
            LifetimeKeeper::Fixture(_) => {
                return Err("Bootstrap requires the original native keeper pin".into())
            }
        }
        self.process
            .require_job_membership(&original.job_name)
            .map_err(|e| e.to_string())?;
        self.process
            .require_outside_job(&self.domain.job_name)
            .map_err(|e| e.to_string())?;
        self.runtime.revalidate()?;
        // The fresh KeeperProbe will pin this worker for its original bounded
        // capture. This checkpoint alone must not create/refresh that capture.
        self.mode.require_current(&self.worker)
    }
    /// A historical nomination must agree with this exact sealed owner. The
    /// result still needs a fresh authenticated probe of the original keeper.
    pub fn nominate_keeper(
        &self,
        selection: &winbridge_update::journal::KeeperSelection,
    ) -> Result<KeeperBinding> {
        self.require_current()?;
        selection.validate().map_err(|e| e.to_string())?;
        let binding = &selection.finalization.command.binding;
        let winbridge_update::activation::finalization::ActivationFinalizationState::Confirmed {
            receipt,
        } = &selection.finalization.state
        else {
            return Err("Bootstrap nomination has no confirmed Active receipt".into());
        };
        if receipt.as_ref() != &self.mode.active.receipt
            || binding.lifetime_command != self.mode.active.resident.command
        {
            return Err(
                "Bootstrap nomination differs from the original in-memory Active decision".into(),
            );
        }
        self.runtime
            .require_installation(&binding.installation_id, &binding.scope_sha256)?;
        let b = &binding.lifetime_command.binding;
        let nomination = keeper_binding_from_receipt(
            b.update_id.clone(),
            b.launch_id.clone(),
            b.keeper.clone(),
            b.job_name.clone(),
            receipt,
            Some(selection.sha256().map_err(|e| e.to_string())?),
        );
        self.require_current()?;
        Ok(nomination)
    }
    /// Resolve historical correlation from the fixed installed journal, then
    /// ask the actual original keeper. Only the latter creates the bounded
    /// capture; neither this scope nor a successful readonly SQL query does.
    pub async fn observe_keeper(&self) -> Result<winbridge_update::supervisor::KeeperProbeCapture> {
        self.require_current()?;
        let directory = Path::new(&self.runtime.identity().image.path)
            .parent()
            .ok_or("Bootstrap runtime directory missing")?
            .join("update-state");
        let original = &self.mode.active.resident.command.binding;
        let selection = winbridge_update::journal::UpdateJournal::inspect_keeper_selection(
            &directory,
            &original.launch_id,
        )
        .map_err(|e| e.to_string())?
        .ok_or("Original keeper selection not committed; observe without creating a coordinator")?;
        let binding = self.nominate_keeper(&selection)?;
        let capture =
            winbridge_update::supervisor::observe_original_keeper(&directory, &binding).await?;
        self.require_current()?;
        capture.require_current()?;
        Ok(capture)
    }
}

#[cfg(test)]
#[path = "lifetime_service/bootstrap_tests.rs"]
mod tests;
