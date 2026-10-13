//! Windows execution authority. The Unix runner is a development/test adapter.
mod artifacts;
mod attention;
mod history;
mod imports;
mod network;
mod recovery;
pub use recovery::RECOVERY_MARKER;
mod retention;
mod settings;
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
use store::Store;
use winbridge_core::config::WorkerConfig;
use winbridge_core::task::TaskEffect;
use winbridge_platform::{trace_process, ManagedChild};
use winbridge_protocol::{
    LocalRequest, LocalResponse, OutputStream, TaskRecord, TaskStatus, WorkerInfo, PROTOCOL_VERSION,
};

pub type Result<T> = std::result::Result<T, String>;
/// The receipt survives early errors. Only this OS owner can prove tree cleanup.
struct ProcessOwner {
    child: ManagedChild,
    receipt: Arc<Mutex<Option<store::CleanupProof>>>,
    readers: Vec<thread::JoinHandle<()>>,
    cleaned: bool,
}
impl std::ops::Deref for ProcessOwner {
    type Target = ManagedChild;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl std::ops::DerefMut for ProcessOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl ProcessOwner {
    fn cleanup(&mut self) -> Result<()> {
        if self.cleaned {
            return Ok(());
        }
        trace_process("cleanup.terminate.begin", Some(self.child.id()));
        self.child
            .terminate_tree()
            .map_err(|e| format!("process tree termination failed: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        trace_process("cleanup.terminate.complete", Some(self.child.id()));
        loop {
            let parent_reaped = self.child.try_wait().map_err(|e| e.to_string())?.is_some();
            let tree_empty = self.child.tree_is_empty().map_err(|e| e.to_string())?;
            if parent_reaped && tree_empty {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!("process tree cleanup was not confirmed within 5 seconds (PID {}, parent_reaped={parent_reaped}, tree_empty={tree_empty}); project remains reserved", self.child.id()));
            }
            thread::sleep(Duration::from_millis(20));
        }
        trace_process("cleanup.confirmed", Some(self.child.id()));
        self.cleaned = true;
        Ok(())
    }
    fn drain_readers(&mut self) -> bool {
        trace_process("readers-drain.begin", Some(self.child.id()));
        let mut failed = false;
        for reader in self.readers.drain(..) {
            failed |= reader.join().is_err();
        }
        trace_process("readers-drain.complete", Some(self.child.id()));
        failed
    }
}
impl Drop for ProcessOwner {
    fn drop(&mut self) {
        if self.cleanup().is_ok() {
            self.drain_readers();
            if let Ok(mut receipt) = self.receipt.lock() {
                *receipt = Some(store::CleanupProof::ProcessTreeEmpty);
            }
        }
    }
}
pub struct Worker {
    config_path: Mutex<Option<std::path::PathBuf>>,
    maintenance: Mutex<(u64, Option<String>)>,
    maintenance_running: AtomicBool,
    pub config: WorkerConfig,
    pub generation: String,
    store: Mutex<Store>,
    active_projects: Mutex<HashMap<String, String>>,
    active_jobs: AtomicUsize,
    readiness_scans: Mutex<HashMap<String, (u64, Vec<u8>)>>,
    recovery_scan: Mutex<Instant>,
    checks: Mutex<HashMap<String, Option<Result<workflows::CheckResult>>>>,
    stop: AtomicBool,
    controller_connection: Mutex<Option<winbridge_protocol::ControllerConnection>>,
    unreconciled_tasks: Mutex<HashMap<String, String>>,
    inspection: bool,
    _instance_lock: File,
}
impl Worker {
    pub fn open(config: WorkerConfig) -> Result<Arc<Self>> {
        Self::open_inner(config, None, false)
    }
    pub fn open_recovery(config: WorkerConfig) -> Result<Arc<Self>> {
        Self::open_inner(config, None, true)
    }
    fn open_inner(
        config: WorkerConfig,
        config_path: Option<std::path::PathBuf>,
        inspection: bool,
    ) -> Result<Arc<Self>> {
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
        let store = if inspection {
            if !db_path.is_file() {
                return Err("Read-only recovery requires an existing journal.sqlite; no journal was created".into());
            }
            recovery::latch(&config.state_directory)?;
            Store::open_read_only(&db_path)?
        } else {
            recovery::reject_normal_start(&config.state_directory)?;
            let mut store = Store::open(&db_path)?;
            store.begin_authority()?;
            store.recover_resource_leases()?;
            store.capture_legacy_resources().map_err(|e| {
                format!("{e}; use --config PATH serve-recovery to inspect without dispatch")
            })?;
            store.reconcile_process_resources()?;
            store.recover()?;
            store
        };
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
            config_path: Mutex::new(config_path),
            maintenance: Mutex::new((0, None)),
            maintenance_running: AtomicBool::new(false),
            config,
            generation: uuid::Uuid::new_v4().to_string(),
            store: Mutex::new(store),
            active_projects: Mutex::new(HashMap::new()),
            active_jobs: AtomicUsize::new(0),
            readiness_scans: Mutex::new(HashMap::new()),
            recovery_scan: Mutex::new(Instant::now()),
            checks: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
            controller_connection: Mutex::new(controller_connection),
            unreconciled_tasks: Mutex::new(HashMap::new()),
            inspection,
            _instance_lock: lock,
        });
        if !inspection {
            worker.recover_imports()?;
            worker.store()?.release_stopped_import_resources()?;
            // Observe application/task consistency before the first IPC response,
            // without advancing workflows or creating any new process.
            let result = {
                let mut store = worker.store()?;
                worker.reconcile_applications(&mut store)
            };
            if let Err(error) = result {
                if worker.journal_recovery_required()?.is_none() {
                    return Err(error);
                }
            }
        }
        Ok(worker)
    }
    fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        let store = self
            .store
            .lock()
            .map_err(|_| "journal lock was poisoned; restart worker and reconcile tasks")?;
        store.check_authority()?;
        Ok(store)
    }
    pub fn handle(&self, request: LocalRequest) -> LocalResponse {
        match self.dispatch(request) {
            Ok(response) => response,
            Err(message) => LocalResponse::Error { message },
        }
    }
    fn dispatch(&self, request: LocalRequest) -> Result<LocalResponse> {
        // An allowlist keeps future protocol additions disabled until their
        // read-only behavior is deliberately reviewed. SQLite is read-only too.
        if self.inspection
            && !matches!(
                &request,
                LocalRequest::Info
                    | LocalRequest::GetRuntimeSettings
                    | LocalRequest::ListTasks
                    | LocalRequest::GetTask { .. }
                    | LocalRequest::ReadOutput { .. }
                    | LocalRequest::History { .. }
                    | LocalRequest::StorageUsage
                    | LocalRequest::PreviewOutputCleanup { .. }
                    | LocalRequest::ListImports
                    | LocalRequest::GetImport { .. }
                    | LocalRequest::ListWorkflows
                    | LocalRequest::GetWorkflow { .. }
                    | LocalRequest::ListApplications
                    | LocalRequest::GetApplication { .. }
                    | LocalRequest::ReadApplicationLog { .. }
                    | LocalRequest::ListAttention { .. }
                    | LocalRequest::ListAttentionEvents { .. }
                    | LocalRequest::ListSnapshots { .. }
                    | LocalRequest::PreviewSnapshot { .. }
            )
        {
            return Err(recovery::INSPECTION_REASON.into());
        }
        if matches!(
            request,
            LocalRequest::Submit { .. }
                | LocalRequest::SubmitWorkflow { .. }
                | LocalRequest::ImportSnapshot { .. }
                | LocalRequest::ImportDeltaSnapshot { .. }
        ) {
            if let Some(reason) = self.journal_recovery_required()? {
                return Err(reason);
            }
        }
        match request {
            LocalRequest::GetRuntimeSettings => Ok(LocalResponse::RuntimeSettings {
                settings: self.runtime_settings()?,
            }),
            LocalRequest::SaveRuntimeSettings { request } => Ok(LocalResponse::SettingsSaved {
                saved: self.save_runtime_settings(&request)?,
            }),
            LocalRequest::PreviewOutputCleanup { query } => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                Ok(LocalResponse::OutputCleanupPreview {
                    preview: store.preview_output_cleanup(&query)?,
                })
            }
            LocalRequest::ApplyOutputCleanup { request } => {
                let mut store = self.store()?;
                if let Some(reason) = self.journal_recovery_required()? {
                    return Err(reason);
                }
                Ok(LocalResponse::OutputCleanupResult {
                    result: store.apply_output_cleanup(&request)?,
                })
            }
            LocalRequest::History { query } => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                Ok(LocalResponse::History {
                    page: store.history(&query)?,
                })
            }
            LocalRequest::PrepareTaskDiagnostic { request } => {
                let mut store = self.store()?;
                self.require_committed_result(&request.task_id)?;
                Ok(LocalResponse::Diagnostic {
                    preview: store.diagnostic(&request)?,
                })
            }
            LocalRequest::StorageUsage => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                let mut usage = store.storage_usage()?;
                for (name, destination) in [
                    ("journal.sqlite", &mut usage.journal_bytes),
                    ("journal.sqlite-wal", &mut usage.wal_bytes),
                ] {
                    let path = self.config.state_directory.join(name);
                    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
                    *destination = match std::fs::metadata(path) {
                        Ok(m) => m.len(),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
                        Err(e) => return Err(e.to_string()),
                    };
                }
                usage.output_limit_bytes_per_task = self.config.max_output_bytes_per_task;
                Ok(LocalResponse::StorageUsage { usage })
            }
            LocalRequest::ListAttentionEvents {
                after_sequence,
                pending_only,
            } => Ok(LocalResponse::AttentionEvents {
                page: self
                    .store()?
                    .list_attention_events(after_sequence, pending_only)?,
            }),
            LocalRequest::AcknowledgeAttentionEvent { attention_id } => {
                Ok(LocalResponse::AttentionEvent {
                    event: self.store()?.acknowledge_attention_event(&attention_id)?,
                })
            }
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
                let pins = winbridge_core::workflow::recipe_pins(&self.config, &request)?;
                Ok(LocalResponse::Workflow {
                    workflow: Box::new(store.accept_workflow_with_pins(*request, Some(pins))?),
                })
            }
            LocalRequest::ListWorkflows => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                Ok(LocalResponse::Workflows {
                    workflows: store.workflows(true)?,
                })
            }
            LocalRequest::GetWorkflow { workflow_id } => {
                let store = self.store()?;
                let workflow = store.workflow(&workflow_id)?;
                self.require_workflow_result(&workflow)?;
                Ok(LocalResponse::Workflow {
                    workflow: Box::new(workflow),
                })
            }
            LocalRequest::CancelWorkflow { workflow_id } => {
                let mut store = self.store()?;
                self.require_workflow_result(&store.workflow(&workflow_id)?)?;
                Ok(LocalResponse::Workflow {
                    workflow: Box::new(store.cancel_workflow(&workflow_id)?),
                })
            }
            LocalRequest::ListApplications => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                Ok(LocalResponse::Applications {
                    applications: store.applications(true)?,
                })
            }
            LocalRequest::GetApplication { application_id } => {
                let store = self.store()?;
                let application = store.application(&application_id)?;
                self.require_application_result(&application)?;
                Ok(LocalResponse::Application { application })
            }
            LocalRequest::StopApplication { application_id } => {
                let mut store = self.store()?;
                self.require_application_result(&store.application(&application_id)?)?;
                Ok(LocalResponse::Application {
                    application: store.stop_application(&application_id)?,
                })
            }
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
            LocalRequest::Info => {
                let store = self.store()?;
                let recovery = self.journal_recovery_required()?;
                let active_task_count = if recovery.is_some() {
                    None
                } else {
                    Some(store.active_task_count()?)
                };
                Ok(LocalResponse::Info {
                    info: Box::new(WorkerInfo {
                        resource_recovery: if self.inspection {
                            store.inspection_resource_summary()?
                        } else {
                            store.resource_recovery_summary()?
                        },
                        journal_inspection: self.inspection.then(|| {
                            winbridge_protocol::JournalInspection {
                                schema_version: store::JOURNAL_SCHEMA_VERSION,
                                reason: recovery::INSPECTION_REASON.into(),
                            }
                        }),
                        build: Some(winbridge_protocol::build_identity()),
                        workflow_features: vec!["artifacts".into(), "netwatch_health".into()],
                        attention_available: true,
                        attention_targets_available: true,
                        history_available: true,
                        maintenance_available: !self.inspection,
                        maintenance_error: self
                            .maintenance
                            .lock()
                            .map_err(|_| "Maintenance state lock poisoned")?
                            .1
                            .clone(),
                        journal_recovery_required: recovery,
                        active_task_count,
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
                        projects: self.config.public_projects()?,
                        command_handlers: Some(winbridge_runner::metadata()),
                        snapshot_import_available: !self.inspection
                            && self.config.delivery.is_some(),
                        snapshot_delta_available: !self.inspection
                            && self.config.delivery.is_some(),
                        snapshot_resume_available: !self.inspection
                            && self.config.delivery.is_some(),
                        workflow_available: !self.inspection && self.config.delivery.is_some(),
                        controller_connection: self
                            .controller_connection
                            .lock()
                            .map_err(|_| "connection state lock poisoned")?
                            .clone(),
                    }),
                })
            }
            LocalRequest::ImportSnapshot { request }
            | LocalRequest::ImportDeltaSnapshot { request } => {
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
            LocalRequest::ListImports => {
                let store = self.store()?;
                self.require_live_or_inspection()?;
                Ok(LocalResponse::Imports {
                    imports: store.imports(true)?,
                })
            }
            LocalRequest::GetImport { import_id } => {
                let store = self.store()?;
                self.require_committed_result(&import_id)?;
                Ok(LocalResponse::Import {
                    import: Box::new(store.get_import(&import_id)?),
                })
            }
            LocalRequest::CancelImport { import_id } => {
                let mut store = self.store()?;
                self.require_committed_result(&import_id)?;
                Ok(LocalResponse::Import {
                    import: Box::new(store.cancel_import(&import_id)?),
                })
            }
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
                let origin = winbridge_runner::recipe_origin(&self.config, &request)?;
                Ok(LocalResponse::Task {
                    task: Box::new(store.accept_with_origin(
                        request.clone(),
                        command,
                        self.snapshot_reference(&request)?,
                        origin,
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
        self.automatic_output_cleanup()?;
        {
            let mut last = self
                .recovery_scan
                .lock()
                .map_err(|_| "recovery scan lock poisoned")?;
            if last.elapsed() >= Duration::from_secs(1) {
                let result = self.store()?.reconcile_process_resources();
                if let Err(error) = result {
                    self.record_journal_fault(
                        "physical-reconciliation",
                        "physical cleanup result was not committed",
                        &error,
                    );
                    return Ok(());
                }
                *last = Instant::now();
            }
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
        for import in imports
            .iter()
            .filter(|r| r.status == TaskStatus::Queued)
            .cloned()
        {
            let import = store.import_snapshot(&import.import_id)?;
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
            let (import, token) = match store.claim_import(&import.import_id) {
                Ok(claim) => claim,
                Err(reason) if reason.starts_with("resource busy:") => {
                    store.note_resource_wait(
                        store::OperationKind::Import,
                        &import.import_id,
                        &reason,
                    )?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            active.insert(project.clone(), import.import_id.clone());
            self.active_jobs.fetch_add(1, Ordering::AcqRel);
            let worker = Arc::clone(self);
            thread::spawn(move || {
                if let Err(error) = worker.execute_import(import.clone(), token.clone()) {
                    match worker.store() {
                        Ok(mut store) => {
                            if let Err(recovery) = store.with_execution(&token, |store| {
                                store.finish_import_execution(
                                    &token,
                                    TaskStatus::Indeterminate,
                                    &error,
                                )
                            }) {
                                worker.record_journal_fault(&import.import_id, &error, &recovery);
                            }
                        }
                        Err(recovery) => {
                            worker.record_journal_fault(&import.import_id, &error, &recovery)
                        }
                    }
                }
                let unreconciled = worker
                    .unreconciled_tasks
                    .lock()
                    .map_or(true, |faults| faults.contains_key(&import.import_id));
                if let Ok(mut projects) = worker.active_projects.lock() {
                    if !unreconciled && projects.get(&project) == Some(&import.import_id) {
                        projects.remove(&project);
                    }
                }
                worker.active_jobs.fetch_sub(1, Ordering::AcqRel);
            });
        }
        for task in tasks {
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
            let (decision, token) = match store.claim_task(&task.task_id) {
                Ok(claim) => claim,
                Err(reason) if reason.starts_with("resource busy:") => {
                    store.note_resource_wait(store::OperationKind::Task, &task.task_id, &reason)?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if decision.effect != TaskEffect::ExecuteAcceptedCommand {
                return Err("Task launch did not produce a committed execution decision".into());
            }
            let task = decision.record;
            active.insert(project.clone(), task.task_id.clone());
            self.active_jobs.fetch_add(1, Ordering::AcqRel);
            let worker = Arc::clone(self);
            thread::spawn(move || {
                let id = task.task_id.clone();
                let project = task.submission.project_id.clone();
                let cleanup = Arc::new(Mutex::new(Some(store::CleanupProof::NoProcessCreated)));
                if let Err(error) = worker.execute(task, token.clone(), cleanup.clone()) {
                    eprintln!("task {id}: {error}; outcome requires reconciliation");
                    // Publish the fault before releasing the journal lock. Admission
                    // rechecks while holding it, so it cannot race the failed commit.
                    match worker.store() {
                        Ok(mut store) => {
                            if let Err(recovery_error) = store.with_execution(&token, |store| {
                                if let Some(proof) = cleanup
                                    .lock()
                                    .map_err(|_| "process cleanup receipt poisoned")?
                                    .as_ref()
                                    .copied()
                                {
                                    // Persist the receipt independently if the outcome writer failed.
                                    // The lease stays held until a terminal result is committed.
                                    store.record_execution_cleanup(&token, proof)?;
                                }
                                store.finish(&id, TaskStatus::Indeterminate, None, &error)
                            }) {
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
        if self.inspection {
            return Ok(Some(recovery::INSPECTION_REASON.into()));
        }
        let faults = self
            .unreconciled_tasks
            .lock()
            .map_err(|_| "journal fault state lock poisoned")?;
        Ok(faults.iter().next().map(|(id,reason)|format!("Journal consistency could not be confirmed ({id}): {reason}. New launches are suspended; repair journal storage and restart worker to reconcile. No command is automatically resubmitted")))
    }
    fn require_live_or_inspection(&self) -> Result<()> {
        if !self.inspection {
            if let Some(reason) = self.journal_recovery_required()? {
                return Err(reason);
            }
        }
        Ok(())
    }
    fn require_committed_result(&self, id: &str) -> Result<()> {
        if let Some(reason) = self
            .unreconciled_tasks
            .lock()
            .map_err(|_| "journal fault state lock poisoned")?
            .get(id)
        {
            return Err(format!("Operation {id}: result could not be committed; journal status is stale. {reason}. Repair journal storage and restart worker to reconcile; do not resubmit the command"));
        }
        Ok(())
    }
    fn require_workflow_result(&self, workflow: &winbridge_protocol::WorkflowRecord) -> Result<()> {
        self.require_committed_result(&workflow.workflow_id)?;
        if !workflow.status.is_terminal() {
            for id in workflow
                .steps
                .iter()
                .filter_map(|s| s.operation_id.as_deref())
            {
                self.require_committed_result(id)?;
            }
        }
        Ok(())
    }
    fn require_application_result(
        &self,
        app: &winbridge_protocol::ApplicationRecord,
    ) -> Result<()> {
        self.require_committed_result(&app.application_id)?;
        self.require_committed_result(&app.task_id)
    }
    fn record_journal_fault(&self, id: &str, error: &str, recovery_error: &str) {
        let reason = format!("Journal consistency failure: {error}; reconciliation was not committed: {recovery_error}");
        eprintln!("operation {id}: {reason}; new launches suspended until journal recovery");
        if let Ok(mut faults) = self.unreconciled_tasks.lock() {
            faults.insert(id.into(), reason);
        }
    }
    fn execution_store<T>(
        &self,
        token: &store::ExecutionToken,
        callback: impl FnOnce(&mut Store) -> Result<T>,
    ) -> Result<T> {
        self.store()?.with_execution(token, callback)
    }
    fn execute(
        self: &Arc<Self>,
        task: TaskRecord,
        token: store::ExecutionToken,
        cleanup: Arc<Mutex<Option<store::CleanupProof>>>,
    ) -> Result<()> {
        let id = task.task_id.clone();
        trace_process(&format!("task={id} execution.begin"), None);
        if self.stop.load(Ordering::Acquire)
            || self
                .execution_store(&token, |store| store.get(&id))?
                .cancel_requested
        {
            self.execution_store(&token, |store| {
                store.finish_task_execution(
                    &token,
                    TaskStatus::Cancelled,
                    None,
                    "Cancelled before process creation",
                    store::CleanupProof::NoProcessCreated,
                )
            })?;
            return Ok(());
        }
        if let Some(snapshot) = &task.submission.snapshot_id {
            if task
                .snapshot
                .as_ref()
                .is_none_or(|r| r.snapshot_id != *snapshot)
            {
                self.execution_store(&token, |store| {
                    store.finish_task_execution(
                        &token,
                        TaskStatus::Failed,
                        None,
                        "Task lacks a bound accepted snapshot manifest; submit a new explicit task",
                        store::CleanupProof::NoProcessCreated,
                    )
                })?;
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
                    if self.stop.load(Ordering::Acquire)
                        || self
                            .execution_store(&token, |store| store.get(&id))?
                            .cancel_requested
                    {
                        return Err("Cancelled during snapshot source verification".into());
                    }
                    Ok(())
                },
            );
            if let Err(reason) = result {
                let cancelled = self.stop.load(Ordering::Acquire)
                    || self
                        .execution_store(&token, |store| store.get(&id))?
                        .cancel_requested;
                self.execution_store(&token, |store| {
                    store.finish_task_execution(
                        &token,
                        if cancelled {
                            TaskStatus::Cancelled
                        } else {
                            TaskStatus::Failed
                        },
                        None,
                        &reason,
                        store::CleanupProof::NoProcessCreated,
                    )
                })?;
                return Ok(());
            }
        }
        self.execution_store(&token, |_| Ok(()))?;
        *cleanup
            .lock()
            .map_err(|_| "process cleanup receipt poisoned")? = None;
        let job_name = Store::execution_job_name(&token)?;
        trace_process(&format!("task={id} prepare.begin"), None);
        let (child, stdout, stderr) = match ManagedChild::prepare(
            &task.resolved_command,
            Some(&job_name),
        ) {
            Ok(child) => child,
            Err(error) => {
                if !error.no_process_created {
                    // A partially initialized OS process has no cleanup receipt.
                    // Fallback records indeterminate and retains the project lease.
                    return Err(format!("Process initialization failed after OS creation; cleanup requires reconciliation: {error}"));
                }
                *cleanup
                    .lock()
                    .map_err(|_| "process cleanup receipt poisoned")? =
                    Some(store::CleanupProof::NoProcessCreated);
                self.execution_store(&token, |store| {
                    store.finish_task_execution(
                        &token,
                        TaskStatus::Failed,
                        None,
                        &format!(
                            "Could not launch executable {:?}: {error}",
                            task.resolved_command.executable
                        ),
                        store::CleanupProof::NoProcessCreated,
                    )
                })?;
                return Ok(());
            }
        };
        let mut child = ProcessOwner {
            child,
            receipt: cleanup,
            readers: vec![],
            cleaned: false,
        };
        trace_process(&format!("task={id} bind.begin"), Some(child.id()));
        {
            let mut store = self.store()?;
            store.with_execution(&token, |store| {
                store.bind_task_process(&token, child.id(), child.identity())
            })?;
        }
        trace_process(&format!("task={id} bind.complete"), Some(child.id()));
        let fault = Arc::new(AtomicBool::new(false));
        child.readers = [
            (stdout, OutputStream::Stdout),
            (stderr, OutputStream::Stderr),
        ]
        .into_iter()
        .map(|(mut reader, stream)| {
            let worker = Arc::clone(self);
            let id = id.clone();
            let fault = Arc::clone(&fault);
            let token = token.clone();
            thread::spawn(move || {
                let mut bytes = [0u8; 8192];
                let mut offset = 0;
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(length) => {
                            if !fault.load(Ordering::Acquire) {
                                let result = worker.execution_store(&token, |store| {
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
        // Recheck fencing immediately before resuming. A failed binding or owner
        // check drops ProcessOwner and kills the still-suspended managed tree.
        let current = self.execution_store(&token, |store| store.get(&id))?;
        if self.stop.load(Ordering::Acquire) || current.cancel_requested {
            child.cleanup()?;
            child.drain_readers();
            self.execution_store(&token, |store| {
                store.finish_task_execution(
                    &token,
                    TaskStatus::Cancelled,
                    None,
                    "Command cancelled before resuming journal-bound process",
                    store::CleanupProof::ProcessTreeEmpty,
                )
            })?;
            return Ok(());
        }
        trace_process(&format!("task={id} resume.begin"), Some(child.id()));
        child
            .resume()
            .map_err(|error| format!("Could not resume journal-bound process: {error}"))?;
        trace_process(&format!("task={id} resume.complete"), Some(child.id()));
        let deadline = if self.store()?.application_for_task(&id)?.is_some() {
            None
        } else {
            Some(Instant::now() + Duration::from_millis(task.resolved_command.timeout_ms))
        };
        let (status, exit_code, reason) = loop {
            if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
                trace_process(
                    &format!("task={id} parent-exit code={:?}", exit.code()),
                    Some(child.id()),
                );
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
            let current = self.execution_store(&token, |store| store.get(&id))?;
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
        trace_process(
            &format!("task={id} cleanup.begin status={status:?}"),
            Some(child.id()),
        );
        // Also reap descendants which outlive a one-shot parent; they must not hold pipes open.
        child.cleanup()?;
        if child.drain_readers() {
            fault.store(true, Ordering::Release);
        }
        let (status, reason) = if fault.load(Ordering::Acquire) {
            (
                TaskStatus::Indeterminate,
                "Output could not be fully persisted; command outcome requires inspection".into(),
            )
        } else {
            (status, reason)
        };
        trace_process(&format!("task={id} outcome-commit.begin"), Some(child.id()));
        self.execution_store(&token, |store| {
            store.finish_task_execution(
                &token,
                status,
                exit_code,
                &reason,
                store::CleanupProof::ProcessTreeEmpty,
            )
        })?;
        trace_process(
            &format!("task={id} outcome-commit.complete"),
            Some(child.id()),
        );
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
            _ = interval.tick() => {
                if let Err(error) = worker.schedule() {
                    worker.record_journal_fault("scheduler", &error, "scheduling failed; new launches suspended");
                    eprintln!("WinBridge scheduling paused; IPC and accepted process owners remain active: {error}");
                }
            },
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
