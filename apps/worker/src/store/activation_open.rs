//! Existing-state connection held together with the same acquired instance lock.
//! Trusted library only: the caller still proves the authenticated child/graph,
//! loaded runtime/config, parent liveness and original monotonic startup window.
use super::{activation, sql, JournalActivationAcceptance, Result, Store, JOURNAL_SCHEMA_VERSION};
use rusqlite::{config::DbConfig, Connection, OpenFlags};
use std::{fmt, path::Path};
use winbridge_protocol::update::{
    WorkerJournalActivationCommand, WorkerJournalActivationReceipt, WorkerShutdownReceipt,
    WorkerShutdownState,
};
use winbridge_update::shutdown::LockedShutdownState;

/// No Store extraction or ordinary command surface. The actual acquired lock
/// remains resident through connection preparation, activation and uncertainty.
pub struct ActivationJournal {
    // Close SQLite before releasing the instance lock, including on Drop.
    store: Store,
    state: LockedShutdownState,
    command: WorkerJournalActivationCommand,
    shutdown: WorkerShutdownReceipt,
    attempted: bool,
}

/// No writable connection escapes on preparation failure. Keep the same lock
/// and state pins instead of returning a reusable startup/admission permit.
pub struct ActivationJournalOpenFailure {
    reason: String,
    state: LockedShutdownState,
}
impl fmt::Debug for ActivationJournalOpenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActivationJournalOpenFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ActivationJournalOpenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for ActivationJournalOpenFailure {}
impl ActivationJournalOpenFailure {
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn state_observation(&self) -> &WorkerShutdownState {
        self.state.observation()
    }
    pub fn revalidate_state(&self) -> Result<()> {
        self.state.revalidate().map_err(|e| e.to_string())
    }
}

