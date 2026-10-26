//! Schema20: retained local-service intent and causal history, not a launch grant.
//! Runtime admission/send remain unavailable until the original guard driver exists.
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

fn ledger(
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
    ledger(c, id, &physical, &writer).map(Some)
}

// Exercise transactional storage without exposing a caller-JSON write route.
// A future native admission must prove original custody and persist before send.
#[cfg(test)]
fn seed(
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
    if ledger(c, id, physical, writer)? != *value {
        return Err(bad("Service admission/readback differs"));
    }
    Ok(())
}

#[cfg(test)]
fn persist(
    c: &Connection,
    old: &ActivationServiceRecord,
    next: &ActivationServiceRecord,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    let id = &old.command.binding.writer_command.binding.update_id;
    if ledger(c, id, physical, writer)? != *old {
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
    if ledger(c, id, physical, writer)? != *next {
        return Err(bad("Service append/readback differs"));
    }
    Ok(())
}

impl UpdateJournal {
    /// Data only: saved service history never authorizes a send, Store opening,
    /// controller connection, maintenance release or child adoption.
    pub fn activation_service(&mut self, id: &str) -> Result<Option<ActivationServiceRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let result = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(result)
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
