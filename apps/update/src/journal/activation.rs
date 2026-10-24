//! Schema18: activation preparation/history; no public raw writer or launch route.
use super::*;
use crate::activation::*;
use crate::runtime_replacement::RuntimeReplacementRecord;
#[cfg(windows)]
use winbridge_protocol::update::SupervisorRecord;
fn bad(value: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, value)
}
#[cfg(windows)]
fn changed(value: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::PreconditionsChanged, value)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_intents(id TEXT PRIMARY KEY REFERENCES runtime_replacement_records(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE activation_records(id TEXT PRIMARY KEY REFERENCES activation_intents(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 7),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE activation_history(id TEXT NOT NULL REFERENCES activation_records(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 7),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER activation_intent_immutable BEFORE UPDATE ON activation_intents BEGIN SELECT RAISE(ABORT,'Activation intent immutable');END;
CREATE TRIGGER activation_intent_retained BEFORE DELETE ON activation_intents BEGIN SELECT RAISE(ABORT,'Activation intent retained');END;
CREATE TRIGGER activation_record_identity BEFORE UPDATE ON activation_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') OR json_extract(OLD.record,'$.state.phase') IN('local_verified','creation_failed','indeterminate') BEGIN SELECT RAISE(ABORT,'Activation identity/revision/terminal immutable');END;
CREATE TRIGGER activation_record_retained BEFORE DELETE ON activation_records BEGIN SELECT RAISE(ABORT,'Activation record retained');END;
CREATE TRIGGER activation_history_immutable BEFORE UPDATE ON activation_history BEGIN SELECT RAISE(ABORT,'Activation history immutable');END;
CREATE TRIGGER activation_history_retained BEFORE DELETE ON activation_history BEGIN SELECT RAISE(ABORT,'Activation history retained');END;
PRAGMA user_version=18;
"#).map_err(storage)
}
fn ledger(c: &Connection, replacement: &RuntimeReplacementRecord) -> Result<ActivationRecord> {
    let id = &replacement.intent.preparation.plan.update_id;
    let raw: String = c
        .query_row(
            "SELECT record FROM activation_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let intent: ActivationIntent = decode(&raw)?;
    intent.validate(replacement).map_err(bad)?;
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationRecord = decode(&raw)?;
    current.validate(replacement).map_err(bad)?;
    if current.intent != intent || current.revision != counter(revision)? {
        return Err(bad("Activation intent/current differs"));
    }
    let mut q = c
        .prepare(
            "SELECT revision,record FROM activation_history WHERE id=?1 ORDER BY revision LIMIT 10",
        )
        .map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Activation history missing/oversized"));
    }
    let mut previous = ActivationRecord::prepared(intent, replacement).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Activation history gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Activation history did not begin Prepared"));
            }
        } else {
            previous.advance(&next, replacement).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Activation history/current differs"));
    }
    Ok(current)
}
/// Derive identity/config/schema context from retained authoritative rows in the
/// same read transaction. Only the newly scoped nonce digest/timeout are inputs.
fn context(
    c: &Connection,
    r: &RuntimeReplacementRecord,
    nonce: &str,
    timeout: u64,
) -> Result<ActivationContext> {
    let p = &r.intent.preparation.plan;
    let backup =
        state_backup::read(c, &p.update_id)?.ok_or_else(|| bad("Activation backup missing"))?;
    let shutdown =
        shutdown::read(c, &p.update_id)?.ok_or_else(|| bad("Activation shutdown missing"))?;
    let health = health::read(c, &p.update_id)?.ok_or_else(|| bad("Activation health missing"))?;
    crate::health::validate_success(&health).map_err(bad)?;
    let h = health
        .context
        .as_ref()
        .ok_or_else(|| bad("Activation health context missing"))?;
    let result = health
        .result
        .as_ref()
        .ok_or_else(|| bad("Activation health result missing"))?;
    let rehearsal = result
        .rehearsal
        .as_ref()
        .ok_or_else(|| bad("Activation rehearsal missing"))?;
    let s = supervisor::read(c, &p.launch_id)?;
    let b = &shutdown.intent.binding;
    if backup.phase != crate::state_backup::StateBackupPhase::Verified
        || backup.intent.binding.shutdown.as_ref() != &shutdown
        || crate::runtime_baseline::digest(&shutdown).map_err(bad)? != p.proofs.shutdown_sha256
        || crate::runtime_baseline::digest(&backup).map_err(bad)? != p.proofs.backup_sha256
        || crate::runtime_baseline::digest(&health).map_err(bad)? != p.proofs.health_sha256
        || b.update_id != p.update_id
        || b.launch_id != p.launch_id
        || b.owner_id != p.owner_id
        || b.owner_epoch != p.owner_epoch
        || s.spec.update_id != p.update_id
        || s.owner_id.as_ref() != Some(&p.owner_id)
        || s.owner_epoch != Some(p.owner_epoch)
        || s.spec.config_sha256 != p.config_sha256
        || s.spec.worker.as_ref() != Some(&b.worker)
        || backup.intent.binding.worker_config != h.worker_config
        || rehearsal.descriptor != p.manifest.artifacts[0].descriptor
    {
        return Err(bad(
            "Activation authoritative shutdown/health/backup/supervisor graph differs",
        ));
    }
    let value = ActivationContext {
        supervisor: s
            .process
            .ok_or_else(|| bad("Activation supervisor stamp missing"))?,
        previous_worker: b.worker.process.clone(),
        previous_generation: b.worker.generation.clone(),
        worker_context: b.worker.context.clone(),
        worker_state: b.worker_state.clone(),
        worker_config: h.worker_config.clone(),
        worker_config_schema: rehearsal.worker_config_schema,
        worker_journal_schema: rehearsal.worker_journal_schema,
        updater_journal_schema: rehearsal.updater_journal_schema,
        protocol_version: rehearsal.protocol_version,
        startup_timeout_ms: timeout,
        channel_nonce_sha256: nonce.into(),
    };
    value.validate().map_err(bad)?;
    Ok(value)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ActivationRecord>> {
    Ok(read_graph(c, id)?.map(|(value, _)| value))
}
/// Return the very same validated replacement used by the context/ledger read.
/// Re-reading its entire history in the same transaction adds no fresh evidence.
fn read_graph(
    c: &Connection,
    id: &str,
) -> Result<Option<(ActivationRecord, RuntimeReplacementRecord)>> {
    if schema(c)? < 18 {
        return Ok(None);
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM activation_intents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Ok(None);
    }
    let r =
        runtime_replacement::read(c, id)?.ok_or_else(|| bad("Activation replacement missing"))?;
    let value = ledger(c, &r)?;
    let source = &value.intent.source;
    let current = record(c, id)?;
    let info = info(c)?;
    let (raw, transition): (String, String) = c
        .query_row(
            "SELECT record,transition FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, source.parent_revision as i64],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let parent: UpdateRecord = decode(&raw)?;
    let edge: UpdateTransition = decode(&transition)?;
    let ctx = &value.intent.context;
    if source.update_id != id
        || source.installation_id != info.installation_id
        || source.scope_sha256 != info.scope_sha256
        || parent.update_id != id
        || parent.revision != source.parent_revision
        || parent.phase != UpdatePhase::VerifyingLocal
        || parent.request != current.request
        || current.revision < parent.revision
        || edge.update_id != id
        || edge.revision != parent.revision
        || edge.phase != parent.phase
        || edge.previous_phase != Some(UpdatePhase::Replacing)
        || edge.occurred_at_ms != parent.updated_at_ms
        || edge.owner_id != source.owner_id
        || edge.owner_epoch != source.owner_epoch
        || edge.command.as_ref().map(|v| v.event)
            != Some(winbridge_protocol::update::UpdateEvent::FilesVerified)
        || edge
            .command
            .as_ref()
            .and_then(|v| v.expected_revision.checked_add(1))
            != Some(parent.revision)
        || value.intent.admitted_at_ms < parent.updated_at_ms
        || *ctx != context(c, &r, &ctx.channel_nonce_sha256, ctx.startup_timeout_ms)?
    {
        return Err(bad(
            "Activation source/root edge/context differs from retained graph",
        ));
    }
    Ok(Some((value, r)))
}
#[cfg(any(windows, test))]
fn seed(c: &Connection, value: &ActivationRecord, r: &RuntimeReplacementRecord) -> Result<()> {
    value.validate(r).map_err(bad)?;
    if *value != ActivationRecord::prepared(value.intent.clone(), r).map_err(bad)? {
        return Err(bad("Activation admission must start Prepared"));
    }
    let id = &value.intent.source.update_id;
    one(c
        .execute(
            "INSERT INTO activation_intents VALUES(?1,?2)",
            params![id, encode(&value.intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_records VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_history VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    if ledger(c, r)? != *value {
        return Err(bad("Activation admission write ignored"));
    }
    Ok(())
}
// No production unbound writer. The eventual driver will use guarded callbacks.
#[cfg(test)]
fn persist(
    c: &Connection,
    old: &ActivationRecord,
    next: &ActivationRecord,
    r: &RuntimeReplacementRecord,
) -> Result<()> {
    old.advance(next, r).map_err(bad)?;
    if ledger(c, r)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Activation revision changed",
        ));
    }
    let id = &old.intent.source.update_id;
    let raw = encode(next)?;
    one(c
        .execute(
            "UPDATE activation_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",
            params![next.revision as i64, raw, id, old.revision as i64],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, r)? != *next {
        return Err(bad("Activation append/readback differs"));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_activation(directory: &Path, id: &str) -> Result<Option<ActivationRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    /// Prepare metadata and a transient channel nonce, not an execution permit.
    /// Only the original resident Windows actor may capture this record.
    #[cfg(windows)]
    pub fn prepare_activation(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        guard: &mut crate::runtime_replacement::RuntimeReplacementGuard,
    ) -> Result<ActivationPreparationAdmission> {
        let mut timing = crate::activation::ActivationTiming::new("prepare", None);
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(value) = read(&tx, &s.spec.update_id)? {
            return Ok(ActivationPreparationAdmission::Existing(Box::new(value)));
        }
        runtime_baseline::current_supervisor(&tx, a, s)?;
        let root = record(&tx, &s.spec.update_id)?;
        let r = runtime_replacement::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| changed("Complete owned replacement required"))?;
        let source = ActivationSource::from_files_moved(&r).map_err(changed)?;
        if root.phase != UpdatePhase::VerifyingLocal
            || root.revision != source.parent_revision
            || source.owner_id != a.owner_id()
            || source.owner_epoch != a.epoch()
            || source.launch_id != s.spec.launch_id
            || s.process.as_ref()
                != Some(
                    &winbridge_platform::infrastructure::stamp(std::process::id())
                        .map_err(storage)?,
                )
            || guard.permit().intent() != &r.intent
        {
            return Err(changed(
                "Activation needs exact original live owner/guard/VerifyingLocal root",
            ));
        }
        timing.mark("original_progress_before", None, None);
        guard.require_progress(&r).map_err(changed)?;
        timing.mark("original_progress_after", None, None);
        let health =
            health::read(&tx, &s.spec.update_id)?.ok_or_else(|| changed("Health missing"))?;
        crate::health::validate_fresh_success(&health).map_err(changed)?;
        let nonce = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let context = context(&tx, &r, &crate::sha256(nonce.as_bytes()), 120000)?;
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(context.startup_timeout_ms))
            .ok_or_else(|| changed("Activation monotonic deadline exhausted"))?;
        timing.observe_deadline(deadline);
        let intent = ActivationIntent {
            schema_version: 1,
            attempt_id: Uuid::new_v4().to_string(),
            admitted_at_ms: now_ms()?
                .max(root.updated_at_ms)
                .max(r.intent.admitted_at_ms),
            source,
            context,
        };
        let value = ActivationRecord::prepared(intent, &r).map_err(bad)?;
        timing.mark("seed_before", None, None);
        seed(&tx, &value, &r)?;
        timing.mark("readback_before", None, None);
        if read(&tx, &s.spec.update_id)?.as_ref() != Some(&value)
            || record(&tx, &s.spec.update_id)? != root
        {
            return Err(bad("Activation graph/root readback differs"));
        }
        timing.mark("final_progress_before", None, None);
        guard.require_progress(&r).map_err(changed)?;
        timing.mark("commit_before", None, None);
        tx.commit().map_err(storage)?;
        timing.mark("committed", None, None);
        Ok(ActivationPreparationAdmission::Prepared(Box::new(
            ActivationPreparation {
                record: value,
                channel_nonce: nonce,
                deadline,
            },
        )))
    }
    /// No mutation or reconstructed grant: compare the fresh in-memory object
    /// with the complete retained graph and this exact original live actor.
    #[cfg(windows)]
    pub(crate) fn require_activation_prepared(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
    ) -> Result<RuntimeReplacementRecord> {
        let mut timing = crate::activation::ActivationTiming::new(
            "require_prepared",
            Some(preparation.deadline),
        );
        preparation.require_live_window().map_err(changed)?;
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        check_authority(&tx, a)?;
        runtime_baseline::current_supervisor(&tx, a, s)?;
        let expected = preparation.record();
        let source = &expected.intent.source;
        timing.mark("graph_before", None, None);
        let (saved, replaced) = read_graph(&tx, &s.spec.update_id)?
            .ok_or_else(|| changed("Activation preparation missing"))?;
        timing.mark("graph_after", None, None);
        let root = record(&tx, &s.spec.update_id)?;
        if saved != *expected
            || expected.revision != 0
            || !matches!(expected.state, ActivationState::Prepared {})
            || source.update_id != s.spec.update_id
            || source.owner_id != a.owner_id()
            || source.owner_epoch != a.epoch()
            || source.launch_id != s.spec.launch_id
            || root.phase != UpdatePhase::VerifyingLocal
            || root.revision != source.parent_revision
            || crate::sha256(preparation.channel_nonce.as_bytes())
                != expected.intent.context.channel_nonce_sha256
            || s.process.as_ref()
                != Some(
                    &winbridge_platform::infrastructure::stamp(std::process::id())
                        .map_err(storage)?,
                )
        {
            return Err(changed("Activation files require exact Prepared root, resident nonce and original live owner"));
        }
        source.validate(&replaced).map_err(changed)?;
        tx.commit().map_err(storage)?;
        // A slow graph read is not permission to continue after the live window.
        preparation.require_live_window().map_err(changed)?;
        timing.mark("validated", None, None);
        Ok(replaced)
    }
}

#[cfg(test)]
mod tests;
