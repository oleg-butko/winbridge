//! Shared Worker initialization; state/authority acquisition remains separate.
use super::*;
use winbridge_update::shutdown::LockedShutdownState;

pub(super) enum InstanceCustody {
    Ordinary { _lock: File },
    Activation { state: Box<LockedShutdownState> },
}
impl InstanceCustody {
    pub(super) fn lock_identity(&self) -> Result<String> {
        match self {
            Self::Ordinary { _lock } => {
                winbridge_platform::file_identity(_lock).map_err(|e| e.to_string())
            }
            Self::Activation { state } => {
                state.revalidate().map_err(|e| e.to_string())?;
                Ok(state.observation().lock_identity.clone())
            }
        }
    }
    pub(super) fn revalidate(&self) -> Result<()> {
        match self {
            Self::Ordinary { .. } => Ok(()),
            Self::Activation { state } => state.revalidate().map_err(|e| e.to_string()),
        }
    }
}
pub(super) struct WorkerParts {
    pub config: WorkerConfig,
    pub config_path: Option<std::path::PathBuf>,
    pub inspection: bool,
    pub local_activation: Option<crate::local_service::LocalServiceScope>,
    pub generation: String,
    pub store: Store,
    pub custody: InstanceCustody,
}
impl Worker {
    pub(super) fn validate_startup_paths(config: &WorkerConfig, create: bool) -> Result<()> {
        config.validate()?;
        ensure_local_state(&config.state_directory)?;
        winbridge_platform::require_local_disk(&config.state_directory)
            .map_err(|e| e.to_string())?;
        for project in &config.projects {
            winbridge_platform::require_local_disk(&project.workspace_root)
                .map_err(|e| format!("project {}: {e}", project.id))?;
        }
        if !config.state_directory.exists() {
            if !create {
                return Err("Activated service requires existing stateDirectory".into());
            }
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
        Ok(())
    }
    pub(super) fn from_parts(parts: WorkerParts) -> Arc<Self> {
        let WorkerParts {
            config,
            config_path,
            inspection,
            local_activation,
            generation,
            store,
            custody,
        } = parts;
        let controller_connection = config.worker.controller_url.as_ref().map(|url| {
            winbridge_protocol::ControllerConnection {
                url: url.clone(),
                state: if local_activation.is_some() {
                    winbridge_protocol::ConnectionState::Disconnected
                } else {
                    winbridge_protocol::ConnectionState::Connecting
                },
                session_id: None,
                controller_node_id: None,
                controller_generation: None,
                last_confirmed_at_ms: None,
                error: local_activation.is_some().then(|| {
                    "Локальная проверка обновления: соединение с controller ещё не разрешено".into()
                }),
            }
        });
        Arc::new(Self {
            runtime_guard: Mutex::new(None),
            config_path: Mutex::new(config_path),
            maintenance: Mutex::new((0, None)),
            maintenance_running: AtomicBool::new(false),
            config,
            generation,
            store: Mutex::new(store),
            active_projects: Mutex::new(HashMap::new()),
            active_jobs: AtomicUsize::new(0),
            readiness_scans: Mutex::new(HashMap::new()),
            recovery_scan: Mutex::new(Instant::now()),
            scheduler_diagnostics: Mutex::new(scheduler_diagnostics::Diagnostics::new()),
            checks: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
            controller_connection: Mutex::new(controller_connection),
            controller_projection: Mutex::new(None),
            unreconciled_tasks: Mutex::new(HashMap::new()),
            inspection,
            local_activation,
            execution_domain: Mutex::new(None),
            update_control: Mutex::new(None),
            coverage: Mutex::new(None),
            _instance_lock: custody,
        })
    }
    pub(super) fn initialize_startup(
        self: &Arc<Self>,
        fresh_journal: bool,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        let mut check = || {
            self._instance_lock.revalidate()?;
            checkpoint()
        };
        check()?;
        {
            let mut store = self.store()?;
            if !self.inspection {
                store.recover_resource_leases()?;
                check()?;
                store.capture_legacy_resources().map_err(|e| {
                    format!("{e}; use --config PATH serve-recovery to inspect without dispatch")
                })?;
                check()?;
                store.reconcile_process_resources()?;
                check()?;
                store.recover()?;
            }
            store.configure_work_scheduling(self.config.work_scheduling.clone())?;
            store.configure_queue_policy(self.config.queue_policy.clone())?;
            store.configure_resource_scheduling(self.config.resource_scheduling.clone())?;
        }
        check()?;
        if !self.inspection {
            self.recover_imports()?;
            self.store()?.release_stopped_import_resources()?;
            check()?;
            let result = {
                let mut store = self.store()?;
                self.reconcile_applications(&mut store)
            };
            if let Err(error) = result {
                if self.local_activation.is_some() || self.journal_recovery_required()?.is_none() {
                    return Err(error);
                }
            }
            check()?;
            let (owner, info) = domain::initialize(&self.config, &*self.store()?, fresh_journal)?;
            *self
                .execution_domain
                .lock()
                .map_err(|_| "execution domain lock poisoned")? = owner;
            *self
                .coverage
                .lock()
                .map_err(|_| "execution coverage lock poisoned")? = Some(info);
        } else {
            let context = domain::context(&self.config, &*self.store()?);
            let info = domain::load(&self.config.state_directory, &context)
                .map(|witness| witness.info())
                .unwrap_or_else(|reason| winbridge_protocol::RecoveryCoverageInfo {
                    complete: false,
                    reason,
                });
            *self
                .coverage
                .lock()
                .map_err(|_| "execution coverage lock poisoned")? = Some(info);
        }
        check()
    }
    pub(super) fn open_inner(
        config: WorkerConfig,
        config_path: Option<std::path::PathBuf>,
        inspection: bool,
    ) -> Result<Arc<Self>> {
        Self::validate_startup_paths(&config, true)?;
        if !inspection {
            recovery::reject_normal_start(&config)?;
            update::reject_start(&config.state_directory)?;
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
        if !inspection {
            update::reject_start(&config.state_directory)?;
        }
        let db_path = config.state_directory.join("journal.sqlite");
        let fresh_journal = !db_path.try_exists().map_err(|e| e.to_string())?;
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
            recovery::latch_source(&config.state_directory)?;
            Store::open_read_only(&db_path)?
        } else {
            recovery::reject_normal_start(&config)?;
            let mut store = Store::open(&db_path)?;
            if config
                .state_directory
                .join(domain::WITNESS)
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                domain::load(&config.state_directory, &domain::context(&config, &store))?;
            }
            store.begin_authority()?;
            store
        };
        let worker = Self::from_parts(WorkerParts {
            config,
            config_path,
            inspection,
            local_activation: None,
            generation: uuid::Uuid::new_v4().to_string(),
            store,
            custody: InstanceCustody::Ordinary { _lock: lock },
        });
        worker.initialize_startup(fresh_journal, || Ok(()))?;
        Ok(worker)
    }
}
