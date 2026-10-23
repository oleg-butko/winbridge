//! Schema15: replacement acceptance/history only. No rename-success callback yet.
use super::*;
use crate::runtime_replacement::*;
use winbridge_protocol::update::SupervisorRecord;
fn bad(e: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, e)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE runtime_replacement_intents(id TEXT PRIMARY KEY REFERENCES runtime_preparation_records(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_replacement_records(id TEXT PRIMARY KEY REFERENCES runtime_replacement_intents(id),revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=23),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_replacement_history(id TEXT NOT NULL REFERENCES runtime_replacement_records(id),revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=23),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER runtime_replacement_intent_immutable BEFORE UPDATE ON runtime_replacement_intents BEGIN SELECT RAISE(ABORT,'Replacement intent immutable');END;
CREATE TRIGGER runtime_replacement_intent_retained BEFORE DELETE ON runtime_replacement_intents BEGIN SELECT RAISE(ABORT,'Replacement intent retained');END;
CREATE TRIGGER runtime_replacement_record_identity BEFORE UPDATE ON runtime_replacement_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') OR json_extract(OLD.record,'$.state.phase') IN('files_moved','indeterminate') BEGIN SELECT RAISE(ABORT,'Replacement identity/revision/terminal immutable');END;
CREATE TRIGGER runtime_replacement_record_retained BEFORE DELETE ON runtime_replacement_records BEGIN SELECT RAISE(ABORT,'Replacement record retained');END;
CREATE TRIGGER runtime_replacement_history_immutable BEFORE UPDATE ON runtime_replacement_history BEGIN SELECT RAISE(ABORT,'Replacement history immutable');END;
CREATE TRIGGER runtime_replacement_history_retained BEFORE DELETE ON runtime_replacement_history BEGIN SELECT RAISE(ABORT,'Replacement history retained');END;
PRAGMA user_version=15;
"#).map_err(storage)
}
fn id(intent: &RuntimeReplacementIntent) -> &str {
    &intent.preparation.plan.update_id
}
fn ledger(c: &Connection, intent: &RuntimeReplacementIntent) -> Result<RuntimeReplacementRecord> {
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM runtime_replacement_records WHERE id=?1",
            [id(intent)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: RuntimeReplacementRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if &current.intent != intent || current.revision != counter(revision)? {
        return Err(bad("Replacement intent/current differs"));
    }
    let mut statement = c.prepare("SELECT revision,record FROM runtime_replacement_history WHERE id=?1 ORDER BY revision LIMIT 25").map_err(storage)?;
    let rows = statement
        .query_map([id(intent)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Replacement history missing/oversized"));
    }
    let mut previous = RuntimeReplacementRecord::planned(intent.clone()).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: RuntimeReplacementRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Replacement history sequence gap"));
        }
        if index == 0 {
            if previous != next {
                return Err(bad("Replacement history must begin Planned"));
            }
        } else {
            previous.advance(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Replacement current/history differ"));
    }
    Ok(current)
}
pub(super) fn read(c: &Connection, update_id: &str) -> Result<Option<RuntimeReplacementRecord>> {
    if schema(c)? < 15 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_replacement_intents WHERE id=?1",
            [update_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let Some(raw) = raw else { return Ok(None) };
    let intent: RuntimeReplacementIntent = decode(&raw)?;
    intent.validate().map_err(bad)?;
    if id(&intent) != update_id
        || runtime_preparation::read(c, update_id)?.as_ref() != Some(&intent.preparation)
    {
        return Err(bad(
            "Replacement differs from retained preparation/causal graph",
        ));
    }
    Ok(Some(ledger(c, &intent)?))
}
fn insert(c: &Connection, r: &RuntimeReplacementRecord) -> Result<()> {
    r.validate().map_err(bad)?;
    if *r != RuntimeReplacementRecord::planned(r.intent.clone()).map_err(bad)? {
        return Err(bad("Replacement admission must begin Planned"));
    }
    one(c
        .execute(
            "INSERT INTO runtime_replacement_intents VALUES(?1,?2)",
            params![id(&r.intent), encode(&r.intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_replacement_records VALUES(?1,0,?2)",
            params![id(&r.intent), encode(r)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_replacement_history VALUES(?1,0,?2)",
            params![id(&r.intent), encode(r)?],
        )
        .map_err(storage)?)?;
    if ledger(c, &r.intent)? != *r {
        return Err(bad("Replacement admission readback differs"));
    }
    Ok(())
}
fn persist(
    c: &Connection,
    old: &RuntimeReplacementRecord,
    next: &RuntimeReplacementRecord,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    if ledger(c, &old.intent)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Replacement revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE runtime_replacement_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id(&old.intent),old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO runtime_replacement_history VALUES(?1,?2,?3)",
            params![id(&old.intent), next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, &old.intent)? != *next {
        return Err(bad("Replacement append readback differs"));
    }
    Ok(())
}
fn current(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    intent: &RuntimeReplacementIntent,
) -> Result<()> {
    runtime_preparation::current(c, a, s, &intent.preparation.plan)
}
/// Keep all checked files/directories pinned until admission COMMIT. This is
/// still not consuming custody: the future guard must revalidate before rename.
struct AdmissionPins {
    _files: Vec<crate::artifact::Pinned>,
    _directories: Vec<winbridge_platform::source_files::PinnedDirectory>,
    _backup: crate::state_backup::StateBackupFilePin,
}
fn physical(
    c: &Connection,
    s: &SupervisorRecord,
    r: &crate::runtime_preparation::RuntimePreparationRecord,
    guard: &crate::shutdown::ShutdownExitGuard,
) -> Result<AdmissionPins> {
    runtime_preparation::physical(c, &r.plan, guard)?;
    let cfg = crate::preparation::configuration(s).map_err(storage)?;
    if cfg.runtime_directory.canonicalize().map_err(storage)?
        != Path::new(&r.plan.runtime_directory)
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Replacement policy runtime differs",
        ));
    }
    let backup = state_backup::read(c, &r.plan.update_id)?
        .ok_or_else(|| bad("Replacement final backup missing"))?;
    let report = backup
        .report
        .ok_or_else(|| bad("Replacement verified backup report missing"))?;
    let backup_pin =
        crate::state_backup::pin_backup_file(Path::new(&report.backup.path)).map_err(storage)?;
    if backup_pin.evidence() != &report.backup {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Replacement final backup changed",
        ));
    }
    let mut pins = Vec::new();
    for (path, hash) in [
        (Path::new(&s.spec.config_path), &s.spec.config_sha256),
        (
            &cfg.worker_config_path,
            &backup.intent.binding.worker_config.sha256,
        ),
    ] {
        let pin = crate::artifact::pin(path).map_err(storage)?;
        if pin.evidence.sha256 != *hash {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Replacement configuration changed",
            ));
        }
        pins.push(pin);
    }
    if pins[1].evidence != backup.intent.binding.worker_config {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Replacement worker configuration identity differs",
        ));
    }
    let previous = runtime_preimages::read(c, &r.plan.update_id)?;
    let pi = previous
        .intent
        .ok_or_else(|| bad("Replacement preimage intent missing"))?;
    let pr = previous
        .receipt
        .ok_or_else(|| bad("Replacement preimage receipt missing"))?;
    let artifacts = candidate::read(c, &r.plan.update_id)?;
    let payload = Path::new(&r.plan.runtime_directory)
        .join("update")
        .join(&r.plan.update_id)
        .join("candidate/payload");
    let mut dirs = Vec::new();
    for (path, identity) in [
        (
            Path::new(&r.plan.runtime_directory),
            r.plan.runtime_identity.as_str(),
        ),
        (
            r.plan
                .directory()
                .parent()
                .ok_or_else(|| bad("Replacement parent missing"))?,
            r.plan.parent_identity.as_str(),
        ),
        (
            r.plan.directory().as_path(),
            r.directory_identity
                .as_deref()
                .ok_or_else(|| bad("Prepared directory receipt missing"))?,
        ),
        (Path::new(&pi.directory), pr.directory_identity.as_str()),
    ] {
        dirs.push(
            winbridge_platform::source_files::PinnedDirectory::acquire(path, Some(identity))
                .map_err(storage)?,
        );
    }
    dirs.push(
        winbridge_platform::source_files::PinnedDirectory::acquire(
            &payload,
            Some(&artifacts.inspection.payload_directory_identity),
        )
        .map_err(storage)?,
    );
    let batch = r.exchange_batch().map_err(bad)?;
    for image in batch
        .files
        .iter()
        .map(|f| &f.spec.prepared)
        .chain(&pr.files)
        .chain(r.plan.files.iter().filter_map(|f| f.source.as_ref()))
    {
        let pin = crate::artifact::pin(Path::new(&image.path)).map_err(storage)?;
        if pin.evidence != *image {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Replacement prepared/source/previous file changed",
            ));
        }
        pins.push(pin);
    }
    let names = fs::read_dir(r.plan.directory())
        .map_err(storage)?
        .map(|e| e.map(|e| e.file_name()).map_err(storage))
        .collect::<Result<std::collections::HashSet<_>>>()?;
    if names
        != r.plan
            .files
            .iter()
            .map(|f| std::ffi::OsString::from(f.file.name()))
            .collect()
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Replacement prepared directory contains unexpected entries",
        ));
    }
    Ok(AdmissionPins {
        _files: pins,
        _directories: dirs,
        _backup: backup_pin,
    })
}
impl UpdateJournal {
    pub fn inspect_runtime_replacement(
        directory: &Path,
        update_id: &str,
    ) -> Result<Option<RuntimeReplacementRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, update_id)?;
        tx.commit().map_err(storage)?;
        Ok(r)
    }
    pub fn admit_runtime_replacement(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        guard: Option<&crate::shutdown::ShutdownExitGuard>,
    ) -> Result<RuntimeReplacementAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(saved) = read(&tx, &s.spec.update_id)? {
            return Ok(RuntimeReplacementAdmission::Existing(Box::new(saved)));
        }
        if !cfg!(windows) {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Fresh runtime replacement admission requires native Windows ownership",
            ));
        }
        let prepared = runtime_preparation::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Replacement preparation missing"))?;
        let intent = RuntimeReplacementIntent {
            schema_version: 1,
            attempt_id: Uuid::new_v4().to_string(),
            admitted_at_ms: now_ms()?,
            preparation: prepared,
        };
        current(&tx, a, s, &intent)?;
        let health = health::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Replacement health missing"))?;
        crate::health::validate_fresh_success(&health)
            .map_err(|e| fail(JournalErrorCode::PreconditionsChanged, e))?;
        let r = RuntimeReplacementRecord::planned(intent.clone()).map_err(bad)?;
        let _pins = physical(
            &tx,
            s,
            &r.intent.preparation,
            guard.ok_or_else(|| {
                fail(
                    JournalErrorCode::PreconditionsChanged,
                    "Replacement needs original OS custody",
                )
            })?,
        )?;
        insert(&tx, &r)?;
        if read(&tx, id(&intent))?.as_ref() != Some(&r) {
            return Err(bad("Replacement causal admission readback differs"));
        }
        tx.commit().map_err(storage)?;
        Ok(RuntimeReplacementAdmission::Fresh(Box::new(
            RuntimeReplacementPermit { intent },
        )))
    }
    /// No success receipts are exposed until consuming physical replacement guard exists.
    pub fn mark_runtime_replacement_indeterminate(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        expected: &RuntimeReplacementRecord,
        permit: &RuntimeReplacementPermit,
        reason: String,
    ) -> Result<RuntimeReplacementRecord> {
        let next = expected.indeterminate(reason).map_err(bad)?;
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        current(&tx, a, s, &expected.intent)?;
        if permit.intent != expected.intent
            || read(&tx, id(&expected.intent))?.as_ref() != Some(expected)
        {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Replacement permission/attempt/revision differs",
            ));
        }
        persist(&tx, expected, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}
#[cfg(test)]
mod tests;