fn filesystem(state: &LockedShutdownState) -> Result<()> {
    state.revalidate().map_err(|e| e.to_string())?;
    let path = Path::new(&state.observation().state_directory).join("journal.sqlite");
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        winbridge_platform::require_safe_path(Path::new(&name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn existing_connection(path: &Path, writable: bool) -> Result<Store> {
    let access = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection =
        Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(sql)?;
    // Without this, dropping the last RW connection (even without a write)
    // checkpoints/deletes WAL before any authority acceptance. Set and check
    // this connection-only option before the first schema/database query.
    if !connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .map_err(sql)?
    {
        return Err("Activation connection cannot disable close-time checkpoint".into());
    }
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(sql)?;
    connection
        .execute_batch("PRAGMA query_only=ON;")
        .map_err(sql)?;
    let schema: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(sql)?;
    if schema != JOURNAL_SCHEMA_VERSION {
        return Err("Activation connection requires exact current schema; no migration".into());
    }
    let authority = Store::read_authority(&connection)?;
    Ok(Store {
        connection,
        authority,
        callback_attempt: None,
        read_only: !writable,
        work_scheduling: None,
        queue_policy: None,
    })
}

fn verify(
    store: &Store,
    command: &WorkerJournalActivationCommand,
    shutdown: &WorkerShutdownReceipt,
) -> Result<()> {
    // Header, exact lease and accepted stop are checked in one read snapshot.
    let tx = store.connection.unchecked_transaction().map_err(sql)?;
    let schema: u32 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(sql)?;
    let mode: String = tx
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .map_err(sql)?;
    if schema != JOURNAL_SCHEMA_VERSION || mode != "wal" {
        return Err("Activation open requires exact current schema and existing WAL mode; no migration or mode change".into());
    }
    if store.state_backup_authority()? != command.binding.expected_authority
        || super::update::read(&tx, &shutdown.update_id)? != shutdown.lease
        || super::shutdown::read(&tx, &shutdown.update_id, &shutdown.command)?.as_ref()
            != Some(shutdown)
    {
        return Err(
            "Activation open old authority/quiesced lease/accepted shutdown differs".into(),
        );
    }
    let recorded: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM worker_journal_activations WHERE update_id=?1)",
            [&shutdown.update_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if recorded {
        return Err(
            "Activation already recorded; observe history without reopening an owner".into(),
        );
    }
    tx.commit().map_err(sql)
}

fn prepare(
    state: &LockedShutdownState,
    command: &WorkerJournalActivationCommand,
    shutdown: &WorkerShutdownReceipt,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Store> {
    activation::scope(command, shutdown)?;
    if state.observation() != &command.binding.physical.state {
        return Err("Activation open requires the exact already acquired state".into());
    }
    activation::window(command)?;
    filesystem(state)?;
    checkpoint()?;
    filesystem(state)?;
    let path = Path::new(&state.observation().state_directory).join("journal.sqlite");
    // Readonly preflight prevents generic opening/migration from being used as
    // a permission to write an old/future or unrelated journal.
    let reader = existing_connection(&path, false)?;
    verify(&reader, command, shutdown)?;
    let authority = reader.authority.clone();
    drop(reader);
    filesystem(state)?;
    checkpoint()?;
    activation::window(command)?;
    filesystem(state)?;
    // SQLITE_OPEN_CREATE deliberately absent; no normal Store::open or
    // begin_authority, journal_mode assignment, recovery replay or scheduler.
    let store = existing_connection(&path, true)?;
    if store.authority != authority {
        return Err("Activation source authority changed between read and writable open".into());
    }
    verify(&store, command, shutdown)?;
    filesystem(state)?;
    checkpoint()?;
    activation::window(command)?;
    filesystem(state)?;
    // Connection-only settings. No persistent schema/journal-mode write.
    store
        .connection
        .execute_batch("PRAGMA synchronous=FULL;PRAGMA foreign_keys=ON;PRAGMA query_only=OFF;")
        .map_err(sql)?;
    Ok(store)
}

impl ActivationJournal {
    pub(super) fn revalidate_state(&self) -> Result<()> {
        filesystem(&self.state)
    }

    /// Same held connection, header, immutable receipt and Quiesced lease.
    /// No CREATE/open/migration/begin_authority/checkpoint-mode change.
    pub(super) fn check_service_transfer(
        &self,
        fresh: &super::JournalActivationPermit,
        command: &winbridge_protocol::update::WorkerServiceActivationCommand,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        winbridge_update::activation::service::validate_command_envelope(command)?;
        if !self.attempted
            || command.binding.writer_command != self.command
            || &command.binding.writer_receipt != fresh.receipt()
        {
            return Err(
                "Service transfer requires the same fresh activation command/receipt".into(),
            );
        }
        filesystem(&self.state)?;
        checkpoint()?;
        activation::window(&self.command)?;
        if super::now_ms() < command.issued_at_ms {
            return Err("Service transfer clock precedes the admitted service command".into());
        }
        self.check_service_writer(fresh)?;
        filesystem(&self.state)?;
        checkpoint()?;
        activation::window(&self.command)?;
        if super::now_ms() < command.issued_at_ms {
            return Err("Service transfer clock moved before admission".into());
        }
        // The external checkpoint may observe a concurrent owner/root change.
        // Recheck the worker header/receipt after it, not just before it.
        self.check_service_writer(fresh)?;
        filesystem(&self.state)
    }

    fn check_service_writer(&self, fresh: &super::JournalActivationPermit) -> Result<()> {
        let tx = self.store.connection.unchecked_transaction().map_err(sql)?;
        if self.store.state_backup_authority()? != fresh.receipt().authority
            || activation::read(&tx, &self.command)?.as_ref() != Some(fresh.receipt())
            || super::update::read(&tx, &self.shutdown.update_id)? != self.shutdown.lease
            || super::shutdown::read(&tx, &self.shutdown.update_id, &self.shutdown.command)?
                .as_ref()
                != Some(&self.shutdown)
        {
            return Err(
                "Service transfer authority/receipt/quiesced lease/shutdown changed".into(),
            );
        }
        tx.commit().map_err(sql)
    }

    /// Consumes the actual already held lock. The checkpoint must reject stale
    /// graph/parent/runtime/child or expired original monotonic custody. This
    /// constructor alone is not an authenticated runtime startup permission.
    pub fn open(
        state: LockedShutdownState,
        command: WorkerJournalActivationCommand,
        shutdown: WorkerShutdownReceipt,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Self, Box<ActivationJournalOpenFailure>> {
        match prepare(&state, &command, &shutdown, &mut checkpoint) {
            Ok(store) => Ok(Self {
                store,
                state,
                command,
                shutdown,
                attempted: false,
            }),
            Err(reason) => Err(Box::new(ActivationJournalOpenFailure { reason, state })),
        }
    }
    pub fn state_observation(&self) -> &WorkerShutdownState {
        self.state.observation()
    }
    /// At most one transaction attempt on this resident owner. A duplicate
    /// observes the recorded outcome; even a pre-commit failure cannot replay.
    pub fn activate(
        &mut self,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<JournalActivationAcceptance> {
        if self.attempted {
            return self
                .receipt()?
                .map(|r| JournalActivationAcceptance::Observed(Box::new(r)))
                .ok_or_else(|| {
                    "Activation already attempted without confirmed receipt; observe/reconcile, no replay"
                        .into()
                });
        }
        self.attempted = true;
        filesystem(&self.state)?;
        let state = &self.state;
        self.store
            .activate_journal_authority_checked(&self.command, &self.shutdown, || {
                filesystem(state)?;
                checkpoint()?;
                filesystem(state)
            })
    }
    /// Historical receipt only, including after an uncertain transaction.
    pub fn receipt(&self) -> Result<Option<WorkerJournalActivationReceipt>> {
        self.store.journal_activation_receipt(&self.command)
    }
}

#[cfg(test)]
#[path = "activation_open_tests.rs"]
pub(super) mod tests;
