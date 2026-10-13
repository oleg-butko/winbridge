//! Durable attention records share the task journal, independently of desktop lifetime.
use crate::store::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, Row};
use winbridge_protocol::{
    ApplicationRecord, ApplicationStatus, AttentionEvent, AttentionEventPage, AttentionPage,
    AttentionRecord, AttentionSeverity, AttentionSubmission, AttentionTarget, WorkflowRecord,
    WorkflowStepStatus,
};

fn counter(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}
fn timestamp(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| "Journal timestamp exceeds integer range".into())
}
fn legacy_record(row: &Row<'_>) -> rusqlite::Result<AttentionRecord> {
    let severity: String = row.get(4)?;
    let severity = serde_json::from_str(&severity).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(AttentionRecord {
        attention_id: row.get(0)?,
        sequence: counter(row, 1)?,
        project_id: row.get(2)?,
        task_id: row.get(3)?,
        severity,
        message: row.get(5)?,
        origin: row.get(6)?,
        created_at_ms: counter(row, 7)?,
        acknowledged_at_ms: row
            .get::<_, Option<i64>>(8)?
            .map(|value| {
                u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(8, value))
            })
            .transpose()?,
    })
}
fn event(row: &Row<'_>) -> rusqlite::Result<AttentionEvent> {
    let target: String = row.get(9)?;
    let target = serde_json::from_str(&target).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let severity: String = row.get(4)?;
    Ok(AttentionEvent {
        attention_id: row.get(0)?,
        sequence: counter(row, 1)?,
        project_id: row.get(2)?,
        target,
        severity: serde_json::from_str(&severity).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        message: row.get(5)?,
        origin: row.get(6)?,
        created_at_ms: counter(row, 7)?,
        acknowledged_at_ms: row
            .get::<_, Option<i64>>(8)?
            .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(8, v)))
            .transpose()?,
    })
}
fn message(text: String) -> String {
    let mut out = String::new();
    for c in text.chars().map(|c| if c.is_control() { ' ' } else { c }) {
        if out.len() + c.len_utf8() > 1020 {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}
pub(crate) fn workflow_outcome(connection: &Connection, flow: &WorkflowRecord) -> Result<()> {
    let Some((severity, _)) = winbridge_core::attention::outcome(flow.status) else {
        return Ok(());
    };
    let step = flow
        .steps
        .iter()
        .find(|s| {
            matches!(
                s.status,
                WorkflowStepStatus::Failed | WorkflowStepStatus::Indeterminate
            )
        })
        .map_or("очередь/восстановление", |s| {
            s.name.as_str()
        });
    insert_outcome(
        connection,
        Outcome {
            project: &flow.submission.project_id,
            task: None,
            target: &AttentionTarget::Workflow {
                workflow_id: flow.workflow_id.clone(),
            },
            severity,
            message: &message(format!(
                "Workflow «{}» · {}: {}",
                flow.submission.name,
                step,
                flow.reason.as_deref().unwrap_or("Исход требует проверки")
            )),
            origin: "workflow_outcome",
            key: &flow.workflow_id,
            created: flow.finished_at_ms.unwrap_or_else(now_ms),
        },
    )
}
pub(crate) fn application_outcome(connection: &Connection, app: &ApplicationRecord) -> Result<()> {
    if app.ready_at_ms.is_none() {
        return Ok(());
    }
    // Startup failure is already represented by a single workflow event.
    let started: bool = connection
        .query_row(
            "SELECT status='succeeded' FROM workflows WHERE id=?1",
            [&app.workflow_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !started {
        return Ok(());
    }
    let severity = match app.status {
        ApplicationStatus::Failed => AttentionSeverity::Error,
        ApplicationStatus::Indeterminate | ApplicationStatus::Exited => AttentionSeverity::Warning,
        _ => return Ok(()),
    };
    let label = match app.status {
        ApplicationStatus::Failed => "ошибка",
        ApplicationStatus::Exited => "самостоятельное завершение",
        _ => "исход неизвестен",
    };
    insert_outcome(
        connection,
        Outcome {
            project: &app.project_id,
            task: Some(&app.task_id),
            target: &AttentionTarget::Application {
                application_id: app.application_id.clone(),
                workflow_id: app.workflow_id.clone(),
            },
            severity,
            message: &message(format!(
                "Приложение после успешного startup · {label}: {}",
                app.reason
                    .as_deref()
                    .unwrap_or("Приложение завершилось самостоятельно")
            )),
            origin: "application_outcome",
            key: &app.application_id,
            created: app.finished_at_ms.unwrap_or_else(now_ms),
        },
    )
}
struct Outcome<'a> {
    project: &'a str,
    task: Option<&'a str>,
    target: &'a AttentionTarget,
    severity: AttentionSeverity,
    message: &'a str,
    origin: &'a str,
    key: &'a str,
    created: u64,
}
fn insert_outcome(connection: &Connection, outcome: Outcome<'_>) -> Result<()> {
    let Outcome {
        project,
        task,
        target,
        severity,
        message,
        origin,
        key,
        created,
    } = outcome;
    let target = serde_json::to_string(target).map_err(|e| e.to_string())?;
    let changed = connection.execute("INSERT INTO attention(id,project,task,target,severity,message,origin,key,payload,created) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'',?9) ON CONFLICT(origin,key) DO NOTHING",
        params![uuid::Uuid::new_v4().to_string(),project,task,target,
            serde_json::to_string(&severity).map_err(|e|e.to_string())?,message,origin,key,timestamp(created)?]).map_err(sql)?;
    if changed != 1 {
        let existing: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM attention WHERE origin=?1 AND key=?2 AND project=?3 AND target=?4)",
            params![origin,key,project,target], |row| row.get(0),
        ).map_err(sql)?;
        if !existing {
            return Err("Outcome attention was not stored; terminal transaction rejected".into());
        }
    }
    Ok(())
}
const COLUMNS: &str = "id,sequence,project,task,severity,message,origin,created,acknowledged";

pub(crate) fn task_outcome(
    connection: &Connection,
    task: &winbridge_protocol::TaskRecord,
) -> Result<()> {
    let owned: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workflows w,json_each(w.record,'$.steps') s WHERE json_extract(s.value,'$.operationId')=?1)", [&task.task_id], |r|r.get(0)).map_err(sql)?;
    if owned {
        return Ok(());
    }
    if let Some((severity, message)) = winbridge_core::attention::outcome(task.status) {
        insert_outcome(
            connection,
            Outcome {
                project: &task.submission.project_id,
                task: Some(&task.task_id),
                target: &AttentionTarget::Task {
                    task_id: task.task_id.clone(),
                },
                severity,
                message,
                origin: "task_outcome",
                key: &task.task_id,
                created: task.finished_at_ms.unwrap_or_else(now_ms),
            },
        )?;
    }
    Ok(())
}

