//! Schema19: immutable writer intent and causal delivery history.
//! These rows do not recreate a private channel, child or startup permit.
use super::*;
use crate::activation::{writer, ActivationRecord, ActivationWriterRecord};
use winbridge_protocol::update::StateBackupAuthority;
#[cfg(windows)]
use winbridge_protocol::update::SupervisorRecord;

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_writer_intents(id TEXT PRIMARY KEY REFERENCES activation_intents(id),command_id TEXT NOT NULL UNIQUE,command TEXT NOT NULL CHECK(json_valid(command)));
CREATE TABLE activation_writer_records(id TEXT PRIMARY KEY REFERENCES activation_writer_intents(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE activation_writer_history(id TEXT NOT NULL REFERENCES activation_writer_records(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER activation_writer_intent_immutable BEFORE UPDATE ON activation_writer_intents BEGIN SELECT RAISE(ABORT,'Writer intent immutable');END;
CREATE TRIGGER activation_writer_intent_retained BEFORE DELETE ON activation_writer_intents BEGIN SELECT RAISE(ABORT,'Writer intent retained');END;
CREATE TRIGGER activation_writer_record_identity BEFORE UPDATE ON activation_writer_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command') OR json_extract(OLD.record,'$.state.phase')='confirmed' BEGIN SELECT RAISE(ABORT,'Writer command/revision/confirmation immutable');END;
CREATE TRIGGER activation_writer_record_retained BEFORE DELETE ON activation_writer_records BEGIN SELECT RAISE(ABORT,'Writer record retained');END;
CREATE TRIGGER activation_writer_history_immutable BEFORE UPDATE ON activation_writer_history BEGIN SELECT RAISE(ABORT,'Writer history immutable');END;
CREATE TRIGGER activation_writer_history_retained BEFORE DELETE ON activation_writer_history BEGIN SELECT RAISE(ABORT,'Writer history retained');END;
PRAGMA user_version=19;
"#).map_err(storage)
}
fn ledger(
    c: &Connection,
    id: &str,
    physical: &ActivationRecord,
    authority: &StateBackupAuthority,
) -> Result<ActivationWriterRecord> {
    let (key, command): (String, String) = c
        .query_row(
            "SELECT command_id,command FROM activation_writer_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let command: winbridge_protocol::update::WorkerJournalActivationCommand = decode(&command)?;
    writer::validate_command(&command, physical, authority).map_err(bad)?;
    if key != command.command_id || command.binding.update_id != id {
        return Err(bad("Writer intent/index scope differs"));
    }
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_writer_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationWriterRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.command != command || current.revision != counter(revision)? {
        return Err(bad("Writer current/intent/revision differs"));
    }
    let mut q = c.prepare("SELECT revision,record FROM activation_writer_history WHERE id=?1 ORDER BY revision LIMIT 4").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Writer causal history missing/oversized"));
    }
    let mut previous = ActivationWriterRecord::issued(command).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationWriterRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Writer history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Writer history must begin Issued"));
            }
        } else {
            previous.advance(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Writer current/history differs"));
    }
    Ok(current)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ActivationWriterRecord>> {
    Ok(load(c, id)?.map(|(value, _)| value))
}
fn backup_binding(
    c: &Connection,
    physical: &ActivationRecord,
) -> Result<crate::state_backup::StateBackupRecord> {
    // The full graph was already verified in this same transaction. Re-walking
    // its recursive backup history would add no new evidence.
    let raw: String = c
        .query_row(
            "SELECT record FROM state_backup_records WHERE id=?1",
            [&physical.intent.source.update_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let backup: crate::state_backup::StateBackupRecord = decode(&raw)?;
    backup.validate().map_err(bad)?;
    let report = backup
        .report
        .as_ref()
        .ok_or_else(|| bad("Writer backup report missing"))?;
    if backup.phase != crate::state_backup::StateBackupPhase::Verified
        || crate::runtime_baseline::digest(&backup).map_err(bad)?
            != physical.intent.source.backup_sha256
        || report.worker_journal_schema != physical.intent.context.worker_journal_schema
    {
        return Err(bad(
            "Writer requires graph-bound verified backup and same worker schema",
        ));
    }
    Ok(backup)
}
fn physical_prefix(c: &Connection, current: &ActivationRecord) -> Result<ActivationRecord> {
    // The full causal graph/current was validated in this same snapshot.
    // Observation after Indeterminate/LocalVerified still binds to immutable6;
    // this helper is not used to issue a new command from a progressed state.
    let raw: String = c
        .query_row(
            "SELECT record FROM activation_history WHERE id=?1 AND revision=6",
            [&current.intent.source.update_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let physical: ActivationRecord = decode(&raw)?;
    if physical.revision != 6
        || !matches!(
            physical.state,
            crate::activation::ActivationState::StateOwned {}
        )
        || current.revision < 6
        || physical.intent != current.intent
        || physical.state_owned != current.state_owned
    {
        return Err(bad(
            "Writer historical StateOwned6 differs from current causal scope",
        ));
    }
    Ok(physical)
}
fn load(
    c: &Connection,
    id: &str,
) -> Result<
    Option<(
        ActivationWriterRecord,
        crate::activation::ActivationWriterSnapshot,
    )>,
> {
    if schema(c)? < 19 {
        return Ok(None);
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM activation_writer_intents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Ok(None);
    }
    let (current, replacement) =
        activation::read_graph(c, id)?.ok_or_else(|| bad("Writer physical activation missing"))?;
    let physical = physical_prefix(c, &current)?;
    physical.validate(&replacement).map_err(bad)?;
    let backup = backup_binding(c, &physical)?;
    let report = backup
        .report
        .as_ref()
        .ok_or_else(|| bad("Writer report missing"))?;
    let value = ledger(c, id, &physical, &report.source_authority)?;
    Ok(Some((
        value.clone(),
        crate::activation::ActivationWriterSnapshot {
            activation: current,
            writer: value,
            shutdown_receipt: report.shutdown_receipt.clone(),
        },
    )))
}
// The runtime driver must additionally admit a fresh original Owned guard,
// authority/root, private capability and original monotonic window. There is
// intentionally no public caller-JSON command-to-success method.
#[cfg(any(windows, test))]
pub(super) fn seed(
    c: &Connection,
    value: &ActivationWriterRecord,
    physical: &ActivationRecord,
    authority: &StateBackupAuthority,
) -> Result<()> {
    value.validate().map_err(bad)?;
    if *value != ActivationWriterRecord::issued(value.command.clone()).map_err(bad)? {
        return Err(bad("Writer admission must start Issued"));
    }
    writer::validate_command(&value.command, physical, authority).map_err(bad)?;
    let id = &value.command.binding.update_id;
    one(c
        .execute(
            "INSERT INTO activation_writer_intents VALUES(?1,?2,?3)",
            params![id, value.command.command_id, encode(&value.command)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_writer_records VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_writer_history VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    if ledger(c, id, physical, authority)? != *value {
        return Err(bad("Writer admission/readback differs"));
    }
    Ok(())
}
#[cfg(any(windows, test))]
pub(super) fn persist(
    c: &Connection,
    old: &ActivationWriterRecord,
    next: &ActivationWriterRecord,
    physical: &ActivationRecord,
    authority: &StateBackupAuthority,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    let id = &old.command.binding.update_id;
    if ledger(c, id, physical, authority)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Writer revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE activation_writer_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_writer_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, id, physical, authority)? != *next {
        return Err(bad("Writer append/readback differs"));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn activation_writer(&mut self, id: &str) -> Result<Option<ActivationWriterRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let value = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(value)
    }
    /// A data snapshot for the already OS-authenticated private child only.
    /// Callers still retain physical state/runtime/parent and monotonic custody.
    pub fn inspect_activation_writer_receiver(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &winbridge_protocol::update::SupervisorProcess,
    ) -> Result<crate::activation::ActivationWriterSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        let (_, snapshot) = load(&tx, id)?.ok_or_else(|| bad("Writer admission missing"))?;
        crate::activation::channel::require_writer_receiver(&snapshot.activation, actual, attempt)
            .map_err(bad)?;
        let source = &snapshot.activation.intent.source;
        let root = record(&tx, id)?;
        let installation = info(&tx)?;
        let owner: String = tx
            .query_row(
                "SELECT owner FROM installation WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let s = supervisor::read(&tx, &source.launch_id)?;
        if root.phase != UpdatePhase::VerifyingLocal
            || root.revision != source.parent_revision
            || installation.active_update_id.as_deref() != Some(id)
            || installation.owner_epoch != source.owner_epoch
            || owner != source.owner_id
            || s.phase != winbridge_protocol::update::SupervisorPhase::Active
            || s.owner_id.as_deref() != Some(source.owner_id.as_str())
            || s.owner_epoch != Some(source.owner_epoch)
            || s.spec.update_id != id
            || s.process.as_ref() != Some(&snapshot.activation.intent.context.supervisor)
            || !matches!(
                snapshot.writer.state,
                crate::activation::ActivationWriterState::Issued {}
            )
        {
            return Err(bad(
                "Writer child has stale owner/root/supervisor or non-issued delivery",
            ));
        }
        tx.commit().map_err(storage)?;
        Ok(snapshot)
    }
    #[cfg(windows)]
    pub(crate) fn admit_activation_writer(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        physical: &ActivationRecord,
        key: &str,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<crate::activation::ActivationWriterAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(value) = read(&tx, &s.spec.update_id)? {
            if value.command.command_id != key {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Writer update already has another command; no second admission",
                ));
            }
            return Ok(crate::activation::ActivationWriterAdmission::Existing(
                Box::new(value),
            ));
        }
        activation::owned_current(&tx, a, s, preparation, physical)?;
        if physical.revision != 6
            || !matches!(
                physical.state,
                crate::activation::ActivationState::StateOwned {}
            )
            || physical.intent.context.updater_journal_schema != UPDATE_SCHEMA_VERSION
        {
            return Err(bad(
                "Writer admission requires exact StateOwned6 and current updater schema",
            ));
        }
        let backup = backup_binding(&tx, physical)?;
        let authority = &backup
            .report
            .as_ref()
            .ok_or_else(|| bad("Writer report missing"))?
            .source_authority;
        let command = writer::prepare_command(
            physical,
            authority,
            key,
            now_ms()?.max(physical.updated_at_ms),
        )
        .map_err(bad)?;
        let value = ActivationWriterRecord::issued(command).map_err(bad)?;
        checkpoint().map_err(bad)?;
        preparation.require_live_window().map_err(bad)?;
        seed(&tx, &value, physical, authority)?;
        checkpoint().map_err(bad)?;
        preparation.require_live_window().map_err(bad)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Writer admission commit unconfirmed; inspect same command, no send/replay: {e}"
            ))
        })?;
        Ok(crate::activation::ActivationWriterAdmission::Issued(
            Box::new(crate::activation::ActivationWriterPermit { record: value }),
        ))
    }
    #[cfg(windows)]
    pub(crate) fn require_activation_writer_issued(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        physical: &ActivationRecord,
        permit: &crate::activation::ActivationWriterPermit,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        activation::owned_current(&tx, a, s, preparation, physical)?;
        let backup = backup_binding(&tx, physical)?;
        let authority = &backup
            .report
            .as_ref()
            .ok_or_else(|| bad("Writer report missing"))?
            .source_authority;
        if ledger(&tx, &s.spec.update_id, physical, authority)? != permit.record
            || !matches!(
                permit.record.state,
                crate::activation::ActivationWriterState::Issued {}
            )
        {
            return Err(bad("Original writer intent is no longer Issued; no send"));
        }
        preparation.require_live_window().map_err(bad)?;
        tx.commit().map_err(storage)
    }
    #[cfg(windows)]
    pub(crate) fn append_activation_writer_owned(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        physical: &ActivationRecord,
        next: &ActivationWriterRecord,
    ) -> Result<ActivationWriterRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        activation::owned_current(&tx, a, s, preparation, physical)?;
        let frozen = physical_prefix(&tx, physical)?;
        let backup = backup_binding(&tx, &frozen)?;
        let authority = &backup
            .report
            .as_ref()
            .ok_or_else(|| bad("Writer report missing"))?
            .source_authority;
        let old = ledger(&tx, &s.spec.update_id, &frozen, authority)?;
        if old == *next {
            return Ok(old);
        }
        persist(&tx, &old, next, &frozen, authority)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Writer result commit unconfirmed; inspect same command, no replay: {e}"
            ))
        })?;
        Ok(next.clone())
    }
    /// Historical data only, including after root/local progress. The immutable
    /// StateOwned6 prefix and final backup remain bound in one read snapshot.
    pub fn inspect_activation_writer(
        directory: &Path,
        id: &str,
    ) -> Result<Option<ActivationWriterRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        let result = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
