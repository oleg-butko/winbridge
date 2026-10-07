//! Windows execution authority. The Unix runner is a development/test adapter.
mod artifacts;
mod attention;
mod imports;
mod network;
pub mod store;
mod workflows;
use fs2::FileExt;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use store::{now_ms, Store};
use winbridge_core::config::WorkerConfig;
use winbridge_platform::ManagedChild;
use winbridge_protocol::{
    LocalRequest, LocalResponse, OutputStream, TaskRecord, TaskStatus, WorkerInfo, PROTOCOL_VERSION,
};

pub type Result<T> = std::result::Result<T, String>;
pub struct Worker {
    pub config: WorkerConfig,
    pub generation: String,
    store: Mutex<Store>,
    active_projects: Mutex<HashMap<String, String>>,
    active_jobs: AtomicUsize,
    readiness_scans: Mutex<HashMap<String, (u64, Vec<u8>)>>,
    checks: Mutex<HashMap<String, Option<Result<workflows::CheckResult>>>>,
    stop: AtomicBool,
    controller_connection: Mutex<Option<winbridge_protocol::ControllerConnection>>,
    unreconciled_tasks: Mutex<HashMap<String, String>>,
    _instance_lock: File,
}
impl Worker {
    pub fn open(config: WorkerConfig) -> Result<Arc<Self>> {
        config.validate()?;
        ensure_local_state(&config.state_directory)?;
        winbridge_platform::require_local_disk(&config.state_directory)
            .map_err(|e| e.to_string())?;
        for project in &config.projects {
            winbridge_platform::require_local_disk(&project.workspace_root)
                .map_err(|e| format!("project {}: {e}", project.id))?;
        }
        if !config.state_directory.exists() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(&config.state_directory)
                .map_err(|e| format!("stateDirectory: {e}"))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata =
                fs::symlink_metadata(&config.state_directory).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() || metadata.uid() != unsafe { libc_uid() } {
                return Err("stateDirectory must be a real directory owned by this user".into());
            }
            // Reject a shared directory; avoid changing permissions of user-maintained state.
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(
                    "stateDirectory must have mode 0700; choose/create a private directory".into(),
                );
            }
        }
        let lock_path = config.state_directory.join("worker.lock");
        reject_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|e| format!("stateDirectory is already owned by another worker: {e}"))?;
        let db_path = config.state_directory.join("journal.sqlite");
        for suffix in ["", "-wal", "-shm"] {
            reject_symlink(
                &config
                    .state_directory
                    .join(format!("journal.sqlite{suffix}")),
            )?;
        }
        let mut store = Store::open(&db_path)?;
        store.recover()?;
        let controller_connection = config.worker.controller_url.as_ref().map(|url| {
            winbridge_protocol::ControllerConnection {
                url: url.clone(),
                state: winbridge_protocol::ConnectionState::Connecting,
                session_id: None,
                controller_node_id: None,
                controller_generation: None,
                last_confirmed_at_ms: None,
                error: None,
            }
        });
        let worker = Arc::new(Self {
            config,
            generation: uuid::Uuid::new_v4().to_string(),
            store: Mutex::new(store),
            active_projects: Mutex::new(HashMap::new()),
            active_jobs: AtomicUsize::new(0),
            readiness_scans: Mutex::new(HashMap::new()),
            checks: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
            controller_connection: Mutex::new(controller_connection),
            unreconciled_tasks: Mutex::new(HashMap::new()),
            _instance_lock: lock,
        });
        worker.recover_imports()?;
        Ok(worker)
    }
    fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| "journal lock was poisoned; restart worker and reconcile tasks".into())
    }
    pub fn handle(&self, request: LocalRequest) -> LocalResponse {
        match self.dispatch(request) {
            Ok(response) => response,
            Err(message) => LocalResponse::Error { message },
        }
    }
    fn dispatch(&self, request: LocalRequest) -> Result<LocalResponse> {
        if matches!(
            request,
            LocalRequest::Submit { .. }
                | LocalRequest::SubmitWorkflow { .. }
                | LocalRequest::ImportSnapshot { .. }
        ) {
            if let Some(reason) = self.journal_recovery_required()? {
                return Err(reason);
            }
        }
        match request {
            LocalRequest::RequestAttention { request } => {
                if self.stop.load(Ordering::Acquire) {
                    return Err("Worker stopping; attention not accepted".into());
                }
                Ok(LocalResponse::Attention {
                    attention: self.store()?.request_attention(&request)?,
                })
            }
            LocalRequest::ListAttention {
                after_sequence,
                pending_only,
            } => Ok(LocalResponse::AttentionList {
                page: self.store()?.list_attention(after_sequence, pending_only)?,
            }),
            LocalRequest::AcknowledgeAttention { attention_id } => Ok(LocalResponse::Attention {
                attention: self.store()?.acknowledge_attention(&attention_id)?,
            }),
            LocalRequest::SubmitWorkflow { request } => {
                if self.stop.load(Ordering::Acquire) {
                    return Err("Worker stopping; workflow not accepted".into());
                }
                let mut store = self.store()?;
                if let Some(reason) = self.journal_recovery_required()? {
                    return Err(reason);
                }
                if let Some(record) = store.existing_workflow(&request)? {
                    return Ok(LocalResponse::Workflow {
                        workflow: Box::new(record),
                    });
                }
                winbridge_core::workflow::validate(&self.config, &request)?;
                if store
                    .applications(false)?
                    .iter()
                    .any(|a| a.project_id == request.project_id)
                {
                    return Err("An owned application is already active for this project; explicitly stop it before another startup workflow".into());
                }
                if store.workflows(false)?.len() >= 128 {
                    return Err("Workflow queue limit reached (128)".into());
                }
                Ok(LocalResponse::Workflow {
                    workflow: Box::new(store.accept_workflow(*request)?),
                })
            }
            LocalRequest::ListWorkflows => Ok(LocalResponse::Workflows {
                workflows: self.store()?.workflows(true)?,
            }),
            LocalRequest::GetWorkflow { workflow_id } => Ok(LocalResponse::Workflow {
                workflow: Box::new(self.store()?.workflow(&workflow_id)?),
            }),
            LocalRequest::CancelWorkflow { workflow_id } => Ok(LocalResponse::Workflow {
                workflow: Box::new(self.store()?.cancel_workflow(&workflow_id)?),
            }),
            LocalRequest::ListApplications => Ok(LocalResponse::Applications {
                applications: self.store()?.applications(true)?,
            }),
            LocalRequest::GetApplication { application_id } => Ok(LocalResponse::Application {
                application: self.store()?.application(&application_id)?,
            }),
            LocalRequest::StopApplication { application_id } => Ok(LocalResponse::Application {
                application: self.store()?.stop_application(&application_id)?,
            }),
            LocalRequest::ReadApplicationLog {
                application_id,
                cursor,
                max_bytes,
            } => {
                let app = self.store()?.application(&application_id)?;
                let project = self
                    .config
                    .projects
                    .iter()
                    .find(|p| p.id == app.project_id)
                    .ok_or("Application project not registered")?;
                Ok(LocalResponse::Log {
                    page: winbridge_log_reader::read(
                        &project.workspace_root,
                        &app.log_path,
                        &application_id,
                        cursor.as_ref(),
                        max_bytes,
                    )?,
                })
            }
            LocalRequest::ListSnapshots { project_id, offset } => {
                // Reuse registration checks before any discovery filesystem access.
                self.import_paths(&project_id, "listing")?;
                Ok(LocalResponse::Snapshots {
                    page: winbridge_workspace_sync::list_snapshots(
                        self.config.delivery.as_ref().unwrap(),
                        &project_id,
                        offset,
                    )?,
                })
            }
            LocalRequest::Info => Ok(LocalResponse::Info {
                info: Box::new(WorkerInfo {
                    build: Some(winbridge_protocol::build_identity()),
                    workflow_features: vec!["artifacts".into(), "netwatch_health".into()],
                    attention_available: true,
                    journal_recovery_required: self.journal_recovery_required()?,
                    protocol_version: PROTOCOL_VERSION,
                    instance_id: self.config.instance_id.clone(),
                    node_id: self.config.node_id.clone(),
                    generation: self.generation.clone(),
                    platform: std::env::consts::OS.into(),
                    runtime_validation: if cfg!(windows) {
                        "Локальное выполнение, отмена и восстановление проверены на Windows"
                    } else {
                        "Локальное выполнение через Unix adapter разработки"
                    }
                    .into(),
                    projects: self.config.public_projects(),
                    snapshot_import_available: self.config.delivery.is_some(),
                    workflow_available: self.config.delivery.is_some(),
                    controller_connection: self
                        .controller_connection
                        .lock()
                        .map_err(|_| "connection state lock poisoned")?
                        .clone(),
                }),
            }),
            LocalRequest::ImportSnapshot { request } => {
                if self.stop.load(Ordering::Acquire) {
                    return Err("Worker stopping; import not accepted".into());
                }
                let mut store = self.store()?;
                if let Some(reason) = self.journal_recovery_required()? {
                    return Err(reason);
                }
                if let Some(import) = store.existing_import(&request)? {
                    return Ok(LocalResponse::Import {
                        import: Box::new(import),
                    });
                }
                winbridge_workspace_sync::validate_submission(&request)?;
                let (shared, local) =
                    self.import_paths(&request.project_id, &request.snapshot_id)?;
                Ok(LocalResponse::Import {
                    import: Box::new(store.accept_import(
                        request,
                        shared.to_string_lossy().into(),
                        local.to_string_lossy().into(),
                    )?),
                })
            }
            LocalRequest::ListImports => Ok(LocalResponse::Imports {
                imports: self.store()?.imports(true)?,
            }),
            LocalRequest::GetImport { import_id } => Ok(LocalResponse::Import {
                import: Box::new(self.store()?.get_import(&import_id)?),
            }),
            LocalRequest::CancelImport { import_id } => Ok(LocalResponse::Import {
                import: Box::new(self.store()?.cancel_import(&import_id)?),
            }),
            LocalRequest::PreviewSnapshot {
                project_id,
                snapshot_id,
                offset,
            } => Ok(LocalResponse::SnapshotPreview {
                preview: self.preview_snapshot(&project_id, &snapshot_id, offset)?,
            }),
            LocalRequest::ControllerStatus => Err(
                "controller_status is available through the Debian controller, not worker IPC"
                    .into(),
            ),
            LocalRequest::Submit { request } => {
                if self.stop.load(Ordering::Acquire) {
                    return Err("worker is stopping; new tasks are not accepted".into());
                }
                let mut store = self.store()?;
                if let Some(reason) = self.journal_recovery_required()? {
                    return Err(reason);
                }
                if let Some(task) = store.existing(&request)? {
                    return Ok(LocalResponse::Task {
                        task: Box::new(task),
                    });
                }
                let command = self.resolve_task(&request)?;
                Ok(LocalResponse::Task {
                    task: Box::new(store.accept(
                        request.clone(),
                        command,
                        self.snapshot_reference(&request)?,
                    )?),
                })
            }
            LocalRequest::GetTask { task_id } => {
                let store = self.store()?;
                self.require_committed_result(&task_id)?;
                Ok(LocalResponse::Task {
                    task: Box::new(store.get(&task_id)?),
                })
            }
            LocalRequest::ListTasks => {
                let tasks = self
                    .store()?
                    .recent()?
                    .into_iter()
                    .map(Into::into)
                    .collect();
                Ok(LocalResponse::Tasks { tasks })
            }
            LocalRequest::ReadOutput {
                task_id,
                after_sequence,
            } => Ok(LocalResponse::Output {
                chunks: self.store()?.output(&task_id, after_sequence)?,
            }),
            LocalRequest::Cancel { task_id } => {
                let mut store = self.store()?;
                self.require_committed_result(&task_id)?;
                Ok(LocalResponse::Task {
                    task: Box::new(store.cancel(&task_id)?),
                })
            }
        }
    }
    pub fn schedule(self: &Arc<Self>) -> Result<()> {
        if self.stop.load(Ordering::Acquire) || self.journal_recovery_required()?.is_some() {
            return Ok(());
        }
        let mut active = self
            .active_projects
            .lock()
            .map_err(|_| "process registry lock poisoned")?;
        // Journal and project admission are inspected while holding both locks.
        let mut store = self.store()?;
        if self.journal_recovery_required()?.is_some() {
            return Ok(());
        }
        self.advance_workflows(&mut store, &mut active)?;
        let workflows = store.workflows(false)?;
        let tasks = store.all()?;
        let imports = store.imports(false)?;
        for mut import in imports
            .iter()
            .filter(|r| r.status == TaskStatus::Queued)
            .cloned()
        {
            let project = import.submission.project_id.clone();
            if active.contains_key(&project)
                || workflows::reserved_for_other(&workflows, &project, &import.import_id)
                || tasks.iter().any(|t| {
                    t.status == TaskStatus::Queued
                        && !workflows::reserved_for_other(&workflows, &project, &t.task_id)
                        && t.submission.project_id == project
                        && t.accepted_at_ms <= import.accepted_at_ms
                })
            {
                continue;
            }
            import.status = TaskStatus::Running;
            import.phase = "validate".into();
            Store::import_event(
                &mut import,
                "Import lease acquired; validating immutable delivery",
            );
            store.save_import(&import)?;
            active.insert(project.clone(), import.import_id.clone());
            self.active_jobs.fetch_add(1, Ordering::AcqRel);
            let worker = Arc::clone(self);
            thread::spawn(move || {
                if let Err(error) = worker.execute_import(import.clone()) {
                    if let Ok(mut store) = worker.store() {
                        let _ = store.finish_import(
                            &import.import_id,
                            TaskStatus::Indeterminate,
                            &error,
                        );
                    }
                }
                if let Ok(mut projects) = worker.active_projects.lock() {
                    if projects.get(&project) == Some(&import.import_id) {
                        projects.remove(&project);
                    }
                }
                worker.active_jobs.fetch_sub(1, Ordering::AcqRel);
            });
        }
        for mut task in tasks {
            let project = task.submission.project_id.clone();
            if task.status != TaskStatus::Queued
                || active.contains_key(&project)
                || workflows::reserved_for_other(&workflows, &project, &task.task_id)
                || imports.iter().any(|r| {
                    r.status == TaskStatus::Queued
                        && !workflows::reserved_for_other(&workflows, &project, &r.import_id)
                        && r.submission.project_id == project
                        && r.accepted_at_ms < task.accepted_at_ms
                })
            {
                continue;
            }
            // Revalidate real paths at launch; never run against a silently changed root/cwd.
            match self.resolve_task(&task.submission) {
                Ok(command) if command == task.resolved_command => {}
                result => {
                    let reason = result.err().unwrap_or_else(|| "resolved command changed since acceptance; submit a new task after checking project paths".into());
                    store.finish(&task.task_id, TaskStatus::Failed, None, &reason)?;
                    continue;
                }
            }
            task.status = TaskStatus::Running;
            Store::event(
                &mut task,
                "Launch intent committed; awaiting process creation",
            );
            store.save(&task)?;
            active.insert(project.clone(), task.task_id.clone());
            self.active_jobs.fetch_add(1, Ordering::AcqRel);
            let worker = Arc::clone(self);
            thread::spawn(move || {
                let id = task.task_id.clone();
                let project = task.submission.project_id.clone();
                if let Err(error) = worker.execute(task) {
                    eprintln!("task {id}: {error}; outcome requires reconciliation");
                    // Publish the fault before releasing the journal lock. Admission
                    // rechecks while holding it, so it cannot race the failed commit.
                    match worker.store() {
                        Ok(mut store) => {
                            if let Err(recovery_error) =
                                store.finish(&id, TaskStatus::Indeterminate, None, &error)
                            {
                                worker.record_journal_fault(&id, &error, &recovery_error);
                            }
                        }
                        Err(recovery_error) => {
                            worker.record_journal_fault(&id, &error, &recovery_error)
                        }
                    }
                }
                let unreconciled = worker
                    .unreconciled_tasks
                    .lock()
                    .map_or(true, |faults| faults.contains_key(&id));
                if let Ok(mut projects) = worker.active_projects.lock() {
                    if !unreconciled && projects.get(&project) == Some(&id) {
                        projects.remove(&project);
                    }
                }
                worker.active_jobs.fetch_sub(1, Ordering::AcqRel);
            });
        }
        Ok(())
    }
    fn journal_recovery_required(&self) -> Result<Option<String>> {
        let faults = self
            .unreconciled_tasks
            .lock()
            .map_err(|_| "journal fault state lock poisoned")?;
        Ok(faults.keys().next().map(|id|format!("Task {id} has an uncommitted outcome. New launches are suspended; repair journal storage and restart worker for indeterminate recovery. No command is automatically resubmitted")))
    }
    fn require_committed_result(&self, id: &str) -> Result<()> {
        if let Some(reason) = self
            .unreconciled_tasks
            .lock()
            .map_err(|_| "journal fault state lock poisoned")?
            .get(id)
        {
            return Err(format!("Task {id}: result could not be committed; journal status is stale. {reason}. Repair journal storage and restart worker to reconcile; do not resubmit the command"));
        }
        Ok(())
    }
    fn record_journal_fault(&self, id: &str, error: &str, recovery_error: &str) {
        let reason = format!(
            "Execution error: {error}; recording unknown outcome also failed: {recovery_error}"
        );
        eprintln!("task {id}: {reason}; new launches suspended until journal recovery");
        if let Ok(mut faults) = self.unreconciled_tasks.lock() {
            faults.insert(id.into(), reason);
        }
    }
    fn execute(self: &Arc<Self>, task: TaskRecord) -> Result<()> {
        let id = task.task_id.clone();
        if self.stop.load(Ordering::Acquire) || self.store()?.get(&id)?.cancel_requested {
            self.store()?.finish(
                &id,
                TaskStatus::Cancelled,
                None,
                "Cancelled before process creation",
            )?;
            return Ok(());
        }
        if let Some(snapshot) = &task.submission.snapshot_id {
            if task
                .snapshot
                .as_ref()
                .is_none_or(|r| r.snapshot_id != *snapshot)
            {
                self.store()?.finish(
                    &id,
                    TaskStatus::Failed,
                    None,
                    "Task lacks a bound accepted snapshot manifest; submit a new explicit task",
                )?;
                return Ok(());
            }
            let project = self
                .config
                .projects
                .iter()
                .find(|p| p.id == task.submission.project_id)
                .ok_or("Snapshot project removed")?;
            let result = winbridge_workspace_sync::verified_generation(
                &project.workspace_root,
                &project.id,
                snapshot,
                task.snapshot.as_ref().map(|r| r.manifest_sha256.as_str()),
                |_, _, _| {
                    if self.stop.load(Ordering::Acquire) || self.store()?.get(&id)?.cancel_requested
                    {
                        return Err("Cancelled during snapshot source verification".into());
                    }
                    Ok(())
                },
            );
            if let Err(reason) = result {
                let cancelled =
                    self.stop.load(Ordering::Acquire) || self.store()?.get(&id)?.cancel_requested;
                self.store()?.finish(
                    &id,
                    if cancelled {
                        TaskStatus::Cancelled
                    } else {
                        TaskStatus::Failed
                    },
                    None,
                    &reason,
                )?;
                return Ok(());
            }
        }
        let (mut child, stdout, stderr) = match ManagedChild::spawn(&task.resolved_command) {
            Ok(child) => child,
            Err(error) => {
                self.store()?.finish(
                    &id,
                    TaskStatus::Failed,
                    None,
                    &format!(
                        "Could not launch executable {:?}: {error}",
                        task.resolved_command.executable
                    ),
                )?;
                return Ok(());
            }
        };
        {
            let mut store = self.store()?;
            let mut current = store.get(&id)?;
            current.process_id = Some(child.id());
            current.started_at_ms = Some(now_ms());
            Store::event(
                &mut current,
                format!(
                    "Process {} started; stdout/stderr are drained independently",
                    child.id()
                ),
            );
            store.save(&current)?;
        }
        let fault = Arc::new(AtomicBool::new(false));
        let readers: Vec<_> = [
            (stdout, OutputStream::Stdout),
            (stderr, OutputStream::Stderr),
        ]
        .into_iter()
        .map(|(mut reader, stream)| {
            let worker = Arc::clone(self);
            let id = id.clone();
            let fault = Arc::clone(&fault);
            thread::spawn(move || {
                let mut bytes = [0u8; 8192];
                let mut offset = 0;
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(length) => {
                            if !fault.load(Ordering::Acquire) {
                                let result = worker.store().and_then(|mut store| {
                                    store.append_output(
                                        &id,
                                        stream,
                                        offset,
                                        &bytes[..length],
                                        worker.config.max_output_bytes_per_task,
                                    )
                                });
                                if let Err(error) = result {
                                    eprintln!("task {id}: output persistence failed: {error}");
                                    fault.store(true, Ordering::Release);
                                }
                            }
                            offset += length as u64;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => {
                            eprintln!("task {id}: {stream:?} read failed: {error}");
                            fault.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            })
        })
        .collect();
        let deadline = if self.store()?.application_for_task(&id)?.is_some() {
            None
        } else {
            Some(Instant::now() + Duration::from_millis(task.resolved_command.timeout_ms))
        };
        let (status, exit_code, reason) = loop {
            if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
                break (
                    if exit.success() {
                        TaskStatus::Succeeded
                    } else {
                        TaskStatus::Failed
                    },
                    exit.code(),
                    exit.code().map_or_else(
                        || "Процесс завершился без числового exit code".into(),
                        |code| format!("Команда завершилась с кодом {code}"),
                    ),
                );
            }
            let current = self.store()?.get(&id)?;
            let stop = self.stop.load(Ordering::Acquire);
            let error = fault.load(Ordering::Acquire);
            if stop
                || current.cancel_requested
                || error
                || deadline.is_some_and(|d| Instant::now() >= d)
            {
                let (status, reason) = if error {
                    (
                        TaskStatus::Indeterminate,
                        "Output persistence/read failed; terminating managed process tree",
                    )
                } else if stop {
                    (
                        TaskStatus::Interrupted,
                        "Worker stopping; managed process tree terminated",
                    )
                } else if current.cancel_requested {
                    (TaskStatus::Cancelled, "Managed process tree cancelled")
                } else {
                    (
                        TaskStatus::Failed,
                        "Command timeout; managed process tree terminated",
                    )
                };
                break (status, None, reason.into());
            }
            thread::sleep(Duration::from_millis(20));
        };
        // Also reap descendants which outlive a one-shot parent; they must not hold pipes open.
        child
            .terminate_tree()
            .map_err(|e| format!("process tree termination failed: {e}"))?;
        let cleanup_deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().map_err(|e| e.to_string())?.is_none()
            || !child.tree_is_empty().map_err(|e| e.to_string())?
        {
            if Instant::now() >= cleanup_deadline {
                return Err("process tree did not terminate within 5 seconds; task remains indeterminate on recovery".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        for reader in readers {
            if reader.join().is_err() {
                fault.store(true, Ordering::Release);
            }
        }
        let (status, reason) = if fault.load(Ordering::Acquire) {
            (
                TaskStatus::Indeterminate,
                "Output could not be fully persisted; command outcome requires inspection".into(),
            )
        } else {
            (status, reason)
        };
        self.store()?.finish(&id, status, exit_code, &reason)?;
        Ok(())
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn is_idle(&self) -> bool {
        self.active_jobs.load(Ordering::Acquire) == 0
    }
}

fn reject_symlink(path: &Path) -> Result<()> {
    winbridge_workspace_sync::safe_path(path).map_err(|reason| {
        format!(
            "state file {} must be a real path without links: {reason}",
            path.display()
        )
    })
}
// Unix UID without another platform-specific dependency in the worker package.
#[cfg(unix)]
unsafe fn libc_uid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}
fn ensure_local_state(path: &Path) -> Result<()> {
    winbridge_workspace_sync::safe_path(path)?;
    if path.to_string_lossy().replace('\\', "/").starts_with("//") {
        return Err("stateDirectory must be on a local disk, not UNC/network storage".into());
    }
    #[cfg(unix)]
    {
        let mut parent = path;
        while !parent.exists() {
            parent = parent
                .parent()
                .ok_or("stateDirectory has no existing parent")?;
        }
        let actual = fs::canonicalize(parent).map_err(|e| e.to_string())?;
        let mounts = fs::read_to_string("/proc/mounts").map_err(|e| e.to_string())?;
        for line in mounts.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() > 2 && fields[2].contains("vmhgfs") && actual.starts_with(fields[1]) {
                return Err(
                    "stateDirectory cannot be in VMware Shared Folders; select a local disk".into(),
                );
            }
        }
    }
    Ok(())
}

pub async fn serve(worker: Arc<Worker>) -> Result<()> {
    let address =
        winbridge_transport::endpoint(&worker.config.state_directory, &worker.config.instance_id)
            .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let mut listener = {
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};
        // The endpoint itself is an expected Unix socket. Validate its ancestors
        // separately; the regular-file guard intentionally rejects special files.
        reject_symlink(
            Path::new(&address)
                .parent()
                .ok_or("IPC path has no parent")?,
        )?;
        if Path::new(&address).exists() {
            if !fs::symlink_metadata(&address)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_socket()
            {
                return Err(format!(
                    "IPC path {address} is not a socket; refusing to remove an existing file"
                ));
            }
            match tokio::net::UnixStream::connect(&address).await {
                Ok(_) => return Err(format!("IPC endpoint {address} is already in use")),
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
                Err(error) => return Err(format!("cannot verify stale IPC endpoint: {error}")),
            }
            fs::remove_file(&address).map_err(|e| e.to_string())?;
        }
        let listener = tokio::net::UnixListener::bind(&address).map_err(|e| e.to_string())?;
        fs::set_permissions(&address, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener
    };
    #[cfg(windows)]
    let mut listener =
        winbridge_transport::create_pipe(&address, true).map_err(|e| e.to_string())?;
    eprintln!(
        "WinBridge worker ready: {address}; generation {}",
        worker.generation
    );
    let permits = Arc::new(tokio::sync::Semaphore::new(32));
    let network = worker
        .config
        .worker
        .controller_url
        .as_ref()
        .map(|_| tokio::spawn(network::connect(Arc::clone(&worker))));
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = interval.tick() => { worker.schedule()?; },
            result = accept_local(&mut listener, &address) => {
                let stream = result.map_err(|e| e.to_string())?;
                accept_connection(stream, Arc::clone(&worker), Arc::clone(&permits));
            },
        }
    }
    if let Some(network) = network {
        network.abort();
    }
    worker.stop();
    while !worker.is_idle() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    #[cfg(unix)]
    {
        fs::remove_file(address).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(unix)]
async fn accept_local(
    listener: &mut tokio::net::UnixListener,
    _address: &str,
) -> std::io::Result<tokio::net::UnixStream> {
    listener.accept().await.map(|(stream, _)| stream)
}
#[cfg(windows)]
async fn accept_local(
    listener: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    address: &str,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    listener.connect().await?;
    let replacement = winbridge_transport::create_pipe(address, false)?;
    Ok(std::mem::replace(listener, replacement))
}
fn accept_connection<S>(mut stream: S, worker: Arc<Worker>, permits: Arc<tokio::sync::Semaphore>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let Ok(permit) = permits.try_acquire_owned() else {
        return;
    };
    tokio::spawn(async move {
        let _permit = permit;
        let operation = async {
            let bytes = winbridge_transport::read_frame(&mut stream).await?;
            let request = serde_json::from_slice::<LocalRequest>(&bytes);
            let response = match request {
                Ok(request) => tokio::task::spawn_blocking(move || worker.handle(request))
                    .await
                    .map_err(std::io::Error::other)?,
                Err(error) => LocalResponse::Error {
                    message: format!("invalid IPC request: {error}"),
                },
            };
            let mut bytes = serde_json::to_vec(&response)?;
            if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
                bytes = serde_json::to_vec(&LocalResponse::Error {
                    message: "Response exceeds 1 MiB; query an individual task".into(),
                })?;
            }
            winbridge_transport::write_frame(&mut stream, &bytes).await
        };
        if let Err(error) = tokio::time::timeout(winbridge_transport::IPC_TIMEOUT, operation).await
        {
            eprintln!("IPC connection deadline: {error}");
        }
    });
}
