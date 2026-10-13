//! Durable project execution ownership. A terminal result is not a cleanup proof.
use super::{now_ms, sql, ExecutionToken, JournalAuthority, OperationKind, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use winbridge_protocol::{
    ApplicationRecord, ApplicationStatus, ImportRecord, TaskStatus, WorkflowRecord,
    WorkflowStepStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseScope {
    ProjectMutation,
    ApplicationLifetime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseState {
    Held,
    Uncertain,
    Released,
}
impl LeaseState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Uncertain => "uncertain",
            Self::Released => "released",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupProof {
    NoProcessCreated,
    ProcessTreeEmpty,
    ImportThreadStopped,
}
impl CleanupProof {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoProcessCreated => "no_process_created",
            Self::ProcessTreeEmpty => "process_tree_empty",
            Self::ImportThreadStopped => "import_thread_stopped",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ResourceLease {
    pub lease_id: String,
    pub project_id: String,
    pub kind: OperationKind,
    pub operation_id: String,
    pub attempt_id: Option<String>,
    pub authority: JournalAuthority,
    pub state: LeaseState,
    pub scope: LeaseScope,
    pub cleanup_proof: Option<CleanupProof>,
    pub legacy_members: String,
    revision: i64,
}
const LEASE_COLUMNS: &str =
    "id,project,kind,operation,attempt,incarnation,epoch,authority,state,proof,members,revision,scope";
fn lease_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourceLease> {
    let epoch: i64 = row.get(6)?;
    let kind: String = row.get(2)?;
    let state: String = row.get(8)?;
    let proof: Option<String> = row.get(9)?;
    Ok(ResourceLease {
        lease_id: row.get(0)?,
        project_id: row.get(1)?,
        kind: match kind.as_str() {
            "task" => OperationKind::Task,
            "import" => OperationKind::Import,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        operation_id: row.get(3)?,
        attempt_id: row.get(4)?,
        authority: JournalAuthority {
            incarnation: row.get(5)?,
            epoch: u64::try_from(epoch).map_err(|_| rusqlite::Error::InvalidQuery)?,
            authority_id: row.get(7)?,
        },
        state: match state.as_str() {
            "held" => LeaseState::Held,
            "uncertain" => LeaseState::Uncertain,
            "released" => LeaseState::Released,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        cleanup_proof: match proof.as_deref() {
            None => None,
            Some("no_process_created") => Some(CleanupProof::NoProcessCreated),
            Some("process_tree_empty") => Some(CleanupProof::ProcessTreeEmpty),
            Some("import_thread_stopped") => Some(CleanupProof::ImportThreadStopped),
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        legacy_members: row.get(10)?,
        revision: row.get(11)?,
        scope: match row.get::<_, String>(12)?.as_str() {
            "project_mutation" => LeaseScope::ProjectMutation,
            "application_lifetime" => LeaseScope::ApplicationLifetime,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
    })
}
impl Store {
    pub fn resource_leases(&self, active_only: bool) -> Result<Vec<ResourceLease>> {
        let query = format!(
            "SELECT {LEASE_COLUMNS} FROM resource_leases {} ORDER BY rowid",
            if active_only {
                "WHERE state!='released'"
            } else {
                ""
            }
        );
        let mut statement = self.connection.prepare(&query).map_err(sql)?;
        let rows = statement.query_map([], lease_row).map_err(sql)?;
        rows.map(|r| r.map_err(sql)).collect()
    }
    fn active_lease(connection: &Connection, project: &str) -> Result<Option<ResourceLease>> {
        connection.query_row(&format!("SELECT {LEASE_COLUMNS} FROM resource_leases WHERE project=?1 AND state!='released' AND (scope='project_mutation' OR state='uncertain') LIMIT 1"),[project],lease_row).optional().map_err(sql)
    }
    fn operation_lease(
        connection: &Connection,
        kind: OperationKind,
        id: &str,
    ) -> Result<Option<ResourceLease>> {
        connection.query_row(&format!("SELECT {LEASE_COLUMNS} FROM resource_leases WHERE kind=?1 AND operation=?2 ORDER BY rowid DESC LIMIT 1"),params![kind.as_str(),id],lease_row).optional().map_err(sql)
    }
    pub fn project_resource_blocker(&self, project: &str) -> Result<Option<String>> {
        Ok(Self::active_lease(&self.connection, project)?.map(|lease| Self::blocked(&lease)))
    }
    fn blocked(lease: &ResourceLease) -> String {
        format!("resource busy: проект {} удерживает {} {}: {}; следующая операция ждёт подтверждённого освобождения ресурса",lease.project_id,if lease.kind==OperationKind::Task {"задача"}else{"импорт"},lease.operation_id,if lease.state==LeaseState::Uncertain {"остановка исполнителя не подтверждена"}else{"исполнитель ещё не освободил ресурс"})
    }
    fn change_lease(
        connection: &Connection,
        lease: &ResourceLease,
        state: LeaseState,
        proof: Option<CleanupProof>,
        reason: &str,
    ) -> Result<()> {
        if lease.state == LeaseState::Released {
            return Err("local journal: released resource lease is immutable".into());
        }
        let next = lease
            .revision
            .checked_add(1)
            .ok_or("local journal: lease revision exhausted")?;
        let changed=connection.execute("UPDATE resource_leases SET state=?2,proof=?3,reason=?4,revision=?5 WHERE id=?1 AND revision=?6",params![lease.lease_id,state.as_str(),proof.map(CleanupProof::as_str),reason,next,lease.revision]).map_err(sql)?;
        if changed != 1 {
            return Err(
                "local journal: resource lease update ignored or stale; release not confirmed"
                    .into(),
            );
        }
        Ok(())
    }
    pub(crate) fn acquire_execution_lease(
        connection: &Connection,
        token: &ExecutionToken,
    ) -> Result<()> {
        if let Some(lease) = Self::active_lease(connection, &token.operation.project_id)? {
            return Err(Self::blocked(&lease));
        }
        let mut statement = connection
            .prepare("SELECT record FROM workflows WHERE project=?1 AND status='running'")
            .map_err(sql)?;
        let rows = statement
            .query_map([&token.operation.project_id], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        for row in rows {
            let flow: WorkflowRecord =
                serde_json::from_str(&row.map_err(sql)?).map_err(|e| e.to_string())?;
            if !flow
                .steps
                .iter()
                .any(|s| s.operation_id.as_deref() == Some(&token.operation.operation_id))
            {
                return Err(format!(
                    "resource busy: проект {} зарезервирован workflow {}; операция ожидает",
                    token.operation.project_id, flow.workflow_id
                ));
            }
        }
        let a = &token.authority;
        let changed=connection.execute("INSERT INTO resource_leases(id,project,kind,operation,attempt,incarnation,epoch,authority,acquired,state,members) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'held','[]')",params![uuid::Uuid::new_v4().to_string(),token.operation.project_id,token.operation.kind.as_str(),token.operation.operation_id,token.attempt_id,a.incarnation,a.epoch as i64,a.authority_id,now_ms() as i64]).map_err(sql)?;
        if changed != 1 {
            return Err("local journal: resource lease was not stored; dispatch rejected".into());
        }
        Ok(())
    }
    pub(crate) fn check_workflow_reservation(
        connection: &Connection,
        flow: &WorkflowRecord,
    ) -> Result<()> {
        let mut statement = connection.prepare(&format!("SELECT {LEASE_COLUMNS} FROM resource_leases WHERE project=?1 AND state!='released'")).map_err(sql)?;
        let leases = statement
            .query_map([&flow.submission.project_id], lease_row)
            .map_err(sql)?;
        for lease in leases {
            let lease = lease.map_err(sql)?;
            if !flow
                .steps
                .iter()
                .any(|s| s.operation_id.as_deref() == Some(&lease.operation_id))
            {
                return Err(Self::blocked(&lease));
            }
        }
        let conflict: Option<String> = connection
            .query_row(
                "SELECT id FROM workflows WHERE project=?1 AND status='running' AND id!=?2 LIMIT 1",
                params![flow.submission.project_id, flow.workflow_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(id) = conflict {
            return Err(format!(
                "resource busy: проект {} зарезервирован workflow {id}",
                flow.submission.project_id
            ));
        }
        Ok(())
    }
    /// Readiness transfers the mutation reservation to retained process ownership;
    /// it does not assert cleanup. The old process cannot release a newer task's lease.
    pub(crate) fn handoff_application_resource(&mut self, id: &str) -> Result<()> {
        self.guard_callback(None, None)?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        let app = Self::read_domain::<ApplicationRecord>(&tx, id)?;
        let flow = Self::read_domain::<WorkflowRecord>(&tx, &app.workflow_id)?;
        let task = Self::read_task_snapshot(&tx, &app.task_id)?;
        if app.status != ApplicationStatus::Ready
            || app.process_id.is_none()
            || app.process_id != task.record.process_id
            || task.record.status != TaskStatus::Running
            || flow.steps[3].status != WorkflowStepStatus::Succeeded
        {
            return Err("local journal: application handoff requires committed readiness and matching live task".into());
        }
        let lease = Self::operation_lease(&tx, OperationKind::Task, &app.task_id)?
            .ok_or("local journal: application resource lease missing")?;
        if lease.authority != Self::read_authority(&tx)?
            || lease.state != LeaseState::Held
            || lease.cleanup_proof.is_some()
            || lease.project_id != app.project_id
            || task.record.submission.project_id != app.project_id
            || flow.submission.project_id != app.project_id
        {
            return Err(
                "local journal: application handoff rejected for stale or stopped resource owner"
                    .into(),
            );
        }
        Self::assert_attempt(
            &tx,
            &ExecutionToken {
                operation: super::OperationRef {
                    incarnation: lease.authority.incarnation.clone(),
                    kind: OperationKind::Task,
                    operation_id: app.task_id.clone(),
                    project_id: app.project_id.clone(),
                },
                authority: lease.authority.clone(),
                attempt_id: lease
                    .attempt_id
                    .clone()
                    .ok_or("local journal: application attempt missing")?,
            },
        )?;
        if lease.scope == LeaseScope::ApplicationLifetime {
            return Ok(());
        }
        let revision = lease
            .revision
            .checked_add(1)
            .ok_or("local journal: lease revision exhausted")?;
        let changed = tx.execute("UPDATE resource_leases SET scope='application_lifetime',reason='Readiness committed; application process remains owned',revision=?2 WHERE id=?1 AND revision=?3",params![lease.lease_id,revision,lease.revision]).map_err(sql)?;
        if changed != 1 {
            return Err("local journal: application resource handoff was not stored".into());
        }
        tx.commit().map_err(sql)
    }
    fn cleanup_on(
        connection: &Connection,
        token: &ExecutionToken,
        proof: CleanupProof,
    ) -> Result<()> {
        Self::assert_attempt(connection, token)?;
        if !matches!(
            (token.operation.kind, proof),
            (
                OperationKind::Task,
                CleanupProof::NoProcessCreated | CleanupProof::ProcessTreeEmpty
            ) | (OperationKind::Import, CleanupProof::ImportThreadStopped)
        ) {
            return Err("local journal: cleanup proof does not match executor kind".into());
        }
        let lease = Self::operation_lease(
            connection,
            token.operation.kind,
            &token.operation.operation_id,
        )?
        .ok_or("local journal: execution lease missing; cleanup rejected")?;
        if lease.authority != token.authority
            || lease.attempt_id.as_deref() != Some(&token.attempt_id)
            || lease.project_id != token.operation.project_id
            || lease.state != LeaseState::Held
        {
            return Err("local journal: stale resource owner/attempt; cleanup rejected".into());
        }
        if lease.cleanup_proof == Some(proof) {
            return Ok(());
        }
        if lease.cleanup_proof.is_some() {
            return Err("local journal: cleanup receipt cannot be replaced".into());
        }
        Self::change_lease(
            connection,
            &lease,
            LeaseState::Held,
            Some(proof),
            "Executor confirmed cleanup; awaiting outcome commit",
        )
    }
    pub fn record_execution_cleanup(
        &mut self,
        token: &ExecutionToken,
        proof: CleanupProof,
    ) -> Result<()> {
        self.guard_callback(
            Some(token.operation.kind),
            Some(&token.operation.operation_id),
        )?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        Self::cleanup_on(&tx, token, proof)?;
        tx.commit().map_err(sql)
    }
    pub(crate) fn complete_execution_lease(
        connection: &Connection,
        kind: OperationKind,
        id: &str,
    ) -> Result<()> {
        let Some(lease) = Self::operation_lease(connection, kind, id)? else {
            return Ok(());
        };
        if lease.state == LeaseState::Released {
            return Ok(());
        }
        if lease.state == LeaseState::Uncertain && lease.cleanup_proof.is_none() {
            return Ok(());
        }
        let (state, reason) = if lease.cleanup_proof.is_some() {
            (
                LeaseState::Released,
                "Terminal result and confirmed cleanup committed",
            )
        } else {
            (
                LeaseState::Uncertain,
                "Outcome recorded without physical cleanup proof; project remains reserved",
            )
        };
        Self::change_lease(connection, &lease, state, lease.cleanup_proof, reason)
    }
    pub fn finish_task_execution(
        &mut self,
        token: &ExecutionToken,
        status: TaskStatus,
        exit: Option<i32>,
        reason: &str,
        proof: CleanupProof,
    ) -> Result<winbridge_protocol::TaskRecord> {
        if token.operation.kind != OperationKind::Task {
            return Err("local journal: task cleanup requires task token".into());
        }
        self.guard_callback(
            Some(OperationKind::Task),
            Some(&token.operation.operation_id),
        )?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        Self::cleanup_on(&tx, token, proof)?;
        let outcome =
            winbridge_core::task::TaskOutcome::try_from(status).map_err(|e| e.to_string())?;
        let decision = Self::transition_task_on(
            &tx,
            &token.operation.operation_id,
            None,
            winbridge_core::task::TaskEvent::Completed {
                outcome,
                exit_code: exit,
                reason: reason.into(),
            },
        )?;
        tx.commit().map_err(sql)?;
        Ok(decision.record)
    }
    pub fn finish_import_execution(
        &mut self,
        token: &ExecutionToken,
        status: TaskStatus,
        reason: &str,
    ) -> Result<ImportRecord> {
        if !status.is_terminal() {
            return Err("local journal: import completion requires terminal outcome".into());
        }
        if token.operation.kind != OperationKind::Import {
            return Err("local journal: import cleanup requires import token".into());
        }
        self.guard_callback(
            Some(OperationKind::Import),
            Some(&token.operation.operation_id),
        )?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        Self::cleanup_on(&tx, token, CleanupProof::ImportThreadStopped)?;
        let mut observed = Self::read_domain::<ImportRecord>(&tx, &token.operation.operation_id)?;
        observed.status = status;
        observed.phase = "finished".into();
        observed.reason = Some(reason.into());
        Self::import_event(&mut observed, reason);
        Self::write_domain(&tx, &observed)?;
        Self::complete_attempt(&tx, OperationKind::Import, &observed.import_id, status)?;
        tx.commit().map_err(sql)?;
        Ok(observed.record)
    }
    pub fn note_resource_wait(
        &mut self,
        kind: OperationKind,
        id: &str,
        reason: &str,
    ) -> Result<()> {
        let reason = reason
            .strip_prefix("resource busy:")
            .unwrap_or(reason)
            .trim();
        self.guard_callback(None, None)?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        match kind {
            OperationKind::Task => {
                let observed = Self::read_task_snapshot(&tx, id)?;
                if observed.record.status == TaskStatus::Queued
                    && observed.record.reason.as_deref() != Some(reason)
                {
                    let mut record = observed.record.clone();
                    record.reason = Some(reason.into());
                    Self::event(&mut record, reason);
                    Self::write_task_snapshot(&tx, &observed, &record)?;
                }
            }
            OperationKind::Import => {
                let mut observed = Self::read_domain::<ImportRecord>(&tx, id)?;
                if observed.status == TaskStatus::Queued
                    && observed.reason.as_deref() != Some(reason)
                {
                    observed.reason = Some(reason.into());
                    Self::import_event(&mut observed, reason);
                    Self::write_domain(&tx, &observed)?;
                }
            }
        }
        tx.commit().map_err(sql)
    }
    /// Runtime holds the exclusive instance lock. Copy threads are gone; old process
    /// ownership is retained until physical reconciliation proves it safe.
    pub fn recover_resource_leases(&mut self) -> Result<()> {
        self.guard_callback(None, None)?;
        let active = self.resource_leases(true)?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        for lease in active {
            if lease.authority == Self::read_authority(&tx)? {
                continue;
            }
            let (state, reason) = if lease.cleanup_proof.is_some() {
                (
                    LeaseState::Released,
                    "Recovered durable physical cleanup receipt",
                )
            } else {
                (
                    LeaseState::Uncertain,
                    "Previous worker stopped; physical cleanup remains unconfirmed",
                )
            };
            Self::change_lease(&tx, &lease, state, lease.cleanup_proof, reason)?;
        }
        tx.commit().map_err(sql)
    }
    pub(crate) fn capture_legacy_resources(&mut self) -> Result<()> {
        use std::collections::BTreeMap;
        self.guard_callback(None, None)?;
        let captured = |kind: OperationKind, id: &str| -> Result<bool> {
            self.connection.query_row("SELECT EXISTS(SELECT 1 FROM resource_leases l,json_each(l.members) m WHERE json_extract(m.value,'$.kind')=?1 AND json_extract(m.value,'$.id')=?2)",params![kind.as_str(),id],|r|r.get(0)).map_err(sql)
        };
        let mut groups: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        for task in self
            .all()?
            .into_iter()
            .filter(|t| matches!(t.status, TaskStatus::Running | TaskStatus::Indeterminate))
        {
            if Self::operation_lease(&self.connection, OperationKind::Task, &task.task_id)?
                .is_none()
                && !captured(OperationKind::Task, &task.task_id)?
            {
                groups.entry(task.submission.project_id).or_default().push(serde_json::json!({"kind":"task","id":task.task_id,"processId":task.process_id}));
            }
        }
        for import in self
            .imports(false)?
            .into_iter()
            .filter(|i| i.status == TaskStatus::Running)
        {
            if Self::operation_lease(&self.connection, OperationKind::Import, &import.import_id)?
                .is_none()
                && !captured(OperationKind::Import, &import.import_id)?
            {
                groups
                    .entry(import.submission.project_id)
                    .or_default()
                    .push(serde_json::json!({"kind":"import","id":import.import_id}));
            }
        }
        let authority = self.authority.clone();
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        for (project, members) in groups {
            if Self::active_lease(&tx, &project)?.is_some() {
                return Err(format!("local journal: legacy execution overlaps a durable lease in project {project}; physical reconciliation required"));
            }
            let kind = if members.iter().all(|m| m["kind"] == "import") {
                "import"
            } else {
                "task"
            };
            let changed=tx.execute("INSERT INTO resource_leases(id,project,kind,operation,incarnation,epoch,authority,acquired,state,members,reason) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'uncertain',?9,'Legacy execution has no physical cleanup receipt')",params![uuid::Uuid::new_v4().to_string(),project,kind,format!("legacy:{}",uuid::Uuid::new_v4()),authority.incarnation,authority.epoch as i64,authority.authority_id,now_ms() as i64,serde_json::to_string(&members).map_err(|e|e.to_string())?]).map_err(sql)?;
            if changed != 1 {
                return Err("local journal: legacy resource uncertainty not stored".into());
            }
        }
        tx.commit().map_err(sql)
    }
    pub(crate) fn release_stopped_import_resources(&mut self) -> Result<()> {
        self.guard_callback(None, None)?;
        let active = self.resource_leases(true)?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        for lease in active
            .into_iter()
            .filter(|l| l.kind == OperationKind::Import)
        {
            let ids: Vec<String> = if lease.legacy_members == "[]" {
                vec![lease.operation_id.clone()]
            } else {
                let members: Vec<serde_json::Value> =
                    serde_json::from_str(&lease.legacy_members).map_err(|e| e.to_string())?;
                members
                    .into_iter()
                    .map(|m| m["id"].as_str().unwrap_or("").to_owned())
                    .collect()
            };
            let stopped = ids.iter().all(|id| {
                Self::read_domain::<ImportRecord>(&tx, id).is_ok_and(|i| i.status.is_terminal())
            });
            if stopped {
                Self::change_lease(&tx,&lease,LeaseState::Released,Some(CleanupProof::ImportThreadStopped),"Exclusive instance lock confirms previous copy threads stopped; imports reconciled")?;
            }
        }
        tx.commit().map_err(sql)
    }
}
