//! Internal task revisions stay out of the legacy wire record.
use super::{sql, JournalAuthority, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use winbridge_protocol::TaskRecord;

/// A record observed at one durable revision. The identity/revision cannot be
/// changed by a caller editing the record for a journal adapter or test fixture.
#[derive(Clone, Debug)]
pub struct TaskSnapshot {
    pub record: TaskRecord,
    task_id: String,
    revision: i64,
    authority: JournalAuthority,
}

impl TaskSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision as u64
    }
}

impl Store {
    pub(crate) fn read_task_snapshot(connection: &Connection, id: &str) -> Result<TaskSnapshot> {
        let row: Option<(String, i64, String, i64, String)> = connection
            .query_row(
                "SELECT t.record,t.revision,a.incarnation,a.epoch,a.authority FROM tasks t CROSS JOIN journal_authority a WHERE t.id=?1 AND a.singleton=1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(sql)?;
        let (raw, revision, incarnation, epoch, authority_id) =
            row.ok_or_else(|| format!("taskId {id:?} was not found"))?;
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
            authority: Self::authority_from_parts(incarnation, epoch, authority_id)?,
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
        Self::assert_authority(connection, &observed.authority)?;
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
        if record.status.is_terminal() {
            Self::complete_attempt(
                connection,
                super::OperationKind::Task,
                &record.task_id,
                record.status,
            )?;
        }
        Ok(())
    }

    /// Low-level record adapter for fixtures; runtime lifecycle uses the FSM.
    /// A bare TaskRecord cannot authorize a write without its observed revision.
    pub fn save(&mut self, observed: &TaskSnapshot) -> Result<TaskSnapshot> {
        self.guard_callback(
            Some(super::OperationKind::Task),
            Some(&observed.record.task_id),
        )?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
        Self::write_task_snapshot(&transaction, observed, &observed.record)?;
        transaction.commit().map_err(sql)?;
        Ok(TaskSnapshot {
            record: observed.record.clone(),
            task_id: observed.task_id.clone(),
            revision: observed.revision + 1,
            authority: observed.authority.clone(),
        })
    }
}
