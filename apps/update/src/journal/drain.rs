//! Durable command outbox/evidence. No RPC or process effects live in this module.
use super::*;
use crate::drain::{
    self as data, DrainAdmission, DrainBinding, DrainCommandRecord, DrainDeliveryPermit,
    DrainDeliveryState, DrainIntent, DrainIssue, DrainPermit, DrainSnapshot,
};
use winbridge_protocol::update::{
    SourceTransactionPhase, SupervisorRecord, UpdateEvent, WorkerUpdateLease,
};
const ADMIT: &str = "drain-admit-v1";
const QUIESCED: &str = "drain-quiesced-v1";

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE drain_intents(id TEXT PRIMARY KEY REFERENCES candidate_health(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE drain_commands(update_id TEXT NOT NULL REFERENCES drain_intents(id),ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 2),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(update_id,ordinal));
CREATE TABLE drain_history(update_id TEXT NOT NULL,ordinal INTEGER NOT NULL,revision INTEGER NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(update_id,ordinal,revision),FOREIGN KEY(update_id,ordinal) REFERENCES drain_commands(update_id,ordinal));
CREATE TRIGGER drain_intent_immutable BEFORE UPDATE ON drain_intents BEGIN SELECT RAISE(ABORT,'Drain intent immutable'); END;
CREATE TRIGGER drain_intent_retained BEFORE DELETE ON drain_intents BEGIN SELECT RAISE(ABORT,'Drain intent retained'); END;
CREATE TRIGGER drain_command_identity BEFORE UPDATE ON drain_commands WHEN NEW.update_id!=OLD.update_id OR NEW.ordinal!=OLD.ordinal OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command') OR json_extract(NEW.record,'$.attemptId') IS NOT json_extract(OLD.record,'$.attemptId') OR json_extract(OLD.record,'$.delivery.state')='confirmed' BEGIN SELECT RAISE(ABORT,'Drain command identity/revision/terminal immutable'); END;
CREATE TRIGGER drain_command_retained BEFORE DELETE ON drain_commands BEGIN SELECT RAISE(ABORT,'Drain command retained'); END;
CREATE TRIGGER drain_history_immutable BEFORE UPDATE ON drain_history BEGIN SELECT RAISE(ABORT,'Drain history immutable'); END;
CREATE TRIGGER drain_history_retained BEFORE DELETE ON drain_history BEGIN SELECT RAISE(ABORT,'Drain history retained'); END;
PRAGMA user_version=11;
"#).map_err(storage)
}
fn quiesce() -> UpdateEvent {
    UpdateEvent::Quiesce {
        dispatch_closed: true,
        accepted_operations: 0,
        physical_owners: 0,
        uncertain_owners: 0,
    }
}
fn valid_step(
    old: &DrainCommandRecord,
    next: &DrainCommandRecord,
    intent: &DrainIntent,
) -> Result<()> {
    old.validate(intent).map_err(bad)?;
    next.validate(intent).map_err(bad)?;
    if next.schema_version != old.schema_version
        || next.update_id != old.update_id
        || next.attempt_id != old.attempt_id
        || next.ordinal != old.ordinal
        || next.command != old.command
        || next.revision != old.revision + 1
        || !matches!(
            (&old.delivery, &next.delivery),
            (
                DrainDeliveryState::Prepared,
                DrainDeliveryState::Issued { .. }
            ) | (
                DrainDeliveryState::Issued { .. },
                DrainDeliveryState::Uncertain { .. } | DrainDeliveryState::Confirmed { .. }
            ) | (
                DrainDeliveryState::Uncertain { .. },
                DrainDeliveryState::Confirmed { .. }
            )
        )
    {
        return Err(bad(
            "Drain delivery history is not an append-only transition",
        ));
    }
    Ok(())
}
fn commands(c: &Connection, intent: &DrainIntent) -> Result<Vec<DrainCommandRecord>> {
    let id = &intent.binding.update_id;
    let mut query = c.prepare("SELECT ordinal,revision,record FROM drain_commands WHERE update_id=?1 ORDER BY ordinal LIMIT 4").map_err(storage)?;
    let rows = query
        .query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() != 3 {
        return Err(bad("Drain must retain exactly three command intents"));
    }
    let mut result: Vec<DrainCommandRecord> = vec![];
    for (index, (ordinal, revision, raw)) in rows.into_iter().enumerate() {
        let record: DrainCommandRecord = decode(&raw)?;
        record.validate(intent).map_err(bad)?;
        if ordinal != index as i64
            || record.ordinal as usize != index
            || record.revision != counter(revision)?
        {
            return Err(bad("Drain command/index/revision mismatch"));
        }
        if index > 0
            && !matches!(record.delivery, DrainDeliveryState::Prepared)
            && !matches!(
                result[index - 1].delivery,
                DrainDeliveryState::Confirmed { .. }
            )
        {
            return Err(bad(
                "Drain command issued before the previous matching receipt",
            ));
        }
        let mut q = c.prepare("SELECT revision,record FROM drain_history WHERE update_id=?1 AND ordinal=?2 ORDER BY revision LIMIT 5").map_err(storage)?;
        let history = q
            .query_map(params![id, ordinal], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(storage)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?;
        if history.len() as u64 != record.revision + 1 {
            return Err(bad("Drain history missing or oversized"));
        }
        let mut previous = DrainCommandRecord::prepare(intent, index as u8).map_err(bad)?;
        for (n, (revision, raw)) in history.into_iter().enumerate() {
            let event: DrainCommandRecord = decode(&raw)?;
            if revision != n as i64 || event.revision != n as u64 {
                return Err(bad("Drain history sequence gap"));
            }
            if n == 0 {
                if event != previous {
                    return Err(bad("Drain initial history is not Prepared"));
                }
            } else {
                valid_step(&previous, &event, intent)?;
            }
            previous = event;
        }
        if previous != record {
            return Err(bad("Drain current record differs from immutable history"));
        }
        result.push(record);
    }
    Ok(result)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<DrainSnapshot> {
    let empty = || DrainSnapshot {
        intent: None,
        commands: vec![],
        quiesced_revision: None,
    };
    if schema(c)? < 11 {
        return Ok(empty());
    }
    let raw: Option<String> = c
        .query_row("SELECT record FROM drain_intents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(empty()) };
    let intent: DrainIntent = decode(&raw)?;
    intent.validate().map_err(bad)?;
    let binding = &intent.binding;
    let parent = record(c, id)?;
    let health =
        health::read(c, id)?.ok_or_else(|| bad("Drain without retained health receipt"))?;
    crate::health::validate_success(&health).map_err(bad)?;
    let source = source::header(c, id)?;
    let sup = supervisor::read(c, &binding.launch_id)?;
    let worker = sup
        .spec
        .worker
        .as_ref()
        .ok_or_else(|| bad("Drain worker binding missing"))?;
    if binding.update_id != id
        || data::spec_digest(&source.worker_source_spec).map_err(bad)? != intent.worker_spec_sha256
        || source.worker_source_spec.updater_owner_id != binding.owner_id
        || source.worker_source_spec.updater_epoch != binding.owner_epoch
        || source.worker_source_spec.instance_id != worker.context.instance_id
        || source.worker_source_spec.journal_incarnation != worker.context.journal_incarnation
        || source.worker_source_spec.worker_generation != worker.generation
        || source.phase != SourceTransactionPhase::Verified
        || source.worker_lease_revision != intent.worker_revision
        || data::source_digest(&source).map_err(bad)? != binding.source_sha256
        || data::health_digest(&health).map_err(bad)? != binding.health_sha256
        || health.intent.owner_id != binding.owner_id
        || health.intent.owner_epoch != binding.owner_epoch
        || health.intent.launch_id != binding.launch_id
        || health.result.as_ref().and_then(|r| r.parent_revision) != Some(binding.parent_revision)
        || sup.spec.update_id != id
        || sup.owner_id.as_deref() != Some(binding.owner_id.as_str())
        || sup.owner_epoch != Some(binding.owner_epoch)
        || binding.parent_revision + 1 > parent.revision
    {
        return Err(bad(
            "Drain retained health/source/supervisor binding differs",
        ));
    }
    runtime_baseline::transition(
        c,
        id,
        binding.parent_revision + 1,
        UpdateEvent::BeginDrain,
        ADMIT,
        &binding.owner_id,
        binding.owner_epoch,
    )?;
    let commands = commands(c, &intent)?;
    let revision: Option<i64> = c
        .query_row(
            "SELECT revision FROM transitions WHERE update_id=?1 AND command_key=?2",
            params![id, QUIESCED],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let quiesced_revision = revision.map(counter).transpose()?;
    if let Some(revision) = quiesced_revision {
        if revision != binding.parent_revision + 2
            || !matches!(commands[2].delivery, DrainDeliveryState::Confirmed { .. })
        {
            return Err(bad(
                "Root quiescence without its matching final worker receipt",
            ));
        }
        runtime_baseline::transition(
            c,
            id,
            revision,
            quiesce(),
            QUIESCED,
            &binding.owner_id,
            binding.owner_epoch,
        )?;
    } else if matches!(commands[2].delivery, DrainDeliveryState::Confirmed { .. })
        && matches!(parent.phase, UpdatePhase::Draining | UpdatePhase::Quiesced)
    {
        return Err(bad(
            "Final worker receipt and active root quiescence were not committed atomically",
        ));
    }
    Ok(DrainSnapshot {
        intent: Some(intent),
        commands,
        quiesced_revision,
    })
}
fn require(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    intent: &DrainIntent,
    cleanup: bool,
) -> Result<UpdateRecord> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let b = &intent.binding;
    let parent = record(c, &b.update_id)?;
    if b.owner_id != a.owner_id()
        || b.owner_epoch != a.epoch()
        || b.launch_id != s.spec.launch_id
        || s.spec.update_id != b.update_id
        || (!cleanup
            && (parent.phase != UpdatePhase::Draining || parent.revision != b.parent_revision + 1))
        || (cleanup
            && !matches!(
                parent.phase,
                UpdatePhase::Draining | UpdatePhase::Aborting | UpdatePhase::Quiesced
            ))
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Fresh drain owner/phase required; no replay or adoption",
        ));
    }
    Ok(parent)
}
fn persist(
    c: &Connection,
    intent: &DrainIntent,
    old: &DrainCommandRecord,
    next: &DrainCommandRecord,
) -> Result<()> {
    valid_step(old, next, intent)?;
    one(c.execute("UPDATE drain_commands SET revision=?1,record=?2 WHERE update_id=?3 AND ordinal=?4 AND revision=?5",
        params![next.revision as i64,encode(next)?,next.update_id,next.ordinal as i64,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO drain_history VALUES(?1,?2,?3,?4)",
            params![
                next.update_id,
                next.ordinal as i64,
                next.revision as i64,
                encode(next)?
            ],
        )
        .map_err(storage)?)?;
    if commands(c, intent)?.get(next.ordinal as usize) != Some(next) {
        return Err(bad("Drain command write/readback ignored"));
    }
    Ok(())
}
impl UpdateJournal {
    pub(crate) fn drain(&self, id: &str) -> Result<DrainSnapshot> {
        read(&self.connection, id)
    }
    pub(crate) fn require_drain_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &DrainPermit,
    ) -> Result<()> {
        self.check_directory(a)?;
        require(&self.connection, a, s, &p.intent, false)?;
        if read(&self.connection, &p.intent.binding.update_id)?
            .intent
            .as_ref()
            != Some(&p.intent)
        {
            return Err(bad("Drain driver permit differs from saved intent"));
        }
        Ok(())
    }
    pub fn inspect_drain(directory: &Path, id: &str) -> Result<DrainSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    /// Trusted admission only. Physical/config/artifact revalidation belongs to
    /// the consuming driver before any RPC; this does not permit installation.
    pub fn admit_drain(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lease: &WorkerUpdateLease,
    ) -> Result<DrainAdmission> {
        self.check_directory(a)?;
        runtime_baseline::current_supervisor(&self.connection, a, s)?;
        let id = &s.spec.update_id;
        let saved = read(&self.connection, id)?;
        if saved.intent.is_some() {
            return Ok(DrainAdmission::Observed(Box::new(saved)));
        }
        let parent = self.get(id)?;
        let health = health::read(&self.connection, id)?
            .ok_or_else(|| bad("Verified health required before drain"))?;
        crate::health::validate_fresh_success(&health).map_err(bad)?;
        let source = source::header(&self.connection, id)?;
        if parent.phase != UpdatePhase::CandidateVerified
            || source.phase != SourceTransactionPhase::Verified
            || health.result.as_ref().and_then(|r| r.parent_revision) != Some(parent.revision)
            || health.intent.owner_id != a.owner_id()
            || health.intent.owner_epoch != a.epoch()
            || health.intent.launch_id != s.spec.launch_id
            || source.worker_source_spec != lease.spec
            || source.worker_lease_revision != lease.revision
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Drain requires this fresh verified candidate/source lease",
            ));
        }
        let intent = DrainIntent::from_lease(
            DrainBinding {
                update_id: id.clone(),
                launch_id: s.spec.launch_id.clone(),
                owner_id: a.owner_id().into(),
                owner_epoch: a.epoch(),
                parent_revision: parent.revision,
                accepted_at_ms: now_ms()?,
                source_sha256: data::source_digest(&source).map_err(bad)?,
                health_sha256: data::health_digest(&health).map_err(bad)?,
            },
            lease,
            Uuid::new_v4().to_string(),
        )
        .map_err(bad)?;
        let cmd = UpdateCommand {idempotency_key:ADMIT.into(),expected_revision:parent.revision,event:UpdateEvent::BeginDrain,
            reason:"Обновление принято: дождаться принятых задач, затем закрыть запуск; runtime ещё не заменяется".into()};
        self.advance_with_driver(a, id, &cmd, Driver::Drain, |c, root| {
            require(c, a, s, &intent, false)?;
            if root.phase != UpdatePhase::Draining
                || root.revision != intent.binding.parent_revision + 1
            {
                return Err(bad("Drain admission parent edge changed"));
            }
            one(c
                .execute(
                    "INSERT INTO drain_intents VALUES(?1,?2)",
                    params![id, encode(&intent)?],
                )
                .map_err(storage)?)?;
            for ordinal in 0..3 {
                let command = DrainCommandRecord::prepare(&intent, ordinal).map_err(bad)?;
                one(c
                    .execute(
                        "INSERT INTO drain_commands VALUES(?1,?2,0,?3)",
                        params![id, ordinal as i64, encode(&command)?],
                    )
                    .map_err(storage)?)?;
                one(c
                    .execute(
                        "INSERT INTO drain_history VALUES(?1,?2,0,?3)",
                        params![id, ordinal as i64, encode(&command)?],
                    )
                    .map_err(storage)?)?;
            }
            if read(c, id)?.intent.as_ref() != Some(&intent) {
                return Err(bad("Drain admission write ignored"));
            }
            Ok(())
        })?;
        Ok(DrainAdmission::Fresh(Box::new(DrainPermit { intent })))
    }
    pub fn issue_drain_command(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &DrainPermit,
        ordinal: u8,
    ) -> Result<DrainIssue> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &permit.intent, false)?;
        let saved = read(&tx, &permit.intent.binding.update_id)?;
        if saved.intent.as_ref() != Some(&permit.intent) {
            return Err(bad("Drain permit differs from saved intent"));
        }
        let old = saved
            .commands
            .get(ordinal as usize)
            .ok_or_else(|| bad("Invalid drain ordinal"))?;
        if !matches!(old.delivery, DrainDeliveryState::Prepared) {
            return Ok(DrainIssue::Observed(Box::new(old.clone())));
        }
        if ordinal > 0
            && !matches!(
                saved.commands[ordinal as usize - 1].delivery,
                DrainDeliveryState::Confirmed { .. }
            )
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Previous drain command has no matching receipt",
            ));
        }
        let next = old.issue(&permit.intent, now_ms()?).map_err(bad)?;
        persist(&tx, &permit.intent, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(DrainIssue::Fresh(Box::new(DrainDeliveryPermit {
            record: next,
        })))
    }
    pub fn mark_drain_uncertain(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &DrainPermit,
        old: &DrainCommandRecord,
        reason: String,
    ) -> Result<DrainCommandRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &permit.intent, true)?;
        let saved = read(&tx, &permit.intent.binding.update_id)?;
        if saved.intent.as_ref() != Some(&permit.intent)
            || saved.commands.get(old.ordinal as usize) != Some(old)
        {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Drain uncertain-state revision changed",
            ));
        }
        let next = old
            .uncertain(&permit.intent, now_ms()?, reason)
            .map_err(bad)?;
        persist(&tx, &permit.intent, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    pub fn confirm_drain_command(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &DrainPermit,
        ordinal: u8,
        receipt: WorkerUpdateLease,
    ) -> Result<DrainCommandRecord> {
        self.check_directory(a)?;
        let parent = require(&self.connection, a, s, &permit.intent, true)?;
        let saved = read(&self.connection, &permit.intent.binding.update_id)?;
        if saved.intent.as_ref() != Some(&permit.intent) {
            return Err(bad("Drain receipt intent changed"));
        }
        let old = saved
            .commands
            .get(ordinal as usize)
            .ok_or_else(|| bad("Invalid drain receipt ordinal"))?;
        let next = old
            .confirm(&permit.intent, now_ms()?, receipt)
            .map_err(bad)?;
        if *old == next {
            return Ok(next);
        }
        let final_active = ordinal == 2 && parent.phase == UpdatePhase::Draining;
        if final_active {
            let command=UpdateCommand {idempotency_key:QUIESCED.into(),expected_revision:parent.revision,event:quiesce(),
                reason:"Worker подтвердил закрытие запуска, завершение принятых операций и отсутствие физических владельцев".into()};
            self.advance_with_driver(
                a,
                &permit.intent.binding.update_id,
                &command,
                Driver::Drain,
                |c, root| {
                    require(c, a, s, &permit.intent, true)?;
                    if root.phase != UpdatePhase::Quiesced
                        || root.revision != permit.intent.binding.parent_revision + 2
                    {
                        return Err(bad("Atomic drain quiescence parent edge changed"));
                    }
                    // The root edge is already stored but its child receipt is not
                    // yet written: validate command rows, not the combined snapshot.
                    let fresh = commands(c, &permit.intent)?;
                    if fresh.get(ordinal as usize) != Some(old) {
                        return Err(fail(
                            JournalErrorCode::RevisionConflict,
                            "Drain final receipt changed",
                        ));
                    }
                    persist(c, &permit.intent, old, &next)?;
                    if read(c, &permit.intent.binding.update_id)?.quiesced_revision
                        != Some(root.revision)
                    {
                        return Err(bad("Quiescence receipt/parent readback failed"));
                    }
                    Ok(())
                },
            )?;
        } else {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(storage)?;
            require(&tx, a, s, &permit.intent, true)?;
            let fresh = read(&tx, &permit.intent.binding.update_id)?;
            if fresh.commands.get(ordinal as usize) != Some(old) {
                return Err(fail(
                    JournalErrorCode::RevisionConflict,
                    "Drain receipt revision changed",
                ));
            }
            persist(&tx, &permit.intent, old, &next)?;
            tx.commit().map_err(storage)?;
        }
        Ok(next)
    }
}

#[cfg(test)]
#[path = "drain_tests.rs"]
mod tests;
