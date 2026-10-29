//! Same-connection release beneath the future authenticated V8 coordinator.
//! No IPC route calls this yet. SQL completion is not an Active-mode receipt.
#![cfg_attr(not(test), allow(dead_code))]

use super::{activation_service_open::require_service_history, now_ms, sql, Result, Store};
use rusqlite::{params, Connection};
use winbridge_core::update::MAX_UPDATE_COUNTER;
use winbridge_protocol::update::*;
use winbridge_update::activation::{finalization, service};

/// A known successful COMMIT on the existing connection. Neither a saved
/// receipt nor a deserialized command can recreate this in-memory value.
pub(crate) struct ActivatedReleaseCommit {
    command: WorkerFinalizationCommand,
    lease: WorkerUpdateLease,
}
impl ActivatedReleaseCommit {
    pub(crate) fn command(&self) -> &WorkerFinalizationCommand {
        &self.command
    }
    pub(crate) fn lease(&self) -> &WorkerUpdateLease {
        &self.lease
    }
}
pub(crate) enum ActivatedReleaseAdmission {
    Committed(Box<ActivatedReleaseCommit>),
    Observed(Box<WorkerUpdateLease>),
}

fn binding(
    service: &WorkerServiceActivationCommand,
    shutdown: &WorkerShutdownReceipt,
    command: &WorkerFinalizationCommand,
) -> Result<WorkerUpdateCommand> {
    service::validate_command_envelope(service)?;
    finalization::validate_command_envelope(command)?;
    let old = &command.binding.lifetime_command;
    let b = &old.binding;
    let writer = &service.binding.writer_command;
    let physical = &writer.binding.physical;
    let compatibility = &b.compatibility;
    if b.update_id != shutdown.update_id
        || b.update_id != writer.binding.update_id
        || b.attempt_id != writer.binding.attempt_id
        || b.launch_id != shutdown.launch_id
        || b.launch_id != writer.binding.launch_id
        || b.updater_owner_id != shutdown.updater_owner_id
        || b.updater_owner_id != writer.binding.updater_owner_id
        || b.updater_epoch != shutdown.updater_epoch
        || b.updater_epoch != writer.binding.updater_epoch
        || b.authority != service.binding.writer_receipt.authority
        || compatibility.context != physical.context
        || compatibility.generation != physical.generation
        || compatibility.identity != service.binding.expected_worker
        || compatibility.worker_config_sha256 != writer.binding.worker_config_sha256
        || compatibility.worker_config_schema != service.binding.worker_config_schema
        || compatibility.worker_journal_schema != writer.binding.worker_journal_schema
        || compatibility.updater_journal_schema != writer.binding.updater_journal_schema
        || compatibility.protocol_version != service.binding.protocol_version
        || old.deadline_at_ms != service.deadline_at_ms
        || old.issued_at_ms < service.issued_at_ms
        || command.issued_at_ms < shutdown.accepted_at_ms
        || command.binding.expected_lease_revision != shutdown.lease.revision
        || command.binding.source_manifest_sha256 != shutdown.lease.spec.target_manifest_sha256
        || [
            &writer.command_id,
            &service.command_id,
            &shutdown.command.command_id,
        ]
        .contains(&&command.command_id)
    {
        return Err("Finalization differs from the same service/writer/shutdown scope".into());
    }
    finalization::release_command(command)
}

fn released(
    old: &WorkerShutdownReceipt,
    command: &WorkerUpdateCommand,
    at: u64,
) -> Result<WorkerUpdateLease> {
    if at < old.accepted_at_ms || at < old.lease.updated_at_ms || at > MAX_UPDATE_COUNTER {
        return Err("Finalization release clock moved backwards or exhausted".into());
    }
    Ok(WorkerUpdateLease {
        phase: WorkerUpdatePhase::Released,
        revision: old.lease.revision + 1,
        updated_at_ms: at,
        reason: command.reason.clone(),
        ..old.lease.clone()
    })
}

fn observe(
    c: &Connection,
    service: &WorkerServiceActivationCommand,
    shutdown: &WorkerShutdownReceipt,
    command: &WorkerFinalizationCommand,
) -> Result<Option<WorkerUpdateLease>> {
    let release = binding(service, shutdown, command)?;
    require_service_history(c, service, shutdown)?;
    let foreign_active: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM worker_update_leases WHERE active=1 AND id!=?1)",
            [&shutdown.update_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if foreign_active {
        return Err("Another active update cannot replace a pending finalization".into());
    }
    let current = super::update::read(c, &shutdown.update_id)?;
    let saved = Store::update_receipt_on(c, &shutdown.lease.spec, &release)?;
    match saved {
        None if current == shutdown.lease => Ok(None),
        Some(lease)
            if lease == current
                && lease == released(shutdown, &release, lease.updated_at_ms)?
                && lease.updated_at_ms >= command.issued_at_ms =>
        {
            Ok(Some(lease))
        }
        _ => {
            Err("Finalization lease/release receipt differs from the exact pending decision".into())
        }
    }
}

