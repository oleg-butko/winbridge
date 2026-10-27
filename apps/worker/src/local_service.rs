//! Same-state local verification service. No controller connection, dispatch,
//! coordinator registration, maintenance release or process-lifetime transfer.
use crate::{
    runtime_guard::RuntimeGuard,
    startup::{InstanceCustody, WorkerParts},
    store::{PreparedServiceJournal, ServiceJournalPreparationFailure},
    *,
};
use std::{path::Path, time::Instant};
use winbridge_protocol::update::WorkerServiceActivationCommand;

pub(crate) const READ_ONLY_REASON: &str = "Локальная проверка обновления: доступны только чтение журнала и диагностика; новые команды ещё не разрешены";

/// Immutable resident binding, not reconstructed from an observation request.
pub(super) struct LocalServiceScope {
    command: WorkerServiceActivationCommand,
    shutdown: winbridge_protocol::update::WorkerShutdownReceipt,
}

enum FailureOwner {
    Prepared(Box<PreparedServiceJournal>),
    Transfer(Box<ServiceJournalPreparationFailure>),
    Worker(Arc<Worker>),
}
/// Retains the exact state owner on failure, never a reusable startup grant.
pub struct LocalServiceFailure {
    reason: String,
    owner: FailureOwner,
    _runtime_guard: Option<RuntimeGuard>,
    _config_pin: Option<File>,
}
impl std::fmt::Debug for LocalServiceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalServiceFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for LocalServiceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for LocalServiceFailure {}
impl LocalServiceFailure {
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn revalidate_state(&self) -> Result<()> {
        match &self.owner {
            FailureOwner::Prepared(p) => p.revalidate_state(),
            FailureOwner::Transfer(e) => e.revalidate_state(),
            FailureOwner::Worker(w) => w._instance_lock.revalidate(),
        }
    }
    fn worker(reason: String, worker: Arc<Worker>) -> Box<Self> {
        Box::new(Self {
            reason,
            owner: FailureOwner::Worker(worker),
            _runtime_guard: None,
            _config_pin: None,
        })
    }
}

