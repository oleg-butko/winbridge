//! Same-process Active seal, consumed by the owning V8 coordinator.
use super::*;
use crate::update_control::ControlServer;
use store::activation_finalization::ActivatedReleaseCommit;

pub(in crate::local_service::lifetime) struct ActiveReadiness {
    pending: Arc<PendingLifetime>,
    control: ReadyControl,
}
/// A failed final checkpoint must keep the known commit and the staged
/// listener. Reconciliation may finish this same seal, never release SQL again.
pub(in crate::local_service::lifetime) struct ActiveSealFailure {
    pub(in crate::local_service::lifetime) reason: String,
    commit: Box<ActivatedReleaseCommit>,
    ready: ActiveReadiness,
}
impl std::fmt::Debug for ActiveSealFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSealFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl ActiveSealFailure {
    pub(in crate::local_service::lifetime) fn into_parts(
        self,
    ) -> (Box<ActivatedReleaseCommit>, ActiveReadiness) {
        (self.commit, self.ready)
    }
}
enum ReadyControl {
    #[cfg(windows)]
    Native {
        server: ControlServer,
        shutdown: Box<winbridge_update::shutdown::ShutdownRecord>,
    },
    #[cfg(test)]
    Fixture,
}
impl ActiveReadiness {
    #[cfg(windows)]
    pub(in crate::local_service::lifetime) async fn close(self) {
        if let Some(server) = self.into_server() {
            server.close().await;
        }
    }
    #[cfg(test)]
    pub(in crate::local_service::lifetime) fn fixture(pending: Arc<PendingLifetime>) -> Self {
        Self {
            pending,
            control: ReadyControl::Fixture,
        }
    }
    fn require_ready(&self, receipt: &WorkerFinalizationReceipt) -> Result<()> {
        match &self.control {
            #[cfg(windows)]
            ReadyControl::Native { server, shutdown } => {
                if !server.is_ready() {
                    return Err("Original update-control listener is not ready".into());
                }
                winbridge_update::activation::finalization::validate_receipt(
                    receipt,
                    &self.pending.command,
                    shutdown,
                )
            }
            #[cfg(test)]
            ReadyControl::Fixture => {
                let _ = receipt;
                Ok(())
            }
        }
    }
    fn into_server(self) -> Option<ControlServer> {
        match self.control {
            #[cfg(windows)]
            ReadyControl::Native { server, .. } => Some(server),
            #[cfg(test)]
            ReadyControl::Fixture => None,
        }
    }
}

impl RunningLocalService {
    /// Stage a listener while all public admissions remain closed. The
    /// caller must retain this owner across the existing SQL release call.
    #[cfg(windows)]
    pub(in crate::local_service::lifetime) fn prepare_active_readiness(
        &mut self,
        peer: &winbridge_transport::local_peer::LocalPeer,
    ) -> Result<ActiveReadiness> {
        let worker = &self.service.worker;
        let pending = {
            let scope = worker
                .local_activation
                .as_ref()
                .ok_or("Original scope missing")?;
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            let ServiceLifetime::ReleasePending(value) = &*mode else {
                return Err("Active readiness requires original pending owner".into());
            };
            Arc::clone(value)
        };
        if self.active_control.is_some()
            || self.exit.is_some()
            || self.task.as_ref().is_none_or(|t| t.is_finished())
            || !matches!(
                self.controller,
                super::super::super::ControllerStartup::Running(_)
            )
        {
            return Err("Original IPC/controller is not ready for Active".into());
        }
        let graph = require_pending_runtime(worker, &pending.resident, &pending.command, peer)?;
        let guard = worker
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .clone()
            .ok_or("Actual loaded runtime guard missing")?;
        let (config, sha) = guard.resident_updater_source()?;
        worker.enable_resident_update_control(&config, &sha)?;
        let server = crate::update_control::start(Arc::clone(worker))?
            .ok_or("Resident control registration missing")?;
        require_pending_runtime(worker, &pending.resident, &pending.command, peer)?;
        Ok(ActiveReadiness {
            pending,
            control: ReadyControl::Native {
                server,
                shutdown: Box::new(graph.shutdown),
            },
        })
    }

