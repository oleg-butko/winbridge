//! Schema14 preparation ledger. Saved progress never recreates a driver permit.
use super::*;
use crate::runtime_preparation::*;
use winbridge_protocol::update::SupervisorRecord;
fn bad(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, e)
}
fn digest(v: &impl Serialize) -> Result<String> {
    crate::runtime_baseline::digest(v).map_err(bad)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE runtime_preparation_intents(id TEXT PRIMARY KEY REFERENCES state_backup_records(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_preparation_records(id TEXT PRIMARY KEY REFERENCES runtime_preparation_intents(id),revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=13),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_preparation_history(id TEXT NOT NULL REFERENCES runtime_preparation_records(id),revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=13),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER runtime_preparation_intent_immutable BEFORE UPDATE ON runtime_preparation_intents BEGIN SELECT RAISE(ABORT,'Preparation intent immutable');END;
CREATE TRIGGER runtime_preparation_intent_retained BEFORE DELETE ON runtime_preparation_intents BEGIN SELECT RAISE(ABORT,'Preparation intent retained');END;
CREATE TRIGGER runtime_preparation_record_identity BEFORE UPDATE ON runtime_preparation_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.plan') IS NOT json_extract(OLD.record,'$.plan') OR json_extract(OLD.record,'$.phase') IN('prepared','indeterminate') BEGIN SELECT RAISE(ABORT,'Preparation identity/revision/terminal immutable');END;
CREATE TRIGGER runtime_preparation_record_retained BEFORE DELETE ON runtime_preparation_records BEGIN SELECT RAISE(ABORT,'Preparation record retained');END;
CREATE TRIGGER runtime_preparation_history_immutable BEFORE UPDATE ON runtime_preparation_history BEGIN SELECT RAISE(ABORT,'Preparation history immutable');END;
CREATE TRIGGER runtime_preparation_history_retained BEFORE DELETE ON runtime_preparation_history BEGIN SELECT RAISE(ABORT,'Preparation history retained');END;
PRAGMA user_version=14;
"#).map_err(storage)
}
/// Rebuild from the whole retained graph in this same SQLite read transaction.
/// No caller-supplied proof flags, hashes or paths are trusted here.
fn plan(
    c: &Connection,
    id: &str,
    attempt: String,
    set_id: &str,
    at: u64,
    available: u64,
) -> Result<RuntimePreparationPlan> {
    let root = record(c, id)?;
    let backup =
        state_backup::read(c, id)?.ok_or_else(|| bad("Preparation requires final backup"))?;
    if backup.phase != crate::state_backup::StateBackupPhase::Verified || at < backup.updated_at_ms
    {
        return Err(bad(
            "Preparation requires verified final backup before admission",
        ));
    }
    let shutdown = &backup.intent.binding.shutdown;
    let b = &shutdown.intent.binding;
    let s = supervisor::read(c, &b.launch_id)?;
    let baseline = runtime_baseline::read(c, id)?
        .receipt
        .ok_or_else(|| bad("Preparation baseline missing"))?;
    let preimages = runtime_preimages::read(c, id)?;
    let pi = preimages
        .intent
        .ok_or_else(|| bad("Preparation preimage intent missing"))?;
    let pr = preimages
        .receipt
        .ok_or_else(|| bad("Preparation preimages missing"))?;
    let artifacts = candidate::read(c, id)?;
    let health = health::read(c, id)?.ok_or_else(|| bad("Preparation health missing"))?;
    crate::health::validate_success(&health).map_err(bad)?;
    let ctx = health
        .context
        .as_ref()
        .ok_or_else(|| bad("Preparation health context missing"))?;
    let parent = Path::new(&s.spec.runtime_directory).join("update").join(id);
    if ctx.artifacts != artifacts
        || ctx.compatibility.identity != baseline.loaded_worker
        || ctx.worker_config != backup.intent.binding.worker_config
        || b.loaded_worker != baseline.loaded_worker
        || b.owner_id != artifacts.owner_id
        || b.owner_epoch != artifacts.owner_epoch
        || b.owner_id != pi.owner_id
        || b.owner_epoch != pi.owner_epoch
        || b.launch_id != pi.launch_id
        || b.update_id != id
        || backup.intent.binding.parent_directory != parent.to_string_lossy()
        || pi.parent_directory != backup.intent.binding.parent_directory
        || pi.parent_identity != backup.intent.binding.parent_identity
        || baseline.installed.runtime_directory != s.spec.runtime_directory
        || root.revision < backup.intent.parent_revision().map_err(bad)?
    {
        return Err(bad(
            "Preparation baseline/preimages/health/shutdown/backup graph differs",
        ));
    }
    let manifest = crate::installed_set::plan_replacement_manifest(
        &baseline.installed.manifest,
        &root.request.source,
        &artifacts.inspection,
        set_id,
    )
    .map_err(bad)?;
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(storage)?;
    let directory = parent.join("prepared-runtime");
    let mut files = Vec::new();
    for (index, file) in FILE_ORDER.into_iter().enumerate() {
        let (old, source, bytes, sha256) = if file == crate::runtime_exchange::RuntimeFile::Manifest
        {
            (
                baseline.installed.manifest_file.clone(),
                None,
                manifest_bytes.len() as u64,
                crate::sha256(&manifest_bytes),
            )
        } else {
            let source = artifacts.inspection.artifacts[index].image.clone();
            (
                baseline.installed.images[index].clone(),
                Some(source.clone()),
                source.bytes,
                source.sha256,
            )
        };
        files.push(PreparationFile {
            file,
            old,
            source,
            bytes,
            sha256,
            destination: directory
                .join(file.name())
                .to_str()
                .ok_or_else(|| bad("Prepared path UTF-8"))?
                .into(),
        });
    }
    let p = RuntimePreparationPlan {
        schema_version: 1,
        update_id: id.into(),
        attempt_id: attempt,
        parent_revision: backup.intent.parent_revision().map_err(bad)?,
        owner_id: b.owner_id.clone(),
        owner_epoch: b.owner_epoch,
        launch_id: b.launch_id.clone(),
        config_sha256: s.spec.config_sha256.clone(),
        runtime_directory: s.spec.runtime_directory.clone(),
        runtime_identity: baseline.installed.runtime_identity.clone(),
        parent_identity: pi.parent_identity,
        admitted_at_ms: at,
        available_bytes_before: available,
        proofs: PreparationProofs {
            request_sha256: digest(&root.request)?,
            supervisor_spec_sha256: digest(&s.spec)?,
            baseline_sha256: digest(&baseline)?,
            preimages_sha256: digest(&pr)?,
            artifacts_sha256: digest(&artifacts)?,
            health_sha256: digest(&health)?,
            shutdown_sha256: digest(shutdown)?,
            backup_sha256: digest(&backup)?,
        },
        manifest,
        files,
    };
    p.validate().map_err(bad)?;
    Ok(p)
}
fn ledger(c: &Connection, p: &RuntimePreparationPlan) -> Result<RuntimePreparationRecord> {
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM runtime_preparation_records WHERE id=?1",
            [&p.update_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: RuntimePreparationRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.plan != *p || current.revision != counter(revision)? {
        return Err(bad("Preparation intent/current differs"));
    }
    let mut query = c.prepare("SELECT revision,record FROM runtime_preparation_history WHERE id=?1 ORDER BY revision LIMIT 15").map_err(storage)?;
    let rows = query
        .query_map([&p.update_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Preparation history missing/oversized"));
    }
    let mut previous = RuntimePreparationRecord::planned(p.clone()).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: RuntimePreparationRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Preparation history sequence gap"));
        }
        if index == 0 {
            if previous != next {
                return Err(bad("Preparation history must start Planned"));
            }
        } else {
            previous.validate_step(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Preparation current/history differ"));
    }
    Ok(current)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<RuntimePreparationRecord>> {
    if schema(c)? < 14 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_preparation_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(None) };
    let saved: RuntimePreparationPlan = decode(&raw)?;
    saved.validate().map_err(bad)?;
    let expected = plan(
        c,
        id,
        saved.attempt_id.clone(),
        &saved.manifest.set_id,
        saved.admitted_at_ms,
        saved.available_bytes_before,
    )?;
    if expected != saved {
        return Err(bad(
            "Preparation intent differs from its retained causal graph",
        ));
    }
    Ok(Some(ledger(c, &saved)?))
}
fn insert(c: &Connection, r: &RuntimePreparationRecord) -> Result<()> {
    r.validate().map_err(bad)?;
    if *r != RuntimePreparationRecord::planned(r.plan.clone()).map_err(bad)? {
        return Err(bad("Preparation insert must be Planned"));
    }
    one(c
        .execute(
            "INSERT INTO runtime_preparation_intents VALUES(?1,?2)",
            params![r.plan.update_id, encode(&r.plan)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_preparation_records VALUES(?1,0,?2)",
            params![r.plan.update_id, encode(r)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_preparation_history VALUES(?1,0,?2)",
            params![r.plan.update_id, encode(r)?],
        )
        .map_err(storage)?)?;
    if ledger(c, &r.plan)? != *r {
        return Err(bad("Preparation admission readback differs"));
    }
    Ok(())
}
fn persist(
    c: &Connection,
    old: &RuntimePreparationRecord,
    next: &RuntimePreparationRecord,
) -> Result<()> {
    old.validate_step(next).map_err(bad)?;
    if ledger(c, &old.plan)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Preparation revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE runtime_preparation_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4", params![next.revision as i64,raw,old.plan.update_id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_preparation_history VALUES(?1,?2,?3)",
            params![old.plan.update_id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, &next.plan)? != *next {
        return Err(bad("Preparation append readback differs"));
    }
    Ok(())
}
fn current(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: &RuntimePreparationPlan,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let root = record(c, &p.update_id)?;
    if p.owner_id != a.owner_id()
        || p.owner_epoch != a.epoch()
        || p.launch_id != s.spec.launch_id
        || s.spec.update_id != p.update_id
        || root.phase != UpdatePhase::Replacing
        || root.revision != p.parent_revision
        || p.config_sha256 != s.spec.config_sha256
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Preparation requires original live owner/launch/unchanged Replacing root",
        ));
    }
    Ok(())
}
fn physical(
    c: &Connection,
    p: &RuntimePreparationPlan,
    guard: &crate::shutdown::ShutdownExitGuard,
) -> Result<()> {
    let backup = state_backup::read(c, &p.update_id)?.ok_or_else(|| bad("Final backup missing"))?;
    guard
        .require(&backup.intent.binding.shutdown.intent)
        .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
    for (path, id) in [
        (
            Path::new(&p.runtime_directory).to_path_buf(),
            &p.runtime_identity,
        ),
        (
            p.directory()
                .parent()
                .ok_or_else(|| bad("Preparation parent missing"))?
                .to_path_buf(),
            &p.parent_identity,
        ),
    ] {
        if winbridge_platform::source_files::directory_identity(&path).map_err(storage)? != *id {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Preparation parent/runtime rebound",
            ));
        }
    }
    if let Some(dir) = backup.report.as_ref() {
        // Admission cannot substitute a copied or changed database for the exact final backup.
        let pin = crate::state_backup::pin_backup_file(Path::new(&dir.backup.path))
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        if pin.evidence() != &dir.backup {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Final backup ID/size/hash changed",
            ));
        }
    } else {
        return Err(bad("Verified backup report missing"));
    }
    Ok(())
}
impl UpdateJournal {
    pub(crate) fn runtime_preparation(&self, id: &str) -> Result<Option<RuntimePreparationRecord>> {
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let result = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    pub(crate) fn require_runtime_preparation_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        r: &RuntimePreparationRecord,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        current(&tx, a, s, &r.plan)?;
        if read(&tx, &r.plan.update_id)?.as_ref() != Some(r) {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Preparation current record changed",
            ));
        }
        tx.commit().map_err(storage)
    }
    /// Small fence check for streaming chunks; the complete graph is checked
    /// before/after each file while the same source/directory pins remain held.
    pub(crate) fn preparation_stream_fence(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        p: &RuntimePreparationPlan,
    ) -> Result<()> {
        self.check_directory(a)?;
        current(&self.connection, a, s, p)
    }
    pub fn inspect_runtime_preparation(
        directory: &Path,
        id: &str,
    ) -> Result<Option<RuntimePreparationRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(r)
    }
    /// Only a new committed intent under original physical custody returns a permit.
    /// Existing/pending/terminal rows are observations and cannot resume effects.
    pub fn admit_runtime_preparation(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        guard: Option<&crate::shutdown::ShutdownExitGuard>,
    ) -> Result<RuntimePreparationAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(r) = read(&tx, &s.spec.update_id)? {
            return Ok(RuntimePreparationAdmission::Existing(Box::new(r)));
        }
        check_authority(&tx, a)?;
        runtime_baseline::current_supervisor(&tx, a, s)?;
        let h = health::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Preparation health missing"))?;
        crate::health::validate_fresh_success(&h)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let cfg = crate::preparation::configuration(s)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        if Some(crate::artifact::inspect_image(&cfg.worker_config_path).map_err(storage)?)
            != h.context.map(|v| v.worker_config)
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Preparation worker config changed",
            ));
        }
        let available = fs2::available_space(&cfg.runtime_directory).map_err(storage)?;
        let p = plan(
            &tx,
            &s.spec.update_id,
            Uuid::new_v4().to_string(),
            &Uuid::new_v4().to_string(),
            now_ms()?,
            available,
        )?;
        current(&tx, a, s, &p)?;
        physical(
            &tx,
            &p,
            guard.ok_or_else(|| {
                fail(
                    JournalErrorCode::PreconditionsChanged,
                    "Preparation needs original OS custody",
                )
            })?,
        )?;
        let r = RuntimePreparationRecord::planned(p.clone()).map_err(bad)?;
        insert(&tx, &r)?;
        if read(&tx, &p.update_id)?.as_ref() != Some(&r) {
            return Err(bad("Preparation causal admission readback differs"));
        }
        tx.commit().map_err(storage)?;
        Ok(RuntimePreparationAdmission::Fresh(Box::new(
            RuntimePreparationPermit { plan: p },
        )))
    }
    /// Record-only trusted callback. It cannot issue a replacement/launch permit.
    /// Future preparation IO must independently pin/hash actual effects before invoking it.
    pub fn update_runtime_preparation(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        old: &RuntimePreparationRecord,
        next: RuntimePreparationRecord,
        permit: &RuntimePreparationPermit,
        guard: &crate::shutdown::ShutdownExitGuard,
    ) -> Result<RuntimePreparationRecord> {
        // Reject changed scope before following any path in a callback.
        old.validate_step(&next).map_err(bad)?;
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        current(&tx, a, s, &old.plan)?;
        if permit.plan != old.plan || read(&tx, &old.plan.update_id)?.as_ref() != Some(old) {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Preparation permit/scope/revision changed",
            ));
        }
        // Recording uncertainty grants no effect. Missing/changed files must not
        // prevent retaining the original issued prefix and its failure reason.
        if next.phase != PreparationPhase::Indeterminate {
            physical(&tx, &old.plan, guard)?;
            if let Some(id) = &next.directory_identity {
                if winbridge_platform::source_files::directory_identity(&next.plan.directory())
                    .map_err(storage)?
                    != *id
                {
                    return Err(fail(
                        JournalErrorCode::PreconditionsChanged,
                        "Prepared directory changed",
                    ));
                }
            }
            for state in &next.files {
                if let PreparationFileState::Verified { image } = state {
                    if crate::artifact::inspect_image(Path::new(&image.path)).map_err(storage)?
                        != *image
                    {
                        return Err(fail(
                            JournalErrorCode::PreconditionsChanged,
                            "Prepared file changed",
                        ));
                    }
                }
            }
        }
        persist(&tx, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests;
