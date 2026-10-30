//! Append an independently observed Active fact, never another release or a
//! keeper grant. Only the original physical owner can mint the fresh permit.
use super::*;

/// No serde/clone/public constructor. The consuming physical owner will mint
/// this only after normal IPC authenticates its original live child/Job.
pub struct FinalizationConfirmationPermit {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) query: ActivationFinalizationQuery,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) observation: ActivationFinalizationObservation,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) observation_deadline: std::time::Instant,
}

#[cfg(any(windows, test))]
pub(super) fn confirmation(
    old: &ActivationFinalizationRecord,
    shutdown: &crate::shutdown::ShutdownRecord,
    permit: &FinalizationConfirmationPermit,
    now: u64,
) -> Result<Option<ActivationFinalizationRecord>> {
    use crate::activation::finalization_observation;
    old.validate().map_err(bad)?;
    if now < old.updated_at_ms || now > winbridge_core::update::MAX_UPDATE_COUNTER {
        return Err(bad(
            "Finalization confirmation clock predates retained history or exceeds contract",
        ));
    }
    if std::time::Instant::now() >= permit.observation_deadline {
        return Err(bad(
            "Original Active observation budget expired; observe again, no new release",
        ));
    }
    if permit.query.command != old.command {
        return Err(bad("Active confirmation changed the original decision"));
    }
    finalization_observation::validate_observation(&permit.query, shutdown, &permit.observation)
        .map_err(bad)?;
    let ActivationFinalizationObservationResult::Observed {
        receipt,
        observed_at_ms,
    } = &permit.observation.result
    else {
        return Err(bad("Independent Active fact missing"));
    };
    if now < *observed_at_ms
        || now.saturating_sub(*observed_at_ms) > winbridge_transport::IPC_TIMEOUT.as_millis() as u64
    {
        return Err(bad(
            "Active confirmation current observation expired or clock moved backwards",
        ));
    }
    if let ActivationFinalizationState::Confirmed { receipt: saved } = &old.state {
        if saved != receipt {
            return Err(bad("Terminal Active receipt is immutable"));
        }
        return Ok(None); // The same saved fact is data, not a new permit/grant.
    }
    old.confirmed(receipt.as_ref().clone(), shutdown, now)
        .map(Some)
        .map_err(bad)
}

#[cfg(any(windows, test))]
pub(super) fn append_record(
    c: &Connection,
    a: &UpdateAuthority,
    graph: &ActivationLifetimeSnapshot,
    shutdown: &crate::shutdown::ShutdownRecord,
    next: &ActivationFinalizationRecord,
    mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
) -> Result<()> {
    check_authority(c, a)?;
    let id = &next.command.binding.lifetime_command.binding.update_id;
    let (_, old) = ledger(c, id, graph, shutdown)?;
    let b = &old.command.binding;
    if a.owner_id() != b.lifetime_command.binding.updater_owner_id
        || a.epoch() != b.lifetime_command.binding.updater_epoch
        || a.installation_id != b.installation_id
        || a.scope_sha256 != b.scope_sha256
    {
        return Err(bad(
            "Finalization append changed original authority/installation",
        ));
    }
    old.advance(next, shutdown).map_err(bad)?;
    checkpoint().map_err(bad)?;
    check_authority(c, a)?;
    let raw = encode(next)?;
    one(c.execute("UPDATE activation_finalization_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",
        params![next.revision as i64, raw, id, old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_finalization_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, id, graph, shutdown)?.1 != *next {
        return Err(bad("Finalization append readback differs"));
    }
    checkpoint().map_err(bad)?;
    check_authority(c, a)
}

#[cfg(windows)]
fn require_coordinator(
    c: &Connection,
    a: &UpdateAuthority,
    expected: &SupervisorRecord,
    command: &WorkerFinalizationCommand,
) -> Result<()> {
    check_authority(c, a)?;
    let b = &command.binding;
    let owner = &b.lifetime_command.binding;
    let current = supervisor::read(c, &owner.launch_id)?;
    if schema(c)? != UPDATE_SCHEMA_VERSION
        || current != *expected
        || current.phase != SupervisorPhase::Active
        || current.spec.update_id != owner.update_id
        || current.spec.launch_id != owner.launch_id
        || current.owner_id.as_deref() != Some(a.owner_id())
        || current.owner_epoch != Some(a.epoch())
        || current.process.as_ref() != Some(&owner.keeper)
        || a.owner_id() != owner.updater_owner_id
        || a.epoch() != owner.updater_epoch
        || a.installation_id != b.installation_id
        || a.scope_sha256 != b.scope_sha256
    {
        return Err(bad(
            "Finalization completion requires the exact original active coordinator",
        ));
    }
    Ok(())
}

#[cfg(windows)]
impl UpdateJournal {
    /// Record a fresh independently authenticated fact under the original
    /// owner. This neither resends V8/release nor converts the process to keeper.
    pub fn confirm_activation_finalization(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: FinalizationConfirmationPermit,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<ActivationFinalizationRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let saved = snapshot(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("No original committed finalization to confirm"))?;
        let command = &saved.record.command;
        require_coordinator(&tx, a, s, command)?;
        let fresh = || confirmation(&saved.record, &saved.shutdown, &permit, now_ms()?);
        let next = fresh()?;
        let mut checked = || {
            checkpoint()?;
            fresh().map_err(|e| e.to_string())?;
            require_coordinator(&tx, a, s, command).map_err(|e| e.to_string())
        };
        checked().map_err(bad)?;
        let result = if let Some(next) = next {
            append_record(
                &tx,
                a,
                &saved.lifetime,
                &saved.shutdown,
                &next,
                &mut checked,
            )?;
            next
        } else {
            saved.record.clone()
        };
        checked().map_err(bad)?;
        tx.commit().map_err(|e| storage(format!("Finalization confirmation COMMIT unconfirmed; inspect/observe same decision without release: {e}")))?;
        Ok(result)
    }

    /// Diagnostic write only, also valid when the retained child has died.
    /// Original owner/scope are still required; terminal success is never erased.
    pub fn mark_activation_finalization_unknown(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        command: &WorkerFinalizationCommand,
        reason: String,
    ) -> Result<ActivationFinalizationRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require_coordinator(&tx, a, s, command)?;
        let saved = snapshot(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("No original finalization to mark unknown"))?;
        if saved.record.command != *command {
            return Err(bad("Unknown outcome belongs to another finalization"));
        }
        let result = if matches!(
            saved.record.state,
            ActivationFinalizationState::ReleasePending {}
        ) {
            let next = saved.record.indeterminate(reason, now_ms()?).map_err(bad)?;
            append_record(&tx, a, &saved.lifetime, &saved.shutdown, &next, || {
                require_coordinator(&tx, a, s, command).map_err(|e| e.to_string())
            })?;
            next
        } else {
            saved.record
        };
        require_coordinator(&tx, a, s, command)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}
