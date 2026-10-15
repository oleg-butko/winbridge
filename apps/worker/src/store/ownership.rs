//! Durable journal authority and execution attempts; these are not wire DTOs.
use super::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use winbridge_core::task::{TaskEffect, TaskEvent, TaskTransition};
use winbridge_protocol::{ImportRecord, TaskStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalAuthority {
    pub incarnation: String,
    pub epoch: u64,
    pub authority_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Task,
    Import,
}
impl OperationKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Import => "import",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRef {
    pub incarnation: String,
    pub kind: OperationKind,
    pub operation_id: String,
    pub project_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionToken {
    pub operation: OperationRef,
    pub authority: JournalAuthority,
    pub attempt_id: String,
}

#[derive(Clone, Debug)]
pub struct ExecutionAttempt {
    pub token: ExecutionToken,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub outcome: Option<TaskStatus>,
}

impl Store {
    pub fn execution_attempt(
        &self,
        kind: OperationKind,
        id: &str,
    ) -> Result<Option<ExecutionAttempt>> {
        type AttemptRow = (
            String,
            String,
            String,
            i64,
            String,
            i64,
            Option<i64>,
            Option<String>,
        );
        let row: Option<AttemptRow> = self.connection.query_row("SELECT project,attempt,incarnation,epoch,authority,started,completed,outcome FROM execution_attempts WHERE kind=?1 AND operation=?2",params![kind.as_str(),id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional().map_err(sql)?;
        row.map(
            |(project, attempt, incarnation, epoch, authority, started, completed, outcome)| {
                Ok(ExecutionAttempt {
                    token: ExecutionToken {
                        operation: OperationRef {
                            incarnation: incarnation.clone(),
                            kind,
                            operation_id: id.into(),
                            project_id: project,
                        },
                        authority: JournalAuthority {
                            incarnation,
                            epoch: u64::try_from(epoch)
                                .map_err(|_| "local journal: negative epoch")?,
                            authority_id: authority,
                        },
                        attempt_id: attempt,
                    },
                    started_at_ms: u64::try_from(started)
                        .map_err(|_| "local journal: negative attempt timestamp")?,
                    completed_at_ms: completed
                        .map(u64::try_from)
                        .transpose()
                        .map_err(|_| "local journal: negative completion timestamp")?,
                    outcome: outcome
                        .map(|s| serde_json::from_value(serde_json::Value::String(s)))
                        .transpose()
                        .map_err(|e| format!("local journal: {e}"))?,
                })
            },
        )
        .transpose()
    }
    pub(crate) fn read_authority(connection: &Connection) -> Result<JournalAuthority> {
        let (incarnation, epoch, authority_id): (String, i64, String) = connection
            .query_row(
                "SELECT incarnation,epoch,authority FROM journal_authority WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql)?;
        Self::authority_from_parts(incarnation, epoch, authority_id)
    }
    pub(crate) fn authority_from_parts(
        incarnation: String,
        epoch: i64,
        authority_id: String,
    ) -> Result<JournalAuthority> {
        if incarnation.is_empty() || authority_id.is_empty() || epoch < 0 {
            return Err("local journal: invalid authority".into());
        }
        Ok(JournalAuthority {
            incarnation,
            epoch: epoch as u64,
            authority_id,
        })
    }
    pub fn journal_authority(&self) -> &JournalAuthority {
        &self.authority
    }
    pub(crate) fn assert_authority(
        connection: &Connection,
        expected: &JournalAuthority,
    ) -> Result<()> {
        if Self::read_authority(connection)? != *expected {
            return Err("local journal: stale journal incarnation/owner epoch; writes and new effects rejected".into());
        }
        Ok(())
    }
    pub fn check_authority(&self) -> Result<()> {
        Self::assert_authority(&self.connection, &self.authority)?;
        self.requires_fenced_requests()?;
        Ok(())
    }

    /// Runtime calls this only after acquiring the existing exclusive instance lock.
    /// It does not authorize takeover of a live process or restoration of a backup.
    pub fn begin_authority(&mut self) -> Result<JournalAuthority> {
        self.guard_callback(None, None)?;
        let expected = self.authority.clone();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        Self::assert_authority(&transaction, &expected)?;
        let next = i64::try_from(expected.epoch)
            .ok()
            .and_then(|e| e.checked_add(1))
            .ok_or("local journal: owner epoch exhausted; stop admission")?;
        let authority_id = uuid::Uuid::new_v4().to_string();
        let changed = transaction.execute("UPDATE journal_authority SET epoch=?2,authority=?3 WHERE singleton=1 AND incarnation=?1 AND epoch=?4 AND authority=?5",params![expected.incarnation,next,authority_id,expected.epoch as i64,expected.authority_id]).map_err(sql)?;
        if changed != 1 {
            return Err("local journal: authority was not committed".into());
        }
        transaction.commit().map_err(sql)?;
        self.authority = JournalAuthority {
            incarnation: expected.incarnation,
            epoch: next as u64,
            authority_id,
        };
        Ok(self.authority.clone())
    }
    pub(crate) fn write_transaction(
        &mut self,
        behavior: TransactionBehavior,
    ) -> Result<Transaction<'_>> {
        self.require_writable()?;
        let expected = self.authority.clone();
        let callback = self.callback_attempt.clone();
        let transaction = self
            .connection
            .transaction_with_behavior(behavior)
            .map_err(sql)?;
        Self::assert_authority(&transaction, &expected)?;
        Self::read_admission_boundary(&transaction, &expected.incarnation)?;
        if let Some(token) = callback {
            Self::assert_attempt(&transaction, &token)?;
        }
        Ok(transaction)
    }
    pub fn with_execution<T>(
        &mut self,
        token: &ExecutionToken,
        callback: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        Self::assert_authority(&self.connection, &self.authority)?;
        if token.authority != self.authority {
            return Err("local journal: callback owner is stale".into());
        }
        Self::assert_attempt(&self.connection, token)?;
        if self
            .callback_attempt
            .as_ref()
            .is_some_and(|previous| previous != token)
        {
            return Err("local journal: nested execution scope cannot switch operation".into());
        }
        let previous = self.callback_attempt.replace(token.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(self)));
        self.callback_attempt = previous;
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    pub fn check_execution(&self, token: &ExecutionToken) -> Result<()> {
        if token.authority != self.authority {
            return Err("local journal: callback owner is stale".into());
        }
        Self::assert_attempt(&self.connection, token)
    }
    pub(crate) fn guard_callback(
        &self,
        kind: Option<OperationKind>,
        id: Option<&str>,
    ) -> Result<()> {
        self.require_writable()?;
        self.check_authority()?;
        if let Some(token) = &self.callback_attempt {
            if kind != Some(token.operation.kind)
                || id != Some(token.operation.operation_id.as_str())
            {
                return Err("local journal: execution callback cannot mutate another operation or administrative state".into());
            }
        }
        Ok(())
    }
    pub(super) fn require_writable(&self) -> Result<()> {
        if self.read_only {
            return Err("local journal: read-only recovery inspection; no mutation or new effect is authorized".into());
        }
        Ok(())
    }
    pub(crate) fn assert_attempt(connection: &Connection, token: &ExecutionToken) -> Result<()> {
        Self::assert_authority(connection, &token.authority)?;
        if token.operation.incarnation != token.authority.incarnation {
            return Err("local journal: operation incarnation mismatch".into());
        }
        let present: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE kind=?1 AND operation=?2 AND project=?3 AND attempt=?4 AND incarnation=?5 AND epoch=?6 AND authority=?7 AND completed IS NULL)",params![token.operation.kind.as_str(),token.operation.operation_id,token.operation.project_id,token.attempt_id,token.authority.incarnation,token.authority.epoch as i64,token.authority.authority_id],|r|r.get(0)).map_err(sql)?;
        if !present {
            return Err("local journal: execution attempt is missing, completed or belongs to another owner/operation".into());
        }
        Ok(())
    }
    fn insert_attempt(
        connection: &Connection,
        authority: &JournalAuthority,
        kind: OperationKind,
        id: &str,
        project: &str,
    ) -> Result<ExecutionToken> {
        if authority.epoch == 0 {
            return Err("local journal: execution requires an active authority".into());
        }
        let token = ExecutionToken {
            operation: OperationRef {
                incarnation: authority.incarnation.clone(),
                kind,
                operation_id: id.into(),
                project_id: project.into(),
            },
            authority: authority.clone(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
        };
        let changed = connection.execute("INSERT INTO execution_attempts(kind,operation,project,attempt,incarnation,epoch,authority,started) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![kind.as_str(),id,project,token.attempt_id,authority.incarnation,authority.epoch as i64,authority.authority_id,now_ms() as i64]).map_err(sql)?;
        if changed != 1 {
            return Err(
                "local journal: execution attempt was not stored; dispatch rejected".into(),
            );
        }
        Ok(token)
    }
    pub fn claim_task(&mut self, id: &str) -> Result<(TaskTransition, ExecutionToken)> {
        self.guard_callback(None, None)?;
        let authority = self.authority.clone();
        let policy = self.work_scheduling.clone();
        let transaction = self.write_transaction(TransactionBehavior::Immediate)?;
        Self::require_work_capacity(&transaction, policy.as_ref())?;
        let observed = Self::read_task_snapshot(&transaction, id)?;
        let decision = Self::transition_task_on(&transaction, id, None, TaskEvent::LaunchIntent)?;
        if decision.effect != TaskEffect::ExecuteAcceptedCommand {
            return Err("local journal: claim did not authorize execution".into());
        }
        let token = Self::insert_attempt(
            &transaction,
            &authority,
            OperationKind::Task,
            id,
            &observed.record.submission.project_id,
        )?;
        Self::acquire_execution_lease(&transaction, &token)?;
        transaction.commit().map_err(sql)?;
        Ok((decision, token))
    }
    pub fn claim_import(&mut self, id: &str) -> Result<(ImportRecord, ExecutionToken)> {
        self.guard_callback(None, None)?;
        let authority = self.authority.clone();
        let policy = self.work_scheduling.clone();
        let transaction = self.write_transaction(TransactionBehavior::Immediate)?;
        Self::require_work_capacity(&transaction, policy.as_ref())?;
        let mut observed = Self::read_domain::<ImportRecord>(&transaction, id)?;
        if observed.status != TaskStatus::Queued || observed.cancel_requested {
            return Err("local journal: import claim requires queued work".into());
        }
        observed.status = TaskStatus::Running;
        observed.phase = "validate".into();
        observed.reason = None;
        Self::import_event(
            &mut observed,
            "Import lease acquired; validating immutable delivery",
        );
        Self::write_domain(&transaction, &observed)?;
        let token = Self::insert_attempt(
            &transaction,
            &authority,
            OperationKind::Import,
            id,
            &observed.submission.project_id,
        )?;
        Self::acquire_execution_lease(&transaction, &token)?;
        transaction.commit().map_err(sql)?;
        Ok((observed.record, token))
    }
    pub(crate) fn complete_attempt(
        connection: &Connection,
        kind: OperationKind,
        id: &str,
        status: TaskStatus,
    ) -> Result<()> {
        let row: Option<(String, Option<i64>)> = connection
            .query_row(
                "SELECT attempt,completed FROM execution_attempts WHERE kind=?1 AND operation=?2",
                params![kind.as_str(), id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((attempt, None)) = row {
            let changed = connection.execute("UPDATE execution_attempts SET completed=?2,outcome=?3 WHERE attempt=?1 AND completed IS NULL",params![attempt,now_ms() as i64,serde_json::to_value(status).map_err(|e|e.to_string())?.as_str()]).map_err(sql)?;
            if changed != 1 {
                return Err(
                    "local journal: attempt completion not stored; outcome rejected".into(),
                );
            }
        }
        Self::complete_execution_lease(connection, kind, id)?;
        Ok(())
    }
}
