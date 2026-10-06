use rusqlite::{params, Connection, OptionalExtension};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use winbridge_protocol::{
    JournalEvent, OutputChunk, OutputStream, ResolvedCommand, TaskRecord, TaskStatus,
    TaskSubmission,
};

pub type Result<T> = std::result::Result<T, String>;
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(crate) fn sql(error: rusqlite::Error) -> String {
    format!("local journal: {error}")
}
pub struct Store {
    pub(crate) connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).map_err(sql)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(sql)?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        if version > 3 {
            return Err(format!("journal schema {version} is unsupported; expected 3; keep the database and use a compatible worker"));
        }
        if version == 0 {
            let tables: u32 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0)).map_err(sql)?;
            if tables != 0 {
                return Err(
                    "unversioned existing journal is not modified; select a new stateDirectory"
                        .into(),
                );
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE tasks (id TEXT PRIMARY KEY, project TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, record TEXT NOT NULL, UNIQUE(project,key));
                CREATE TABLE output (task TEXT NOT NULL REFERENCES tasks(id), sequence INTEGER NOT NULL, stream TEXT NOT NULL, offset INTEGER NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY(task,sequence));
                PRAGMA user_version=1; COMMIT;").map_err(sql)?;
        }
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
            )
            .map_err(sql)?;
        if version <= 1 {
            if version == 1 {
                let backup = path.with_file_name(format!(
                    "journal-v1-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE imports (id TEXT PRIMARY KEY, project TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, record TEXT NOT NULL, UNIQUE(project,key));
                PRAGMA user_version=2; COMMIT;").map_err(sql)?;
        }
        if version <= 2 {
            if version == 2 {
                let backup = path.with_file_name(format!(
                    "journal-v2-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE workflows (id TEXT PRIMARY KEY, project TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, status TEXT NOT NULL, record TEXT NOT NULL, UNIQUE(project,key));
                CREATE TABLE applications (id TEXT PRIMARY KEY, workflow TEXT NOT NULL REFERENCES workflows(id), task TEXT NOT NULL UNIQUE REFERENCES tasks(id), status TEXT NOT NULL, record TEXT NOT NULL);
                PRAGMA user_version=3; COMMIT;").map_err(sql)?;
        }
        Ok(Self { connection })
    }
    fn fingerprint(request: &TaskSubmission) -> Result<String> {
        let mut payload = request.clone();
        payload.request_id.clear(); // Correlation ID may change on a reconnect.
        serde_json::to_string(&payload).map_err(|e| e.to_string())
    }
    pub fn existing(&self, request: &TaskSubmission) -> Result<Option<TaskRecord>> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT payload,record FROM tasks WHERE project=?1 AND key=?2",
                params![request.project_id, request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        match row {
            None => Ok(None),
            Some((payload, record)) => {
                if payload != Self::fingerprint(request)? {
                    return Err("idempotencyKey already belongs to a different payload; use the original request or a new key for a new task".into());
                }
                Ok(Some(
                    serde_json::from_str(&record).map_err(|e| e.to_string())?,
                ))
            }
        }
    }
    pub fn accept(
        &mut self,
        request: TaskSubmission,
        command: ResolvedCommand,
        snapshot: Option<winbridge_protocol::VerifiedSnapshotRef>,
    ) -> Result<TaskRecord> {
        if let Some(task) = self.existing(&request)? {
            return Ok(task);
        }
        let now = now_ms();
        let task = TaskRecord {
            task_id: Uuid::new_v4().to_string(),
            submission: request,
            resolved_command: command,
            snapshot,
            status: TaskStatus::Queued,
            accepted_at_ms: now,
            started_at_ms: None,
            finished_at_ms: None,
            process_id: None,
            exit_code: None,
            reason: None,
            cancel_requested: false,
            output_bytes: 0,
            output_truncated: false,
            events: vec![JournalEvent {
                sequence: 1,
                occurred_at_ms: now,
                status: TaskStatus::Queued,
                summary: "Task accepted and committed to the local journal".into(),
            }],
        };
        let transaction = self.connection.transaction().map_err(sql)?;
        transaction
            .execute(
                "INSERT INTO tasks(id,project,key,payload,record) VALUES (?1,?2,?3,?4,?5)",
                params![
                    task.task_id,
                    task.submission.project_id,
                    task.submission.idempotency_key,
                    Self::fingerprint(&task.submission)?,
                    serde_json::to_string(&task).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        transaction.commit().map_err(sql)?; // Acceptance response is sent only after this commit.
        Ok(task)
    }
    pub fn get(&self, id: &str) -> Result<TaskRecord> {
        let record: Option<String> = self
            .connection
            .query_row("SELECT record FROM tasks WHERE id=?1", [id], |r| r.get(0))
            .optional()
            .map_err(sql)?;
        serde_json::from_str(&record.ok_or_else(|| format!("taskId {id:?} was not found"))?)
            .map_err(|e| e.to_string())
    }
    pub fn all(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT record FROM tasks ORDER BY rowid")
            .map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| serde_json::from_str(&r.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }
    pub fn recent(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT record FROM tasks ORDER BY rowid DESC LIMIT 10")
            .map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| serde_json::from_str(&r.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }
    pub fn save(&mut self, task: &TaskRecord) -> Result<()> {
        self.connection
            .execute(
                "UPDATE tasks SET record=?2 WHERE id=?1",
                params![
                    task.task_id,
                    serde_json::to_string(task).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn event(task: &mut TaskRecord, summary: impl Into<String>) {
        task.events.push(JournalEvent {
            sequence: task.events.len() as u64 + 1,
            occurred_at_ms: now_ms(),
            status: task.status,
            summary: summary.into(),
        });
    }
    pub fn finish(
        &mut self,
        id: &str,
        status: TaskStatus,
        exit_code: Option<i32>,
        reason: &str,
    ) -> Result<TaskRecord> {
        let mut task = self.get(id)?;
        if task.status.is_terminal() {
            return Ok(task);
        }
        task.status = status;
        task.exit_code = exit_code;
        task.finished_at_ms = Some(now_ms());
        task.reason = Some(reason.into());
        Self::event(&mut task, reason);
        self.save(&task)?;
        Ok(task)
    }
    pub fn cancel(&mut self, id: &str) -> Result<TaskRecord> {
        let mut task = self.get(id)?;
        if task.status.is_terminal() || task.cancel_requested {
            return Ok(task);
        }
        if task.status == TaskStatus::Queued {
            return self.finish(
                id,
                TaskStatus::Cancelled,
                None,
                "Cancelled before process launch",
            );
        }
        task.cancel_requested = true;
        Self::event(
            &mut task,
            "Cancellation requested; waiting for managed process tree termination",
        );
        self.save(&task)?;
        Ok(task)
    }
    pub fn recover(&mut self) -> Result<()> {
        for task in self.all()? {
            if task.status == TaskStatus::Running {
                self.finish(&task.task_id, TaskStatus::Indeterminate, None, "Worker restarted after launch intent; external command outcome is unknown. Automatic retry is disabled")?;
            }
        }
        Ok(())
    }
    pub fn append_output(
        &mut self,
        id: &str,
        stream: OutputStream,
        offset: u64,
        bytes: &[u8],
        limit: u64,
    ) -> Result<()> {
        let mut task = self.get(id)?;
        let was_truncated = task.output_truncated;
        let remaining = limit.saturating_sub(task.output_bytes) as usize;
        let accepted = bytes.len().min(remaining);
        if accepted < bytes.len() && !task.output_truncated {
            task.output_truncated = true;
            Self::event(&mut task, "Output quota reached; excess bytes are discarded while both pipes continue draining");
        }
        if accepted == 0 && was_truncated {
            return Ok(());
        }
        let transaction = self.connection.transaction().map_err(sql)?;
        if accepted > 0 {
            let sequence: i64 = transaction
                .query_row(
                    "SELECT coalesce(max(sequence),0)+1 FROM output WHERE task=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            transaction
                .execute(
                    "INSERT INTO output(task,sequence,stream,offset,bytes) VALUES (?1,?2,?3,?4,?5)",
                    params![
                        id,
                        sequence,
                        match stream {
                            OutputStream::Stdout => "stdout",
                            OutputStream::Stderr => "stderr",
                        },
                        i64::try_from(offset).map_err(|_| "output offset exceeds SQLite range")?,
                        &bytes[..accepted]
                    ],
                )
                .map_err(sql)?;
            task.output_bytes += accepted as u64;
        }
        transaction
            .execute(
                "UPDATE tasks SET record=?2 WHERE id=?1",
                params![id, serde_json::to_string(&task).map_err(|e| e.to_string())?],
            )
            .map_err(sql)?;
        transaction.commit().map_err(sql)
    }
    pub fn output(&self, id: &str, after_sequence: u64) -> Result<Vec<OutputChunk>> {
        self.get(id)?;
        if after_sequence > i64::MAX as u64 {
            return Err("afterSequence exceeds journal range".into());
        }
        let mut statement = self.connection.prepare("SELECT sequence,stream,offset,bytes FROM output WHERE task=?1 AND sequence>?2 ORDER BY sequence LIMIT 16").map_err(sql)?;
        let rows = statement
            .query_map(params![id, after_sequence as i64], |row| {
                Ok(OutputChunk {
                    sequence: row.get::<_, i64>(0)? as u64,
                    stream: if row.get::<_, String>(1)? == "stdout" {
                        OutputStream::Stdout
                    } else {
                        OutputStream::Stderr
                    },
                    offset: row.get::<_, i64>(2)? as u64,
                    bytes: row.get(3)?,
                })
            })
            .map_err(sql)?;
        rows.map(|r| r.map_err(sql)).collect()
    }
}
