//! Atomic journal metadata activation. No OS acquisition, startup or lease release.
use super::{now_ms, sql, JournalAuthority, Result, Store, JOURNAL_SCHEMA_VERSION};
use rusqlite::{params, Connection, OpenFlags};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use winbridge_protocol::update::{
    StateBackupAuthority, WorkerJournalActivationCommand, WorkerJournalActivationReceipt,
    WorkerShutdownReceipt, WorkerUpdatePhase,
};
use winbridge_update::activation::writer::{
    command_sha256, validate_command_envelope, validate_receipt,
};

pub struct JournalActivationPermit {
    receipt: WorkerJournalActivationReceipt,
}
impl JournalActivationPermit {
    /// Database acceptance only, never a physical startup/lifetime permit.
    pub fn receipt(&self) -> &WorkerJournalActivationReceipt {
        &self.receipt
    }
}
pub enum JournalActivationAcceptance {
    Fresh(Box<JournalActivationPermit>),
    Observed(Box<WorkerJournalActivationReceipt>),
}
pub(super) struct ActivationMigrationBackup {
    pub(super) authority: JournalAuthority,
    file: File,
    path: PathBuf,
    identity: String,
}
impl ActivationMigrationBackup {
    pub(super) fn revalidate(&self) -> Result<()> {
        super::archive::require_destination(&self.path, &self.file, &self.identity)
    }
}
fn one(n: usize) -> Result<()> {
    if n == 1 {
        Ok(())
    } else {
        Err("Journal activation write ignored; no activation acceptance".into())
    }
}
fn encode(value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn stamp(authority: &JournalAuthority) -> Result<StateBackupAuthority> {
    let mut raw = b"winbridge.worker-state-authority.v1\n".to_vec();
    raw.extend(
        serde_json::to_vec(&(
            &authority.incarnation,
            authority.epoch,
            &authority.authority_id,
        ))
        .map_err(|e| e.to_string())?,
    );
    Ok(StateBackupAuthority {
        incarnation: authority.incarnation.clone(),
        epoch: authority.epoch,
        fingerprint_sha256: winbridge_update::sha256(&raw),
    })
}
fn scope(command: &WorkerJournalActivationCommand, expected: &WorkerShutdownReceipt) -> Result<()> {
    validate_command_envelope(command)?;
    let b = &command.binding;
    let p = &b.physical;
    if b.update_id != expected.update_id
        || b.launch_id != expected.launch_id
        || b.updater_owner_id != expected.updater_owner_id
        || b.updater_epoch != expected.updater_epoch
        || b.worker_journal_schema != JOURNAL_SCHEMA_VERSION
        || p.context.instance_id != expected.lease.spec.instance_id
        || p.context.journal_incarnation != expected.lease.spec.journal_incarnation
        || p.state != expected.state
        || p.observed_at_ms < expected.accepted_at_ms
        || p.generation == expected.lease.spec.worker_generation
        || p.process == expected.process
        || p.process.process_id == expected.process.process_id
        || p.process.host_id != expected.process.host_id
        || p.process.session_id != expected.process.session_id
    {
        return Err(
            "Journal activation differs from accepted shutdown/physical scope/schema".into(),
        );
    }
    Ok(())
}
fn window(command: &WorkerJournalActivationCommand) -> Result<()> {
    let at = now_ms();
    if at < command.issued_at_ms || at >= command.deadline_at_ms {
        Err("Original journal activation window expired or clock moved backwards".into())
    } else {
        Ok(())
    }
}
fn read(
    c: &Connection,
    command: &WorkerJournalActivationCommand,
) -> Result<Option<WorkerJournalActivationReceipt>> {
    use rusqlite::OptionalExtension;
    let schema: u32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(sql)?;
    if schema != JOURNAL_SCHEMA_VERSION {
        return Err("Journal activation requires exact current schema; no migration".into());
    }
    let row: Option<(String,String,String,String)> = c.query_row(
        "SELECT command_id,command_sha256,command,receipt FROM worker_journal_activations WHERE update_id=?1",
        [&command.binding.update_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).optional().map_err(sql)?;
    let Some((key, hash, payload, raw)) = row else {
        return Ok(None);
    };
    if key != command.command_id
        || hash != command_sha256(command)?
        || payload.len() > 32768
        || raw.len() > 32768
        || payload != encode(command)?
    {
        return Err("Journal activation key/payload conflict; no second authority".into());
    }
    let receipt: WorkerJournalActivationReceipt =
        serde_json::from_str(&raw).map_err(|_| "Corrupt journal activation receipt")?;
    validate_receipt(&receipt, command)?;
    let shutdown_command: String = c
        .query_row(
            "SELECT command FROM worker_shutdown_receipts WHERE update_id=?1",
            [&command.binding.update_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let shutdown_command =
        serde_json::from_str(&shutdown_command).map_err(|_| "Corrupt accepted shutdown command")?;
    let accepted = super::shutdown::read(c, &command.binding.update_id, &shutdown_command)?
        .ok_or("Accepted shutdown missing from activation history")?;
    scope(command, &accepted)?;
    let header = Store::read_authority(c)?;
    if header.incarnation != receipt.authority.incarnation
        || header.epoch < receipt.authority.epoch
        || header.epoch == receipt.authority.epoch && stamp(&header)? != receipt.authority
    {
        return Err("Journal activation receipt differs from current authority history".into());
    }
    Ok(Some(receipt))
}

impl Store {
    pub fn state_backup_authority(&self) -> Result<StateBackupAuthority> {
        self.check_authority()?;
        stamp(&self.authority)
    }
    pub(super) fn preflight_activation_migration(path: &Path) -> Result<()> {
        // try_exists follows symlinks: a dangling one must not become permission
        // to create a new database at an unrelated target.
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            winbridge_platform::require_safe_path(Path::new(&sidecar))
                .map_err(|e| e.to_string())?;
        }
        let c = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sql)?;
        c.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(sql)?;
        let version: u32 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        if version > JOURNAL_SCHEMA_VERSION {
            return Err(format!("journal schema {version} is unsupported; expected {JOURNAL_SCHEMA_VERSION}; no writable open or migration"));
        }
        if (15..JOURNAL_SCHEMA_VERSION).contains(&version) {
            Self::reject_pending_update_start(path)?;
        }
        Ok(())
    }
    pub(super) fn backup_activation_migration(
        path: &Path,
        c: &Connection,
    ) -> Result<ActivationMigrationBackup> {
        Self::require_update_activation(c)?;
        let original = Self::read_authority(c)?;
        let backup = path.with_file_name(format!(
            "journal-v16-{}-{}.sqlite",
            now_ms(),
            uuid::Uuid::new_v4()
        ));
        winbridge_platform::require_safe_path(&backup).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(&backup).map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0x1 | 0x2);
        }
        let file = options.open(&backup).map_err(|e| e.to_string())?;
        let guard = ActivationMigrationBackup {
            authority: original.clone(),
            identity: winbridge_platform::file_identity(&file).map_err(|e| e.to_string())?,
            file,
            path: backup.clone(),
        };
        guard.file.sync_all().map_err(|e| e.to_string())?;
        {
            let mut target = Connection::open_with_flags(
                &backup,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(sql)?;
            guard.revalidate()?;
            rusqlite::backup::Backup::new(c, &mut target)
                .map_err(sql)?
                .run_to_completion(64, std::time::Duration::from_millis(10), None)
                .map_err(sql)?;
        }
        guard.revalidate()?;
        let snapshot =
            Connection::open_with_flags(&backup, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(sql)?;
        let version: u32 = snapshot
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        let integrity: String = snapshot
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(sql)?;
        if version != 16 || integrity != "ok" || Self::read_authority(&snapshot)? != original {
            return Err("Activation migration backup verification failed; source preserved".into());
        }
        drop(snapshot);
        guard.file.sync_all().map_err(|e| e.to_string())?;
        winbridge_platform::source_files::sync_parent(&backup).map_err(|e| e.to_string())?;
        guard.revalidate()?;
        eprintln!("Journal migration consistent backup: {}", backup.display());
        Ok(guard)
    }
    pub(super) fn migrate_activation(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE worker_journal_activations(update_id TEXT PRIMARY KEY REFERENCES worker_update_leases(id),command_id TEXT NOT NULL UNIQUE,command_sha256 TEXT NOT NULL CHECK(length(command_sha256)=64),command TEXT NOT NULL CHECK(json_valid(command)),receipt TEXT NOT NULL CHECK(json_valid(receipt)));
CREATE TRIGGER worker_journal_activation_immutable BEFORE UPDATE ON worker_journal_activations BEGIN SELECT RAISE(ABORT,'Journal activation receipt immutable');END;
CREATE TRIGGER worker_journal_activation_retained BEFORE DELETE ON worker_journal_activations BEGIN SELECT RAISE(ABORT,'Journal activation receipt retained');END;
DROP TRIGGER authority_epoch_guard;
CREATE TRIGGER authority_epoch_guard BEFORE UPDATE ON journal_authority WHEN NEW.singleton!=OLD.singleton OR NEW.incarnation!=OLD.incarnation OR NEW.epoch!=OLD.epoch+1 OR NEW.authority=OLD.authority BEGIN SELECT RAISE(ABORT,'Authority activation requires same incarnation, next epoch and new identity');END;
PRAGMA user_version=17;").map_err(sql)
    }
    /// Trusted handler must first validate the current updater graph, actual
    /// private child, exact acquired instance lock and original monotonic window.
    /// `checkpoint` repeats those live checks before writes and before commit.
    /// This method itself grants no process startup, migration or lease release.
    pub fn activate_journal_authority_checked(
        &mut self,
        command: &WorkerJournalActivationCommand,
        expected: &WorkerShutdownReceipt,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<JournalActivationAcceptance> {
        scope(command, expected)?;
        let previous = self.authority.clone();
        let tx = self.coordinated_update_transaction()?;
        if super::shutdown::read(&tx, &expected.update_id, &expected.command)?.as_ref()
            != Some(expected)
        {
            return Err("Exact immutable accepted shutdown receipt missing/different".into());
        }
        if let Some(receipt) = read(&tx, command)? {
            return Ok(JournalActivationAcceptance::Observed(Box::new(receipt)));
        }
        if stamp(&previous)? != command.binding.expected_authority
            || super::update::read(&tx, &expected.update_id)? != expected.lease
            || expected.lease.phase != WorkerUpdatePhase::Quiesced
        {
            return Err("Fresh exact old authority/quiesced lease required for activation".into());
        }
        let next = JournalAuthority {
            incarnation: previous.incarnation.clone(),
            epoch: previous.epoch + 1,
            authority_id: uuid::Uuid::new_v4().to_string(),
        };
        checkpoint()?;
        window(command)?;
        one(tx.execute("UPDATE journal_authority SET epoch=?1,authority=?2 WHERE singleton=1 AND incarnation=?3 AND epoch=?4 AND authority=?5",
            params![next.epoch as i64,next.authority_id,previous.incarnation,previous.epoch as i64,previous.authority_id]).map_err(sql)?)?;
        if Self::read_authority(&tx)? != next {
            return Err("Authority update/readback mismatch".into());
        }
        let receipt = WorkerJournalActivationReceipt {
            schema_version: 1,
            command_id: command.command_id.clone(),
            update_id: command.binding.update_id.clone(),
            attempt_id: command.binding.attempt_id.clone(),
            command_sha256: command_sha256(command)?,
            authority: stamp(&next)?,
            committed_at_ms: now_ms(),
        };
        validate_receipt(&receipt, command)?;
        one(tx
            .execute(
                "INSERT INTO worker_journal_activations VALUES(?1,?2,?3,?4,?5)",
                params![
                    receipt.update_id,
                    receipt.command_id,
                    receipt.command_sha256,
                    encode(command)?,
                    encode(&receipt)?
                ],
            )
            .map_err(sql)?)?;
        if read(&tx, command)?.as_ref() != Some(&receipt) {
            return Err("Activation receipt write/readback mismatch".into());
        }
        checkpoint()?;
        window(command)?;
        tx.commit().map_err(|e| format!("Journal activation commit outcome unconfirmed; inspect receipt/header, no automatic retry: {e}"))?;
        self.authority = next;
        Ok(JournalActivationAcceptance::Fresh(Box::new(
            JournalActivationPermit { receipt },
        )))
    }
    /// Historical data only; a duplicate/cold receipt never mints a permit.
    pub fn journal_activation_receipt(
        &self,
        command: &WorkerJournalActivationCommand,
    ) -> Result<Option<WorkerJournalActivationReceipt>> {
        validate_command_envelope(command)?;
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        let result = read(&tx, command)?;
        tx.commit().map_err(sql)?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "activation_tests.rs"]
mod tests;