    /// Native owner only: the new listener and exact known SQL commit are both
    /// consumed. No caller may substitute a saved receipt for this commit.
    #[cfg(windows)]
    pub(in crate::local_service::lifetime) fn finish_active_release(
        &mut self,
        commit: Box<ActivatedReleaseCommit>,
        ready: ActiveReadiness,
        peer: &winbridge_transport::local_peer::LocalPeer,
    ) -> std::result::Result<WorkerFinalizationReceipt, ActiveSealFailure> {
        let worker = Arc::clone(&self.service.worker);
        let pending = Arc::clone(&ready.pending);
        self.seal_active_commit(commit, ready, || {
            require_pending_runtime(&worker, &pending.resident, &pending.command, peer).map(|_| ())
        })
    }

    pub(in crate::local_service::lifetime) fn seal_active_commit(
        &mut self,
        commit: Box<ActivatedReleaseCommit>,
        ready: ActiveReadiness,
        checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<WorkerFinalizationReceipt, ActiveSealFailure> {
        let mut staged = Some(ready);
        match self.seal_active_inner(&commit, &mut staged, checkpoint) {
            Ok(receipt) => Ok(receipt),
            Err(reason) => Err(ActiveSealFailure {
                reason,
                commit,
                ready: staged.expect("Readiness moves only after the final fallible check"),
            }),
        }
    }

    fn seal_active_inner(
        &mut self,
        commit: &ActivatedReleaseCommit,
        staged: &mut Option<ActiveReadiness>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<WorkerFinalizationReceipt> {
        let ready = staged.as_ref().ok_or("Original readiness owner missing")?;
        let worker = &self.service.worker;
        if worker.stop.load(Ordering::Acquire)
            || self.exit.is_some()
            || self.task.as_ref().is_none_or(|t| t.is_finished())
            || self.service._config_pin.is_none()
            || self.active_control.is_some()
        {
            return Err("Original service/config pin is not ready for Active".into());
        }
        worker._instance_lock.revalidate()?;
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Original scope missing")?;
        let state = worker.store()?;
        let mut mode = scope
            .lifetime
            .lock()
            .map_err(|_| "Lifetime mode lock poisoned")?;
        let ServiceLifetime::ReleasePending(pending) = &*mode else {
            return Err("Active seal requires the original pending owner; no replay".into());
        };
        if !Arc::ptr_eq(pending, &ready.pending) || commit.command() != &pending.command {
            return Err("Active commit/readiness belong to another original owner".into());
        }
        pending.resident.keeper.require_live()?;
        require_quiet_registries(worker)?;
        if state
            .observe_service_finalization_release(
                &scope.command,
                &scope.shutdown,
                &pending.command,
            )?
            .as_ref()
            != Some(commit.lease())
        {
            return Err("Known release commit differs from the same Store".into());
        }
        let mut compatibility = pending.resident.command.binding.compatibility.clone();
        compatibility.observed_at_ms = store::now_ms();
        if compatibility.observed_at_ms < commit.lease().updated_at_ms
            || compatibility.observed_at_ms > 9_007_199_254_740_991
        {
            return Err("Active observation clock moved backwards/outside contract".into());
        }
        let receipt = WorkerFinalizationReceipt {
            schema_version: 1,
            command_id: pending.command.command_id.clone(),
            command_sha256: winbridge_update::activation::finalization::command_sha256(
                &pending.command,
            )?,
            compatibility,
            authority: pending
                .command
                .binding
                .lifetime_command
                .binding
                .authority
                .clone(),
            released_lease: commit.lease().clone(),
            mode: WorkerFinalizationMode::Active,
            released_at_ms: commit.lease().updated_at_ms,
        };
        checkpoint()?;
        ready.require_ready(&receipt)?;
        pending.resident.keeper.require_live()?;
        let active = Arc::new(ActiveLifetime {
            resident: Arc::clone(&pending.resident),
            receipt: receipt.clone(),
        });
        // All fallible checks precede pin retirement. The same Store and mode
        // locks keep Info/watchdog/admissions from seeing a partial transition.
        drop(self.service._config_pin.take());
        self.active_control = staged
            .take()
            .expect("Checked readiness owner")
            .into_server();
        *mode = ServiceLifetime::Active(active);
        Ok(receipt)
    }
}
