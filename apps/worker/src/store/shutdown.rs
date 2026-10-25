//! Durable acceptance only. This module never stops a process or clears a lease.
use super::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use winbridge_core::update::{valid_hash, validate_update_key, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::*;

fn one(n: usize) -> Result<()> {
    if n == 1 {
        Ok(())
    } else {
        Err("Shutdown receipt write ignored; no ACK/stop permitted".into())
    }
}
fn encode(v: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}
fn valid(r: &WorkerShutdownReceipt) -> Result<()> {
    super::update::validate(&r.lease.spec)?;
    for id in [&r.update_id, &r.launch_id, &r.updater_owner_id] {
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid shutdown receipt identity")?;
    }
    validate_update_key(&r.command.command_id)?;
    let state = &r.state;
    if r.schema_version != 1
        || r.update_id != r.lease.spec.update_id
        || r.updater_owner_id != r.lease.spec.updater_owner_id
        || r.updater_epoch != r.lease.spec.updater_epoch
        || r.lease.phase != WorkerUpdatePhase::Quiesced
        || r.command.expected_lease_revision != r.lease.revision
        || r.lease.revision == 0
        || r.lease.revision >= MAX_UPDATE_COUNTER
        || r.lease.accepted_at_ms > r.lease.updated_at_ms
        || r.lease.updated_at_ms > MAX_UPDATE_COUNTER
        || r.lease.reason.len() > 4096
        || !valid_hash(&r.command.intent_sha256, &[64])
        || r.command.reason.is_empty()
        || r.command.reason.len() > 4096
        || r.command
            .reason
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        || r.accepted_at_ms == 0
        || r.accepted_at_ms > MAX_UPDATE_COUNTER
        || r.process.process_id == 0
        || r.process.birth.is_empty()
        || r.process.birth.len() > 64
        || !r.process.birth.bytes().all(|c| c.is_ascii_digit())
        || !valid_hash(&r.process.host_id, &[64])
        || state.state_directory.is_empty()
        || state.state_directory.len() > 4096
        || state.state_directory.chars().any(char::is_control)
        || [
            &state.directory_identity,
            &state.journal_identity,
            &state.lock_identity,
        ]
        .into_iter()
        .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        || state.directory_identity == state.journal_identity
        || state.directory_identity == state.lock_identity
        || state.journal_identity == state.lock_identity
    {
        return Err("Shutdown receipt scope/command/process/state invalid".into());
    }
    Ok(())
}
fn read(
    c: &Connection,
    id: &str,
    command: &WorkerShutdownCommand,
) -> Result<Option<WorkerShutdownReceipt>> {
    let row: Option<(String, String)> = c
        .query_row(
            "SELECT command,record FROM worker_shutdown_receipts WHERE update_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    let Some((payload, raw)) = row else {
        return Ok(None);
    };
    if raw.len() > 32 * 1024 || payload != encode(command)? {
        return Err("Shutdown command ID/payload differs; no replay".into());
    }
    let receipt: WorkerShutdownReceipt =
        serde_json::from_str(&raw).map_err(|_| "Corrupt shutdown receipt")?;
    valid(&receipt)?;
    if receipt.update_id != id || receipt.command != *command {
        return Err("Shutdown receipt/index mismatch".into());
    }
    let lease = super::update::read(c, id)?;
    if lease.spec != receipt.lease.spec || lease.revision < receipt.lease.revision {
        return Err("Shutdown history differs from its retained lease scope".into());
    }
    // Read the original immutable ConfirmQuiesced receipt, not a later live
    // phase. A future activation/release must not invalidate historical data or
    // create a fresh stop permit from it.
    let mut q=c.prepare("SELECT payload,receipt FROM worker_update_commands WHERE update_id=?1 AND json_extract(receipt,'$.revision')=?2 LIMIT 2").map_err(sql)?;
    let rows = q
        .query_map(params![id, receipt.lease.revision as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql)?;
    if rows.len() != 1 {
        return Err("Original shutdown quiescence receipt missing/ambiguous".into());
    }
    let command: WorkerUpdateCommand =
        serde_json::from_str(&rows[0].0).map_err(|_| "Corrupt original quiescence command")?;
    let original: WorkerUpdateLease =
        serde_json::from_str(&rows[0].1).map_err(|_| "Corrupt original quiescence receipt")?;
    validate_update_key(&command.command_id)?;
    if command.action != WorkerUpdateAction::ConfirmQuiesced
        || command.expected_revision.checked_add(1) != Some(receipt.lease.revision)
        || command.source_manifest_sha256.is_some()
        || command.reason != receipt.lease.reason
        || original != receipt.lease
    {
        return Err("Shutdown original quiescence command/receipt differs".into());
    }
    Ok(Some(receipt))
}
pub(super) fn require_no_accepted_shutdown(c: &Connection) -> Result<()> {
    let pending:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM worker_shutdown_receipts r JOIN worker_update_leases l ON l.id=r.update_id WHERE l.active=1)",[],|r|r.get(0)).map_err(sql)?;
    if pending {
        Err("Worker shutdown accepted; ordinary lease changes/release are blocked".into())
    } else {
        Ok(())
    }
}
/// A fresh durable acceptance; serialized history cannot recreate this value.
pub struct ShutdownAcceptancePermit {
    receipt: WorkerShutdownReceipt,
}
impl ShutdownAcceptancePermit {
    pub fn receipt(&self) -> &WorkerShutdownReceipt {
        &self.receipt
    }
}
pub enum ShutdownAcceptance {
    Fresh(Box<ShutdownAcceptancePermit>),
    Observed(Box<WorkerShutdownReceipt>),
}
impl Store {
    pub(super) fn migrate_shutdown(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE worker_shutdown_receipts(update_id TEXT PRIMARY KEY REFERENCES worker_update_leases(id),command TEXT NOT NULL CHECK(json_valid(command)),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER worker_shutdown_immutable BEFORE UPDATE ON worker_shutdown_receipts BEGIN SELECT RAISE(ABORT,'Shutdown receipt immutable');END;
CREATE TRIGGER worker_shutdown_retained BEFORE DELETE ON worker_shutdown_receipts BEGIN SELECT RAISE(ABORT,'Shutdown receipt retained');END;
PRAGMA user_version=16;").map_err(sql)
    }
    /// A trusted control handler constructs the receipt after checking both
    /// journals and physical scope. A saved duplicate never grants a fresh stop.
    pub fn accept_worker_shutdown(
        &mut self,
        mut receipt: WorkerShutdownReceipt,
    ) -> Result<ShutdownAcceptance> {
        valid(&receipt)?;
        self.guard_callback(None, None)?;
        let authority = self.authority.clone();
        let tx = self.coordinated_update_transaction()?;
        if let Some(old) = read(&tx, &receipt.update_id, &receipt.command)? {
            return Ok(ShutdownAcceptance::Observed(Box::new(old)));
        }
        if receipt.lease.spec.journal_incarnation != authority.incarnation
            || super::update::read(&tx, &receipt.update_id)? != receipt.lease
        {
            return Err("Fresh exact quiesced lease required before shutdown acceptance".into());
        }
        receipt.accepted_at_ms = now_ms();
        valid(&receipt)?;
        one(tx
            .execute(
                "INSERT INTO worker_shutdown_receipts VALUES(?1,?2,?3)",
                params![
                    receipt.update_id,
                    encode(&receipt.command)?,
                    encode(&receipt)?
                ],
            )
            .map_err(sql)?)?;
        if read(&tx, &receipt.update_id, &receipt.command)?.as_ref() != Some(&receipt) {
            return Err("Shutdown receipt write/readback mismatch".into());
        }
        tx.commit().map_err(sql)?;
        Ok(ShutdownAcceptance::Fresh(Box::new(
            ShutdownAcceptancePermit { receipt },
        )))
    }
    /// Read current-schema immutable data without owner acquisition/migration.
    /// Config/path/process binding is additionally checked by the consuming
    /// control/readonly verifier; this does not prove that the worker exited.
    pub fn worker_shutdown_receipt(
        &self,
        id: &str,
        command: &WorkerShutdownCommand,
    ) -> Result<Option<WorkerShutdownReceipt>> {
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        let r = read(&tx, id, command)?;
        if r.as_ref()
            .is_some_and(|r| r.lease.spec.journal_incarnation != self.authority.incarnation)
        {
            return Err("Shutdown receipt belongs to another journal incarnation".into());
        }
        tx.commit().map_err(sql)?;
        Ok(r)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{fs, path::Path};
    // Store-only metadata fixture: no physical exit or updater authority claimed.
    pub(crate) fn ready(path: &Path) -> (Store, WorkerShutdownReceipt, WorkerUpdateCommand) {
        let mut s = Store::open(path).unwrap();
        s.begin_authority().unwrap();
        let spec = WorkerUpdateSpec {
            update_id: uuid::Uuid::new_v4().to_string(),
            instance_id: "shutdown-test".into(),
            journal_incarnation: s.journal_authority().incarnation.clone(),
            worker_generation: uuid::Uuid::new_v4().to_string(),
            updater_owner_id: uuid::Uuid::new_v4().to_string(),
            updater_epoch: 2,
            source_directory: "synthetic-source".into(),
            baseline_manifest_sha256: "b".repeat(64),
            target_manifest_sha256: "c".repeat(64),
            projects: vec!["source".into()],
        };
        s.begin_worker_update(spec.clone()).unwrap();
        let cmd = |n: usize| WorkerUpdateCommand {
            command_id: format!("step-{n}"),
            expected_revision: n as u64,
            action: [
                WorkerUpdateAction::AcquireSource,
                WorkerUpdateAction::BeginDrain,
                WorkerUpdateAction::CloseDispatch,
                WorkerUpdateAction::ConfirmQuiesced,
            ][n],
            source_manifest_sha256: None,
            reason: "Metadata fixture only".into(),
        };
        let mut lease = None;
        for n in 0..4 {
            lease = Some(
                s.advance_worker_update(
                    &spec,
                    &cmd(n),
                    WorkerUpdateBlockers {
                        accepted_operations: 0,
                        logical_owners: 0,
                        in_flight_effects: 0,
                        physical_owners: 0,
                        uncertain_owners: 0,
                    },
                )
                .unwrap(),
            );
        }
        let receipt = WorkerShutdownReceipt {
            schema_version: 1,
            update_id: spec.update_id.clone(),
            launch_id: uuid::Uuid::new_v4().to_string(),
            updater_owner_id: spec.updater_owner_id.clone(),
            updater_epoch: 2,
            command: WorkerShutdownCommand {
                command_id: "shutdown-fixture".into(),
                expected_lease_revision: 4,
                intent_sha256: "d".repeat(64),
                reason: "Fixture".into(),
            },
            lease: lease.unwrap(),
            process: winbridge_platform::infrastructure::stamp(std::process::id()).unwrap(),
            state: WorkerShutdownState {
                state_directory: "synthetic-state".into(),
                directory_identity: "dir".into(),
                journal_identity: "db".into(),
                lock_identity: "lock".into(),
            },
            accepted_at_ms: 1,
        };
        (s, receipt, cmd(0))
    }
    #[test]
    fn ignored_shutdown_receipt_never_mints_permit_then_duplicate_is_observation_only() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let (mut s, r, source_command) = ready(&path);
        s.connection.execute_batch("CREATE TRIGGER deny_shutdown BEFORE INSERT ON worker_shutdown_receipts BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        assert!(s.accept_worker_shutdown(r.clone()).is_err());
        assert!(s
            .worker_shutdown_receipt(&r.update_id, &r.command)
            .unwrap()
            .is_none());
        assert_eq!(s.active_worker_update().unwrap(), Some(r.lease.clone()));
        s.connection
            .execute_batch("DROP TRIGGER deny_shutdown;")
            .unwrap();
        let ShutdownAcceptance::Fresh(permit) = s.accept_worker_shutdown(r.clone()).unwrap() else {
            panic!("Fresh commit required")
        };
        let saved = permit.receipt().clone();
        assert!(saved.accepted_at_ms > 1);
        let ShutdownAcceptance::Observed(old) = s.accept_worker_shutdown(r.clone()).unwrap() else {
            panic!("Saved receipt must not recreate permit")
        };
        assert_eq!(*old, saved);
        let read_only = Store::open_read_only(&path).unwrap();
        assert_eq!(
            read_only
                .worker_shutdown_receipt(&r.update_id, &r.command)
                .unwrap(),
            Some(saved.clone())
        );
        assert!(s
            .connection
            .execute("UPDATE worker_shutdown_receipts SET record=record", [])
            .is_err());
        assert!(s
            .connection
            .execute("DELETE FROM worker_shutdown_receipts", [])
            .is_err());
        let release = WorkerUpdateCommand {
            command_id: "forbidden-release".into(),
            expected_revision: 4,
            action: WorkerUpdateAction::Release,
            source_manifest_sha256: Some(r.lease.spec.target_manifest_sha256.clone()),
            reason: "No handoff proof".into(),
        };
        let empty = || WorkerUpdateBlockers {
            accepted_operations: 0,
            logical_owners: 0,
            in_flight_effects: 0,
            physical_owners: 0,
            uncertain_owners: 0,
        };
        assert!(s
            .advance_worker_update(&r.lease.spec, &release, empty())
            .unwrap_err()
            .contains("shutdown accepted"));
        assert_eq!(
            s.advance_worker_update(&r.lease.spec, &source_command, empty())
                .unwrap()
                .phase,
            WorkerUpdatePhase::SourceHeld
        );
        assert_eq!(s.active_worker_update().unwrap(), Some(r.lease));
    }
    #[test]
    fn wrong_shutdown_lease_epoch_or_payload_cannot_be_accepted_or_rebound() {
        let root = tempfile::tempdir().unwrap();
        let (mut s, r, _) = ready(&root.path().join("journal.sqlite"));
        let mut bad = r.clone();
        bad.updater_epoch += 1;
        bad.lease.spec.updater_epoch += 1;
        assert!(s.accept_worker_shutdown(bad).is_err());
        let mut bad = r.clone();
        bad.lease.revision += 1;
        bad.command.expected_lease_revision += 1;
        assert!(s.accept_worker_shutdown(bad).is_err());
        s.accept_worker_shutdown(r.clone()).unwrap();
        let mut bad = r.command.clone();
        bad.reason.push('x');
        assert!(s.worker_shutdown_receipt(&r.update_id, &bad).is_err());
    }
    #[test]
    fn saved_shutdown_remains_history_after_a_later_explicit_handoff() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let (mut s, r, _) = ready(&path);
        let ShutdownAcceptance::Fresh(p) = s.accept_worker_shutdown(r.clone()).unwrap() else {
            panic!("Fresh receipt")
        };
        let saved = p.receipt().clone();
        // Metadata-only simulation of a FUTURE checked activation. Ordinary
        // release remains blocked above; this test creates no execution grant.
        let mut later = r.lease.clone();
        later.revision += 1;
        later.phase = WorkerUpdatePhase::Released;
        later.reason = "Synthetic later handoff".into();
        s.connection.execute("UPDATE worker_update_leases SET revision=?1,phase=?2,active=0,record=?3 WHERE id=?4",params![later.revision as i64,encode(&later.phase).unwrap(),encode(&later).unwrap(),r.update_id]).unwrap();
        assert_eq!(
            s.worker_shutdown_receipt(&r.update_id, &r.command).unwrap(),
            Some(saved.clone())
        );
        assert!(matches!(
            s.accept_worker_shutdown(r.clone()).unwrap(),
            ShutdownAcceptance::Observed(_)
        ));
        drop(s);
        let read_only = Store::open_read_only(&path).unwrap();
        assert_eq!(
            read_only
                .worker_shutdown_receipt(&r.update_id, &r.command)
                .unwrap(),
            Some(saved)
        );
    }
    #[test]
    fn schema_fifteen_migration_backups_but_refuses_active_update_without_writing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        let authority = s.journal_authority().clone();
        s.connection.execute_batch("DROP TABLE worker_shutdown_receipts;PRAGMA user_version=15;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.journal_authority(), &authority);
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM worker_shutdown_receipts", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        let backup = fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("journal-v15-")
            })
            .unwrap();
        let c = Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            15
        );
        assert_eq!(
            c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(s);
        let active_root = tempfile::tempdir().unwrap();
        let active_path = active_root.path().join("journal.sqlite");
        let (s, _, _) = ready(&active_path);
        s.connection.execute_batch("DROP TABLE worker_shutdown_receipts;PRAGMA user_version=15;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(s);
        let before = fs::read(&active_path).unwrap();
        assert!(Store::open(&active_path)
            .err()
            .unwrap()
            .contains("unfinished"));
        assert_eq!(fs::read(active_path).unwrap(), before);
        assert!(!fs::read_dir(active_root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("journal-v15-")));
    }
}
