//! Schema21: immutable controller intent/current/causal history, data only.
//! Production guarded admission/dispatch is added as a separate step. These
//! readbacks never construct a connector, original owner or startup permit.
use super::*;
use crate::activation::{
    controller::{self, ActivationControllerRecord},
    service::ActivationServiceRecord,
    ActivationRecord, ActivationState, ActivationWriterRecord,
};

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_controller_intents(id TEXT PRIMARY KEY REFERENCES activation_service_records(id),command_id TEXT NOT NULL UNIQUE,command TEXT NOT NULL CHECK(json_valid(command)));
CREATE TABLE activation_controller_records(id TEXT PRIMARY KEY REFERENCES activation_controller_intents(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE activation_controller_history(id TEXT NOT NULL REFERENCES activation_controller_records(id),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),record TEXT NOT NULL CHECK(json_valid(record)),PRIMARY KEY(id,revision));
CREATE TRIGGER activation_controller_intent_immutable BEFORE UPDATE ON activation_controller_intents BEGIN SELECT RAISE(ABORT,'Controller intent immutable');END;
CREATE TRIGGER activation_controller_intent_retained BEFORE DELETE ON activation_controller_intents BEGIN SELECT RAISE(ABORT,'Controller intent retained');END;
CREATE TRIGGER activation_controller_record_identity BEFORE UPDATE ON activation_controller_records WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(NEW.record,'$.revision') IS NOT NEW.revision OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command') OR json_extract(OLD.record,'$.state.phase')='confirmed' BEGIN SELECT RAISE(ABORT,'Controller command/revision/confirmation immutable');END;
CREATE TRIGGER activation_controller_record_retained BEFORE DELETE ON activation_controller_records BEGIN SELECT RAISE(ABORT,'Controller record retained');END;
CREATE TRIGGER activation_controller_history_immutable BEFORE UPDATE ON activation_controller_history BEGIN SELECT RAISE(ABORT,'Controller history immutable');END;
CREATE TRIGGER activation_controller_history_retained BEFORE DELETE ON activation_controller_history BEGIN SELECT RAISE(ABORT,'Controller history retained');END;
PRAGMA user_version=21;
"#).map_err(storage)
}

/// All four exact records come from one read transaction. Neither historical
/// StateOwned6 nor LocalVerified7 is reconstructed from the latest row.
struct Graph {
    physical: ActivationRecord,
    writer: ActivationWriterRecord,
    service: ActivationServiceRecord,
    local: ActivationRecord,
}
impl Graph {
    fn load(c: &Connection, id: &str) -> Result<Self> {
        let (current, replacement) = activation::read_graph(c, id)?
            .ok_or_else(|| bad("Controller activation graph missing"))?;
        let physical = activation_writer::physical_prefix(c, &current)?;
        physical.validate(&replacement).map_err(bad)?;
        let writer = activation_writer::bound_record(c, &physical)?;
        let service = activation_service::bound_record(c, id, &physical, &writer)?;
        let raw: String = c
            .query_row(
                "SELECT record FROM activation_history WHERE id=?1 AND revision=7",
                [id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let local: ActivationRecord = decode(&raw)?;
        local.validate(&replacement).map_err(bad)?;
        if current.revision < 7
            || local.revision != 7
            || local.state != (ActivationState::LocalVerified {})
            || current.intent != local.intent
            || current.state_owned != local.state_owned
        {
            return Err(bad(
                "Controller requires the retained LocalVerified7 prefix",
            ));
        }
        Ok(Self {
            physical,
            writer,
            service,
            local,
        })
    }
    fn validate(
        &self,
        command: &winbridge_protocol::update::WorkerControllerActivationCommand,
    ) -> Result<()> {
        // History retains the selection authenticated at fresh admission. This
        // self-comparison is data readback, never validation of a fresh request:
        // fresh admission must derive expected topology from pinned config.
        controller::validate_command(
            command,
            &self.physical,
            &self.writer,
            &self.service,
            &self.local,
            &command.binding.topology,
        )
        .map_err(bad)
    }
}

fn ledger(c: &Connection, id: &str, graph: &Graph) -> Result<ActivationControllerRecord> {
    let (key, raw): (String, String) = c
        .query_row(
            "SELECT command_id,command FROM activation_controller_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let command: winbridge_protocol::update::WorkerControllerActivationCommand = decode(&raw)?;
    graph.validate(&command)?;
    if key != command.command_id || command.binding.service_receipt.update_id != id {
        return Err(bad("Controller intent/index scope differs"));
    }
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_controller_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationControllerRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.command != command || current.revision != counter(revision)? {
        return Err(bad("Controller current/intent/revision differs"));
    }
    let mut q=c.prepare("SELECT revision,record FROM activation_controller_history WHERE id=?1 ORDER BY revision LIMIT 4").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Controller history missing/oversized"));
    }
    let mut previous = ActivationControllerRecord::issued(command).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationControllerRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Controller history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Controller history must start Issued"));
            }
        } else {
            previous.advance(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Controller current/history differs"));
    }
    Ok(current)
}

pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ActivationControllerRecord>> {
    if schema(c)? < 21 {
        return Ok(None);
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM activation_controller_intents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Ok(None);
    }
    let graph = Graph::load(c, id)?;
    ledger(c, id, &graph).map(Some)
}

// SQL primitive regression seam only. There is deliberately no production JSON
// writer or fresh permit factory until the original-custody driver is wired.
#[cfg(test)]
fn seed(c: &Connection, value: &ActivationControllerRecord, graph: &Graph) -> Result<()> {
    value.validate().map_err(bad)?;
    graph.validate(&value.command)?;
    if *value != ActivationControllerRecord::issued(value.command.clone()).map_err(bad)? {
        return Err(bad("Controller admission must start Issued"));
    }
    let id = &value.command.binding.service_receipt.update_id;
    one(c
        .execute(
            "INSERT INTO activation_controller_intents VALUES(?1,?2,?3)",
            params![id, value.command.command_id, encode(&value.command)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_controller_records VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_controller_history VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    if ledger(c, id, graph)? != *value {
        return Err(bad("Controller admission/readback differs"));
    }
    Ok(())
}

#[cfg(test)]
fn existing_for_key(
    value: ActivationControllerRecord,
    key: &str,
    topology: &winbridge_protocol::update::WorkerControllerActivationTopology,
) -> Result<ActivationControllerRecord> {
    if value.command.command_id != key || value.command.binding.topology != *topology {
        return Err(fail(
            JournalErrorCode::KeyConflict,
            "Controller update has another key or frozen selection; no second admission",
        ));
    }
    Ok(value)
}

#[cfg(test)]
fn persist(
    c: &Connection,
    old: &ActivationControllerRecord,
    next: &ActivationControllerRecord,
    graph: &Graph,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    let id = &old.command.binding.service_receipt.update_id;
    if ledger(c, id, graph)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Controller revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE activation_controller_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_controller_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, id, graph)? != *next {
        return Err(bad("Controller append/readback differs"));
    }
    Ok(())
}

impl UpdateJournal {
    /// Retained data only. It does not open a network manager, adopt a child,
    /// acquire an original startup grant or change the root/maintenance state.
    pub fn activation_controller(
        &mut self,
        id: &str,
    ) -> Result<Option<ActivationControllerRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let value = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(value)
    }
    pub fn inspect_activation_controller(
        directory: &Path,
        id: &str,
    ) -> Result<Option<ActivationControllerRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        let value = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
