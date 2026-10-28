//! Schema22: retained lifetime decisions, never restored keeper/OS custody.
//! SQL writers are test-only until original-owner admission is implemented.
use super::*;
use crate::activation::{
    controller::ActivationControllerSnapshot,
    lifetime::{self, ActivationLifetimeRecord},
};

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_lifetime_intents(
 id TEXT NOT NULL PRIMARY KEY REFERENCES activation_controller_records(id),
 command_id TEXT NOT NULL UNIQUE,
 command TEXT NOT NULL CHECK(json_valid(command)),
 CHECK(json_extract(command,'$.binding.updateId') IS id),
 CHECK(json_extract(command,'$.commandId') IS command_id)
);
CREATE TABLE activation_lifetime_records(
 id TEXT NOT NULL PRIMARY KEY REFERENCES activation_lifetime_intents(id),
 revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),
 record TEXT NOT NULL CHECK(json_valid(record)),
 CHECK(json_extract(record,'$.revision') IS revision),
 CHECK(json_extract(record,'$.command.binding.updateId') IS id)
);
CREATE TABLE activation_lifetime_history(
 id TEXT NOT NULL REFERENCES activation_lifetime_records(id),
 revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),
 record TEXT NOT NULL CHECK(json_valid(record)),
 PRIMARY KEY(id,revision),
 CHECK(json_extract(record,'$.revision') IS revision),
 CHECK(json_extract(record,'$.command.binding.updateId') IS id)
);
CREATE TRIGGER activation_lifetime_intent_immutable BEFORE UPDATE ON activation_lifetime_intents BEGIN SELECT RAISE(ABORT,'Lifetime intent immutable');END;
CREATE TRIGGER activation_lifetime_intent_retained BEFORE DELETE ON activation_lifetime_intents BEGIN SELECT RAISE(ABORT,'Lifetime intent retained');END;
CREATE TRIGGER activation_lifetime_record_identity BEFORE UPDATE ON activation_lifetime_records
 WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1
 OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command')
 OR json_extract(OLD.record,'$.state.phase')='confirmed'
 BEGIN SELECT RAISE(ABORT,'Lifetime command/revision/confirmation immutable');END;
CREATE TRIGGER activation_lifetime_record_retained BEFORE DELETE ON activation_lifetime_records BEGIN SELECT RAISE(ABORT,'Lifetime record retained');END;
CREATE TRIGGER activation_lifetime_history_immutable BEFORE UPDATE ON activation_lifetime_history BEGIN SELECT RAISE(ABORT,'Lifetime history immutable');END;
CREATE TRIGGER activation_lifetime_history_retained BEFORE DELETE ON activation_lifetime_history BEGIN SELECT RAISE(ABORT,'Lifetime history retained');END;
PRAGMA user_version=22;
"#).map_err(storage)
}

fn ledger(
    c: &Connection,
    id: &str,
    graph: &ActivationControllerSnapshot,
) -> Result<ActivationLifetimeRecord> {
    let (key, raw): (String, String) = c
        .query_row(
            "SELECT command_id,command FROM activation_lifetime_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let command: winbridge_protocol::update::WorkerLifetimeActivationCommand = decode(&raw)?;
    lifetime::validate_command(&command, graph).map_err(bad)?;
    if key != command.command_id || command.binding.update_id != id {
        return Err(bad("Lifetime intent/index scope differs"));
    }
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_lifetime_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationLifetimeRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.command != command || current.revision != counter(revision)? {
        return Err(bad("Lifetime current/intent/revision differs"));
    }
    let mut q = c.prepare("SELECT revision,record FROM activation_lifetime_history WHERE id=?1 ORDER BY revision LIMIT 4").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Lifetime causal history missing/oversized"));
    }
    let mut previous = ActivationLifetimeRecord::issued(command).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationLifetimeRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Lifetime history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Lifetime history must start Issued"));
            }
        } else {
            previous.advance(&next).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Lifetime current/history differs"));
    }
    Ok(current)
}