impl Store {
    pub fn request_attention(&mut self, request: &AttentionSubmission) -> Result<AttentionRecord> {
        self.guard_callback(None, None)?;
        winbridge_core::attention::validate(request)?;
        let mut frozen = request.clone();
        frozen.request_id.clear();
        let payload = serde_json::to_string(&frozen).map_err(|e| e.to_string())?;
        let task = self.get(&request.task_id)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = transaction
            .query_row(
                "SELECT id,payload FROM attention WHERE origin='agent_request' AND key=?1",
                [&request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((id, previous)) = existing {
            if previous != payload {
                return Err("Attention idempotencyKey belongs to a different payload; use the original request or a new key".into());
            }
            return Self::read_attention(&transaction, &id);
        }
        // Automatic outcomes are never dropped to satisfy this explicit-request limit.
        let pending: u64 = transaction.query_row("SELECT count(*) FROM attention WHERE origin='agent_request' AND acknowledged IS NULL", [], |r|counter(r,0)).map_err(sql)?;
        if pending >= 1000 {
            return Err(
                "Attention queue limit reached: acknowledge pending agent requests (1000)".into(),
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        let changed = transaction.execute(
            "INSERT INTO attention(id,project,task,target,severity,message,origin,key,payload,created) VALUES (?1,?2,?3,?9,?4,?5,'agent_request',?6,?7,?8)",
            params![id, task.submission.project_id, request.task_id,
                serde_json::to_string(&request.severity).map_err(|e|e.to_string())?, request.message,
                request.idempotency_key, payload, timestamp(now_ms())?, serde_json::to_string(&AttentionTarget::Task {task_id:request.task_id.clone()}).map_err(|e|e.to_string())?],
        ).map_err(sql)?;
        if changed != 1 {
            return Err(
                "local journal: attention acceptance not stored; acknowledgement rejected".into(),
            );
        }
        let record = Self::read_attention(&transaction, &id)?;
        transaction.commit().map_err(sql)?;
        Ok(record)
    }

    pub fn attention(&self, id: &str) -> Result<AttentionRecord> {
        Self::read_attention(&self.connection, id)
    }
    fn read_attention(connection: &Connection, id: &str) -> Result<AttentionRecord> {
        connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM attention WHERE id=?1 AND json_extract(target,'$.kind')='task'"),
                [id],
                legacy_record,
            )
            .optional()
            .map_err(sql)?
            .ok_or_else(|| format!("attentionId {id:?} was not found"))
    }

    pub fn acknowledge_attention(&mut self, id: &str) -> Result<AttentionRecord> {
        self.acknowledge_attention_inner(id, None, true)?;
        self.attention(id)
    }
    fn acknowledge_attention_inner(
        &mut self,
        id: &str,
        expected: Option<u64>,
        legacy: bool,
    ) -> Result<AttentionEvent> {
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if legacy {
            Self::read_attention(&transaction, id)?;
        }
        let record = Self::read_attention_event(&transaction, id)?;
        let revision: i64 = transaction
            .query_row("SELECT revision FROM attention WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        if expected.is_some_and(|value| value != revision as u64) {
            return Err("local journal: stale attention revision".into());
        }
        if record.acknowledged_at_ms.is_some() {
            return Ok(record);
        }
        let next = revision
            .checked_add(1)
            .ok_or("local journal: attention revision exhausted")?;
        let changed = transaction.execute("UPDATE attention SET acknowledged=?2,revision=?4 WHERE id=?1 AND revision=?3 AND acknowledged IS NULL",params![id,timestamp(now_ms())?,revision,next]).map_err(sql)?;
        if changed != 1 {
            return Err("local journal: attention acknowledgement not stored".into());
        }
        let updated = Self::read_attention_event(&transaction, id)?;
        transaction.commit().map_err(sql)?;
        Ok(updated)
    }
    pub fn list_attention(&self, after_sequence: u64, pending_only: bool) -> Result<AttentionPage> {
        let after = i64::try_from(after_sequence)
            .map_err(|_| "Attention cursor exceeds journal integer range")?;
        let pending_count = self
            .connection
            .query_row(
                "SELECT count(*) FROM attention WHERE acknowledged IS NULL AND json_extract(target,'$.kind')='task'",
                [],
                |r| counter(r, 0),
            )
            .map_err(sql)?;
        let mut query = self.connection.prepare(&format!("SELECT {COLUMNS} FROM attention WHERE sequence>?1 AND (?2=0 OR acknowledged IS NULL) AND json_extract(target,'$.kind')='task' ORDER BY sequence LIMIT 51")).map_err(sql)?;
        let mut records = query
            .query_map(params![after, pending_only], legacy_record)
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        let has_more = records.len() > 50;
        records.truncate(50);
        let next_sequence = records.last().map_or(after_sequence, |r| r.sequence);
        Ok(AttentionPage {
            records,
            next_sequence,
            has_more,
            pending_count,
        })
    }
    pub fn attention_event(&self, id: &str) -> Result<AttentionEvent> {
        Self::read_attention_event(&self.connection, id)
    }
    fn read_attention_event(connection: &Connection, id: &str) -> Result<AttentionEvent> {
        connection
            .query_row(
                &format!("SELECT {COLUMNS},target FROM attention WHERE id=?1"),
                [id],
                event,
            )
            .optional()
            .map_err(sql)?
            .ok_or_else(|| format!("attentionId {id:?} was not found"))
    }
    pub fn acknowledge_attention_event(&mut self, id: &str) -> Result<AttentionEvent> {
        self.acknowledge_attention_inner(id, None, false)
    }
    pub fn acknowledge_attention_event_at(
        &mut self,
        id: &str,
        revision: u64,
    ) -> Result<AttentionEvent> {
        self.acknowledge_attention_inner(id, Some(revision), false)
    }
    pub fn list_attention_events(
        &self,
        after_sequence: u64,
        pending_only: bool,
    ) -> Result<AttentionEventPage> {
        let after = i64::try_from(after_sequence)
            .map_err(|_| "Attention cursor exceeds journal integer range")?;
        let pending_count = self
            .connection
            .query_row(
                "SELECT count(*) FROM attention WHERE acknowledged IS NULL",
                [],
                |r| counter(r, 0),
            )
            .map_err(sql)?;
        let mut statement = self.connection.prepare(&format!("SELECT {COLUMNS},target FROM attention WHERE sequence>?1 AND (?2=0 OR acknowledged IS NULL) ORDER BY sequence LIMIT 51")).map_err(sql)?;
        let mut records = statement
            .query_map(params![after, pending_only], event)
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        let has_more = records.len() > 50;
        records.truncate(50);
        let next_sequence = records.last().map_or(after_sequence, |r| r.sequence);
        Ok(AttentionEventPage {
            records,
            next_sequence,
            has_more,
            pending_count,
        })
    }
}
