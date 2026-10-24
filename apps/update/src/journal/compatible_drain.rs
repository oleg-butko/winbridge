//! Schema17 production prerequisite: blocked decisions never create a drain outbox.
use super::*;
use crate::candidate_support::CandidateSupportRecord;
use crate::compatible_drain::*;
use winbridge_protocol::update::SupervisorRecord;

fn bad(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, e)
}
fn changed(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::PreconditionsChanged, e)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE compatible_drain_records(id TEXT PRIMARY KEY REFERENCES candidate_support_records(id),drain_update_id TEXT UNIQUE REFERENCES drain_intents(id),record TEXT NOT NULL CHECK(json_valid(record)),CHECK(drain_update_id IS NULL OR drain_update_id=id));
CREATE TRIGGER compatible_drain_immutable BEFORE UPDATE ON compatible_drain_records BEGIN SELECT RAISE(ABORT,'Production drain decision immutable');END;
CREATE TRIGGER compatible_drain_retained BEFORE DELETE ON compatible_drain_records BEGIN SELECT RAISE(ABORT,'Production drain decision retained');END;
PRAGMA user_version=17;
"#).map_err(storage)
}
fn load(c: &Connection, id: &str) -> Result<Option<(Option<String>, CompatibleDrainRecord)>> {
    let row: Option<(Option<String>, String)> = c
        .query_row(
            "SELECT drain_update_id,record FROM compatible_drain_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    row.map(|(id, raw)| Ok((id, decode(&raw)?))).transpose()
}
fn put(c: &Connection, r: &CompatibleDrainRecord) -> Result<()> {
    let drain_id =
        matches!(r.decision, CompatibleDrainDecision::Admitted { .. }).then_some(&r.update_id);
    one(c
        .execute(
            "INSERT INTO compatible_drain_records VALUES(?1,?2,?3)",
            params![r.update_id, drain_id, encode(r)?],
        )
        .map_err(storage)?)?;
    let actual = load(c, &r.update_id)?.ok_or_else(|| bad("Production decision write ignored"))?;
    if actual.0.as_ref() != drain_id || actual.1 != *r {
        return Err(bad("Production decision readback differs"));
    }
    Ok(())
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<CompatibleDrainRecord>> {
    if schema(c)? < 17 {
        return Ok(None);
    }
    let Some((drain_id, r)) = load(c, id)? else {
        return Ok(None);
    };
    let support =
        candidate_support::read(c, id)?.ok_or_else(|| bad("Production decision has no support"))?;
    r.validate(&support).map_err(bad)?;
    let root = record(c, id)?;
    if r.update_id != id || r.parent_revision > root.revision {
        return Err(bad("Production decision root differs"));
    }
    let drain = drain::read(c, id)?;
    match &r.decision {
        CompatibleDrainDecision::Blocked { .. } if drain_id.is_none() && drain.intent.is_none() => {
        }
        CompatibleDrainDecision::Admitted { drain_attempt_id }
            if drain_id.as_deref() == Some(id) =>
        {
            let i = drain
                .intent
                .ok_or_else(|| bad("Production admission lost drain intent"))?;
            if i.attempt_id != *drain_attempt_id
                || i.binding.update_id != id
                || i.binding.parent_revision != r.parent_revision
                || i.binding.owner_id != r.owner_id
                || i.binding.owner_epoch != r.owner_epoch
                || i.binding.launch_id != support.intent.launch_id
            {
                return Err(bad("Production decision differs from atomic drain outbox"));
            }
        }
        _ => {
            return Err(bad(
                "Blocked/admitted production decision has inconsistent effects",
            ))
        }
    }
    Ok(Some(r))
}
pub(super) fn insert(c: &Connection, r: &CompatibleDrainRecord) -> Result<()> {
    put(c, r)?;
    if read(c, &r.update_id)?.as_ref() != Some(r) {
        return Err(bad("Production decision graph readback differs"));
    }
    Ok(())
}
/// Only the exact current owner can derive a new decision; historical reads do
/// not require a live process or fresh health timestamps.
pub(super) fn current_source(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    draining: bool,
) -> Result<CandidateSupportRecord> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let root = record(c, &s.spec.update_id)?;
    let support = candidate_support::read(c, &s.spec.update_id)?
        .ok_or_else(|| changed("Run updater support before production drain"))?;
    let phase = if draining {
        UpdatePhase::Draining
    } else {
        UpdatePhase::CandidateVerified
    };
    let revision = support.intent.parent_revision + u64::from(draining);
    if root.phase != phase
        || root.revision != revision
        || support.intent.owner_id != a.owner_id()
        || support.intent.owner_epoch != a.epoch()
        || support.intent.launch_id != s.spec.launch_id
        || support.result.is_none()
    {
        return Err(changed(
            "Production drain requires current terminal support/root/owner, no adoption",
        ));
    }
    crate::compatible_drain::revalidate_executor(s, &support).map_err(changed)?;
    Ok(support)
}
pub(super) fn require_bound(c: &Connection, p: &crate::drain::DrainPermit) -> Result<()> {
    let saved = read(c, &p.intent.binding.update_id)?;
    match (&p.production, saved) {
        (None, None) => Ok(()),
        (Some(expected), Some(saved))
            if *expected == saved
                && matches!(&saved.decision,CompatibleDrainDecision::Admitted{drain_attempt_id} if *drain_attempt_id==p.intent.attempt_id) =>
        {
            Ok(())
        }
        _ => Err(changed(
            "Drain permit cannot bypass or adopt a production prerequisite decision",
        )),
    }
}
impl UpdateJournal {
    pub fn inspect_compatible_drain(
        directory: &Path,
        id: &str,
    ) -> Result<Option<CompatibleDrainRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    /// Decision uses trusted metadata/worker adapters, never caller JSON. The
    /// allowed path must retain preflight pins through this call.
    pub fn consider_compatible_drain(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lease: &winbridge_protocol::update::WorkerUpdateLease,
    ) -> Result<CompatibleDrainAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(r) = read(&tx, &s.spec.update_id)? {
            return Ok(CompatibleDrainAdmission::Observed(Box::new(r)));
        }
        let support = current_source(&tx, a, s, false)?;
        if let Some(decision) = crate::compatible_drain::blocked(&support).map_err(changed)? {
            let before = record(&tx, &s.spec.update_id)?;
            let r = CompatibleDrainRecord::new(&support, decision, now_ms()?).map_err(bad)?;
            insert(&tx, &r)?;
            if record(&tx, &s.spec.update_id)? != before {
                return Err(bad("Blocked decision changed root"));
            }
            tx.commit().map_err(storage)?;
            return Ok(CompatibleDrainAdmission::Blocked(Box::new(r)));
        }
        // There was no effect. The same support/root is rechecked inside the
        // atomic root+outbox+link callback; no JSON value grants a permit.
        tx.commit().map_err(storage)?;
        match self.admit_drain_bound(a, s, lease, &support)? {
            crate::drain::DrainAdmission::Fresh(p) => Ok(CompatibleDrainAdmission::Fresh(p)),
            crate::drain::DrainAdmission::Observed(_) => {
                Err(changed("Legacy drain cannot be relabeled production"))
            }
        }
    }
}

#[cfg(test)]
mod tests;
