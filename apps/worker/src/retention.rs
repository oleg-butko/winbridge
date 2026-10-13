use crate::{
    store::{now_ms, sql, Store},
    Result, Worker,
};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::sync::{atomic::Ordering, Arc};
use winbridge_protocol::*;

const DAY: u64 = 86_400_000;
fn preview(
    connection: &Connection,
    project: Option<&str>,
    before: u64,
) -> Result<OutputCleanupPreview> {
    if before > now_ms().saturating_sub(DAY)
        || project.is_some_and(|p| !winbridge_core::config::valid_id(p))
    {
        return Err("Cleanup requires a valid project and output at least one day old".into());
    }
    // Pending attention protects its target and linked workflow children. Active
    // applications also protect their entire startup workflow, including builds.
    let mut statement = connection.prepare("SELECT t.id,t.project,t.record,sum(length(o.bytes)) AS bytes,t.revision
        FROM tasks t JOIN output o ON o.task=t.id
        WHERE (?1 IS NULL OR t.project=?1)
          AND json_extract(t.record,'$.status') IN ('succeeded','failed','cancelled','interrupted')
          AND json_extract(t.record,'$.finishedAtMs') < ?2
          AND NOT EXISTS (SELECT 1 FROM workflows w WHERE
            (w.status IN ('queued','running','indeterminate') OR EXISTS (
              SELECT 1 FROM applications a WHERE a.workflow=w.id AND a.status IN ('starting','ready','stopping','indeterminate')))
            AND EXISTS (SELECT 1 FROM json_each(w.record,'$.steps') s WHERE json_extract(s.value,'$.operationId')=t.id))
          AND NOT EXISTS (SELECT 1 FROM applications a WHERE a.task=t.id AND a.status IN ('starting','ready','stopping','indeterminate'))
          AND NOT EXISTS (SELECT 1 FROM attention n WHERE n.acknowledged IS NULL AND (
            n.task=t.id OR json_extract(n.target,'$.taskId')=t.id OR EXISTS (
              SELECT 1 FROM workflows w,json_each(w.record,'$.steps') s
              WHERE w.id=coalesce(json_extract(n.target,'$.workflowId'),
                (SELECT a.workflow FROM applications a WHERE a.id=json_extract(n.target,'$.applicationId')))
                AND json_extract(s.value,'$.operationId')=t.id)))
        GROUP BY t.id ORDER BY json_extract(t.record,'$.finishedAtMs'),t.id LIMIT 51").map_err(sql)?;
    let mut rows = statement
        .query(params![
            project,
            i64::try_from(before).map_err(|_| "Cleanup date exceeds journal range")?
        ])
        .map_err(sql)?;
    let mut entries = Vec::new();
    let mut raw_records = Vec::new();
    while let Some(row) = rows.next().map_err(sql)? {
        let raw: String = row.get(2).map_err(sql)?;
        let task: TaskRecord = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let bytes: i64 = row.get(3).map_err(sql)?;
        entries.push(OutputCleanupEntry {
            task_id: row.get(0).map_err(sql)?,
            project_id: row.get(1).map_err(sql)?,
            bytes: u64::try_from(bytes).map_err(|_| "Invalid output size")?,
            finished_at_ms: task
                .finished_at_ms
                .ok_or("Terminal task lacks finish time")?,
        });
        let revision: i64 = row.get(4).map_err(sql)?;
        raw_records.push((raw, revision));
    }
    let has_more = entries.len() > 50;
    entries.truncate(50);
    raw_records.truncate(50);
    let hash =
        serde_json::to_vec(&(before, project, &entries, raw_records)).map_err(|e| e.to_string())?;
    let plan = OutputCleanupPlan {
        before_ms: before,
        project_id: project.map(str::to_owned),
        task_ids: entries.iter().map(|e| e.task_id.clone()).collect(),
        sha256: format!("{:x}", Sha256::digest(hash)),
    };
    Ok(OutputCleanupPreview {
        bytes: entries.iter().map(|e| e.bytes).sum(),
        plan,
        entries,
        has_more,
        observed_at_ms: now_ms(),
    })
}
impl Store {
    pub fn preview_output_cleanup(
        &self,
        query: &OutputCleanupQuery,
    ) -> Result<OutputCleanupPreview> {
        if !(1..=3650).contains(&query.older_than_days) {
            return Err("Output age must be 1–3650 days".into());
        }
        preview(
            &self.connection,
            query.project_id.as_deref(),
            now_ms().saturating_sub(u64::from(query.older_than_days) * DAY),
        )
    }
    pub fn apply_output_cleanup(
        &mut self,
        request: &ApplyOutputCleanup,
    ) -> Result<OutputCleanupResult> {
        self.guard_callback(None, None)?;
        if !winbridge_core::config::valid_id(&request.idempotency_key)
            || request.plan.task_ids.len() > 50
            || request.plan.sha256.len() != 64
        {
            return Err("Invalid cleanup key/plan; preview a batch first".into());
        }
        let payload = serde_json::to_string(&request.plan).map_err(|e| e.to_string())?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = transaction
            .query_row(
                "SELECT payload,record FROM output_cleanup WHERE key=?1",
                [&request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((old, record)) = existing {
            if old != payload {
                return Err(
                    "Cleanup key belongs to another plan; use the original request to reconcile"
                        .into(),
                );
            }
            return serde_json::from_str(&record).map_err(|e| e.to_string());
        }
        let current = preview(
            &transaction,
            request.plan.project_id.as_deref(),
            request.plan.before_ms,
        )?;
        if current.plan.sha256 != request.plan.sha256
            || current.plan.task_ids != request.plan.task_ids
        {
            return Err("Cleanup preview changed: output, operation or attention state differs; refresh before deleting".into());
        }
        let result = OutputCleanupResult {
            idempotency_key: request.idempotency_key.clone(),
            removed_tasks: current.entries.len() as u64,
            removed_bytes: current.bytes,
            occurred_at_ms: now_ms(),
        };
        for entry in &current.entries {
            let observed = Store::read_task_snapshot(&transaction, &entry.task_id)?;
            let mut task = observed.record.clone();
            task.output_removed_at_ms = Some(result.occurred_at_ms);
            task.removed_output_bytes += entry.bytes;
            Store::event(&mut task, format!("Output retention removed {} saved stdout/stderr bytes; command, result and idempotency key retained", entry.bytes));
            transaction
                .execute("DELETE FROM output WHERE task=?1", [&entry.task_id])
                .map_err(sql)?;
            let remains: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM output WHERE task=?1)",
                    [&entry.task_id],
                    |row| row.get(0),
                )
                .map_err(sql)?;
            if remains {
                return Err(
                    "Output cleanup did not remove every approved chunk; transaction rejected"
                        .into(),
                );
            }
            Store::write_task_snapshot(&transaction, &observed, &task)?;
        }
        let changed = transaction
            .execute(
                "INSERT INTO output_cleanup(key,payload,record) VALUES (?1,?2,?3)",
                params![
                    result.idempotency_key,
                    payload,
                    serde_json::to_string(&result).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        if changed != 1 {
            return Err("Output cleanup receipt was not stored; transaction rejected".into());
        }
        transaction.commit().map_err(sql)?;
        Ok(result)
    }
}
impl Worker {
    pub fn automatic_output_cleanup(self: &Arc<Self>) -> Result<()> {
        if !self.config.output_retention.automatic
            || self.stop.load(Ordering::Acquire)
            || self.journal_recovery_required()?.is_some()
        {
            return Ok(());
        }
        let mut state = self
            .maintenance
            .lock()
            .map_err(|_| "Maintenance lock poisoned")?;
        let now = now_ms();
        if now < state.0.saturating_add(60_000)
            || self.maintenance_running.swap(true, Ordering::AcqRel)
        {
            return Ok(());
        }
        state.0 = now;
        self.active_jobs.fetch_add(1, Ordering::AcqRel);
        let worker = Arc::clone(self);
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut store = worker.store()?;
                if let Some(reason) = worker.journal_recovery_required()? {
                    return Err(reason);
                }
                let preview = store.preview_output_cleanup(&OutputCleanupQuery {
                    project_id: None,
                    older_than_days: worker.config.output_retention.older_than_days,
                })?;
                if !preview.entries.is_empty() {
                    store.apply_output_cleanup(&ApplyOutputCleanup {
                        idempotency_key: format!("automatic-{}", uuid::Uuid::new_v4()),
                        plan: preview.plan,
                    })?;
                }
                Ok(())
            })();
            if let Err(error) = &result {
                eprintln!("Automatic output cleanup not confirmed: {error}");
            }
            if let Ok(mut state) = worker.maintenance.lock() {
                state.1 = result.err();
            }
            worker.maintenance_running.store(false, Ordering::Release);
            worker.active_jobs.fetch_sub(1, Ordering::AcqRel);
        });
        Ok(())
    }
}
