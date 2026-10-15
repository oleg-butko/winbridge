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

mod admission;
mod archive;
mod inspection;
mod leases;
mod ownership;
pub use leases::{CleanupProof, LeaseScope, LeaseState, ResourceLease};
pub use ownership::{
    ExecutionAttempt, ExecutionToken, JournalAuthority, OperationKind, OperationRef,
};
mod domain_records;
pub use domain_records::JournalSnapshot;
mod operation_pages;
mod task_records;
mod work_scheduling;
pub use task_records::TaskSnapshot;
pub const JOURNAL_SCHEMA_VERSION: u32 = 12;

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
    authority: JournalAuthority,
    callback_attempt: Option<ExecutionToken>,
    read_only: bool,
    work_scheduling: Option<winbridge_protocol::WorkSchedulingPolicy>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path).map_err(sql)?;
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
        if version <= 8 {
            if version == 8 {
                let backup = path.with_file_name(format!(
                    "journal-v8-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            let migration = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sql)?;
            migration.execute_batch("CREATE TABLE journal_authority(singleton INTEGER PRIMARY KEY CHECK(singleton=1), incarnation TEXT NOT NULL CHECK(length(incarnation)>0), epoch INTEGER NOT NULL CHECK(typeof(epoch)='integer' AND epoch>=0), authority TEXT NOT NULL CHECK(length(authority)>0));
                CREATE TABLE execution_attempts(kind TEXT NOT NULL CHECK(kind IN ('task','import')), operation TEXT NOT NULL, project TEXT NOT NULL, attempt TEXT NOT NULL UNIQUE, incarnation TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(typeof(epoch)='integer' AND epoch>0), authority TEXT NOT NULL, started INTEGER NOT NULL, completed INTEGER, outcome TEXT, PRIMARY KEY(kind,operation), CHECK((completed IS NULL AND outcome IS NULL) OR (completed IS NOT NULL AND outcome IS NOT NULL AND outcome IN ('succeeded','failed','cancelled','interrupted','indeterminate'))));
                CREATE TRIGGER authority_epoch_guard BEFORE UPDATE ON journal_authority WHEN NEW.singleton != OLD.singleton OR NEW.epoch != OLD.epoch+1 OR NEW.authority=OLD.authority BEGIN SELECT RAISE(ABORT,'Authority activation requires next epoch and new identity'); END;
                CREATE TRIGGER attempt_identity_guard BEFORE UPDATE ON execution_attempts WHEN NEW.kind != OLD.kind OR NEW.operation != OLD.operation OR NEW.project != OLD.project OR NEW.attempt != OLD.attempt OR NEW.incarnation != OLD.incarnation OR NEW.epoch != OLD.epoch OR NEW.authority != OLD.authority OR NEW.started != OLD.started OR OLD.completed IS NOT NULL OR NEW.completed IS NULL BEGIN SELECT RAISE(ABORT,'Attempt identity is immutable and completion is final'); END;
                CREATE TRIGGER attempt_no_delete BEFORE DELETE ON execution_attempts BEGIN SELECT RAISE(ABORT,'Execution attempt history must be retained'); END;").map_err(sql)?;
            let changed = migration.execute("INSERT INTO journal_authority(singleton,incarnation,epoch,authority) VALUES(1,?1,0,?2)",params![Uuid::new_v4().to_string(),Uuid::new_v4().to_string()]).map_err(sql)?;
            if changed != 1 {
                return Err("local journal: migration authority not stored".into());
            }
            migration
                .pragma_update(None, "user_version", 9)
                .map_err(sql)?;
            migration.commit().map_err(sql)?;
        }
        if version < 10 {
            if version == 9 {
                let backup = path.with_file_name(format!(
                    "journal-v9-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            let migration = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sql)?;
            migration.execute_batch("CREATE TABLE resource_leases(id TEXT PRIMARY KEY,project TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('task','import')),operation TEXT NOT NULL,attempt TEXT UNIQUE,incarnation TEXT NOT NULL,epoch INTEGER NOT NULL CHECK(typeof(epoch)='integer' AND epoch>=0),authority TEXT NOT NULL,acquired INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN ('held','uncertain','released')),scope TEXT NOT NULL DEFAULT 'project_mutation' CHECK(scope IN ('project_mutation','application_lifetime')),proof TEXT CHECK(proof IN ('no_process_created','process_tree_empty','import_thread_stopped')),members TEXT NOT NULL CHECK(json_valid(members)),reason TEXT,revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0),CHECK((kind='task' AND (proof IS NULL OR proof IN ('no_process_created','process_tree_empty'))) OR (kind='import' AND (proof IS NULL OR proof='import_thread_stopped'))),CHECK(state!='released' OR proof IS NOT NULL));
                CREATE UNIQUE INDEX resource_project_owner ON resource_leases(project) WHERE state!='released' AND scope='project_mutation';
                CREATE TRIGGER resource_revision_guard BEFORE UPDATE ON resource_leases WHEN NEW.revision!=OLD.revision+1 OR OLD.state='released' OR NEW.id!=OLD.id OR NEW.project!=OLD.project OR NEW.kind!=OLD.kind OR NEW.operation!=OLD.operation OR NEW.attempt IS NOT OLD.attempt OR NEW.incarnation!=OLD.incarnation OR NEW.epoch!=OLD.epoch OR NEW.authority!=OLD.authority OR NEW.acquired!=OLD.acquired OR NEW.members!=OLD.members OR (NEW.scope!=OLD.scope AND NOT(OLD.scope='project_mutation' AND NEW.scope='application_lifetime' AND OLD.state='held' AND NEW.state='held' AND OLD.kind='task' AND OLD.proof IS NULL AND NEW.proof IS NULL AND EXISTS(SELECT 1 FROM applications a JOIN workflows w ON w.id=a.workflow WHERE a.task=OLD.operation AND a.status='ready' AND json_extract(w.record,'$.steps[3].status')='succeeded'))) OR (OLD.proof IS NOT NULL AND NEW.proof IS NOT OLD.proof) OR (OLD.state='uncertain' AND NEW.state='held') BEGIN SELECT RAISE(ABORT,'Resource identity/receipt is immutable and revision must advance'); END;
                CREATE TRIGGER resource_no_delete BEFORE DELETE ON resource_leases BEGIN SELECT RAISE(ABORT,'Resource ownership history must be retained'); END;").map_err(sql)?;
            migration
                .pragma_update(None, "user_version", 10)
                .map_err(sql)?;
            migration.commit().map_err(sql)?;
        }
        if version < 11 {
            if version == 10 {
                let backup = path.with_file_name(format!(
                    "journal-v10-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            let migration = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sql)?;
            migration.execute_batch("ALTER TABLE resource_leases ADD COLUMN physical TEXT CHECK(physical IS NULL OR (json_valid(physical) AND kind='task' AND attempt IS NOT NULL AND (proof IS NULL OR proof='process_tree_empty')));
                CREATE TRIGGER resource_physical_guard BEFORE UPDATE ON resource_leases WHEN (OLD.physical IS NOT NULL AND NEW.physical IS NOT OLD.physical) OR (OLD.physical IS NULL AND NEW.physical IS NOT NULL AND (OLD.kind!='task' OR OLD.attempt IS NULL OR OLD.state!='held' OR NEW.state!='held' OR OLD.proof IS NOT NULL OR NEW.proof IS NOT NULL)) BEGIN SELECT RAISE(ABORT,'Physical execution identity is immutable and may bind only to an active task'); END;
                PRAGMA user_version=11;").map_err(sql)?;
            migration.commit().map_err(sql)?;
        }
        if version <= 11 {
            Self::read_authority(&connection)?;
            if version == 11 {
                let backup = path.with_file_name(format!(
                    "journal-v11-{}-{}.sqlite",
                    now_ms(),
                    Uuid::new_v4()
                ));
                connection
                    .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                    .map_err(sql)?;
                eprintln!("Journal migration consistent backup: {}", backup.display());
            }
            let migration = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sql)?;
            migration.execute_batch("CREATE TABLE journal_admission(singleton INTEGER PRIMARY KEY CHECK(singleton=1),incarnation TEXT NOT NULL,require_fenced INTEGER NOT NULL DEFAULT 0 CHECK(require_fenced IN (0,1)));
                INSERT INTO journal_admission SELECT 1,incarnation,0 FROM journal_authority WHERE singleton=1;
                CREATE TRIGGER admission_insert_guard BEFORE INSERT ON journal_admission WHEN EXISTS(SELECT 1 FROM journal_admission) BEGIN SELECT RAISE(ABORT,'Admission boundary is already initialized'); END;
                CREATE TRIGGER admission_update_guard BEFORE UPDATE ON journal_admission WHEN NEW.singleton!=OLD.singleton OR NEW.incarnation!=OLD.incarnation OR OLD.require_fenced=1 OR NEW.require_fenced!=1 BEGIN SELECT RAISE(ABORT,'Fenced admission cannot be downgraded or rebound'); END;
                CREATE TRIGGER admission_delete_guard BEFORE DELETE ON journal_admission BEGIN SELECT RAISE(ABORT,'Admission boundary cannot be deleted'); END;
                PRAGMA user_version=12;").map_err(sql)?;
            migration.commit().map_err(sql)?;
        }
        let authority = Self::read_authority(&connection)?;
        Ok(Self {
            connection,
            authority,
            callback_attempt: None,
            read_only: false,
            work_scheduling: None,
        })
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
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
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
        self.guard_callback(Some(OperationKind::Task), Some(id))?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
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
        self.guard_callback(None, None)?;
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
        self.guard_callback(Some(OperationKind::Task), Some(id))?;
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
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
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