fn one(n: usize) -> Result<()> {
    if n != 1 {
        return Err("Finalization release write ignored; no COMMIT permit".into());
    }
    Ok(())
}

impl Store {
    /// Read facts in one snapshot, including the SQL/mode gap after a known or
    /// uncertain COMMIT. This does not retry a write or mint a fresh permit.
    pub(crate) fn observe_service_finalization_release(
        &self,
        service: &WorkerServiceActivationCommand,
        shutdown: &WorkerShutdownReceipt,
        command: &WorkerFinalizationCommand,
    ) -> Result<Option<WorkerUpdateLease>> {
        self.check_authority()?;
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        self.check_authority()?;
        let saved = observe(&tx, service, shutdown, command)?;
        tx.commit().map_err(sql)?;
        Ok(saved)
    }

    /// Internal effect only: the owning coordinator must already hold the
    /// Store/mode boundary in ReleasePending, authenticate the original live
    /// keeper and child, and validate the committed ledger23 decision. Its
    /// checkpoint repeats those non-Store checks before and after SQL writes;
    /// it must not reacquire the Store mutex. Data arguments are not a grant.
    /// Ordinary post-shutdown Release remains rejected by update_transaction.
    pub(crate) fn release_activated_service(
        &mut self,
        service: &WorkerServiceActivationCommand,
        shutdown: &WorkerShutdownReceipt,
        command: &WorkerFinalizationCommand,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<ActivatedReleaseAdmission> {
        let release = binding(service, shutdown, command)?;
        if self.service_startup.is_some() {
            return Err("Finalization cannot release during service initialization".into());
        }
        checkpoint()?;
        let tx = self.coordinated_update_transaction()?;
        if let Some(saved) = observe(&tx, service, shutdown, command)? {
            checkpoint()?;
            tx.commit().map_err(sql)?;
            return Ok(ActivatedReleaseAdmission::Observed(Box::new(saved)));
        }
        let blockers = Self::update_blockers_on(&tx, None)?;
        if blockers.accepted_operations != 0 || blockers.logical_owners != 0 {
            return Err("Finalization release refuses unexpected accepted work or owners".into());
        }
        if winbridge_core::update::advance_worker_update(shutdown.lease.phase, &release, &blockers)?
            != WorkerUpdatePhase::Released
        {
            return Err("Finalization requires the defined Quiesced -> Released transition".into());
        }
        let at = now_ms();
        if at < command.issued_at_ms {
            return Err("Finalization command is in the future".into());
        }
        let lease = released(shutdown, &release, at)?;
        checkpoint()?;
        one(tx.execute(
            "UPDATE worker_update_leases SET record=?1,revision=?2,phase=?3,active=0 WHERE id=?4 AND revision=?5 AND active=1",
            params![serde_json::to_string(&lease).map_err(|e| e.to_string())?, lease.revision as i64,
                serde_json::to_string(&lease.phase).map_err(|e| e.to_string())?, shutdown.update_id,
                shutdown.lease.revision as i64],
        ).map_err(sql)?)?;
        one(tx.execute(
            "INSERT INTO worker_update_commands(update_id,key,payload,receipt) VALUES(?1,?2,?3,?4)",
            params![shutdown.update_id, release.command_id,
                serde_json::to_string(&release).map_err(|e| e.to_string())?,
                serde_json::to_string(&lease).map_err(|e| e.to_string())?],
        ).map_err(sql)?)?;
        if observe(&tx, service, shutdown, command)? != Some(lease.clone()) {
            return Err("Finalization release readback differs".into());
        }
        checkpoint()?;
        // No in-memory success value exists before SQLite acknowledges COMMIT.
        // A COMMIT error is indeterminate; callers may observe, never redeliver.
        tx.commit().map_err(|e| {
            format!("Finalization COMMIT outcome requires observation; no retry: {e}")
        })?;
        Ok(ActivatedReleaseAdmission::Committed(Box::new(
            ActivatedReleaseCommit {
                command: command.clone(),
                lease,
            },
        )))
    }
}

#[cfg(test)]
#[path = "activation_finalization_tests.rs"]
mod tests;
