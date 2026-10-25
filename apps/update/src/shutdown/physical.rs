//! A physical proof can only be constructed from retained kernel/file handles.
//! It neither replaces binaries nor starts a new worker.
use super::{LockedShutdownState, ShutdownExitEvidence, ShutdownIntent};
use crate::installed_set::PinnedInstalledSet;
use std::sync::{Arc, Mutex};
use winbridge_platform::infrastructure::PinnedProcess;
/// Supervisor-resident custody survives retirement of the blocking driver.
/// It is never reconstructed from saved rows or exposed in a wire DTO.
#[derive(Clone, Default)]
pub struct Custody(Arc<Mutex<Option<Arc<ShutdownExitGuard>>>>);
impl Custody {
    pub fn hold(&self, guard: ShutdownExitGuard) -> Result<(), String> {
        let mut slot = self
            .0
            .lock()
            .map_err(|_| "Shutdown custody lock poisoned")?;
        if slot.is_some() {
            return Err("Shutdown physical custody is already held".into());
        }
        *slot = Some(Arc::new(guard));
        Ok(())
    }
    /// Loan the same immutable guard to an owned blocking driver. The mailbox
    /// keeps custody; losing the mailbox cannot drop a still-running loan.
    pub fn loan(&self) -> Result<Option<Arc<ShutdownExitGuard>>, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "Shutdown custody lock poisoned")?
            .clone())
    }
    pub fn present(&self) -> Result<bool, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "Shutdown custody lock poisoned")?
            .is_some())
    }
    /// Only the original fresh Planned admission may consume resident custody.
    /// Slow SQL/file checks run through an immutable loan, outside the slot mutex.
    /// A racing loan makes extraction busy and restores the very same Arc.
    #[cfg(windows)]
    pub fn take_for_replacement(
        &self,
        j: &crate::journal::UpdateJournal,
        a: &crate::journal::UpdateAuthority,
        s: &winbridge_protocol::update::SupervisorRecord,
        permit: &crate::runtime_replacement::RuntimeReplacementPermit,
    ) -> Result<ShutdownExitGuard, String> {
        let record =
            crate::runtime_replacement::RuntimeReplacementRecord::planned(permit.intent().clone())?;
        j.require_runtime_replacement_current(a, s, &record, permit)
            .map_err(|e| e.to_string())?;
        let loan = self.loan()?.ok_or("Original shutdown custody missing")?;
        loan.require(permit.shutdown_intent())?;
        drop(loan);
        let mut slot = self
            .0
            .lock()
            .map_err(|_| "Shutdown custody lock poisoned")?;
        take_unique(&mut slot)?.ok_or_else(|| "Original shutdown custody already consumed".into())
    }
}
#[cfg(any(windows, test))]
fn take_unique<T>(slot: &mut Option<Arc<T>>) -> Result<Option<T>, String> {
    let Some(guard) = slot.take() else {
        return Ok(None);
    };
    match Arc::try_unwrap(guard) {
        Ok(guard) => Ok(Some(guard)),
        Err(guard) => {
            *slot = Some(guard);
            Err("Shutdown custody has outstanding loans; no handoff".into())
        }
    }
}
pub struct ShutdownExitGuard {
    intent: ShutdownIntent,
    process: PinnedProcess,
    state: LockedShutdownState,
    runtime: PinnedInstalledSet,
}
impl ShutdownExitGuard {
    /// `process` must have been pinned while alive, before delivery. There is no
    /// constructor that turns a missing PID/pipe or JSON flags into this guard.
    pub fn observe(
        intent: &ShutdownIntent,
        process: PinnedProcess,
        state: LockedShutdownState,
        runtime: PinnedInstalledSet,
    ) -> Result<Self, String> {
        let guard = Self {
            intent: intent.clone(),
            process,
            state,
            runtime,
        };
        guard.require(intent)?;
        Ok(guard)
    }
    pub(crate) fn require(&self, intent: &ShutdownIntent) -> Result<(), String> {
        self.require_exit_state(intent)?;
        self.runtime.revalidate()?;
        let runtime = self.runtime.observation();
        let loaded = &intent.binding.loaded_worker;
        if runtime.runtime_identity != loaded.runtime_identity
            || runtime.manifest.set_id != loaded.set_id
            || runtime.manifest_file.sha256 != loaded.manifest_sha256
            || runtime.images.first() != Some(&loaded.image)
            || runtime.manifest.artifacts.first().map(|a| &a.descriptor) != Some(&loaded.descriptor)
        {
            return Err("Exited worker runtime differs from its retained baseline".into());
        }
        Ok(())
    }
    pub(crate) fn require_exit_state(&self, intent: &ShutdownIntent) -> Result<(), String> {
        intent.validate()?;
        if self.intent != *intent
            || self.process.identity() != &intent.binding.worker.process
            || !self.process.exited().map_err(|e| e.to_string())?
            || self.state.observation() != &intent.binding.worker_state
        {
            return Err("Exact worker exit/state lock or shutdown attempt differs".into());
        }
        self.state.revalidate().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(windows)]
    pub(crate) fn into_replacement_parts(self) -> (ExitedStateCustody, PinnedInstalledSet) {
        (
            ExitedStateCustody {
                intent: self.intent,
                process: self.process,
                state: self.state,
            },
            self.runtime,
        )
    }
    pub(crate) fn evidence(&self, intent: &ShutdownIntent) -> Result<ShutdownExitEvidence, String> {
        self.require(intent)?;
        Ok(ShutdownExitEvidence {
            process: self.process.identity().clone(),
            state: self.state.observation().clone(),
            runtime_identity: self.runtime.observation().runtime_identity.clone(),
            exact_process_exited: true,
            instance_lock_acquired: true,
            observed_at_ms: crate::journal::now_ms().map_err(|e| e.to_string())?,
        })
    }
}
#[cfg(windows)]
pub(crate) struct ExitedStateCustody {
    intent: ShutdownIntent,
    process: PinnedProcess,
    state: LockedShutdownState,
}
#[cfg(windows)]
impl ExitedStateCustody {
    pub(crate) fn require(&self, intent: &ShutdownIntent) -> Result<(), String> {
        intent.validate()?;
        if self.intent != *intent
            || self.process.identity() != &intent.binding.worker.process
            || !self.process.exited().map_err(|e| e.to_string())?
            || self.state.observation() != &intent.binding.worker_state
        {
            return Err("Transferred original exit/state scope differs".into());
        }
        self.state.revalidate().map_err(|e| e.to_string())
    }
    /// Only the original owner can consume this resource. Failure keeps the
    /// very same old-process pin and residual state handles, never new custody.
    pub(crate) fn release(
        self,
        checkpoint: impl FnMut() -> std::io::Result<()>,
    ) -> ReleasedExitCustody {
        let state = self.state.release_for_handoff_checked(checkpoint);
        ReleasedExitCustody {
            intent: self.intent,
            process: self.process,
            state,
        }
    }
}
#[cfg(windows)]
pub(crate) struct ReleasedExitCustody {
    intent: ShutdownIntent,
    process: PinnedProcess,
    state: std::result::Result<super::ReleasedShutdownState, super::ShutdownStateReleaseFailure>,
}
#[cfg(windows)]
impl ReleasedExitCustody {
    pub(crate) fn original_lock_held(&self) -> Option<bool> {
        match &self.state {
            Ok(_) => Some(false),
            Err(e) => e.original_lock_held(),
        }
    }
    pub(crate) fn require_released(&self, intent: &ShutdownIntent) -> Result<(), String> {
        intent.validate()?;
        let state = self.state.as_ref().map_err(|e| e.to_string())?;
        if self.intent != *intent
            || self.process.identity() != &intent.binding.worker.process
            || !self.process.exited().map_err(|e| e.to_string())?
            || state.observation() != &intent.binding.worker_state
        {
            return Err("Released original state/exit scope changed".into());
        }
        state.revalidate().map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod custody_tests {
    use super::*;
    #[test]
    fn custody_busy_restores_same_arc_and_unique_take_keeps_original_resource() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Resource(Arc<AtomicUsize>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut slot = Some(Arc::new(Resource(dropped.clone())));
        let loan = slot.as_ref().unwrap().clone();
        let pointer = Arc::as_ptr(&loan);
        assert!(take_unique(&mut slot).is_err());
        assert_eq!(Arc::as_ptr(slot.as_ref().unwrap()), pointer);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(loan);
        let taken = take_unique(&mut slot).unwrap().unwrap();
        assert!(slot.is_none());
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(taken);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(take_unique(&mut slot).unwrap().is_none());
    }
}
