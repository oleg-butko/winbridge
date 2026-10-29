//! Schema16: immutable support intent/history. No saved data mints process authority.
use super::*;
use crate::candidate_support::*;
use winbridge_protocol::update::{
    SupervisorPhase, SupervisorRecord, WorkerUpdateLease, WorkerUpdatePhase,
};
fn bad(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, e)
}

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE candidate_support_intents(id TEXT PRIMARY KEY REFERENCES candidate_health(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE candidate_support_records(id TEXT PRIMARY KEY REFERENCES candidate_support_intents(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 5),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE candidate_support_history(id TEXT NOT NULL REFERENCES candidate_support_records(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 5),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER candidate_support_intent_immutable BEFORE UPDATE ON candidate_support_intents BEGIN SELECT RAISE(ABORT,'Support intent immutable');END;
CREATE TRIGGER candidate_support_intent_retained BEFORE DELETE ON candidate_support_intents BEGIN SELECT RAISE(ABORT,'Support intent retained');END;
CREATE TRIGGER candidate_support_record_identity BEFORE UPDATE ON candidate_support_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') OR json_extract(OLD.record,'$.result') IS NOT NULL BEGIN SELECT RAISE(ABORT,'Support identity/revision/terminal immutable');END;
CREATE TRIGGER candidate_support_record_retained BEFORE DELETE ON candidate_support_records BEGIN SELECT RAISE(ABORT,'Support record retained');END;
CREATE TRIGGER candidate_support_history_immutable BEFORE UPDATE ON candidate_support_history BEGIN SELECT RAISE(ABORT,'Support history immutable');END;
CREATE TRIGGER candidate_support_history_retained BEFORE DELETE ON candidate_support_history BEGIN SELECT RAISE(ABORT,'Support history retained');END;
PRAGMA user_version=16;
"#).map_err(storage)
}

fn ledger(c: &Connection, intent: &CandidateSupportIntent) -> Result<CandidateSupportRecord> {
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM candidate_support_records WHERE id=?1",
            [&intent.update_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: CandidateSupportRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if &current.intent != intent || current.revision != counter(revision)? {
        return Err(bad("Support intent/current revision differs"));
    }
    let mut q=c.prepare("SELECT revision,record FROM candidate_support_history WHERE id=?1 ORDER BY revision LIMIT 7").map_err(storage)?;
    let rows = q
        .query_map([&intent.update_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Support causal history missing/oversized"));
    }
    let mut previous = CandidateSupportRecord::admitted(intent.clone()).map_err(bad)?;
    for (n, (revision, raw)) in rows.into_iter().enumerate() {
        let record: CandidateSupportRecord = decode(&raw)?;
        if revision != n as i64 || record.revision != n as u64 {
            return Err(bad("Support history sequence gap"));
        }
        if n == 0 {
            if record != previous {
                return Err(bad("Support history did not start at admission"));
            }
        } else {
            previous.advance(&record).map_err(bad)?;
        }
        previous = record;
    }
    if previous != current {
        return Err(bad("Support current/history differ"));
    }
    Ok(current)
}

pub(super) fn read(c: &Connection, id: &str) -> Result<Option<CandidateSupportRecord>> {
    if schema(c)? < 16 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM candidate_support_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(None) };
    let intent: CandidateSupportIntent = decode(&raw)?;
    intent.validate().map_err(bad)?;
    let parent = record(c, id)?;
    let installation = installation_info(c)?;
    let sup = supervisor::read(c, &intent.launch_id)?;
    let health = health::read(c, id)?.ok_or_else(|| bad("Support health missing"))?;
    crate::health::validate_success(&health).map_err(bad)?;
    let artifacts = candidate::read(c, id)?;
    let source = source::header(c, id)?;
    let updater = artifacts
        .inspection
        .artifacts
        .get(3)
        .ok_or_else(|| bad("Support updater artifact missing"))?;
    let config = &health
        .context
        .as_ref()
        .ok_or_else(|| bad("Support health context missing"))?
        .worker_config;
    if intent.update_id != id
        || intent.installation_id != installation.installation_id
        || intent.scope_sha256 != installation.scope_sha256
        || intent.parent_revision
            != health
                .result
                .as_ref()
                .and_then(|r| r.parent_revision)
                .ok_or_else(|| bad("Support health parent missing"))?
        || intent.parent_revision > parent.revision
        || intent.admitted_at_ms < health.result.as_ref().unwrap().completed_at_ms
        || intent.expected_updater_journal_schema > schema(c)?
        || sup.spec.update_id != id
        || sup.owner_id.as_deref() != Some(intent.owner_id.as_str())
        || sup.owner_epoch != Some(intent.owner_epoch)
        || sup.spec.config_sha256 != intent.config_sha256
        || sup.spec.image_sha256 != intent.supervisor_image_sha256
        || health.intent.owner_id != intent.owner_id
        || health.intent.owner_epoch != intent.owner_epoch
        || health.intent.launch_id != intent.launch_id
        || digest(&artifacts).map_err(bad)? != intent.artifacts_sha256
        || digest(&source).map_err(bad)? != intent.source_sha256
        || crate::drain::health_digest(&health).map_err(bad)? != intent.health_sha256
        || updater.image != intent.image
        || updater.descriptor != intent.descriptor
        || config != &intent.worker_config
    {
        return Err(bad(
            "Support source/health/artifacts/config/executor scope differs",
        ));
    }
    let r = ledger(c, &intent)?;
    if let Some(final_worker) = r.result.as_ref().and_then(|r| r.final_worker.as_ref()) {
        final_worker
            .validate_binding(
                sup.spec
                    .worker
                    .as_ref()
                    .ok_or_else(|| bad("Support worker binding missing"))?,
            )
            .map_err(bad)?;
        let mut normalized = final_worker.clone();
        normalized.observed_at_ms = health
            .context
            .as_ref()
            .unwrap()
            .compatibility
            .observed_at_ms;
        if normalized != health.context.as_ref().unwrap().compatibility {
            return Err(bad("Support final worker differs from health/baseline"));
        }
    }
    Ok(Some(r))
}
fn insert(c: &Connection, r: &CandidateSupportRecord) -> Result<()> {
    if *r != CandidateSupportRecord::admitted(r.intent.clone()).map_err(bad)? {
        return Err(bad("Support initial record is not admission"));
    }
    one(c
        .execute(
            "INSERT INTO candidate_support_intents VALUES(?1,?2)",
            params![r.intent.update_id, encode(&r.intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO candidate_support_records VALUES(?1,0,?2)",
            params![r.intent.update_id, encode(r)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO candidate_support_history VALUES(?1,0,?2)",
            params![r.intent.update_id, encode(r)?],
        )
        .map_err(storage)?)?;
    if ledger(c, &r.intent)? != *r {
        return Err(bad("Support admission write ignored"));
    }
    Ok(())
}
fn persist(
    c: &Connection,
    old: &CandidateSupportRecord,
    next: &CandidateSupportRecord,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    if ledger(c, &old.intent)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Support record changed",
        ));
    }
    one(c.execute("UPDATE candidate_support_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,encode(next)?,old.intent.update_id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO candidate_support_history VALUES(?1,?2,?3)",
            params![old.intent.update_id, next.revision as i64, encode(next)?],
        )
        .map_err(storage)?)?;
    if ledger(c, &old.intent)? != *next {
        return Err(bad("Support receipt write ignored"));
    }
    Ok(())
}
fn require(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    r: &CandidateSupportRecord,
    cleanup: bool,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let parent = record(c, &r.intent.update_id)?;
    if r.intent.owner_id != a.owner_id()
        || r.intent.owner_epoch != a.epoch()
        || r.intent.launch_id != s.spec.launch_id
        || r.result.is_some()
        || (!cleanup
            && (parent.phase != UpdatePhase::CandidateVerified
                || parent.revision != r.intent.parent_revision))
        || (cleanup
            && !matches!(
                parent.phase,
                UpdatePhase::CandidateVerified | UpdatePhase::Aborting
            ))
        || read(c, &r.intent.update_id)?.as_ref() != Some(r)
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Support owner/root/attempt changed; no replay",
        ));
    }
    Ok(())
}

impl UpdateJournal {
    pub fn inspect_candidate_support(
        directory: &Path,
        id: &str,
    ) -> Result<Option<CandidateSupportRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn candidate_support(&self, id: &str) -> Result<Option<CandidateSupportRecord>> {
        read(&self.connection, id)
    }
    pub(crate) fn require_candidate_support_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        r: &CandidateSupportRecord,
    ) -> Result<()> {
        self.check_directory(a)?;
        require(&self.connection, a, s, r, false)
    }
    pub(crate) fn admit_candidate_support(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lease: &WorkerUpdateLease,
    ) -> Result<CandidateSupportAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(r) = read(&tx, &s.spec.update_id)? {
            return Ok(CandidateSupportAdmission::Existing(Box::new(r)));
        }
        // A duplicate is historical observation even after ownership/lifecycle
        // changed. Only a fresh admission needs the current live supervisor.
        runtime_baseline::current_supervisor(&tx, a, s)?;
        let parent = record(&tx, &s.spec.update_id)?;
        let health = health::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Support requires verified worker health"))?;
        crate::health::validate_fresh_success(&health).map_err(bad)?;
        let ctx = health
            .context
            .as_ref()
            .ok_or_else(|| bad("Support requires health context"))?;
        let artifacts = candidate::read(&tx, &s.spec.update_id)?;
        let source = source::header(&tx, &s.spec.update_id)?;
        let installation = info(&tx)?;
        if parent.phase != UpdatePhase::CandidateVerified
            || health.result.as_ref().and_then(|r| r.parent_revision) != Some(parent.revision)
            || s.phase != SupervisorPhase::Active
            || health.intent.launch_id != s.spec.launch_id
            || health.intent.owner_id != a.owner_id()
            || health.intent.owner_epoch != a.epoch()
            || lease.phase != WorkerUpdatePhase::SourceHeld
            || lease.spec != source.worker_source_spec
            || lease.revision != source.worker_lease_revision
            || source.phase != winbridge_protocol::update::SourceTransactionPhase::Verified
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Support needs current verified candidate and held source lease",
            ));
        }
        let local = crate::updater_support::current().map_err(bad)?;
        if s.process.as_ref() != Some(&local.process) {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Support executor is not the actual supervisor",
            ));
        }
        let updater = artifacts
            .inspection
            .artifacts
            .get(3)
            .ok_or_else(|| bad("Candidate updater missing"))?;
        let intent = CandidateSupportIntent {
            schema_version: 1,
            update_id: s.spec.update_id.clone(),
            attempt_id: Uuid::new_v4().to_string(),
            installation_id: installation.installation_id,
            scope_sha256: installation.scope_sha256,
            launch_id: s.spec.launch_id.clone(),
            owner_id: a.owner_id().into(),
            owner_epoch: a.epoch(),
            parent_revision: parent.revision,
            admitted_at_ms: now_ms()?,
            config_sha256: s.spec.config_sha256.clone(),
            supervisor_image_sha256: s.spec.image_sha256.clone(),
            executor: local.descriptor.clone(),
            drivers: local.driver_contracts(),
            source_sha256: digest(&source).map_err(bad)?,
            health_sha256: crate::drain::health_digest(&health).map_err(bad)?,
            artifacts_sha256: digest(&artifacts).map_err(bad)?,
            worker_config: ctx.worker_config.clone(),
            image: updater.image.clone(),
            descriptor: updater.descriptor.clone(),
            expected_updater_journal_schema: UPDATE_SCHEMA_VERSION,
        };
        let r = CandidateSupportRecord::admitted(intent.clone()).map_err(bad)?;
        insert(&tx, &r)?;
        if read(&tx, &r.intent.update_id)?.as_ref() != Some(&r)
            || record(&tx, &r.intent.update_id)? != parent
        {
            return Err(bad("Support admission/root readback differs"));
        }
        tx.commit().map_err(storage)?;
        Ok(CandidateSupportAdmission::Fresh(Box::new(
            CandidateSupportPermit { intent },
        )))
    }
    pub(crate) fn update_candidate_support(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        old: &CandidateSupportRecord,
        next: CandidateSupportRecord,
        cleanup: bool,
    ) -> Result<CandidateSupportRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, old, cleanup)?;
        persist(&tx, old, &next)?;
        if read(&tx, &old.intent.update_id)?.as_ref() != Some(&next) {
            return Err(bad("Support final readback differs"));
        }
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests;
