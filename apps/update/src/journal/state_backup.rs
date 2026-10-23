//! Schema13 final-backup outbox. A saved row cannot recreate physical custody.
use super::*;
use crate::{
    shutdown::{ShutdownDeliveryState, ShutdownExitGuard},
    state_backup::*,
};
use winbridge_protocol::update::{SupervisorRecord, UpdatePhase};
fn bad(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, e)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE state_backup_intents(id TEXT PRIMARY KEY REFERENCES shutdown_intents(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE state_backup_records(id TEXT PRIMARY KEY REFERENCES state_backup_intents(id),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE state_backup_history(id TEXT NOT NULL REFERENCES state_backup_records(id),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER state_backup_intent_immutable BEFORE UPDATE ON state_backup_intents BEGIN SELECT RAISE(ABORT,'Backup intent immutable');END;
CREATE TRIGGER state_backup_intent_retained BEFORE DELETE ON state_backup_intents BEGIN SELECT RAISE(ABORT,'Backup intent retained');END;
CREATE TRIGGER state_backup_record_identity BEFORE UPDATE ON state_backup_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') OR json_extract(OLD.record,'$.phase') IN('verified','indeterminate') BEGIN SELECT RAISE(ABORT,'Backup identity/revision/terminal immutable');END;
CREATE TRIGGER state_backup_record_retained BEFORE DELETE ON state_backup_records BEGIN SELECT RAISE(ABORT,'Backup record retained');END;
CREATE TRIGGER state_backup_history_immutable BEFORE UPDATE ON state_backup_history BEGIN SELECT RAISE(ABORT,'Backup history immutable');END;
CREATE TRIGGER state_backup_history_retained BEFORE DELETE ON state_backup_history BEGIN SELECT RAISE(ABORT,'Backup history retained');END;
PRAGMA user_version=13;
"#).map_err(storage)
}
fn step(old: &StateBackupRecord, next: &StateBackupRecord) -> Result<()> {
    old.validate().map_err(bad)?;
    next.validate().map_err(bad)?;
    let expected = match next.phase {
        StateBackupPhase::Issued => old.issue(
            next.input_file
                .clone()
                .ok_or_else(|| bad("Backup input missing"))?,
            next.updated_at_ms,
        ),
        StateBackupPhase::ResumeGranted => old.grant_resume(
            next.process
                .clone()
                .ok_or_else(|| bad("Backup PID missing"))?,
            next.updated_at_ms,
        ),
        StateBackupPhase::Running => old.resumed(next.updated_at_ms),
        StateBackupPhase::Cleaned => old.finished(
            next.cleanup
                .clone()
                .ok_or_else(|| bad("Backup cleanup missing"))?,
            next.updated_at_ms,
        ),
        StateBackupPhase::Verified => old.verify(
            next.report
                .as_deref()
                .cloned()
                .ok_or_else(|| bad("Backup report missing"))?,
            next.updated_at_ms,
        ),
        StateBackupPhase::Indeterminate => old.indeterminate(
            next.reason
                .clone()
                .ok_or_else(|| bad("Backup reason missing"))?,
            next.updated_at_ms,
        ),
        _ => return Err(bad("Backup cannot return to Prepared")),
    }
    .map_err(bad)?;
    if expected != *next || next.revision != old.revision + 1 {
        return Err(bad("Backup history must be one exact causal append"));
    }
    Ok(())
}
fn ledger(c: &Connection, intent: &StateBackupIntent) -> Result<StateBackupRecord> {
    let id = intent.update_id();
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM state_backup_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: StateBackupRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.intent != *intent || current.revision != counter(revision)? {
        return Err(bad("Backup intent/current revision differs"));
    }
    let mut query=c.prepare("SELECT revision,record FROM state_backup_history WHERE id=?1 ORDER BY revision LIMIT 8").map_err(storage)?;
    let rows = query
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Backup history missing/oversized"));
    }
    let mut previous = StateBackupRecord::prepare(intent.clone()).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: StateBackupRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Backup history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Backup history did not start Prepared"));
            }
        } else {
            step(&previous, &next)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Backup current/history differs"));
    }
    Ok(current)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<StateBackupRecord>> {
    if schema(c)? < 13 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM state_backup_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(None) };
    let intent: StateBackupIntent = decode(&raw)?;
    intent.validate().map_err(bad)?;
    let b = &intent.binding;
    let old = &b.shutdown.intent.binding;
    let root = record(c, id)?;
    let sup = supervisor::read(c, &old.launch_id)?;
    let health = health::read(c, id)?.ok_or_else(|| bad("Backup health missing"))?;
    let context = health
        .context
        .ok_or_else(|| bad("Backup health context missing"))?;
    let expected = Path::new(&sup.spec.runtime_directory)
        .join("update")
        .join(id);
    if intent.update_id() != id
        || shutdown::read(c, id)?.as_ref() != Some(b.shutdown.as_ref())
        || root.revision < intent.parent_revision().map_err(bad)?
        || sup.spec.update_id != id
        || sup.spec.worker.as_ref() != Some(&old.worker)
        || sup.owner_id.as_deref() != Some(&old.owner_id)
        || sup.owner_epoch != Some(old.owner_epoch)
        || context.worker_config != b.worker_config
        || context.compatibility.worker_journal_schema != b.worker_journal_schema
        || expected.to_str() != Some(&b.parent_directory)
    {
        return Err(bad(
            "Backup retained shutdown/health/policy/owner/root scope differs",
        ));
    }
    Ok(Some(ledger(c, &intent)?))
}
fn require(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    intent: &StateBackupIntent,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let b = &intent.binding.shutdown.intent.binding;
    let root = record(c, &b.update_id)?;
    if b.owner_id != a.owner_id()
        || b.owner_epoch != a.epoch()
        || s.spec.update_id != b.update_id
        || s.spec.launch_id != b.launch_id
        || root.phase != UpdatePhase::Replacing
        || root.revision != intent.parent_revision().map_err(bad)?
    {
        return Err(fail(JournalErrorCode::PreconditionsChanged,"Backup requires original current owner and unchanged Replacing root; never adopt saved exit"));
    }
    Ok(())
}
fn insert(c: &Connection, r: &StateBackupRecord) -> Result<()> {
    let id = r.intent.update_id();
    one(c
        .execute(
            "INSERT INTO state_backup_intents VALUES(?1,?2)",
            params![id, encode(&r.intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO state_backup_records VALUES(?1,0,?2)",
            params![id, encode(r)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO state_backup_history VALUES(?1,0,?2)",
            params![id, encode(r)?],
        )
        .map_err(storage)?)?;
    Ok(())
}
fn persist(c: &Connection, old: &StateBackupRecord, next: &StateBackupRecord) -> Result<()> {
    step(old, next)?;
    if ledger(c, &old.intent)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Backup revision changed",
        ));
    }
    let id = old.intent.update_id();
    let raw = encode(next)?;
    one(c
        .execute(
            "UPDATE state_backup_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",
            params![next.revision as i64, raw, id, old.revision as i64],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO state_backup_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, &next.intent)? != *next {
        return Err(bad("Backup write readback differs"));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn state_backup(&self, id: &str) -> Result<Option<StateBackupRecord>> {
        read(&self.connection, id)
    }
    /// Metadata checks for the already-running trusted driver; no launch permit.
    pub(crate) fn require_state_backup_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        r: &StateBackupRecord,
    ) -> Result<()> {
        self.check_directory(a)?;
        require(&self.connection, a, s, &r.intent)?;
        if read(&self.connection, r.intent.update_id())?.as_ref() != Some(r)
            || winbridge_platform::source_files::directory_identity(Path::new(
                &r.intent.binding.parent_directory,
            ))
            .map_err(storage)?
                != r.intent.binding.parent_identity
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Backup record/directory changed",
            ));
        }
        Ok(())
    }

    pub fn inspect_state_backup(directory: &Path, id: &str) -> Result<Option<StateBackupRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(r)
    }
    /// Readonly metadata for the old helper. This never constructs a launch
    /// permit or physical exit guard. The helper must independently pin/check
    /// its own image, process, policy paths and the live owning supervisor.
    pub fn inspect_state_backup_export(
        directory: &Path,
        id: &str,
        process: &winbridge_protocol::update::SupervisorProcess,
        input: &winbridge_protocol::update::StateBackupFileEvidence,
    ) -> Result<(UpdateJournalInfo, SupervisorRecord, StateBackupRecord)> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        if schema(&tx)? != UPDATE_SCHEMA_VERSION {
            return Err(fail(
                JournalErrorCode::UnsupportedSchema,
                "Final state export requires the exporter's compiled updater schema; no migration",
            ));
        }
        let saved = read(&tx, id)?.ok_or_else(|| {
            fail(
                JournalErrorCode::NotFound,
                "No saved final backup attempt; export denied",
            )
        })?;
        saved
            .require_export_process(process, input)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let old = &saved.intent.binding.shutdown.intent.binding;
        let info = info(&tx)?;
        let sup = supervisor::read(&tx, &old.launch_id)?;
        let owner: String = tx
            .query_row(
                "SELECT owner FROM installation WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let root = record(&tx, id)?;
        if info.active_update_id.as_deref() != Some(id)
            || owner != old.owner_id
            || info.owner_epoch != old.owner_epoch
            || sup.phase != winbridge_protocol::update::SupervisorPhase::Active
            || sup.process.is_none()
            || sup.process.as_ref() == Some(process)
            || sup.owner_id.as_deref() != Some(owner.as_str())
            || sup.owner_epoch != Some(info.owner_epoch)
            || root.phase != UpdatePhase::Replacing
            || root.revision != saved.intent.parent_revision().map_err(bad)?
        {
            return Err(fail(JournalErrorCode::PreconditionsChanged, "Export owner/supervisor/root changed; saved Exited cannot authorize another helper"));
        }
        tx.commit().map_err(storage)?;
        Ok((info, sup, saved))
    }
    /// Existing rows are observations. Only live kernel custody can admit a
    /// fresh intent; neither saved Exited nor arbitrary caller paths suffice.
    pub fn admit_state_backup(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        guard: Option<&ShutdownExitGuard>,
    ) -> Result<StateBackupAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(saved) = read(&tx, &s.spec.update_id)? {
            return Ok(StateBackupAdmission::Existing(Box::new(saved)));
        }
        check_authority(&tx, a)?;
        runtime_baseline::current_supervisor(&tx, a, s)?;
        let shutdown = shutdown::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Backup requires committed shutdown"))?;
        let guard = guard.ok_or_else(|| {
            fail(
                JournalErrorCode::PreconditionsChanged,
                "Fresh backup requires retained OS custody",
            )
        })?;
        guard
            .require(&shutdown.intent)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let cfg = crate::preparation::configuration(s)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let health =
            health::read(&tx, &s.spec.update_id)?.ok_or_else(|| bad("Backup health missing"))?;
        let context = health
            .context
            .ok_or_else(|| bad("Backup health context missing"))?;
        if crate::artifact::inspect_image(&cfg.worker_config_path).map_err(storage)?
            != context.worker_config
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Backup config differs from approved health",
            ));
        }
        let parent = Path::new(&s.spec.runtime_directory)
            .join("update")
            .join(&s.spec.update_id);
        let parent_identity =
            winbridge_platform::source_files::directory_identity(&parent).map_err(storage)?;
        let ShutdownDeliveryState::Exited { at_ms, .. } = &shutdown.delivery else {
            return Err(bad("Backup shutdown exit missing"));
        };
        let at = now_ms()?.max(*at_ms);
        let intent = StateBackupIntent::from_exited(
            StateBackupBinding {
                shutdown: Box::new(shutdown),
                worker_config: context.worker_config,
                worker_journal_schema: context.compatibility.worker_journal_schema,
                parent_directory: parent
                    .to_str()
                    .ok_or_else(|| bad("Backup directory UTF-8"))?
                    .into(),
                parent_identity,
                admitted_at_ms: at,
            },
            Uuid::new_v4().to_string(),
        )
        .map_err(bad)?;
        require(&tx, a, s, &intent)?;
        let initial = StateBackupRecord::prepare(intent.clone()).map_err(bad)?;
        insert(&tx, &initial)?;
        if read(&tx, &s.spec.update_id)?.as_ref() != Some(&initial) {
            return Err(bad("Backup admission readback differs"));
        }
        tx.commit().map_err(storage)?;
        Ok(StateBackupAdmission::Fresh(Box::new(StateBackupPermit {
            intent,
        })))
    }
    /// Append data under the original live custody. This records a transition;
    /// callers must independently verify helper effects before accepting a report.
    pub fn update_state_backup(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        old: &StateBackupRecord,
        next: StateBackupRecord,
        guard: &ShutdownExitGuard,
    ) -> Result<StateBackupRecord> {
        self.check_directory(a)?;
        guard
            .require(&old.intent.binding.shutdown.intent)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, &old.intent)?;
        if read(&tx, old.intent.update_id())?.as_ref() != Some(old) {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Backup current revision/scope changed",
            ));
        }
        let parent = Path::new(&old.intent.binding.parent_directory);
        if winbridge_platform::source_files::directory_identity(parent).map_err(storage)?
            != old.intent.binding.parent_identity
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Backup directory rebound",
            ));
        }
        persist(&tx, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}
#[cfg(test)]
#[path = "state_backup_tests.rs"]
mod tests;
