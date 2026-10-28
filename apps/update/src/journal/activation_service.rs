//! Schema20: retained local-service intent and causal history, not a launch grant.
//! Fresh admission additionally requires the resident original process guard.
use super::*;
use crate::activation::{
    service::{self, ActivationServiceRecord},
    ActivationRecord, ActivationWriterRecord,
};

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_service_intents(id TEXT PRIMARY KEY REFERENCES activation_writer_records(id),command_id TEXT NOT NULL UNIQUE,command TEXT NOT NULL CHECK(json_valid(command)));
CREATE TABLE activation_service_records(id TEXT PRIMARY KEY REFERENCES activation_service_intents(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE activation_service_history(id TEXT NOT NULL REFERENCES activation_service_records(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER activation_service_intent_immutable BEFORE UPDATE ON activation_service_intents BEGIN SELECT RAISE(ABORT,'Service intent immutable');END;
CREATE TRIGGER activation_service_intent_retained BEFORE DELETE ON activation_service_intents BEGIN SELECT RAISE(ABORT,'Service intent retained');END;
CREATE TRIGGER activation_service_record_identity BEFORE UPDATE ON activation_service_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command') OR json_extract(OLD.record,'$.state.phase')='confirmed' BEGIN SELECT RAISE(ABORT,'Service command/revision/confirmation immutable');END;
CREATE TRIGGER activation_service_record_retained BEFORE DELETE ON activation_service_records BEGIN SELECT RAISE(ABORT,'Service record retained');END;
CREATE TRIGGER activation_service_history_immutable BEFORE UPDATE ON activation_service_history BEGIN SELECT RAISE(ABORT,'Service history immutable');END;
CREATE TRIGGER activation_service_history_retained BEFORE DELETE ON activation_service_history BEGIN SELECT RAISE(ABORT,'Service history retained');END;
PRAGMA user_version=20;
"#).map_err(storage)
}

pub(super) fn bound_record(
    c: &Connection,
    id: &str,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<ActivationServiceRecord> {
    let (key, raw): (String, String) = c
        .query_row(
            "SELECT command_id,command FROM activation_service_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let command: winbridge_protocol::update::WorkerServiceActivationCommand = decode(&raw)?;
    service::validate_command(&command, physical, writer).map_err(bad)?;
    if key != command.command_id || command.binding.writer_command.binding.update_id != id {
        return Err(bad("Service intent/index scope differs"));
    }
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_service_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationServiceRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.command != command || current.revision != counter(revision)? {
        return Err(bad("Service current/intent/revision differs"));
    }
    let mut q = c.prepare("SELECT revision,record FROM activation_service_history WHERE id=?1 ORDER BY revision LIMIT 4").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Service causal history missing/oversized"));
    }
    let mut previous = ActivationServiceRecord::issued(command).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationServiceRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Service history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Service history must begin Issued"));
            }
        } else {
            previous.advance(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Service current/history differs"));
    }
    Ok(current)
}

pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ActivationServiceRecord>> {
    if schema(c)? < 20 {
        return Ok(None);
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM activation_service_intents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Ok(None);
    }
    // Reuse the fully verified writer graph in this very same read snapshot.
    // Current LocalVerified/Indeterminate progress cannot rewrite StateOwned6.
    let (writer, snapshot) = activation_writer::load(c, id)?
        .ok_or_else(|| bad("Service confirmed writer graph missing"))?;
    let physical = activation_writer::physical_prefix(c, &snapshot.activation)?;
    bound_record(c, id, &physical, &writer).map(Some)
}

// No caller-JSON write route. Native admission proves original custody first.
#[cfg(any(windows, test))]
pub(super) fn seed(
    c: &Connection,
    value: &ActivationServiceRecord,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<()> {
    value.validate().map_err(bad)?;
    if *value != ActivationServiceRecord::issued(value.command.clone()).map_err(bad)? {
        return Err(bad("Service admission must start Issued"));
    }
    service::validate_command(&value.command, physical, writer).map_err(bad)?;
    let id = &value.command.binding.writer_command.binding.update_id;
    one(c
        .execute(
            "INSERT INTO activation_service_intents VALUES(?1,?2,?3)",
            params![id, value.command.command_id, encode(&value.command)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_service_records VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_service_history VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    if bound_record(c, id, physical, writer)? != *value {
        return Err(bad("Service admission/readback differs"));
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn seed_checked(
    c: &Connection,
    value: &ActivationServiceRecord,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    seed(c, value, physical, writer)?;
    checkpoint()
}

#[cfg(any(windows, test))]
fn existing_for_key(
    value: ActivationServiceRecord,
    key: &str,
) -> Result<service::ActivationServiceAdmission> {
    if value.command.command_id != key {
        return Err(fail(
            JournalErrorCode::KeyConflict,
            "Service update already has another command; no second admission",
        ));
    }
    Ok(service::ActivationServiceAdmission::Existing(Box::new(
        value,
    )))
}

#[cfg(any(windows, test))]
pub(super) fn persist(
    c: &Connection,
    old: &ActivationServiceRecord,
    next: &ActivationServiceRecord,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    let id = &old.command.binding.writer_command.binding.update_id;
    if bound_record(c, id, physical, writer)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Service revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE activation_service_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_service_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if bound_record(c, id, physical, writer)? != *next {
        return Err(bad("Service append/readback differs"));
    }
    Ok(())
}

impl UpdateJournal {
    #[cfg(windows)]
    pub(crate) fn admit_activation_service(
        &mut self,
        a: &UpdateAuthority,
        s: &winbridge_protocol::update::SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        physical: &ActivationRecord,
        key: &str,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<service::ActivationServiceAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(value) = read(&tx, &s.spec.update_id)? {
            return existing_for_key(value, key);
        }
        activation::owned_current(&tx, a, s, preparation, physical)?;
        if physical.revision != 6
            || physical.state != (crate::activation::ActivationState::StateOwned {})
            || physical.intent.context.updater_journal_schema != UPDATE_SCHEMA_VERSION
        {
            return Err(bad(
                "Service admission requires original StateOwned6 and current updater schema",
            ));
        }
        let writer = activation_writer::bound_record(&tx, physical)?;
        let command = service::prepare_command(
            physical,
            &writer,
            key,
            now_ms()?
                .max(physical.updated_at_ms)
                .max(writer.updated_at_ms),
        )
        .map_err(bad)?;
        let value = ActivationServiceRecord::issued(command).map_err(bad)?;
        seed_checked(&tx, &value, physical, &writer, || {
            checkpoint().map_err(bad)?;
            preparation.require_live_window().map_err(bad)
        })?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Service admission commit unconfirmed; inspect same command, no send/replay: {e}"
            ))
        })?;
        // A permission is minted only AFTER confirmed commit. Cold/duplicate
        // readback, including an Issued row, never returns this variant.
        Ok(service::ActivationServiceAdmission::Issued(Box::new(
            service::ActivationServicePermit { record: value },
        )))
    }

    #[cfg(windows)]
    pub(crate) fn require_activation_service_issued(
        &self,
        a: &UpdateAuthority,
        s: &winbridge_protocol::update::SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        physical: &ActivationRecord,
        permit: &service::ActivationServicePermit,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        activation::owned_current(&tx, a, s, preparation, physical)?;
        let writer = activation_writer::bound_record(&tx, physical)?;
        if bound_record(&tx, &s.spec.update_id, physical, &writer)? != permit.record
            || !matches!(
                permit.record.state,
                service::ActivationServiceState::Issued {}
            )
        {
            return Err(bad(
                "Service intent is no longer the original Issued record; no send",
            ));
        }
        preparation.require_live_window().map_err(bad)?;
        tx.commit().map_err(storage)
    }

    #[cfg(windows)]
    pub(crate) fn append_activation_service_owned(
        &mut self,
        a: &UpdateAuthority,
        s: &winbridge_protocol::update::SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        current: &ActivationRecord,
        next: &ActivationServiceRecord,
    ) -> Result<ActivationServiceRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        activation::owned_current(&tx, a, s, preparation, current)?;
        let physical = activation_writer::physical_prefix(&tx, current)?;
        let writer = activation_writer::bound_record(&tx, &physical)?;
        let old = bound_record(&tx, &s.spec.update_id, &physical, &writer)?;
        if old == *next {
            return Ok(old);
        }
        persist(&tx, &old, next, &physical, &writer)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Service result commit unconfirmed; inspect same command, no replay: {e}"
            ))
        })?;
        Ok(next.clone())
    }

    /// Data only: saved service history never authorizes a send, Store opening,
    /// controller connection, maintenance release or child adoption.
    pub fn activation_service(&mut self, id: &str) -> Result<Option<ActivationServiceRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let result = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    pub fn inspect_activation_service_receiver(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &winbridge_protocol::update::SupervisorProcess,
    ) -> Result<crate::activation::ActivationServiceSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? != UPDATE_SCHEMA_VERSION {
            return Err(bad("Service receiver requires exact updater schema"));
        }
        let tx = c.transaction().map_err(storage)?;
        let (_, writer) =
            activation_writer::load(&tx, id)?.ok_or_else(|| bad("Service writer graph missing"))?;
        activation_writer::require_receiver_custody(&tx, id, attempt, actual, &writer)?;
        if writer.activation.intent.context.updater_journal_schema != UPDATE_SCHEMA_VERSION {
            return Err(bad(
                "Service receiver cannot adopt a pre-migration activation scope",
            ));
        }
        let service = bound_record(&tx, id, &writer.activation, &writer.writer)?;
        if !matches!(service.state, service::ActivationServiceState::Issued {}) {
            return Err(bad(
                "Service delivery is not Issued; no startup from history",
            ));
        }
        let value = crate::activation::ActivationServiceSnapshot { writer, service };
        tx.commit().map_err(storage)?;
        Ok(value)
    }

    pub fn inspect_activation_service(
        directory: &Path,
        id: &str,
    ) -> Result<Option<ActivationServiceRecord>> {
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
