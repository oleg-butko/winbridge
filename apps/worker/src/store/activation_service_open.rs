//! Consuming existing-connection transfer. Does not open a second Store or lock,
//! run reconciliation, bind IPC, start dispatch or release maintenance.
use super::{activation_open::ActivationJournal, JournalActivationPermit, Result};
use winbridge_protocol::update::{WorkerServiceActivationCommand, WorkerShutdownState};

/// Sealed ownership of the same activated connection and full locked state.
/// Future Worker construction consumes this object; serialized receipts cannot
/// recreate it. No public method exposes the Store or instance-lock handle.
pub struct PreparedServiceJournal {
    journal: ActivationJournal,
    command: WorkerServiceActivationCommand,
}
impl PreparedServiceJournal {
    pub fn command(&self) -> &WorkerServiceActivationCommand {
        &self.command
    }
    pub fn state_observation(&self) -> &WorkerShutdownState {
        self.journal.state_observation()
    }
    pub fn receipt(
        &self,
    ) -> Result<Option<winbridge_protocol::update::WorkerJournalActivationReceipt>> {
        self.journal.receipt()
    }
    pub fn revalidate_state(&self) -> Result<()> {
        self.journal.revalidate_state()
    }
}

/// Preparation failed after consuming the fresh permit. Retain the same journal
/// and state lock without offering another activation or service attempt.
pub struct ServiceJournalPreparationFailure {
    reason: String,
    journal: ActivationJournal,
}
impl std::fmt::Debug for ServiceJournalPreparationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceJournalPreparationFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ServiceJournalPreparationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for ServiceJournalPreparationFailure {}
impl ServiceJournalPreparationFailure {
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn state_observation(&self) -> &WorkerShutdownState {
        self.journal.state_observation()
    }
    pub fn revalidate_state(&self) -> Result<()> {
        self.journal.revalidate_state()
    }
}

impl ActivationJournal {
    /// Trusted receiver must authenticate service4/current updater graph,
    /// actual loaded process/runtime/config and original monotonic deadline in
    /// checkpoint. This transfer alone never grants normal IPC/readiness.
    pub fn prepare_local_service(
        self,
        fresh: Box<JournalActivationPermit>,
        command: WorkerServiceActivationCommand,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<PreparedServiceJournal, Box<ServiceJournalPreparationFailure>> {
        let result = self.check_service_transfer(&fresh, &command, &mut checkpoint);
        // Fresh is consumed on BOTH outcomes. The error retains the connection
        // and complete state owner, not a clone of its instance-lock file.
        drop(fresh);
        match result {
            Ok(()) => Ok(PreparedServiceJournal {
                journal: self,
                command,
            }),
            Err(reason) => Err(Box::new(ServiceJournalPreparationFailure {
                reason,
                journal: self,
            })),
        }
    }
}

#[cfg(test)]
mod tests;