fn present(c: &Connection, id: &str) -> Result<bool> {
    let (intent, current, history): (bool, bool, bool) = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM activation_lifetime_intents WHERE id=?1),EXISTS(SELECT 1 FROM activation_lifetime_records WHERE id=?1),EXISTS(SELECT 1 FROM activation_lifetime_history WHERE id=?1)",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(storage)?;
    if intent != current || intent != history {
        return Err(bad("Lifetime partial ledger must not be treated as absent"));
    }
    Ok(intent)
}

pub(super) fn read(c: &Connection, id: &str) -> Result<Option<ActivationLifetimeRecord>> {
    if schema(c)? < 22 || !present(c, id)? {
        return Ok(None);
    }
    // Every node and causal prefix is checked in the caller's one transaction.
    // Historical graph validation never refreshes config/owner/window grants.
    let graph = activation_controller::snapshot(c, id)?;
    ledger(c, id, &graph).map(Some)
}

#[cfg(test)]
fn seed(
    c: &Connection,
    value: &ActivationLifetimeRecord,
    graph: &ActivationControllerSnapshot,
) -> Result<()> {
    value.validate().map_err(bad)?;
    lifetime::validate_command(&value.command, graph).map_err(bad)?;
    if *value != ActivationLifetimeRecord::issued(value.command.clone()).map_err(bad)? {
        return Err(bad("Lifetime admission must start Issued"));
    }
    let id = &value.command.binding.update_id;
    one(c
        .execute(
            "INSERT INTO activation_lifetime_intents VALUES(?1,?2,?3)",
            params![id, value.command.command_id, encode(&value.command)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_lifetime_records VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_lifetime_history VALUES(?1,0,?2)",
            params![id, encode(value)?],
        )
        .map_err(storage)?)?;
    if ledger(c, id, graph)? != *value {
        return Err(bad("Lifetime admission/readback differs"));
    }
    Ok(())
}

#[cfg(test)]
fn seed_checked(
    c: &Connection,
    value: &ActivationLifetimeRecord,
    graph: &ActivationControllerSnapshot,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    seed(c, value, graph)?;
    checkpoint()
}

#[cfg(test)]
fn existing_for_key(
    value: ActivationLifetimeRecord,
    key: &str,
) -> Result<ActivationLifetimeRecord> {
    if value.command.command_id != key {
        return Err(fail(
            JournalErrorCode::KeyConflict,
            "Lifetime update has another key; no second admission",
        ));
    }
    Ok(value)
}

#[cfg(test)]
fn persist(
    c: &Connection,
    old: &ActivationLifetimeRecord,
    next: &ActivationLifetimeRecord,
    graph: &ActivationControllerSnapshot,
) -> Result<()> {
    old.advance(next).map_err(bad)?;
    let id = &old.command.binding.update_id;
    if ledger(c, id, graph)? != *old {
        return Err(fail(
            JournalErrorCode::RevisionConflict,
            "Lifetime revision changed",
        ));
    }
    let raw = encode(next)?;
    one(c.execute("UPDATE activation_lifetime_records SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,raw,id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_lifetime_history VALUES(?1,?2,?3)",
            params![id, next.revision as i64, raw],
        )
        .map_err(storage)?)?;
    if ledger(c, id, graph)? != *next {
        return Err(bad("Lifetime append/readback differs"));
    }
    Ok(())
}

impl UpdateJournal {
    /// Read/inspect only. Retained rows never recreate a keeper, child, Job,
    /// startup permit, fresh dispatch, root commit or maintenance release.
    pub fn activation_lifetime(&mut self, id: &str) -> Result<Option<ActivationLifetimeRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let value = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(value)
    }
    pub fn inspect_activation_lifetime(
        directory: &Path,
        id: &str,
    ) -> Result<Option<ActivationLifetimeRecord>> {
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
