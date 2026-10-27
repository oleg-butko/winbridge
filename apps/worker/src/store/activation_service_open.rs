//! Consuming existing-connection transfer. Does not open a second Store or lock,
//! run reconciliation, bind IPC, start dispatch or release maintenance.
use super::{
    activation, activation_open::ActivationJournal, sql, JournalActivationPermit, Result, Store,
};
use winbridge_protocol::update::{WorkerServiceActivationCommand, WorkerShutdownState};
use winbridge_update::shutdown::LockedShutdownState;

/// Only the sealed existing-state transfer can construct this transient fence.
/// Every startup write rechecks the exact receipt, header, lease and deadline;
/// ordinary root-admission/dispatch/admin checks remain enabled throughout.
pub(super) struct ServiceStartupFence {
    command: WorkerServiceActivationCommand,
    shutdown: winbridge_protocol::update::WorkerShutdownReceipt,
    deadline: std::time::Instant,
}
impl ServiceStartupFence {
    pub(super) fn require(&self, c: &rusqlite::Connection) -> Result<()> {
        if std::time::Instant::now() >= self.deadline
            || super::now_ms() < self.command.issued_at_ms
            || super::now_ms() >= self.command.deadline_at_ms
        {
            return Err(
                "Original local-service initialization window expired or clock moved backwards"
                    .into(),
            );
        }
        activation::window(&self.command.binding.writer_command)?;
        Self::require_binding(c, &self.command, &self.shutdown)
    }
    fn require_binding(
        c: &rusqlite::Connection,
        command: &WorkerServiceActivationCommand,
        shutdown: &winbridge_protocol::update::WorkerShutdownReceipt,
    ) -> Result<()> {
        let schema: u32 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        if schema != super::JOURNAL_SCHEMA_VERSION
            || activation::stamp(&Store::read_authority(c)?)?
                != command.binding.writer_receipt.authority
            || activation::read(c, &command.binding.writer_command)?.as_ref()
                != Some(&command.binding.writer_receipt)
            || super::update::read(c, &shutdown.update_id)? != shutdown.lease
            || super::shutdown::read(c, &shutdown.update_id, &shutdown.command)?.as_ref()
                != Some(shutdown)
        {
            return Err("Service initialization authority/receipt/lease/schema changed".into());
        }
        Ok(())
    }
}
pub(crate) struct ServiceRuntimeState {
    pub(crate) store: Store,
    pub(crate) state: LockedShutdownState,
    pub(crate) command: WorkerServiceActivationCommand,
    pub(crate) deadline: std::time::Instant,
    pub(crate) shutdown: winbridge_protocol::update::WorkerShutdownReceipt,
}

/// Sealed ownership of the same activated connection and full locked state.
/// Future Worker construction consumes this object; serialized receipts cannot
/// recreate it. No public method exposes the Store or instance-lock handle.
pub struct PreparedServiceJournal {
    journal: ActivationJournal,
    command: WorkerServiceActivationCommand,
}
impl PreparedServiceJournal {
    /// Crate-private consuming factory; no Store/lock extraction from JSON.
    pub(crate) fn into_runtime(
        self,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<ServiceRuntimeState, Box<ServiceJournalPreparationFailure>> {
        let result = (|| {
            self.journal.check_service_continuation(&self.command)?;
            checkpoint()?;
            self.journal.check_service_continuation(&self.command)
        })();
        if let Err(reason) = result {
            return Err(Box::new(ServiceJournalPreparationFailure {
                reason,
                journal: self.journal,
            }));
        }
        let (mut store, state, shutdown, deadline) = self.journal.into_service_parts();
        store.service_startup = Some(ServiceStartupFence {
            command: self.command.clone(),
            shutdown: shutdown.clone(),
            deadline,
        });
        Ok(ServiceRuntimeState {
            store,
            state,
            command: self.command,
            deadline,
            shutdown,
        })
    }
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

impl Store {
    pub(crate) fn observe_local_service_binding(
        &self,
        command: &WorkerServiceActivationCommand,
        shutdown: &winbridge_protocol::update::WorkerShutdownReceipt,
    ) -> Result<(winbridge_protocol::update::StateBackupAuthority, u32)> {
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        self.check_authority()?;
        ServiceStartupFence::require_binding(&tx, command, shutdown)?;
        let authority = self.state_backup_authority()?;
        let schema = tx
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok((authority, schema))
    }
    pub(crate) fn check_local_service_identity(
        &self,
        command: &WorkerServiceActivationCommand,
        shutdown: &winbridge_protocol::update::WorkerShutdownReceipt,
        deadline: std::time::Instant,
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        self.check_authority()?;
        ServiceStartupFence {
            command: command.clone(),
            shutdown: shutdown.clone(),
            deadline,
        }
        .require(&tx)?;
        tx.commit().map_err(sql)
    }
    pub(crate) fn finish_service_initialization(&mut self) -> Result<()> {
        let scope = self
            .service_startup
            .as_ref()
            .ok_or("Service initialization has no fresh sealed fence")?;
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        self.check_authority()?;
        scope.require(&tx)?;
        tx.commit().map_err(sql)?;
        self.service_startup = None;
        Ok(())
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
