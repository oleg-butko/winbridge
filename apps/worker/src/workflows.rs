use crate::{
    store::{now_ms, sql, Store},
    Result, Worker,
};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;
use winbridge_protocol::*;

impl Store {
    pub fn existing_workflow(
        &self,
        request: &WorkflowSubmission,
    ) -> Result<Option<WorkflowRecord>> {
        let row: Option<(String, String)> = self
            .connection
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
        if let Some(existing) = self.existing_workflow(&request)? {
            return Ok(existing);
        }
        let mut payload = request.clone();
        payload.request_id.clear();
        let mut workflow = WorkflowRecord {
            workflow_id: uuid::Uuid::new_v4().to_string(),
            submission: request,
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
        };
        Self::workflow_event(
            &mut workflow,
            "Workflow accepted; parameters and step IDs committed before ACK",
        );
        self.connection.execute("INSERT INTO workflows(id,project,key,payload,status,record) VALUES (?1,?2,?3,?4,'queued',?5)",params![workflow.workflow_id,workflow.submission.project_id,workflow.submission.idempotency_key,serde_json::to_string(&payload).map_err(|e|e.to_string())?,serde_json::to_string(&workflow).map_err(|e|e.to_string())?]).map_err(sql)?;
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
    fn save_workflow(&mut self, record: &WorkflowRecord) -> Result<()> {
        self.connection
            .execute(
                "UPDATE workflows SET status=?2,record=?3 WHERE id=?1",
                params![
                    record.workflow_id,
                    serde_json::to_value(record.status)
                        .map_err(|e| e.to_string())?
                        .as_str(),
                    serde_json::to_string(record).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        Ok(())
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
        record: &mut WorkflowRecord,
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
        let mut record = self.workflow(id)?;
        if record.status.is_terminal() || record.cancel_requested {
            return Ok(record);
        }
        record.cancel_requested = true;
        Self::workflow_event(
            &mut record,
            "Cancellation requested; waiting for owned child operations to terminate",
        );
        self.save_workflow(&record)?;
        Ok(record)
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
    fn save_application(&mut self, app: &ApplicationRecord) -> Result<()> {
        self.connection.execute("INSERT INTO applications(id,workflow,task,status,record) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET status=excluded.status,record=excluded.record",params![app.application_id,app.workflow_id,app.task_id,serde_json::to_value(app.status).map_err(|e|e.to_string())?.as_str(),serde_json::to_string(app).map_err(|e|e.to_string())?]).map_err(sql)?;
        Ok(())
    }
    pub fn stop_application(&mut self, id: &str) -> Result<ApplicationRecord> {
        let mut app = self.application(id)?;
        if !app.status.is_terminal() {
            self.cancel(&app.task_id)?;
            app.status = ApplicationStatus::Stopping;
            app.reason = Some("Stop requested; awaiting managed process tree termination".into());
            self.save_application(&app)?;
        }
        Ok(app)
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
        &self,
        store: &mut Store,
        active: &mut HashMap<String, String>,
    ) -> Result<()> {
        // Process state is reconciled independently of completed startup workflows.
        for mut app in store.applications(false)? {
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
                store.save_application(&app)?;
            }
        }
        let flows = store.workflows(false)?;
        let mut reserved: HashMap<String, String> = flows
            .iter()
            .filter(|f| f.status == TaskStatus::Running)
            .map(|f| (f.submission.project_id.clone(), f.workflow_id.clone()))
            .collect();
        for mut flow in flows {
            let project = flow.submission.project_id.clone();
            if flow.status == TaskStatus::Queued {
                if flow.cancel_requested
                    || now_ms().saturating_sub(flow.accepted_at_ms) >= flow.submission.timeout_ms
                {
                    let cancelled = flow.cancel_requested;
                    store.finish_workflow(
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
                    )?;
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
                reserved.insert(project.clone(), flow.workflow_id.clone());
                Store::workflow_event(
                    &mut flow,
                    "Project reserved for import/build/start/readiness; unrelated mutations wait",
                );
                store.save_workflow(&flow)?;
            }
            if let Err(reason) = self.advance_workflow(store, active, &mut flow) {
                for (index, step) in flow.steps.iter().enumerate().take(3) {
                    if let Some(id) = &step.operation_id {
                        if index == 0 {
                            store.cancel_import(id)?;
                        } else {
                            store.cancel(id)?;
                        }
                    }
                }
                store.finish_workflow(
                    &mut flow,
                    TaskStatus::Indeterminate,
                    &format!(
                        "Workflow reconciliation failed; automatic restart disabled: {reason}"
                    ),
                )?;
            }
        }
        let pending = store.workflows(false)?;
        self.readiness_scans
            .lock()
            .map_err(|_| "readiness scan lock poisoned")?
            .retain(|id, _| pending.iter().any(|f| &f.workflow_id == id));
        Ok(())
    }
    fn advance_workflow(
        &self,
        store: &mut Store,
        active: &mut HashMap<String, String>,
        flow: &mut WorkflowRecord,
    ) -> Result<()> {
        let timeout = now_ms().saturating_sub(flow.accepted_at_ms) >= flow.submission.timeout_ms;
        if flow.cancel_requested || timeout {
            if timeout && !flow.cancel_requested {
                if let Some(id) = &flow.application_id {
                    let mut app = store.application(id)?;
                    if !app.status.is_terminal() && app.failure_reason.is_none() {
                        app.failure_reason =
                            Some("Workflow startup timeout; owned application stopped".into());
                        app.reason = app.failure_reason.clone();
                        store.save_application(&app)?;
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
                let import = if let Some(existing) = store.existing_import(&request)? {
                    existing
                } else {
                    store.accept_import(
                        request,
                        shared.to_string_lossy().into(),
                        local.to_string_lossy().into(),
                    )?
                };
                flow.steps[0].operation_id = Some(import.import_id);
                update_step(
                    flow,
                    0,
                    WorkflowStepStatus::Running,
                    "Import accepted; verified generation required before build",
                );
                store.save_workflow(flow)?;
            }
            let import = store.get_import(flow.steps[0].operation_id.as_ref().unwrap())?;
            return self.child_result(store, flow, 0, import.status, import.reason.as_deref());
        }
        if index == 1 || index == 2 {
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
                let task = if let Some(task) = store.existing(&request)? {
                    task
                } else {
                    match self.resolve_task(&request).and_then(|command| {
                        self.snapshot_reference(&request)
                            .map(|snapshot| (command, snapshot))
                    }) {
                        Ok((command, snapshot)) => store.accept(request, command, snapshot)?,
                        Err(reason) => {
                            update_step(flow, index, WorkflowStepStatus::Failed, &reason);
                            return store.finish_workflow(flow, TaskStatus::Failed, &reason);
                        }
                    }
                };
                if task
                    .snapshot
                    .as_ref()
                    .is_none_or(|s| s.manifest_sha256 != flow.submission.manifest_sha256)
                {
                    store.cancel(&task.task_id)?;
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
                flow.steps[index].operation_id = Some(task.task_id.clone());
                update_step(
                    flow,
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
                        application_id: flow.steps[2].step_id.clone(),
                        workflow_id: flow.workflow_id.clone(),
                        project_id: flow.submission.project_id.clone(),
                        task_id: task.task_id,
                        status: ApplicationStatus::Starting,
                        process_id: None,
                        created_at_ms: now_ms(),
                        ready_at_ms: None,
                        finished_at_ms: None,
                        log_path: flow.submission.log_path.clone(),
                        reason: Some("Awaiting process and explicit stdout readiness probe".into()),
                        failure_reason: None,
                    };
                    store.save_application(&app)?;
                    flow.application_id = Some(app.application_id);
                }
                store.save_workflow(flow)?;
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
        let mut app = store.application(&id)?;
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
            store.cancel(&app.task_id)?;
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
                store.save_application(&app)?;
                // Mark the failed step only after process termination is confirmed on a later tick.
                if let Some(step) = flow.steps.get_mut(index) {
                    step.reason = app.reason.clone();
                }
                store.save_workflow(flow)?;
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
                    store.save_application(&app)?;
                    update_step(
                        flow,
                        3,
                        WorkflowStepStatus::Succeeded,
                        "Readiness confirmed from this application's stdout",
                    );
                    store.save_workflow(flow)?;
                    break;
                }
                let keep = needle.len().saturating_sub(1);
                if bytes.len() > keep {
                    bytes.drain(..bytes.len() - keep);
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
    fn child_result(
        &self,
        store: &mut Store,
        flow: &mut WorkflowRecord,
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