/// Constructed from a sealed Fresh transfer, not persisted JSON. Binding IPC is
/// a separate operation; neither operation confirms the updater's LocalVerified.
pub struct LocalService {
    worker: Arc<Worker>,
    command: WorkerServiceActivationCommand,
    shutdown: winbridge_protocol::update::WorkerShutdownReceipt,
    deadline: Instant,
    _config_pin: File,
}
pub struct RunningLocalService {
    service: LocalService,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
    exit: Option<Result<()>>,
}
impl LocalService {
    /// Trusted private receiver must supply the original graph/liveness/window
    /// checkpoint. RuntimeGuard is bound to this actual loaded process/image.
    pub fn prepare(
        prepared: PreparedServiceJournal,
        config_path: &Path,
        guard: RuntimeGuard,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Self, Box<LocalServiceFailure>> {
        if guard.identity() != &prepared.command().binding.expected_worker {
            return Err(Box::new(LocalServiceFailure {
                reason: "Local service loaded runtime differs from admission".into(),
                owner: FailureOwner::Prepared(Box::new(prepared)),
                _runtime_guard: Some(guard),
                _config_pin: None,
            }));
        }
        let result = Self::prepare_inner(prepared, config_path, || {
            guard.revalidate()?;
            checkpoint()
        });
        match result {
            Ok(service) => match service.worker.attach_runtime_guard(guard) {
                Ok(()) => Ok(service),
                Err(reason) => Err(LocalServiceFailure::worker(
                    reason,
                    Arc::clone(&service.worker),
                )),
            },
            Err(mut error) => {
                error._runtime_guard = Some(guard);
                Err(error)
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn prepare_fixture(
        prepared: PreparedServiceJournal,
        config: &Path,
        checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Self, Box<LocalServiceFailure>> {
        Self::prepare_inner(prepared, config, checkpoint)
    }
    fn prepare_inner(
        prepared: PreparedServiceJournal,
        config_path: &Path,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Self, Box<LocalServiceFailure>> {
        let config = (|| {
            winbridge_platform::require_safe_path(config_path).map_err(|e| e.to_string())?;
            winbridge_platform::require_local_disk(config_path).map_err(|e| e.to_string())?;
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(1);
            }
            let pin = options.open(config_path).map_err(|e| e.to_string())?;
            let (config, check) = compatibility::load_checked_config(
                config_path,
                &prepared
                    .command()
                    .binding
                    .writer_command
                    .binding
                    .worker_config_sha256,
            )?;
            if winbridge_platform::file_identity(&pin).map_err(|e| e.to_string())?
                != check.config.file_identity
                || config.instance_id
                    != prepared
                        .command()
                        .binding
                        .writer_command
                        .binding
                        .physical
                        .context
                        .instance_id
                || config
                    .state_directory
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .to_str()
                    != Some(prepared.state_observation().state_directory.as_str())
            {
                return Err("Local service config/instance/state differs from held state".into());
            }
            Worker::validate_startup_paths(&config, false)?;
            recovery::reject_normal_start(&config)?;
            checkpoint()?;
            Ok::<_, String>((
                config,
                pin,
                config_path.canonicalize().map_err(|e| e.to_string())?,
            ))
        })();
        let (config, pin, path) = match config {
            Ok(value) => value,
            Err(reason) => {
                return Err(Box::new(LocalServiceFailure {
                    reason,
                    owner: FailureOwner::Prepared(Box::new(prepared)),
                    _runtime_guard: None,
                    _config_pin: None,
                }))
            }
        };
        let state = prepared.into_runtime(&mut checkpoint).map_err(|error| {
            Box::new(LocalServiceFailure {
                reason: error.reason().into(),
                owner: FailureOwner::Transfer(error),
                _runtime_guard: None,
                _config_pin: None,
            })
        })?;
        let worker = Worker::from_parts(WorkerParts {
            config,
            config_path: Some(path),
            inspection: false,
            local_activation: Some(LocalServiceScope {
                command: state.command.clone(),
                shutdown: state.shutdown.clone(),
            }),
            generation: state
                .command
                .binding
                .writer_command
                .binding
                .physical
                .generation
                .clone(),
            store: state.store,
            custody: InstanceCustody::Activation {
                state: Box::new(state.state),
            },
        });
        let result = (|| {
            if worker
                .config
                .state_directory
                .join(domain::WITNESS)
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                domain::load(
                    &worker.config.state_directory,
                    &domain::context(&worker.config, &*worker.store()?),
                )?;
            }
            worker.initialize_startup(false, &mut checkpoint)?;
            worker.store()?.finish_service_initialization()?;
            checkpoint()?;
            worker.store()?.check_authority()?;
            worker._instance_lock.revalidate()
        })();
        if let Err(reason) = result {
            let mut failure = LocalServiceFailure::worker(reason, worker);
            failure._config_pin = Some(pin);
            return Err(failure);
        }
        Ok(Self {
            worker,
            command: state.command,
            shutdown: state.shutdown,
            deadline: state.deadline,
            _config_pin: pin,
        })
    }
    pub fn command(&self) -> &WorkerServiceActivationCommand {
        &self.command
    }
    pub fn endpoint(&self) -> Result<String> {
        winbridge_transport::endpoint(
            &self.worker.config.state_directory,
            &self.worker.config.instance_id,
        )
        .map_err(|e| e.to_string())
    }
    fn check_ready(&self) -> Result<()> {
        self.worker._instance_lock.revalidate()?;
        if let Some(guard) = self
            .worker
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .as_ref()
        {
            guard.revalidate()?;
        }
        self.worker.store()?.check_local_service_identity(
            &self.command,
            &self.shutdown,
            self.deadline,
        )
    }
    pub async fn start(self) -> std::result::Result<RunningLocalService, Box<LocalServiceFailure>> {
        if let Err(reason) = self.check_ready() {
            return Err(LocalServiceFailure::worker(
                reason,
                Arc::clone(&self.worker),
            ));
        }
        let (ready, observed) = tokio::sync::oneshot::channel();
        let worker = Arc::clone(&self.worker);
        let mut running = RunningLocalService {
            service: self,
            exit: None,
            task: Some(tokio::spawn(serve_inner(
                worker,
                ReadinessOutput::Local(ready),
            ))),
        };
        let bound = tokio::time::timeout_at(
            tokio::time::Instant::from_std(running.service.deadline),
            observed,
        )
        .await;
        if bound.is_err() {
            return Err(LocalServiceFailure::worker(
                "Original local-service startup window expired before IPC readiness".into(),
                Arc::clone(&running.service.worker),
            ));
        }
        if bound.expect("Checked deadline").is_err() {
            let task = running.task.take().expect("One local IPC owner");
            let reason = match task.await {
                Ok(Err(e)) => e,
                _ => "Local IPC exited before readiness".into(),
            };
            return Err(LocalServiceFailure::worker(
                reason,
                Arc::clone(&running.service.worker),
            ));
        }
        if let Err(reason) = running.service.check_ready() {
            return Err(LocalServiceFailure::worker(
                reason,
                Arc::clone(&running.service.worker),
            ));
        }
        Ok(running)
    }
}
impl RunningLocalService {
    pub fn endpoint(&self) -> Result<String> {
        self.service.endpoint()
    }
    /// Cancel-safe monitor. Consume the join result exactly once; close must
    /// not poll a completed JoinHandle again or discard an earlier IPC failure.
    pub(crate) async fn wait_until_stopped(&mut self) -> Result<()> {
        if let Some(exit) = &self.exit {
            return exit.clone();
        }
        let result = match self.task.as_mut().ok_or("Local IPC owner missing")?.await {
            Ok(result) => result,
            Err(error) => Err(format!("Local IPC task exit unconfirmed: {error}")),
        };
        self.task.take();
        self.exit = Some(result.clone());
        result
    }
    #[cfg(test)]
    pub(crate) fn stop_fixture(&self) {
        self.service.worker.stop();
    }
    pub async fn close(mut self) -> std::result::Result<(), Box<LocalServiceFailure>> {
        self.service.worker.stop();
        if let Err(reason) = self.wait_until_stopped().await {
            return Err(LocalServiceFailure::worker(
                reason,
                Arc::clone(&self.service.worker),
            ));
        }
        let deadline = Instant::now() + winbridge_transport::IPC_TIMEOUT + Duration::from_secs(1);
        while Arc::strong_count(&self.service.worker) > 1 {
            if Instant::now() >= deadline {
                return Err(LocalServiceFailure::worker(
                    "Local IPC handlers have not all stopped; no clean handoff".into(),
                    Arc::clone(&self.service.worker),
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }
}
impl Drop for RunningLocalService {
    fn drop(&mut self) {
        self.service.worker.stop();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
pub(crate) fn readonly_queue_result(result: &mut winbridge_protocol::QueueControlResult) {
    match result {
        winbridge_protocol::QueueControlResult::Observed { observation, .. }
        | winbridge_protocol::QueueControlResult::Applied { observation, .. } => {
            observation.read_only = true
        }
        _ => {}
    }
}

pub(crate) fn rejected_observation(
    message: String,
) -> winbridge_protocol::update::ActivationServiceObservation {
    use winbridge_protocol::update::*;
    ActivationServiceObservation {
        service_observation_version: 1,
        result: ActivationServiceObservationResult::Rejected { message },
    }
}
impl Worker {
    fn observe_service(
        &self,
        request: winbridge_protocol::update::ActivationServiceQuery,
    ) -> Result<winbridge_protocol::update::WorkerServiceActivationReceipt> {
        use winbridge_protocol::update::*;
        if self.stop.load(Ordering::Acquire) {
            return Err("Local service stopping; readiness unconfirmed".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No resident staged local service")?;
        if request.service_observation_version != 1 || request.command != scope.command {
            return Err("Service observation contract/binding differs; no rebind".into());
        }
        self._instance_lock.revalidate()?;
        let guard = self
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .clone()
            .ok_or("No actual loaded runtime guard; fixture metadata is not identity proof")?;
        guard.revalidate()?;
        if guard.identity() != &scope.command.binding.expected_worker {
            return Err("Service loaded identity changed".into());
        }
        let config = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?
            .clone()
            .ok_or("Service config path missing")?;
        let check = compatibility::check_config(
            &config,
            &scope
                .command
                .binding
                .writer_command
                .binding
                .worker_config_sha256,
        )?;
        if check.instance_id != self.config.instance_id
            || check.worker_config_schema != self.config.schema_version
        {
            return Err("Service config binding changed".into());
        }
        let updater_directory = Path::new(&guard.identity().image.path)
            .parent()
            .ok_or("Service runtime parent missing")?
            .join("update-state");
        let updater = winbridge_update::journal::UpdateJournal::inspect(&updater_directory)
            .map_err(|e| e.to_string())?;
        let store = self.store()?;
        let (authority, schema) =
            store.observe_local_service_binding(&scope.command, &scope.shutdown)?;
        let receipt = WorkerServiceActivationReceipt {
            schema_version: 1,
            command_id: scope.command.command_id.clone(),
            update_id: scope
                .command
                .binding
                .writer_command
                .binding
                .update_id
                .clone(),
            attempt_id: scope
                .command
                .binding
                .writer_command
                .binding
                .attempt_id
                .clone(),
            command_sha256: winbridge_update::activation::service::command_sha256(&scope.command)?,
            authority,
            compatibility: winbridge_protocol::WorkerRuntimeCompatibility {
                schema_version: 1,
                kind: "worker_runtime_compatibility".into(),
                identity: guard.identity().clone(),
                context: RequestContext {
                    instance_id: self.config.instance_id.clone(),
                    journal_incarnation: store.journal_authority().incarnation.clone(),
                },
                generation: self.generation.clone(),
                worker_config_sha256: check.config.sha256,
                worker_config_schema: self.config.schema_version,
                worker_journal_schema: schema,
                updater_journal_schema: updater.schema_version,
                protocol_version: PROTOCOL_VERSION,
                observed_at_ms: store::now_ms(),
            },
        };
        winbridge_update::activation::service::validate_receipt(&receipt, &scope.command)?;
        guard.revalidate()?;
        self._instance_lock.revalidate()?;
        Ok(receipt)
    }
    pub(crate) fn handle_service_observation(
        &self,
        request: winbridge_protocol::update::ActivationServiceQuery,
    ) -> winbridge_protocol::update::ActivationServiceObservation {
        use winbridge_protocol::update::*;
        match self.observe_service(request) {
            Ok(receipt) => ActivationServiceObservation {
                service_observation_version: 1,
                result: ActivationServiceObservationResult::Observed {
                    receipt: Box::new(receipt),
                },
            },
            Err(message) => rejected_observation(message),
        }
    }
}
