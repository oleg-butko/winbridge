//! A physical proof can only be constructed from retained kernel/file handles.
//! It neither replaces binaries nor starts a new worker.
use super::{LockedShutdownState, ShutdownExitEvidence, ShutdownIntent};
use crate::installed_set::PinnedInstalledSet;
use winbridge_platform::infrastructure::PinnedProcess;
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
        intent.validate()?;
        if self.intent != *intent
            || self.process.identity() != &intent.binding.worker.process
            || !self.process.exited().map_err(|e| e.to_string())?
            || self.state.observation() != &intent.binding.worker_state
        {
            return Err("Exact worker exit/state lock or shutdown attempt differs".into());
        }
        self.state.revalidate().map_err(|e| e.to_string())?;
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
