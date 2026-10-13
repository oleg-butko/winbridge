//! Internal task revisions stay out of the legacy wire record.
use super::{sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use winbridge_protocol::TaskRecord;

/// A record observed at one durable revision. The identity/revision cannot be
/// changed by a caller editing the record for a journal adapter or test fixture.
#[derive(Clone, Debug)]
pub struct TaskSnapshot {
    pub record: TaskRecord,
    task_id: String,
    revision: i64,
}

impl TaskSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision as u64
    }
}

impl Store {
    pub(crate) fn read_task_snapshot(connection: &Connection, id: &str) -> Result<TaskSnapshot> {
        let row: Option<(String, i64)> = connection
            .query_row(
                "SELECT record,revision FROM tasks WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        let (raw, revision) = row.ok_or_else(|| format!("taskId {id:?} was not found"))?;
        let record: TaskRecord = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if record.task_id != id || revision < 0 {
            return Err(
                "Task journal identity/revision mismatch; execution decision rejected".into(),
            );
        }
        Ok(TaskSnapshot {
            record,
            task_id: id.into(),
            revision,
        })
    }

    pub fn task_snapshot(&self, id: &str) -> Result<TaskSnapshot> {
        Self::read_task_snapshot(&self.connection, id)
    }

    /// All task writers share this CAS, including output and retention. The
    /// caller owns the transaction containing associated rows/events/effects.
    pub(crate) fn write_task_snapshot(
        connection: &Connection,
        observed: &TaskSnapshot,
        record: &TaskRecord,
    ) -> Result<()> {
        if record.task_id != observed.task_id {
            return Err("Task journal identity mismatch; execution decision rejected".into());
        }
        let next = observed
            .revision
            .checked_add(1)
            .ok_or("Task revision exhausted; keep the journal and stop admission")?;
        let changed = connection
            .execute(
                "UPDATE tasks SET record=?2,revision=?4 WHERE id=?1 AND revision=?3 AND project=?5 AND key=?6 AND payload=?7",
                params![
                    observed.task_id,
                    serde_json::to_string(record).map_err(|e| e.to_string())?,
                    observed.revision,
                    next,
                    record.submission.project_id,
                    record.submission.idempotency_key,
                    Self::fingerprint(&record.submission)?
                ],
            )
            .map_err(sql)?;
        if changed != 1 {
            return Err(format!("Task transition was not stored; expected revision {} is stale, the row is missing or storage ignored the write; execution decision rejected", observed.revision));
        }
        Ok(())
    }

    /// Low-level record adapter for fixtures; runtime lifecycle uses the FSM.
    /// A bare TaskRecord cannot authorize a write without its observed revision.
    pub fn save(&mut self, observed: &TaskSnapshot) -> Result<TaskSnapshot> {
        let transaction = self.connection.transaction().map_err(sql)?;
        Self::write_task_snapshot(&transaction, observed, &observed.record)?;
        transaction.commit().map_err(sql)?;
        Ok(TaskSnapshot {
            record: observed.record.clone(),
            task_id: observed.task_id.clone(),
            revision: observed.revision + 1,
        })
    }
}
