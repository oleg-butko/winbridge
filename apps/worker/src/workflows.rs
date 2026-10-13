use crate::{
    store::{now_ms, sql, JournalSnapshot, Store},
    Result, Worker,
};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;
use std::sync::{atomic::Ordering, Arc};
use winbridge_protocol::*;

impl Store {
    pub fn existing_workflow(
        &self,
        request: &WorkflowSubmission,
    ) -> Result<Option<WorkflowRecord>> {
        Self::existing_workflow_on(&self.connection, request)
    }
    fn existing_workflow_on(
        connection: &rusqlite::Connection,
        request: &WorkflowSubmission,
    ) -> Result<Option<WorkflowRecord>> {
        let row: Option<(String, String)> = connection
            .query_row(
                "SELECT payload,record FROM workflows WHERE project=?1 AND key=?2",
                params![request.project_id, request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        let mut payload = request.clone();
        payload.request_id.clear();
        match row {
            None => Ok(None),
            Some((stored, record)) => {
                if stored != serde_json::to_string(&payload).map_err(|e| e.to_string())? {
                    return Err("Workflow idempotencyKey belongs to a different payload".into());
                }
                Ok(Some(
                    serde_json::from_str(&record).map_err(|e| e.to_string())?,
                ))
            }
        }
    }
    pub fn accept_workflow(&mut self, request: WorkflowSubmission) -> Result<WorkflowRecord> {
        self.accept_workflow_with_pins(request, None)
    }
    pub fn accept_workflow_with_pins(
        &mut self,
        request: WorkflowSubmission,
        recipe_pins: Option<Vec<WorkflowRecipePin>>,
    ) -> Result<WorkflowRecord> {
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(existing) = Self::existing_workflow_on(&transaction, &request)? {
            return Ok(existing);
        }
        let mut payload = request.clone();
        payload.request_id.clear();
        let mut workflow = WorkflowRecord {
            workflow_id: uuid::Uuid::new_v4().to_string(),
            submission: request,
            recipe_pins,
            status: TaskStatus::Queued,
            accepted_at_ms: now_ms(),
            finished_at_ms: None,
            cancel_requested: false,
            reason: None,
            application_id: None,
            steps: ["import", "build", "start", "readiness", "log"]
                .iter()
                .map(|name| WorkflowStep {
                    step_id: uuid::Uuid::new_v4().to_string(),
                    name: (*name).into(),
                    status: WorkflowStepStatus::Pending,
                    operation_id: None,
                    started_at_ms: None,
                    finished_at_ms: None,
                    reason: None,
                })
                .collect(),
            events: vec![],
            artifacts: vec![],
        };
        Self::workflow_event(
            &mut workflow,
            "Workflow accepted; parameters and step IDs committed before ACK",
        );
        let changed = transaction.execute("INSERT INTO workflows(id,project,key,payload,status,record) VALUES (?1,?2,?3,?4,'queued',?5)",params![workflow.workflow_id,workflow.submission.project_id,workflow.submission.idempotency_key,serde_json::to_string(&payload).map_err(|e|e.to_string())?,serde_json::to_string(&workflow).map_err(|e|e.to_string())?]).map_err(sql)?;
        if changed != 1 {
            return Err(
                "local journal: workflow acceptance not stored; acknowledgement rejected".into(),
            );
        }
        transaction.commit().map_err(sql)?;
        Ok(workflow)
    }
    pub fn workflow(&self, id: &str) -> Result<WorkflowRecord> {
        let record: String = self
            .connection
            .query_row("SELECT record FROM workflows WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        serde_json::from_str(&record).map_err(|e| e.to_string())
    }
    pub fn workflows(&self, recent: bool) -> Result<Vec<WorkflowRecord>> {
        let query = if recent {
            "SELECT record FROM workflows ORDER BY rowid DESC LIMIT 10"
        } else {
            "SELECT record FROM workflows WHERE status IN ('queued','running') ORDER BY rowid"
        };
        let mut statement = self.connection.prepare(query).map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| serde_json::from_str(&r.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }
    pub fn save_workflow(&mut self, record: &mut JournalSnapshot<WorkflowRecord>) -> Result<()> {
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
        Self::write_domain(&transaction, record)?;
        crate::attention::workflow_outcome(&transaction, record)?;
        transaction.commit().map_err(sql)?;
        record.committed()
    }
    fn workflow_event(record: &mut WorkflowRecord, summary: impl Into<String>) {
        record.events.push(JournalEvent {
            sequence: record.events.len() as u64 + 1,
            occurred_at_ms: now_ms(),
            status: record.status,
            summary: summary.into(),
        });
    }
    fn finish_workflow(
        &mut self,
        record: &mut JournalSnapshot<WorkflowRecord>,
        status: TaskStatus,
        reason: &str,
    ) -> Result<()> {
        record.status = status;
        record.reason = Some(reason.into());
        record.finished_at_ms = Some(now_ms());
        for step in &mut record.steps {
            if matches!(
                step.status,
                WorkflowStepStatus::Pending | WorkflowStepStatus::Running
            ) {
                step.status = if step.status == WorkflowStepStatus::Pending {
                    WorkflowStepStatus::Skipped
                } else {
                    match status {
                        TaskStatus::Cancelled => WorkflowStepStatus::Cancelled,
                        TaskStatus::Indeterminate => WorkflowStepStatus::Indeterminate,
                        _ => WorkflowStepStatus::Failed,
                    }
                };
                step.reason = Some(reason.into());
                step.finished_at_ms = Some(now_ms());
            }
        }
        Self::workflow_event(record, reason);
        self.save_workflow(record)
    }
    pub fn cancel_workflow(&mut self, id: &str) -> Result<WorkflowRecord> {
        self.guard_callback(None, None)?;
        let mut record = self.workflow_snapshot(id)?;
        if record.status.is_terminal() || record.cancel_requested {
            return Ok(record.record);
        }
        record.cancel_requested = true;
        Self::workflow_event(
            &mut record,
            "Cancellation requested; waiting for owned child operations to terminate",
        );
        self.save_workflow(&mut record)?;
        Ok(record.record)
    }
    pub fn application(&self, id: &str) -> Result<ApplicationRecord> {
        let record: String = self
            .connection
            .query_row("SELECT record FROM applications WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        serde_json::from_str(&record).map_err(|e| e.to_string())
    }
    pub fn application_for_task(&self, id: &str) -> Result<Option<ApplicationRecord>> {
        let record: Option<String> = self
            .connection
            .query_row("SELECT record FROM applications WHERE task=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)?;
        record
            .map(|r| serde_json::from_str(&r).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn applications(&self, recent: bool) -> Result<Vec<ApplicationRecord>> {
        let query = if recent {
            "SELECT record FROM applications ORDER BY rowid DESC LIMIT 10"
        } else {
            "SELECT record FROM applications WHERE status IN ('starting','ready','stopping') ORDER BY rowid"
        };
        let mut statement = self.connection.prepare(query).map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| serde_json::from_str(&r.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }
    pub fn save_application(&mut self, app: &mut JournalSnapshot<ApplicationRecord>) -> Result<()> {
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
        Self::write_domain(&transaction, app)?;
        crate::attention::application_outcome(&transaction, app)?;
        transaction.commit().map_err(sql)?;
        app.committed()
    }
    fn insert_application(
        connection: &rusqlite::Connection,
        app: &ApplicationRecord,
    ) -> Result<()> {
        let existing: Option<String> = connection
            .query_row(
                "SELECT record FROM applications WHERE id=?1",
                [&app.application_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(raw) = existing {
            let existing: ApplicationRecord =
                serde_json::from_str(&raw).map_err(|e| format!("local journal: {e}"))?;
            if existing.application_id != app.application_id
                || existing.workflow_id != app.workflow_id
                || existing.task_id != app.task_id
                || existing.project_id != app.project_id
                || existing.log_path != app.log_path
            {
                return Err("local journal: existing application has different ownership".into());
            }
            // Reconcile a legacy crash after registry commit but before parent link.
            // Preserve its actual outcome, PID and timestamps; never reset to Starting.
            return Ok(());
        }
        let changed = connection
            .execute(
                "INSERT INTO applications(id,workflow,task,status,record) VALUES (?1,?2,?3,?4,?5)",
                params![
                    app.application_id,
                    app.workflow_id,
                    app.task_id,
                    serde_json::to_value(app.status)
                        .map_err(|e| e.to_string())?
                        .as_str(),
                    serde_json::to_string(app).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        if changed != 1 {
            return Err("local journal: application acceptance not stored".into());
        }
        Ok(())
    }
    pub fn accept_workflow_import(
        &mut self,
        flow: &mut JournalSnapshot<WorkflowRecord>,
        request: ImportSubmission,
        shared: String,
        workspace: String,
    ) -> Result<()> {
        self.guard_callback(None, None)?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if flow.status.is_terminal()
            || flow.cancel_requested
            || flow.steps[0].operation_id.is_some()
            || request.project_id != flow.submission.project_id
            || request.snapshot_id != flow.submission.snapshot_id
            || request.manifest_sha256 != flow.submission.manifest_sha256
        {
            return Err("local journal: import child admission contradicts workflow".into());
        }
        let mut updated = flow.clone();
        let import = Self::accept_import_on(&transaction, request, shared, workspace)?;
        updated.steps[0].operation_id = Some(import.import_id);
        update_step(
            &mut updated,
            0,
            WorkflowStepStatus::Running,
            "Import accepted; verified generation required before build",
        );
        Self::write_domain(&transaction, &updated)?;
        transaction.commit().map_err(sql)?;
        updated.committed()?;
        *flow = updated;
        Ok(())
    }
    pub fn accept_workflow_task(
        &mut self,
        flow: &mut JournalSnapshot<WorkflowRecord>,
        index: usize,
        request: TaskSubmission,
        command: ResolvedCommand,
        snapshot: Option<VerifiedSnapshotRef>,
        origin: Option<RecipeOrigin>,
    ) -> Result<()> {
        self.guard_callback(None, None)?;
        if !matches!(index, 1 | 2) {
            return Err("local journal: invalid child step".into());
        }
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if flow.status.is_terminal()
            || flow.cancel_requested
            || flow.steps[index].operation_id.is_some()
            || request.project_id != flow.submission.project_id
            || request.snapshot_id.as_ref() != Some(&flow.submission.snapshot_id)
        {
            return Err("local journal: task child admission contradicts workflow".into());
        }
        let mut updated = flow.clone();
        let task = Self::accept_task_on(&transaction, request, command, snapshot, origin)?;
        if task
            .snapshot
            .as_ref()
            .is_none_or(|s| s.manifest_sha256 != updated.submission.manifest_sha256)
        {
            return Err(
                "Imported manifest differs from accepted workflow; child not admitted".into(),
            );
        }
        updated.steps[index].operation_id = Some(task.task_id.clone());
        update_step(
            &mut updated,
            index,
            WorkflowStepStatus::Running,
            if index == 1 {
                "Build task accepted; start waits for exit 0"
            } else {
                "Start task accepted; PID does not confirm readiness"
            },
        );
        if index == 2 {
            let app = ApplicationRecord {
                application_id: updated.steps[2].step_id.clone(),
                workflow_id: updated.workflow_id.clone(),
                project_id: updated.submission.project_id.clone(),
                task_id: task.task_id,
                status: ApplicationStatus::Starting,
                process_id: None,
                created_at_ms: now_ms(),
                ready_at_ms: None,
                finished_at_ms: None,
                log_path: updated.submission.log_path.clone(),
                reason: Some("Awaiting process and explicit stdout readiness probe".into()),
                failure_reason: None,
            };
            Self::insert_application(&transaction, &app)?;
            updated.application_id = Some(app.application_id);
        }
        Self::write_domain(&transaction, &updated)?;
        transaction.commit().map_err(sql)?;
        updated.committed()?;
        *flow = updated;
        Ok(())
    }
    pub fn save_workflow_application(
        &mut self,
        flow: &mut JournalSnapshot<WorkflowRecord>,
        app: &mut JournalSnapshot<ApplicationRecord>,
        cancel_task: bool,
    ) -> Result<()> {
        self.guard_callback(None, None)?;
        if app.workflow_id != flow.workflow_id
            || flow.application_id.as_ref() != Some(&app.application_id)
            || flow.steps[2].operation_id.as_ref() != Some(&app.task_id)
        {
            return Err("local journal: workflow/application association mismatch".into());
        }
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if cancel_task {
            Self::transition_task_on(
                &transaction,
                &app.task_id,
                None,
                winbridge_core::task::TaskEvent::CancellationRequested,
            )?;
        }
        Self::write_domain(&transaction, app)?;
        Self::write_domain(&transaction, flow)?;
        crate::attention::application_outcome(&transaction, app)?;
        crate::attention::workflow_outcome(&transaction, flow)?;
        transaction.commit().map_err(sql)?;
        app.committed()?;
        flow.committed()
    }
    pub fn stop_application(&mut self, id: &str) -> Result<ApplicationRecord> {
        self.guard_callback(None, None)?;
        let mut app = self.application_snapshot(id)?;
        if !app.status.is_terminal() {
            app.status = ApplicationStatus::Stopping;
            app.reason = Some("Stop requested; awaiting managed process tree termination".into());
            let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
            Self::transition_task_on(
                &transaction,
                &app.task_id,
                None,
                winbridge_core::task::TaskEvent::CancellationRequested,
            )?;
            Self::write_domain(&transaction, &app)?;
            crate::attention::application_outcome(&transaction, &app)?;
            transaction.commit().map_err(sql)?;
            app.committed()?;
        }
        Ok(app.record)
    }
}

fn update_step(
    flow: &mut WorkflowRecord,
    index: usize,
    status: WorkflowStepStatus,
    reason: impl Into<String>,
) {
    let step = &mut flow.steps[index];
    step.status = status;
    step.reason = Some(reason.into());
    if step.started_at_ms.is_none() {
        step.started_at_ms = Some(now_ms());
    }
    if !matches!(
        status,
        WorkflowStepStatus::Pending | WorkflowStepStatus::Running
    ) {
        step.finished_at_ms = Some(now_ms());
    }
}

pub(crate) fn reserved_for_other(flows: &[WorkflowRecord], project: &str, operation: &str) -> bool {
    flows.iter().any(|f| {
        f.status == TaskStatus::Running
            && f.submission.project_id == project
            && !f
                .steps
                .iter()
                .any(|s| s.operation_id.as_deref() == Some(operation))
    })
}

impl Worker {
    pub(crate) fn application_log(
        &self,
        store: &Store,
        id: &str,
        cursor: Option<&LogCursor>,
        max: u32,
    ) -> Result<LogPage> {
        let app = store.application(id)?;
        let project = self
            .config
            .projects
            .iter()
            .find(|p| p.id == app.project_id)
            .ok_or("Application project is not registered")?;
        winbridge_log_reader::read(&project.workspace_root, &app.log_path, id, cursor, max)
    }
    pub(crate) fn advance_workflows(
        self: &Arc<Self>,
        store: &mut Store,
        active: &mut HashMap<String, String>,
    ) -> Result<()> {
        // Process state is reconciled independently of completed startup workflows.
        for app in store.applications(false)? {
            let mut app = store.application_snapshot(&app.application_id)?;
            let task = store.get(&app.task_id)?;
            let changed = app.process_id != task.process_id || task.status.is_terminal();
            app.process_id = task.process_id;
            if task.status.is_terminal() {
                app.status = match task.status {
                    TaskStatus::Cancelled if app.failure_reason.is_some() => {
                        ApplicationStatus::Failed
                    }
                    TaskStatus::Cancelled => ApplicationStatus::Stopped,
                    TaskStatus::Succeeded => ApplicationStatus::Exited,
                    TaskStatus::Indeterminate | TaskStatus::Interrupted => {
                        ApplicationStatus::Indeterminate
                    }
                    _ => ApplicationStatus::Failed,
                };
                app.finished_at_ms = task.finished_at_ms;
                app.reason = app.failure_reason.clone().or(task.reason.clone());
            }
            if changed {
                if let Err(error) = store.save_application(&mut app) {
                    self.record_journal_fault(
                        &app.application_id,
                        &error,
                        "application outcome remains uncommitted",
                    );
                    return Err(error);
                }
            }
        }
        let flows = store.workflows(false)?;
        let mut reserved: HashMap<String, String> = flows
            .iter()
            .filter(|f| f.status == TaskStatus::Running)
            .map(|f| (f.submission.project_id.clone(), f.workflow_id.clone()))
            .collect();
        for flow in flows {
            let mut flow = store.workflow_snapshot(&flow.workflow_id)?;
            let project = flow.submission.project_id.clone();
            if flow.status == TaskStatus::Queued {
                if flow.cancel_requested
                    || now_ms().saturating_sub(flow.accepted_at_ms) >= flow.submission.timeout_ms
                {
                    let cancelled = flow.cancel_requested;
                    if let Err(error) = store.finish_workflow(
                        &mut flow,
                        if cancelled {
                            TaskStatus::Cancelled
                        } else {
                            TaskStatus::Failed
                        },
                        if cancelled {
                            "Queued workflow cancelled before any child operation"
                        } else {
                            "Workflow expired while waiting; no child operation started"
                        },
                    ) {
                        self.record_journal_fault(
                            &flow.workflow_id,
                            &error,
                            "queued workflow outcome remains uncommitted",
                        );
                        return Err(error);
                    }
                    continue;
                }
                if let Err(reason) =
                    winbridge_core::workflow::validate_recipe_pins(&self.config, &flow)
                {
                    if let Err(error) =
                        store.finish_workflow(&mut flow, TaskStatus::Failed, &reason)
                    {
                        self.record_journal_fault(&flow.workflow_id, &reason, &error);
                        return Err(error);
                    }
                    continue;
                }
                if store
                    .applications(false)?
                    .iter()
                    .any(|a| a.project_id == project)
                {
                    continue;
                }
                if reserved.contains_key(&project) || active.contains_key(&project) {
                    continue;
                }
                let earlier_task = store.all()?.iter().any(|t| {
                    t.status == TaskStatus::Queued
                        && t.submission.project_id == project
                        && t.accepted_at_ms < flow.accepted_at_ms
                });
                let earlier_import = store.imports(false)?.iter().any(|t| {
                    t.status == TaskStatus::Queued
                        && t.submission.project_id == project
                        && t.accepted_at_ms < flow.accepted_at_ms
                });
                if earlier_task || earlier_import {
                    continue;
                }
                flow.status = TaskStatus::Running;
                Store::workflow_event(
                    &mut flow,
                    "Project reserved for import/build/start/readiness; unrelated mutations wait",
                );
                if let Err(error) = store.save_workflow(&mut flow) {
                    self.record_journal_fault(
                        &flow.workflow_id,
                        &error,
                        "workflow launch intent remains uncommitted",
                    );
                    return Err(error);
                }
                reserved.insert(project.clone(), flow.workflow_id.clone());
            }
            if let Err(reason) = self.advance_workflow(store, active, &mut flow) {
                if reason.starts_with("local journal:") {
                    self.record_journal_fault(
                        &flow.workflow_id,
                        &reason,
                        "workflow outcome remains uncommitted",
                    );
                    if let Some(id) = &flow.application_id {
                        self.record_journal_fault(
                            id,
                            &reason,
                            "application startup state may remain uncommitted",
                        );
                    }
                    return Err(reason);
                }
                for (index, step) in flow.steps.iter().enumerate().take(3) {
                    if let Some(id) = &step.operation_id {
                        if index == 0 {
                            store.cancel_import(id)?;
                        } else {
                            store.cancel(id)?;
                        }
                    }
                }
                if let Err(error) = store.finish_workflow(
                    &mut flow,
                    TaskStatus::Indeterminate,
                    &format!(
                        "Workflow reconciliation failed; automatic restart disabled: {reason}"
                    ),
                ) {
                    self.record_journal_fault(&flow.workflow_id, &reason, &error);
                    return Err(error);
                }
            }
        }
        let pending = store.workflows(false)?;
        self.readiness_scans
            .lock()
            .map_err(|_| "readiness scan lock poisoned")?
            .retain(|id, _| pending.iter().any(|f| &f.workflow_id == id));
        self.checks
            .lock()
            .map_err(|_| "background checks lock poisoned")?
            .retain(|id, _| pending.iter().any(|f| id.starts_with(&f.workflow_id)));
        Ok(())
    }
    fn advance_workflow(
        self: &Arc<Self>,
        store: &mut Store,
        active: &mut HashMap<String, String>,
        flow: &mut JournalSnapshot<WorkflowRecord>,
    ) -> Result<()> {
        let timeout = now_ms().saturating_sub(flow.accepted_at_ms) >= flow.submission.timeout_ms;
        if flow.cancel_requested || timeout {
            if timeout && !flow.cancel_requested {
                if let Some(id) = &flow.application_id {
                    let mut app = store.application_snapshot(id)?;
                    if !app.status.is_terminal() && app.failure_reason.is_none() {
                        app.failure_reason =
                            Some("Workflow startup timeout; owned application stopped".into());
                        app.reason = app.failure_reason.clone();
                        store.save_application(&mut app)?;
                    }
                }
            }
            let mut waiting = false;
            for (index, step) in flow.steps.iter().enumerate().take(3) {
                if let Some(id) = &step.operation_id {
                    if index == 0 {
                        waiting |= !store.cancel_import(id)?.status.is_terminal();
                    } else {
                        waiting |= !store.cancel(id)?.status.is_terminal();
                    }
                }
            }
            if waiting {
                return Ok(());
            }
            return store.finish_workflow(
                flow,
                if flow.cancel_requested {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Failed
                },
                if flow.cancel_requested {
                    "Workflow cancelled; owned operations stopped"
                } else {
                    "Workflow timeout; owned operations stopped"
                },
            );
        }
        let Some(index) = flow
            .steps
            .iter()
            .position(|s| s.status != WorkflowStepStatus::Succeeded)
        else {
            flow.status = TaskStatus::Succeeded;
            flow.finished_at_ms = Some(now_ms());
            flow.reason=Some("Import/build/start/readiness/log confirmed; application lifecycle remains separately tracked".into());
            Store::workflow_event(
                flow,
                "Startup workflow completed; owned application continues independently",
            );
            self.readiness_scans
                .lock()
                .map_err(|_| "readiness scan lock poisoned")?
                .remove(&flow.workflow_id);
            return store.save_workflow(flow);
        };
        let key = format!(
            "workflow:{}:{}",
            flow.workflow_id, flow.steps[index].step_id
        );
        if index == 0 {
            if flow.steps[0].operation_id.is_none() {
                let request = ImportSubmission {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: flow.steps[0].step_id.clone(),
                    project_id: flow.submission.project_id.clone(),
                    snapshot_id: flow.submission.snapshot_id.clone(),
                    manifest_sha256: flow.submission.manifest_sha256.clone(),
                    idempotency_key: key,
                    timeout_ms: flow.submission.timeout_ms,
                };
                let (shared, local) =
                    self.import_paths(&request.project_id, &request.snapshot_id)?;
                if let Err(reason) =
                    winbridge_core::workflow::validate_recipe_pins(&self.config, flow)
                {
                    return store.finish_workflow(flow, TaskStatus::Failed, &reason);
                }
                store.accept_workflow_import(
                    flow,
                    request,
                    shared.to_string_lossy().into(),
                    local.to_string_lossy().into(),
                )?;
            }
            let import = store.get_import(flow.steps[0].operation_id.as_ref().unwrap())?;
            return self.child_result(store, flow, 0, import.status, import.reason.as_deref());
        }
        if index == 1 || index == 2 {
            if index == 2 && !flow.submission.artifacts.is_empty() && flow.artifacts.is_empty() {
                let project = self
                    .config
                    .projects
                    .iter()
                    .find(|p| p.id == flow.submission.project_id)
                    .ok_or("Project missing")?;
                let root = project.workspace_root.clone();
                let specs = flow.submission.artifacts.clone();
                let task = store.get(
                    flow.steps[1]
                        .operation_id
                        .as_ref()
                        .ok_or("Build task missing")?,
                )?;
                let snapshot = task.snapshot.ok_or("Build snapshot missing")?;
                match self.background_check(
                    format!("{}:artifacts", flow.workflow_id),
                    move || {
                        crate::artifacts::capture(&root, &specs, &snapshot, &task.task_id)
                            .map(CheckResult::Artifacts)
                    },
                )? {
                    None => return Ok(()),
                    Some(Ok(CheckResult::Artifacts(artifacts))) => {
                        flow.artifacts = artifacts;
                        Store::workflow_event(flow, "Declared build artifacts hashed and linked to verified source before start");
                        store.save_workflow(flow)?;
                    }
                    Some(Err(reason)) => {
                        return store.finish_workflow(
                            flow,
                            TaskStatus::Failed,
                            &format!("Build artifact verification failed; start skipped: {reason}"),
                        )
                    }
                    _ => return Err("Invalid artifact check result".into()),
                }
            }
            if flow.steps[index].operation_id.is_none() {
                if index == 2 && store.applications(false)?.len() >= 32 {
                    return store.finish_workflow(flow,TaskStatus::Failed,"Application registry limit reached (32 live applications); start not submitted");
                }
                let request = TaskSubmission {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: flow.steps[index].step_id.clone(),
                    project_id: flow.submission.project_id.clone(),
                    idempotency_key: key,
                    timeout_ms: flow.submission.timeout_ms,
                    snapshot_id: Some(flow.submission.snapshot_id.clone()),
                    command: if index == 1 {
                        flow.submission.build.clone()
                    } else {
                        flow.submission.start.clone()
                    },
                };
                let (command, snapshot, origin) = if let Some(task) = store.existing(&request)? {
                    (task.resolved_command, task.snapshot, task.recipe_origin)
                } else {
                    if let Err(reason) =
                        winbridge_core::workflow::validate_recipe_pins(&self.config, flow)
                    {
                        return store.finish_workflow(flow, TaskStatus::Failed, &reason);
                    }
                    match self.resolve_task(&request).and_then(|command| {
                        self.snapshot_reference(&request)
                            .map(|snapshot| (command, snapshot))
                    }) {
                        Ok((command, snapshot)) => {
                            let origin = flow
                                .recipe_pins
                                .as_ref()
                                .and_then(|pins| {
                                    pins.iter()
                                        .find(|pin| pin.step_name == flow.steps[index].name)
                                })
                                .map(|pin| pin.origin.clone());
                            let origin = match origin {
                                Some(origin) => Some(origin),
                                None => winbridge_runner::recipe_origin(&self.config, &request)?,
                            };
                            (command, snapshot, origin)
                        }
                        Err(reason) => {
                            update_step(flow, index, WorkflowStepStatus::Failed, &reason);
                            return store.finish_workflow(flow, TaskStatus::Failed, &reason);
                        }
                    }
                };
                if snapshot
                    .as_ref()
                    .is_none_or(|s| s.manifest_sha256 != flow.submission.manifest_sha256)
                {
                    if let Some(existing) = store.existing(&request)? {
                        store.cancel(&existing.task_id)?;
                    }
                    update_step(
                        flow,
                        index,
                        WorkflowStepStatus::Failed,
                        "Imported manifest differs from accepted workflow",
                    );
                    return store.finish_workflow(
                        flow,
                        TaskStatus::Failed,
                        "Imported manifest differs from accepted workflow",
                    );
                }
                store.accept_workflow_task(flow, index, request, command, snapshot, origin)?;
            }
            let task = store.get(flow.steps[index].operation_id.as_ref().unwrap())?;
            if index == 2 && task.status == TaskStatus::Running && task.process_id.is_some() {
                update_step(
                    flow,
                    2,
                    WorkflowStepStatus::Succeeded,
                    "Owned process started; readiness remains pending",
                );
                store.save_workflow(flow)?;
                return Ok(());
            }
            // A startup process which exits before a readiness confirmation never counts as ready.
            if index == 2 && task.status == TaskStatus::Succeeded {
                return store.finish_workflow(
                    flow,
                    TaskStatus::Failed,
                    "Application exited before readiness confirmation",
                );
            }
            return self.child_result(store, flow, index, task.status, task.reason.as_deref());
        }
        let id = flow
            .application_id
            .as_ref()
            .ok_or("Workflow application record missing")?
            .clone();
        let mut app = store.application_snapshot(&id)?;
        let task = store.get(&app.task_id)?;
        if task.status.is_terminal() {
            let status = if task.status == TaskStatus::Indeterminate
                || task.status == TaskStatus::Interrupted
            {
                TaskStatus::Indeterminate
            } else if task.status == TaskStatus::Cancelled && app.failure_reason.is_none() {
                TaskStatus::Cancelled
            } else {
                TaskStatus::Failed
            };
            self.readiness_scans
                .lock()
                .map_err(|_| "readiness scan lock poisoned")?
                .remove(&flow.workflow_id);
            return store.finish_workflow(flow,status,app.reason.as_deref().unwrap_or("Application stopped before startup/readiness/log completed; automatic restart disabled"));
        }
        if task.cancel_requested {
            // A stop ACK is an intent, not readiness or confirmed termination.
            return Ok(());
        }
        let startup_elapsed =
            now_ms().saturating_sub(task.started_at_ms.unwrap_or(app.created_at_ms));
        if startup_elapsed >= flow.submission.readiness.timeout_ms {
            if app.failure_reason.is_none() {
                let detail = flow.steps[index]
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Configured stdout token not observed".into());
                app.status = ApplicationStatus::Stopping;
                app.failure_reason = Some(format!(
                    "Readiness/log deadline exceeded; owned application stopped. {detail}"
                ));
                app.reason = app.failure_reason.clone();
                // Mark the failed step only after process termination is confirmed on a later tick.
                if let Some(step) = flow.steps.get_mut(index) {
                    step.reason = app.reason.clone();
                }
                store.save_workflow_application(flow, &mut app, true)?;
            }
            return Ok(());
        }
        if flow.steps[index].status == WorkflowStepStatus::Pending {
            update_step(
                flow,
                index,
                WorkflowStepStatus::Running,
                if index == 3 {
                    "Checking actual persisted stdout for configured readiness token"
                } else {
                    "Checking registered log readability"
                },
            );
            store.save_workflow(flow)?;
        }
        if index == 3 {
            if let Some(probe) = &flow.submission.readiness.netwatch {
                let root = self
                    .config
                    .projects
                    .iter()
                    .find(|p| p.id == app.project_id)
                    .ok_or("Project missing")?
                    .workspace_root
                    .clone();
                let probe = probe.clone();
                let pid = task.process_id.ok_or("Readiness requires an owned PID")?;
                let key = format!("{}:health", flow.workflow_id);
                if let Some(result) = self.background_check(key.clone(), move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| e.to_string())?;
                    runtime
                        .block_on(winbridge_transport::netwatch_health(&root, &probe, pid))
                        .map(CheckResult::Ready)
                })? {
                    self.checks
                        .lock()
                        .map_err(|_| "background checks lock poisoned")?
                        .remove(&key);
                    match result {
                        Ok(CheckResult::Ready(reason)) => {
                            app.status = ApplicationStatus::Ready;
                            app.ready_at_ms = Some(now_ms());
                            app.reason = Some(reason.clone());
                            update_step(flow, 3, WorkflowStepStatus::Succeeded, reason);
                            store.save_workflow_application(flow, &mut app, false)?;
                        }
                        Err(reason) => {
                            flow.steps[3].reason = Some(reason);
                        }
                        _ => return Err("Invalid health check result".into()),
                    }
                    if flow.steps[3].status != WorkflowStepStatus::Succeeded {
                        store.save_workflow(flow)?;
                    }
                }
            } else {
                // Ready output is bounded by the task's output quota. No PID or stale log confirms readiness.
                let needle = flow.submission.readiness.stdout_contains.as_bytes();
                let mut scans = self
                    .readiness_scans
                    .lock()
                    .map_err(|_| "readiness scan lock poisoned")?;
                let (after, bytes) = scans.entry(flow.workflow_id.clone()).or_default();
                for _ in 0..4 {
                    // Bound work per scheduler tick; resume without re-reading old output.
                    let page = store.output(&app.task_id, *after)?;
                    if page.is_empty() {
                        break;
                    }
                    *after = page.last().unwrap().sequence;
                    bytes.extend(
                        page.into_iter()
                            .filter(|c| matches!(c.stream, OutputStream::Stdout))
                            .flat_map(|c| c.bytes),
                    );
                    if bytes.windows(needle.len()).any(|w| w == needle) {
                        app.status = ApplicationStatus::Ready;
                        app.ready_at_ms = Some(now_ms());
                        app.reason = Some(
                            "Configured stdout readiness token confirmed from this process".into(),
                        );
                        update_step(
                            flow,
                            3,
                            WorkflowStepStatus::Succeeded,
                            "Readiness confirmed from this application's stdout",
                        );
                        store.save_workflow_application(flow, &mut app, false)?;
                        break;
                    }
                    let keep = needle.len().saturating_sub(1);
                    if bytes.len() > keep {
                        bytes.drain(..bytes.len() - keep);
                    }
                }
            }
        } else {
            match self.application_log(store, &id, None, 1) {
                Ok(_) => {
                    update_step(
                        flow,
                        4,
                        WorkflowStepStatus::Succeeded,
                        "Registered UTF-8 log readable; byte cursor available",
                    );
                    store.save_workflow(flow)?;
                }
                Err(reason) => {
                    if flow.steps[4].reason.as_ref() != Some(&reason) {
                        flow.steps[4].reason = Some(reason);
                        store.save_workflow(flow)?;
                    }
                }
            }
        }
        if flow.steps[3].status == WorkflowStepStatus::Succeeded
            && active.get(&app.project_id) == Some(&app.task_id)
        {
            active.remove(&app.project_id); // The process remains owned; only the startup mutation lease ends.
        }
        Ok(())
    }
    fn background_check<F>(
        self: &Arc<Self>,
        key: String,
        check: F,
    ) -> Result<Option<Result<CheckResult>>>
    where
        F: FnOnce() -> Result<CheckResult> + Send + 'static,
    {
        let mut checks = self
            .checks
            .lock()
            .map_err(|_| "background checks lock poisoned")?;
        if let Some(result) = checks.get(&key) {
            return Ok(result.clone());
        }
        checks.insert(key.clone(), None);
        self.active_jobs.fetch_add(1, Ordering::AcqRel);
        let worker = Arc::clone(self);
        std::thread::spawn(move || {
            let result = check();
            if let Ok(mut checks) = worker.checks.lock() {
                checks.insert(key, Some(result));
            }
            worker.active_jobs.fetch_sub(1, Ordering::AcqRel);
        });
        Ok(None)
    }
    fn child_result(
        &self,
        store: &mut Store,
        flow: &mut JournalSnapshot<WorkflowRecord>,
        index: usize,
        status: TaskStatus,
        reason: Option<&str>,
    ) -> Result<()> {
        if status == TaskStatus::Succeeded {
            update_step(
                flow,
                index,
                WorkflowStepStatus::Succeeded,
                reason.unwrap_or("Child operation completed"),
            );
            store.save_workflow(flow)?;
        } else if status.is_terminal() {
            update_step(
                flow,
                index,
                if status == TaskStatus::Indeterminate {
                    WorkflowStepStatus::Indeterminate
                } else {
                    WorkflowStepStatus::Failed
                },
                reason.unwrap_or("Child operation failed"),
            );
            store.finish_workflow(
                flow,
                status,
                reason.unwrap_or("Child operation failed; following steps skipped"),
            )?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) enum CheckResult {
    Artifacts(Vec<BuildArtifact>),
    Ready(String),
}
