use rusqlite::{params, Connection, OptionalExtension};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use winbridge_core::task::{self, TaskEffect, TaskEvent, TaskOutcome, TaskTransition};
use winbridge_protocol::{
    JournalEvent, OutputChunk, OutputStream, ResolvedCommand, TaskRecord, TaskStatus,
    TaskSubmission,
};

mod domain_records;
pub use domain_records::JournalSnapshot;
mod task_records;
pub use task_records::TaskSnapshot;
pub const JOURNAL_SCHEMA_VERSION: u32 = 8;

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
        if version > JOURNAL_SCHEMA_VERSION {
            return Err(format!("journal schema {version} is unsupported; expected {JOURNAL_SCHEMA_VERSION}; keep the database and use a compatible worker"));
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
        if version <= 3 {
            if version == 3 {
                let backup = path.with_file_name(format!(
                    "journal-v3-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE attention (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, project TEXT NOT NULL, task TEXT NOT NULL REFERENCES tasks(id), severity TEXT NOT NULL, message TEXT NOT NULL, origin TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, created INTEGER NOT NULL, acknowledged INTEGER, UNIQUE(origin,key));
                CREATE INDEX attention_pending ON attention(acknowledged,sequence);
                PRAGMA user_version=4; COMMIT;").map_err(sql)?;
        }
        if version <= 4 {
            if version == 4 {
                let backup = path.with_file_name(format!(
                    "journal-v4-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE attention_v5 (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, project TEXT NOT NULL, task TEXT REFERENCES tasks(id), target TEXT NOT NULL CHECK(json_valid(target)), severity TEXT NOT NULL, message TEXT NOT NULL, origin TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, created INTEGER NOT NULL, acknowledged INTEGER, UNIQUE(origin,key));
                INSERT INTO attention_v5(sequence,id,project,task,target,severity,message,origin,key,payload,created,acknowledged)
                  SELECT sequence,id,project,task,json_object('kind','task','taskId',task),severity,message,origin,key,payload,created,acknowledged FROM attention;
                INSERT INTO sqlite_sequence(name,seq) SELECT 'attention_v5',seq FROM sqlite_sequence WHERE name='attention' AND NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='attention_v5');
                UPDATE sqlite_sequence SET seq=max(seq,coalesce((SELECT seq FROM sqlite_sequence WHERE name='attention'),0)) WHERE name='attention_v5';
                DROP TABLE attention;
                ALTER TABLE attention_v5 RENAME TO attention;
                CREATE INDEX attention_pending ON attention(acknowledged,sequence);
                PRAGMA user_version=5; COMMIT;").map_err(sql)?;
        }
        if version <= 5 {
            if version == 5 {
                let backup = path.with_file_name(format!(
                    "journal-v5-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE output_cleanup (key TEXT PRIMARY KEY, payload TEXT NOT NULL, record TEXT NOT NULL);
                PRAGMA user_version=6; COMMIT;").map_err(sql)?;
        }
        if version <= 6 {
            if version == 6 {
                let backup = path.with_file_name(format!(
                    "journal-v6-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                ALTER TABLE tasks ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
                CREATE TRIGGER tasks_revision_guard BEFORE UPDATE ON tasks
                WHEN NEW.revision != OLD.revision+1 OR NEW.id != OLD.id OR NEW.project != OLD.project OR NEW.key != OLD.key OR NEW.payload != OLD.payload
                BEGIN SELECT RAISE(ABORT,'Task updates require the next journal revision and immutable identity/payload'); END;
                PRAGMA user_version=7; COMMIT;").map_err(sql)?;
        }
        if version <= 7 {
            if version == 7 {
                let backup = path.with_file_name(format!(
                    "journal-v7-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            connection.execute_batch("BEGIN IMMEDIATE;
                ALTER TABLE imports ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
                ALTER TABLE workflows ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
                ALTER TABLE applications ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
                ALTER TABLE attention ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
                CREATE TRIGGER imports_revision_guard BEFORE UPDATE ON imports WHEN NEW.revision != OLD.revision+1 OR NEW.id != OLD.id OR NEW.project != OLD.project OR NEW.key != OLD.key OR NEW.payload != OLD.payload BEGIN SELECT RAISE(ABORT,'Import revision/identity guard'); END;
                CREATE TRIGGER workflows_revision_guard BEFORE UPDATE ON workflows WHEN NEW.revision != OLD.revision+1 OR NEW.id != OLD.id OR NEW.project != OLD.project OR NEW.key != OLD.key OR NEW.payload != OLD.payload BEGIN SELECT RAISE(ABORT,'Workflow revision/identity guard'); END;
                CREATE TRIGGER applications_revision_guard BEFORE UPDATE ON applications WHEN NEW.revision != OLD.revision+1 OR NEW.id != OLD.id OR NEW.workflow != OLD.workflow OR NEW.task != OLD.task BEGIN SELECT RAISE(ABORT,'Application revision/identity guard'); END;
                CREATE TRIGGER attention_revision_guard BEFORE UPDATE ON attention WHEN NEW.revision != OLD.revision+1 OR NEW.id != OLD.id OR NEW.sequence != OLD.sequence OR NEW.project != OLD.project OR NEW.task IS NOT OLD.task OR NEW.target != OLD.target OR NEW.severity != OLD.severity OR NEW.message != OLD.message OR NEW.origin != OLD.origin OR NEW.key != OLD.key OR NEW.payload != OLD.payload OR NEW.created != OLD.created OR OLD.acknowledged IS NOT NULL OR NEW.acknowledged IS NULL BEGIN SELECT RAISE(ABORT,'Attention revision/identity/ack guard'); END;
                PRAGMA user_version=8; COMMIT;").map_err(sql)?;
        }
        Ok(Self { connection })
    }
    fn fingerprint(request: &TaskSubmission) -> Result<String> {
        let mut payload = request.clone();
        payload.request_id.clear(); // Correlation ID may change on a reconnect.
        serde_json::to_string(&payload).map_err(|e| e.to_string())
    }
    pub fn existing(&self, request: &TaskSubmission) -> Result<Option<TaskRecord>> {
        Self::existing_task(&self.connection, request)
    }
    fn existing_task(
        connection: &Connection,
        request: &TaskSubmission,
    ) -> Result<Option<TaskRecord>> {
        let row: Option<(String, String)> = connection
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
        self.accept_with_origin(request, command, snapshot, None)
    }
    pub fn accept_with_origin(
        &mut self,
        request: TaskSubmission,
        command: ResolvedCommand,
        snapshot: Option<winbridge_protocol::VerifiedSnapshotRef>,
        recipe_origin: Option<winbridge_protocol::RecipeOrigin>,
    ) -> Result<TaskRecord> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql)?;
        let task = Self::accept_task_on(&transaction, request, command, snapshot, recipe_origin)?;
        transaction.commit().map_err(sql)?;
        Ok(task)
    }
    pub(crate) fn accept_task_on(
        connection: &Connection,
        request: TaskSubmission,
        command: ResolvedCommand,
        snapshot: Option<winbridge_protocol::VerifiedSnapshotRef>,
        recipe_origin: Option<winbridge_protocol::RecipeOrigin>,
    ) -> Result<TaskRecord> {
        if let Some(task) = Self::existing_task(connection, &request)? {
            return Ok(task);
        }
        let now = now_ms();
        let task = task::accepted_record(
            Uuid::new_v4().to_string(),
            request,
            command,
            snapshot,
            recipe_origin,
            now,
        );
        let changed = connection
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
        if changed != 1 {
            return Err(
                "local journal: Task acceptance was not stored; acknowledgement rejected".into(),
            );
        }
        Ok(task)
    }
    pub fn get(&self, id: &str) -> Result<TaskRecord> {
        self.task_snapshot(id).map(|snapshot| snapshot.record)
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
    pub fn active_task_count(&self) -> Result<u64> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM tasks WHERE json_extract(record,'$.status') IN ('queued','running')",
            [], |r| r.get(0),
        ).map_err(sql)?;
        u64::try_from(count).map_err(|_| "Invalid negative active task count".into())
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
        let outcome = TaskOutcome::try_from(status).map_err(|error| error.to_string())?;
        self.transition(
            id,
            TaskEvent::Completed {
                outcome,
                exit_code,
                reason: reason.into(),
            },
        )
        .map(|decision| decision.record)
    }

    /// Only a successfully committed decision may be dispatched by the execution owner.
    pub fn transition(&mut self, id: &str, event: TaskEvent) -> Result<TaskTransition> {
        self.transition_inner(id, None, event)
    }

    /// An asynchronous caller must reject a decision based on a stale observation.
    /// Execution completion uses a fresh record; launch revision is not an owner token.
    pub fn transition_at(
        &mut self,
        id: &str,
        revision: u64,
        event: TaskEvent,
    ) -> Result<TaskTransition> {
        let revision =
            i64::try_from(revision).map_err(|_| "Expected task revision exceeds journal range")?;
        self.transition_inner(id, Some(revision), event)
    }

    fn transition_inner(
        &mut self,
        id: &str,
        expected: Option<i64>,
        event: TaskEvent,
    ) -> Result<TaskTransition> {
        let transaction = self.connection.transaction().map_err(sql)?;
        let decision = Self::transition_task_on(&transaction, id, expected, event)?;
        transaction.commit().map_err(sql)?;
        Ok(decision)
    }
    pub(crate) fn transition_task_on(
        connection: &Connection,
        id: &str,
        expected: Option<i64>,
        event: TaskEvent,
    ) -> Result<TaskTransition> {
        let observed = Self::read_task_snapshot(connection, id)?;
        if let Some(revision) = expected {
            if revision != observed.revision() as i64 {
                return Err(format!(
                    "Task transition rejected: stale revision {revision}; current revision {}",
                    observed.revision()
                ));
            }
        }
        let decision =
            task::reduce(&observed.record, event, now_ms()).map_err(|error| error.to_string())?;
        if decision.effect != TaskEffect::None {
            Self::write_task_snapshot(connection, &observed, &decision.record)?;
            if decision.effect == TaskEffect::RecordTerminalOutcome {
                crate::attention::task_outcome(connection, &decision.record)?;
            }
        }
        Ok(decision)
    }
    pub fn cancel(&mut self, id: &str) -> Result<TaskRecord> {
        self.transition(id, TaskEvent::CancellationRequested)
            .map(|decision| decision.record)
    }
    pub fn recover(&mut self) -> Result<()> {
        for task in self.all()? {
            if task.status == TaskStatus::Running {
                self.transition(&task.task_id, TaskEvent::Recovered)?;
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
        let observed = self.task_snapshot(id)?;
        let mut task = observed.record.clone();
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
            let changed = transaction
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
            if changed != 1 {
                return Err("Output chunk was not stored; task output is not confirmed".into());
            }
            task.output_bytes += accepted as u64;
        }
        Self::write_task_snapshot(&transaction, &observed, &task)?;
        transaction.commit().map_err(sql)
    }
    pub fn output(&self, id: &str, after_sequence: u64) -> Result<Vec<OutputChunk>> {
        self.get(id)?;
        if after_sequence > i64::MAX as u64 {
            return Err("afterSequence exceeds journal range".into());
        }
        // Bound bytes and JSON, rather than making small pipe reads cost an RPC each.
        // Cursors still identify individual durable rows; never skip an unreturned row.
        const RAW_BUDGET: usize = 192 * 1024;
        const JSON_BUDGET: usize = 900 * 1024;
        let mut statement = self.connection.prepare("SELECT sequence,stream,offset,bytes FROM output WHERE task=?1 AND sequence>?2 ORDER BY sequence LIMIT 1024").map_err(sql)?;
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
        let mut chunks = Vec::new();
        let mut raw_bytes = 0;
        let mut json_bytes = 2; // JSON array brackets.
        for row in rows {
            let chunk = row.map_err(sql)?;
            // Conservative JSON size: decimal byte + comma, and <=128 bytes for
            // fixed field names, stream, two u64 values, punctuation and array comma.
            // Avoid serializing every row twice just to measure the page.
            let encoded = 128
                + chunk
                    .bytes
                    .iter()
                    .map(|byte| {
                        if *byte < 10 {
                            2
                        } else if *byte < 100 {
                            3
                        } else {
                            4
                        }
                    })
                    .sum::<usize>();
            if !chunks.is_empty()
                && (raw_bytes + chunk.bytes.len() > RAW_BUDGET
                    || json_bytes + encoded > JSON_BUDGET)
            {
                break;
            }
            if json_bytes + encoded > JSON_BUDGET {
                return Err("A saved output row exceeds the bounded IPC page; use a task diagnostic to inspect it".into());
            }
            raw_bytes += chunk.bytes.len();
            json_bytes += encoded;
            chunks.push(chunk);
        }
        Ok(chunks)
    }
}
