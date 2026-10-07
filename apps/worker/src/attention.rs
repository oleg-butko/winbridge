//! Durable attention records share the task journal, independently of desktop lifetime.
use crate::store::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, Row};
use winbridge_protocol::{AttentionPage, AttentionRecord, AttentionSubmission};

fn counter(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}
fn timestamp(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| "Journal timestamp exceeds integer range".into())
}
fn record(row: &Row<'_>) -> rusqlite::Result<AttentionRecord> {
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
const COLUMNS: &str = "id,sequence,project,task,severity,message,origin,created,acknowledged";

pub(crate) fn task_outcome(
    connection: &Connection,
    task: &winbridge_protocol::TaskRecord,
) -> Result<()> {
    if let Some((severity, message)) = winbridge_core::attention::outcome(task.status) {
        connection.execute(
            "INSERT INTO attention(id,project,task,severity,message,origin,key,payload,created) VALUES (?1,?2,?3,?4,?5,'task_outcome',?3,'',?6) ON CONFLICT(origin,key) DO NOTHING",
            params![uuid::Uuid::new_v4().to_string(), task.submission.project_id, task.task_id,
                serde_json::to_string(&severity).map_err(|e|e.to_string())?, message,
                timestamp(task.finished_at_ms.unwrap_or_else(now_ms))?],
        ).map_err(sql)?;
    }
    Ok(())
}

impl Store {
    pub fn request_attention(&mut self, request: &AttentionSubmission) -> Result<AttentionRecord> {
        winbridge_core::attention::validate(request)?;
        let mut frozen = request.clone();
        frozen.request_id.clear();
        let payload = serde_json::to_string(&frozen).map_err(|e| e.to_string())?;
        let existing: Option<(String, String)> = self
            .connection
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
            return self.attention(&id);
        }
        let task = self.get(&request.task_id)?;
        // Automatic outcomes are never dropped to satisfy this explicit-request limit.
        let pending: u64 = self.connection.query_row("SELECT count(*) FROM attention WHERE origin='agent_request' AND acknowledged IS NULL", [], |r|counter(r,0)).map_err(sql)?;
        if pending >= 1000 {
            return Err(
                "Attention queue limit reached: acknowledge pending agent requests (1000)".into(),
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO attention(id,project,task,severity,message,origin,key,payload,created) VALUES (?1,?2,?3,?4,?5,'agent_request',?6,?7,?8)",
            params![id, task.submission.project_id, request.task_id,
                serde_json::to_string(&request.severity).map_err(|e|e.to_string())?, request.message,
                request.idempotency_key, payload, timestamp(now_ms())?],
        ).map_err(sql)?; // FULL synchronous WAL commit precedes the response.
        self.attention(&id)
    }

    pub fn attention(&self, id: &str) -> Result<AttentionRecord> {
        self.connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM attention WHERE id=?1"),
                [id],
                record,
            )
            .optional()
            .map_err(sql)?
            .ok_or_else(|| format!("attentionId {id:?} was not found"))
    }

    pub fn acknowledge_attention(&mut self, id: &str) -> Result<AttentionRecord> {
        self.connection
            .execute(
                "UPDATE attention SET acknowledged=COALESCE(acknowledged,?2) WHERE id=?1",
                params![id, timestamp(now_ms())?],
            )
            .map_err(sql)?;
        self.attention(id)
    }

    pub fn list_attention(&self, after_sequence: u64, pending_only: bool) -> Result<AttentionPage> {
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
        let mut query = self.connection.prepare(&format!("SELECT {COLUMNS} FROM attention WHERE sequence>?1 AND (?2=0 OR acknowledged IS NULL) ORDER BY sequence LIMIT 51")).map_err(sql)?;
        let mut records = query
            .query_map(params![after, pending_only], record)
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
}
