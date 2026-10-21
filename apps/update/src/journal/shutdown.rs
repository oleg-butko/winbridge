//! Durable stop outbox. These methods perform database effects only. The future
//! physical driver must additionally pin/validate the live worker and state files.
use super::*;
use crate::shutdown::{
    self as data, ShutdownAdmission, ShutdownBinding, ShutdownDeliveryPermit,
    ShutdownDeliveryState, ShutdownExitGuard, ShutdownIntent, ShutdownIssue, ShutdownPermit,
    ShutdownRecord,
};
use winbridge_protocol::{update::*, LoadedWorkerIdentity};
const ADMIT: &str = "shutdown-admit-v1";
const EXIT: &str = "shutdown-exit-v1";
fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE shutdown_intents(id TEXT PRIMARY KEY REFERENCES drain_intents(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE shutdown_records(id TEXT PRIMARY KEY REFERENCES shutdown_intents(id),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE shutdown_history(id TEXT NOT NULL REFERENCES shutdown_records(id),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER shutdown_intent_immutable BEFORE UPDATE ON shutdown_intents BEGIN SELECT RAISE(ABORT,'Shutdown intent immutable');END;
CREATE TRIGGER shutdown_intent_retained BEFORE DELETE ON shutdown_intents BEGIN SELECT RAISE(ABORT,'Shutdown intent retained');END;
CREATE TRIGGER shutdown_record_identity BEFORE UPDATE ON shutdown_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') OR json_extract(OLD.record,'$.delivery.state')='exited' BEGIN SELECT RAISE(ABORT,'Shutdown identity/revision/terminal immutable');END;
CREATE TRIGGER shutdown_record_retained BEFORE DELETE ON shutdown_records BEGIN SELECT RAISE(ABORT,'Shutdown record retained');END;
CREATE TRIGGER shutdown_history_immutable BEFORE UPDATE ON shutdown_history BEGIN SELECT RAISE(ABORT,'Shutdown history immutable');END;
CREATE TRIGGER shutdown_history_retained BEFORE DELETE ON shutdown_history BEGIN SELECT RAISE(ABORT,'Shutdown history retained');END;
PRAGMA user_version=12;
"#).map_err(storage)
}
fn valid_step(old: &ShutdownRecord, next: &ShutdownRecord) -> Result<()> {
    old.validate().map_err(bad)?;
    next.validate().map_err(bad)?;
    let expected = match &next.delivery {
        ShutdownDeliveryState::Issued { at_ms } => old.issue(*at_ms),
        ShutdownDeliveryState::Uncertain { at_ms, reason } => old.uncertain(*at_ms, reason.clone()),
        ShutdownDeliveryState::Acknowledged { at_ms, receipt } => {
            old.acknowledge(*at_ms, receipt.as_ref().clone())
        }
        ShutdownDeliveryState::Exited {
            at_ms, physical, ..
        } => old.confirm_exit(*at_ms, physical.as_ref().clone()),
        _ => return Err(bad("Unsupported shutdown delivery transition")),
    }
    .map_err(bad)?;
    if expected != *next || next.revision != old.revision + 1 {
        return Err(bad(
            "Shutdown history is not an exact append-only transition",
        ));
    }
    Ok(())
}
fn delivery(c: &Connection, intent: &ShutdownIntent) -> Result<ShutdownRecord> {
    let id = &intent.binding.update_id;
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM shutdown_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ShutdownRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.intent != *intent || current.revision != counter(revision)? {
        return Err(bad("Shutdown intent/current revision differs"));
    }
    let mut q = c
        .prepare(
            "SELECT revision,record FROM shutdown_history WHERE id=?1 ORDER BY revision LIMIT 6",
        )
        .map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Shutdown history missing or oversized"));
    }
    let mut previous = ShutdownRecord::prepare(intent.clone()).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ShutdownRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Shutdown history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Shutdown history must start Prepared"));
            }
        } else {
            valid_step(&previous, &next)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Shutdown current/history differs"));
    }
    Ok(current)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ShutdownRecord>> {
    if schema(c)? < 12 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM shutdown_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(None) };
    let intent: ShutdownIntent = decode(&raw)?;
    intent.validate().map_err(bad)?;
    let b = &intent.binding;
    let parent = record(c, id)?;
    let drain = drain::read(c, id)?;
    let d = drain
        .intent
        .as_ref()
        .ok_or_else(|| bad("Shutdown without drain intent"))?;
    let baseline = runtime_baseline::read(c, id)?
        .receipt
        .ok_or_else(|| bad("Shutdown without baseline receipt"))?;
    let sup = supervisor::read(c, &b.launch_id)?;
    let final_lease = match drain.commands.last().map(|r| &r.delivery) {
        Some(crate::drain::DrainDeliveryState::Confirmed { receipt, .. }) => receipt.as_ref(),
        _ => return Err(bad("Shutdown without matching final quiescence receipt")),
    };
    if b.update_id != id
        || b.drain_sha256 != data::drain_digest(&drain).map_err(bad)?
        || drain.quiesced_revision != Some(b.parent_revision)
        || b.launch_id != d.binding.launch_id
        || b.owner_id != d.binding.owner_id
        || b.owner_epoch != d.binding.owner_epoch
        || b.lease != *final_lease
        || sup.spec.update_id != id
        || sup.spec.worker.as_ref() != Some(&b.worker)
        || sup.owner_id.as_deref() != Some(b.owner_id.as_str())
        || sup.owner_epoch != Some(b.owner_epoch)
        || baseline.loaded_worker != b.loaded_worker
        || parent.revision < b.parent_revision + 1
    {
        return Err(bad(
            "Shutdown retained drain/worker/baseline/owner scope differs",
        ));
    }
    let at = runtime_baseline::transition(
        c,
        id,
        b.parent_revision + 1,
        UpdateEvent::BeginShutdown,
        ADMIT,
        &b.owner_id,
        b.owner_epoch,
    )?;
    if b.admitted_at_ms > at {
        return Err(bad("Shutdown intent timestamp exceeds transition"));
    }
    let saved = delivery(c, &intent)?;
    let exit_revision: Option<i64> = c
        .query_row(
            "SELECT revision FROM transitions WHERE update_id=?1 AND command_key=?2",
            params![id, EXIT],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    match (&saved.delivery, exit_revision) {
        (ShutdownDeliveryState::Exited { .. }, Some(revision))
            if counter(revision)? == b.parent_revision + 2 =>
        {
            runtime_baseline::transition(
                c,
                id,
                b.parent_revision + 2,
                UpdateEvent::ShutdownVerified,
                EXIT,
                &b.owner_id,
                b.owner_epoch,
            )?;
        }
        (ShutdownDeliveryState::Exited { .. }, _) | (_, Some(_)) => {
            return Err(bad("Shutdown exit receipt/root edge were not atomic"))
        }
        _ => {}
    }
    Ok(Some(saved))
}
fn require(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    intent: &ShutdownIntent,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let b = &intent.binding;
    let parent = record(c, &b.update_id)?;
    if b.owner_id != a.owner_id()
        || b.owner_epoch != a.epoch()
        || b.launch_id != s.spec.launch_id
        || b.update_id != s.spec.update_id
        || parent.phase != UpdatePhase::Stopping
        || parent.revision != b.parent_revision + 1
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Fresh shutdown owner/Stopping required; no adoption or replay",
        ));
    }
    Ok(())
}
fn persist(c: &Connection, old: &ShutdownRecord, next: &ShutdownRecord) -> Result<()> {
    valid_step(old, next)?;
    let id = &old.intent.binding.update_id;
    if delivery(c, &old.intent)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Shutdown delivery changed",
        ));
    }
    one(c
        .execute(
            "UPDATE shutdown_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",
            params![next.revision as i64, encode(next)?, id, old.revision as i64],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO shutdown_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, encode(next)?],
        )
        .map_err(storage)?)?;
    if delivery(c, &old.intent)? != *next {
        return Err(bad("Shutdown write/readback ignored"));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_shutdown(directory: &Path, id: &str) -> Result<Option<ShutdownRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    /// Trusted data admission; no RPC, shutdown flag or runtime writes. The
    /// consuming physical driver must revalidate the worker/state before use.
    pub fn admit_shutdown(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lease: &WorkerUpdateLease,
        loaded_worker: &LoadedWorkerIdentity,
        worker_state: WorkerShutdownState,
    ) -> Result<ShutdownAdmission> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        runtime_baseline::current_supervisor(&self.connection, a, s)?;
        let id = &s.spec.update_id;
        if let Some(saved) = read(&self.connection, id)? {
            return Ok(ShutdownAdmission::Observed(Box::new(saved)));
        }
        let parent = record(&self.connection, id)?;
        let drain = drain::read(&self.connection, id)?;
        if parent.phase != UpdatePhase::Quiesced || drain.quiesced_revision != Some(parent.revision)
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Matching root/worker quiescence required before shutdown",
            ));
        }
        let intent = ShutdownIntent::from_quiesced(
            ShutdownBinding {
                update_id: id.clone(),
                launch_id: s.spec.launch_id.clone(),
                owner_id: a.owner_id().into(),
                owner_epoch: a.epoch(),
                parent_revision: parent.revision,
                admitted_at_ms: now_ms()?,
                drain_sha256: data::drain_digest(&drain).map_err(bad)?,
                worker: s
                    .spec
                    .worker
                    .as_ref()
                    .ok_or_else(|| bad("Shutdown worker binding missing"))?
                    .clone(),
                lease: lease.clone(),
                loaded_worker: loaded_worker.clone(),
                worker_state,
            },
            Uuid::new_v4().to_string(),
        )
        .map_err(bad)?;
        let initial = ShutdownRecord::prepare(intent.clone()).map_err(bad)?;
        let command = UpdateCommand {
            idempotency_key: ADMIT.into(),
            expected_revision: parent.revision,
            event: UpdateEvent::BeginShutdown,
            reason: "Остановка для обновления подготовлена; завершение worker ещё не подтверждено"
                .into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Shutdown, |c, root| {
            require(c, a, s, &intent)?;
            if root.phase != UpdatePhase::Stopping
                || root.revision != intent.binding.parent_revision + 1
            {
                return Err(bad("Shutdown admission parent edge changed"));
            }
            one(c
                .execute(
                    "INSERT INTO shutdown_intents VALUES(?1,?2)",
                    params![id, encode(&intent)?],
                )
                .map_err(storage)?)?;
            one(c
                .execute(
                    "INSERT INTO shutdown_records VALUES(?1,0,?2)",
                    params![id, encode(&initial)?],
                )
                .map_err(storage)?)?;
            one(c
                .execute(
                    "INSERT INTO shutdown_history VALUES(?1,0,?2)",
                    params![id, encode(&initial)?],
                )
                .map_err(storage)?)?;
            if read(c, id)?.as_ref() != Some(&initial) {
                return Err(bad("Shutdown admission write/readback ignored"));
            }
            Ok(())
        })?;
        Ok(ShutdownAdmission::Fresh(Box::new(ShutdownPermit {
            intent,
        })))
    }
    pub fn issue_shutdown(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &ShutdownPermit,
    ) -> Result<ShutdownIssue> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &p.intent)?;
        let old =
            read(&tx, &p.intent.binding.update_id)?.ok_or_else(|| bad("Shutdown intent absent"))?;
        if old.intent != p.intent {
            return Err(bad("Shutdown permit differs from retained intent"));
        }
        if old.delivery != ShutdownDeliveryState::Prepared {
            return Ok(ShutdownIssue::Observed(Box::new(old)));
        }
        let next = old.issue(now_ms()?).map_err(bad)?;
        persist(&tx, &old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(ShutdownIssue::Fresh(Box::new(ShutdownDeliveryPermit {
            record: next,
        })))
    }
    pub fn mark_shutdown_uncertain(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &ShutdownPermit,
        old: &ShutdownRecord,
        reason: String,
    ) -> Result<ShutdownRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &p.intent)?;
        if old.intent != p.intent || read(&tx, &p.intent.binding.update_id)?.as_ref() != Some(old) {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Shutdown uncertain-state revision changed",
            ));
        }
        let next = old.uncertain(now_ms()?, reason).map_err(bad)?;
        persist(&tx, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    pub fn acknowledge_shutdown(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &ShutdownPermit,
        receipt: WorkerShutdownReceipt,
    ) -> Result<ShutdownRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &p.intent)?;
        let old =
            read(&tx, &p.intent.binding.update_id)?.ok_or_else(|| bad("Shutdown intent absent"))?;
        if old.intent != p.intent {
            return Err(bad("Shutdown ACK intent changed"));
        }
        let next = old.acknowledge(now_ms()?, receipt).map_err(bad)?;
        if old == next {
            return Ok(old);
        }
        persist(&tx, &old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    /// The guard retains the exact process handle, original state-file handles,
    /// exclusive instance lock and original runtime pins through this commit.
    /// It is not Serde data and cannot be reconstructed by replaying a receipt.
    pub fn confirm_shutdown_exit(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &ShutdownPermit,
        guard: &ShutdownExitGuard,
    ) -> Result<ShutdownRecord> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        runtime_baseline::current_supervisor(&self.connection, a, s)?;
        guard.require(&p.intent).map_err(bad)?;
        let id = &p.intent.binding.update_id;
        let old = read(&self.connection, id)?.ok_or_else(|| bad("Shutdown intent absent"))?;
        if old.intent != p.intent {
            return Err(bad("Shutdown exit attempt changed"));
        }
        if matches!(old.delivery, ShutdownDeliveryState::Exited { .. }) {
            return Ok(old);
        }
        require(&self.connection, a, s, &p.intent)?;
        let physical = guard.evidence(&p.intent).map_err(bad)?;
        let next = old.confirm_exit(now_ms()?, physical).map_err(bad)?;
        let command=UpdateCommand {
            idempotency_key:EXIT.into(),expected_revision:p.intent.binding.parent_revision+1,event:UpdateEvent::ShutdownVerified,
            reason:"Завершение точного процесса worker подтверждено; updater удерживает прежнюю instance lock и проверенные runtime/state handles".into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Shutdown, |c, root| {
            runtime_baseline::current_supervisor(c, a, s)?;
            guard.require(&p.intent).map_err(bad)?;
            if root.phase != UpdatePhase::Replacing
                || root.revision != p.intent.binding.parent_revision + 2
            {
                return Err(bad("Shutdown exit parent edge changed"));
            }
            // Root edge already stored: inspect the child ledger directly until
            // its atomic Exited counterpart has been inserted.
            persist(c, &old, &next)?;
            if read(c, id)?.as_ref() != Some(&next) {
                return Err(bad("Shutdown exit/root readback failed"));
            }
            Ok(())
        })?;
        Ok(next)
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
