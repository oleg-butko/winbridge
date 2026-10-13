//! Versioned local journal observations. None of these fields change wire v1.
use super::{sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::ops::{Deref, DerefMut};
use winbridge_protocol::{ApplicationRecord, ImportRecord, WorkflowRecord};

mod sealed {
    pub trait Sealed {}
    impl Sealed for winbridge_protocol::ImportRecord {}
    impl Sealed for winbridge_protocol::WorkflowRecord {}
    impl Sealed for winbridge_protocol::ApplicationRecord {}
}

pub trait JournalRecord: sealed::Sealed + Serialize + DeserializeOwned + Clone {
    const TABLE: &'static str;
    fn id(&self) -> &str;
    fn identity(&self) -> Result<String>;
    fn terminal(&self) -> bool;
    fn status(&self) -> Result<String>;
}

impl JournalRecord for ImportRecord {
    const TABLE: &'static str = "imports";
    fn id(&self) -> &str {
        &self.import_id
    }
    fn identity(&self) -> Result<String> {
        let mut submission = self.submission.clone();
        submission.request_id.clear();
        serde_json::to_string(&(
            submission,
            &self.shared_path,
            &self.workspace_path,
            self.accepted_at_ms,
        ))
        .map_err(|e| e.to_string())
    }
    fn terminal(&self) -> bool {
        self.status.is_terminal()
    }
    fn status(&self) -> Result<String> {
        status(self.status)
    }
}
impl JournalRecord for WorkflowRecord {
    const TABLE: &'static str = "workflows";
    fn id(&self) -> &str {
        &self.workflow_id
    }
    fn identity(&self) -> Result<String> {
        let mut submission = self.submission.clone();
        submission.request_id.clear();
        let steps: Vec<_> = self.steps.iter().map(|s| (&s.step_id, &s.name)).collect();
        serde_json::to_string(&(submission, &self.recipe_pins, steps, self.accepted_at_ms))
            .map_err(|e| e.to_string())
    }
    fn terminal(&self) -> bool {
        self.status.is_terminal()
    }
    fn status(&self) -> Result<String> {
        status(self.status)
    }
}
impl JournalRecord for ApplicationRecord {
    const TABLE: &'static str = "applications";
    fn id(&self) -> &str {
        &self.application_id
    }
    fn identity(&self) -> Result<String> {
        serde_json::to_string(&(
            &self.workflow_id,
            &self.project_id,
            &self.task_id,
            &self.log_path,
            self.created_at_ms,
        ))
        .map_err(|e| e.to_string())
    }
    fn terminal(&self) -> bool {
        self.status.is_terminal()
    }
    fn status(&self) -> Result<String> {
        status(self.status)
    }
}
fn status(value: impl Serialize) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|e| e.to_string())?
        .as_str()
        .map(str::to_owned)
        .ok_or("local journal: invalid status".into())
}

#[derive(Clone, Debug)]
pub struct JournalSnapshot<T> {
    pub record: T,
    id: String,
    revision: i64,
    raw: String,
    identity: String,
    terminal: bool,
}
impl<T> Deref for JournalSnapshot<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.record
    }
}
impl<T> DerefMut for JournalSnapshot<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.record
    }
}
impl<T: JournalRecord> JournalSnapshot<T> {
    pub fn revision(&self) -> u64 {
        self.revision as u64
    }
    pub(crate) fn committed(&mut self) -> Result<()> {
        self.revision += 1;
        self.raw = serde_json::to_string(&self.record).map_err(|e| e.to_string())?;
        self.terminal = self.record.terminal();
        Ok(())
    }
}

impl Store {
    pub(crate) fn read_domain<T: JournalRecord>(
        connection: &Connection,
        id: &str,
    ) -> Result<JournalSnapshot<T>> {
        let row: Option<(String, i64)> = connection
            .query_row(
                &format!("SELECT record,revision FROM {} WHERE id=?1", T::TABLE),
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        let (raw, revision) =
            row.ok_or_else(|| format!("local journal: {} ID {id:?} not found", T::TABLE))?;
        let record: T = serde_json::from_str(&raw).map_err(|e| format!("local journal: {e}"))?;
        if record.id() != id || revision < 0 {
            return Err("local journal: identity/revision mismatch".into());
        }
        let identity = record.identity()?;
        let terminal = record.terminal();
        Ok(JournalSnapshot {
            record,
            id: id.into(),
            revision,
            raw,
            identity,
            terminal,
        })
    }
    pub(crate) fn write_domain<T: JournalRecord>(
        connection: &Connection,
        observed: &JournalSnapshot<T>,
    ) -> Result<()> {
        if observed.record.id() != observed.id || observed.record.identity()? != observed.identity {
            return Err(
                "local journal: immutable record identity changed; decision rejected".into(),
            );
        }
        let raw = serde_json::to_string(&observed.record).map_err(|e| e.to_string())?;
        if observed.terminal && raw != observed.raw {
            return Err("local journal: terminal record is immutable".into());
        }
        let next = observed
            .revision
            .checked_add(1)
            .ok_or("local journal: revision exhausted; stop admission")?;
        let changed = if T::TABLE == "imports" {
            connection.execute("UPDATE imports SET record=?2,revision=?4 WHERE id=?1 AND revision=?3 AND record=?5", params![observed.id,raw,observed.revision,next,observed.raw])
        } else {
            connection.execute(&format!("UPDATE {} SET record=?2,revision=?4,status=?6 WHERE id=?1 AND revision=?3 AND record=?5",T::TABLE), params![observed.id,raw,observed.revision,next,observed.raw,observed.record.status()?])
        }.map_err(sql)?;
        if changed != 1 {
            return Err(format!("local journal: {} transition not stored; stale revision {}, missing row or ignored write; decision rejected", T::TABLE, observed.revision));
        }
        Ok(())
    }
    pub fn import_snapshot(&self, id: &str) -> Result<JournalSnapshot<ImportRecord>> {
        Self::read_domain(&self.connection, id)
    }
    pub fn workflow_snapshot(&self, id: &str) -> Result<JournalSnapshot<WorkflowRecord>> {
        Self::read_domain(&self.connection, id)
    }
    pub fn application_snapshot(&self, id: &str) -> Result<JournalSnapshot<ApplicationRecord>> {
        Self::read_domain(&self.connection, id)
    }
}
