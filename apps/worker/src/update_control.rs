//! Trusted worker launcher and a separate supervisor-only local channel.
//! No network/CLI/MCP route enables this module. Runtime replacement is absent.
use crate::{Result, Worker};
mod shutdown;
#[cfg(feature = "update-validation")]
pub mod validation;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use winbridge_platform::infrastructure;
use winbridge_protocol::{build_identity, update::*};
use winbridge_transport::supervisor as ipc;
use winbridge_update::{
    journal::{JournalErrorCode, UpdateJournal},
    supervisor::{Preparation, SupervisorHandle},
};

pub(crate) struct Registration {
    config_path: PathBuf,
    config_sha256: String,
    source: PathBuf,
    directory: PathBuf,
    scope: String,
    image_sha256: String,
    pub(crate) binding: SupervisorWorkerBinding,
    ready: AtomicBool,
    launching: AtomicBool,
    in_flight: AtomicUsize,
    #[cfg(test)]
    test_gate: std::sync::Mutex<Option<Arc<ControlTestGate>>>,
    #[cfg(test)]
    runtime_verified_gate: std::sync::Mutex<Option<Arc<ControlTestGate>>>,
    #[cfg(test)]
    drop_drain_ack: AtomicBool,
    #[cfg(any(test, feature = "update-validation"))]
    drop_shutdown_ack: AtomicBool,
    #[cfg(any(test, feature = "update-validation"))]
    shutdown_requests: AtomicUsize,
    #[cfg(any(test, feature = "update-validation"))]
    shutdown_test_gate: std::sync::Mutex<Option<Arc<ControlTestGate>>>,
    #[cfg(test)]
    begin_drain_requests: AtomicUsize,
}
#[cfg(any(test, feature = "update-validation"))]
struct ControlTestGate {
    entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
pub enum WorkerSupervisorLaunch {
    Started(Box<SupervisorHandle>),
    Existing {
        update: Box<UpdateRecord>,
        supervisor: Option<Box<SupervisorRecord>>,
    },
}
struct LaunchGuard(Arc<Registration>);
impl Drop for LaunchGuard {
    fn drop(&mut self) {
        self.0.launching.store(false, Ordering::Release);
    }
}
fn hash_config(path: &Path) -> Result<String> {
    use std::io::Read;
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Expected config file".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Config exceeds 64 KiB".into());
    }
    Ok(winbridge_update::sha256(&bytes))
}
impl Worker {
    /// Future installation policy calls this before serve; never a command payload.
    /// The updater image pin must come from the trusted installed release metadata.
    pub fn enable_update_control(
        &self,
        config_path: &Path,
        image_sha256: &str,
    ) -> Result<SupervisorWorkerBinding> {
        let mut slot = self
            .update_control
            .lock()
            .map_err(|_| "Update control lock poisoned")?;
        if slot.is_some() {
            return Err("Update control is already configured; no rebind".into());
        }
        if self.inspection
            || self.stop.load(Ordering::Acquire)
            || self.journal_recovery_required()?.is_some()
        {
            return Err("Update control unavailable during recovery/stop".into());
        }
        if !winbridge_core::update::valid_hash(image_sha256, &[64]) {
            return Err("Pinned installed updater SHA-256 required".into());
        }
        let cfg = winbridge_update::load_config(config_path)?;
        let runtime = cfg
            .runtime_directory
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let config_path = config_path.canonicalize().map_err(|e| e.to_string())?;
        if config_path != runtime.join("winbridge-update.local.json") {
            return Err("Updater config must be beside installed binaries".into());
        }
        let path = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?
            .clone()
            .ok_or("Update control requires Worker::open_file")?;
        if path.canonicalize().map_err(|e| e.to_string())?
            != cfg
                .worker_config_path
                .canonicalize()
                .map_err(|e| e.to_string())?
        {
            return Err("Updater points to another worker config".into());
        }
        let settings = self.runtime_settings()?;
        if settings.restart_required {
            return Err("ConfigRestartPending: restart before enabling update control".into());
        }
        let binding = SupervisorWorkerBinding {
            context: self.request_context()?,
            generation: self.generation.clone(),
            build: build_identity(),
            process: infrastructure::stamp(std::process::id()).map_err(|e| e.to_string())?,
            config_sha256: settings.saved_sha256,
            endpoint: ipc::endpoint(&self.config.state_directory, &self.generation)
                .map_err(|e| e.to_string())?,
        };
        let scope = winbridge_update::installation_scope(&cfg)?;
        let directory = runtime.join("update-state");
        UpdateJournal::open(&directory, &scope).map_err(|e| e.to_string())?;
        *slot = Some(Arc::new(Registration {
            config_sha256: hash_config(&config_path)?,
            config_path,
            source: cfg
                .source_directory
                .canonicalize()
                .map_err(|e| e.to_string())?,
            directory,
            scope,
            image_sha256: image_sha256.into(),
            binding: binding.clone(),
            ready: AtomicBool::new(false),
            launching: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            #[cfg(test)]
            test_gate: std::sync::Mutex::new(None),
            #[cfg(test)]
            runtime_verified_gate: std::sync::Mutex::new(None),
            #[cfg(test)]
            drop_drain_ack: AtomicBool::new(false),
            #[cfg(any(test, feature = "update-validation"))]
            drop_shutdown_ack: AtomicBool::new(false),
            #[cfg(any(test, feature = "update-validation"))]
            shutdown_requests: AtomicUsize::new(0),
            #[cfg(any(test, feature = "update-validation"))]
            shutdown_test_gate: std::sync::Mutex::new(None),
            #[cfg(test)]
            begin_drain_requests: AtomicUsize::new(0),
        }));
        Ok(binding)
    }
    fn control_registration(&self) -> Result<Arc<Registration>> {
        self.update_control
            .lock()
            .map_err(|_| "Update control lock poisoned")?
            .clone()
            .ok_or_else(|| {
                "Update control is not enabled by the trusted installation driver".into()
            })
    }
    /// Uses this worker's actual execution domain. No JSON can nominate a domain.
    pub async fn launch_update_supervisor(
        &self,
        request: &UpdateRequest,
    ) -> Result<WorkerSupervisorLaunch> {
        self.launch_supervisor_inner(request, false).await
    }
    async fn launch_supervisor_inner(
        &self,
        request: &UpdateRequest,
        fixture: bool,
    ) -> Result<WorkerSupervisorLaunch> {
        let reg = self.control_registration()?;
        // Historical lookup needs neither owner lock nor fresh effect permission.
        if let Some(update) =
            UpdateJournal::find_request(&reg.directory, request).map_err(|e| e.to_string())?
        {
            let supervisor =
                match UpdateJournal::inspect_supervisor(&reg.directory, &update.update_id) {
                    Ok(r) => Some(Box::new(r)),
                    Err(e) if e.code == JournalErrorCode::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                };
            return Ok(WorkerSupervisorLaunch::Existing {
                update: Box::new(update),
                supervisor,
            });
        }
        if !reg.ready.load(Ordering::Acquire) {
            return Err("Worker update-control listener is not ready".into());
        }
        if reg
            .launching
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(
                "Worker supervisor launch is already in progress; observe, do not spawn again"
                    .into(),
            );
        }
        let _launch = LaunchGuard(reg.clone());
        self.require_control_current(&reg)?;
        let cfg = winbridge_update::load_config(&reg.config_path)?;
        // A test-only caller in this module may remain in its enclosing validation
        // Job. Production never permits an unverified external Job/environment.
        #[cfg(not(any(test, feature = "update-validation")))]
        if fixture {
            return Err("Test environment override unavailable".into());
        }
        if !fixture {
            if infrastructure::job_observation(&reg.binding.process).map_err(|e| e.to_string())?
                == Some(true)
            {
                return Err("Supervisor launch deferred: worker belongs to an unverified external Job; no breakaway permitted".into());
            }
            let filename = if cfg!(windows) {
                "winbridge-worker.exe"
            } else {
                "winbridge-worker"
            };
            if std::env::current_exe()
                .map_err(|e| e.to_string())?
                .canonicalize()
                .map_err(|e| e.to_string())?
                != cfg
                    .runtime_directory
                    .join(filename)
                    .canonicalize()
                    .map_err(|e| e.to_string())?
            {
                return Err("Worker must run from its fixed installed runtime".into());
            }
        }
        let mut journal =
            UpdateJournal::open(&reg.directory, &reg.scope).map_err(|e| e.to_string())?;
        let owner = journal.acquire_owner().map_err(|e| e.to_string())?;
        let installed = journal.info().map_err(|e| e.to_string())?;
        let baseline = winbridge_workspace_sync::source_tree::read_manifest(&reg.source)?.1;
        let observed = UpdateInstallationPin {
            installation_id: installed.installation_id,
            scope_sha256: reg.scope.clone(),
            worker_build_id: reg.binding.build.build_id.clone(),
            worker_config_sha256: reg.binding.config_sha256.clone(),
            baseline_manifest_sha256: baseline,
        };
        let update = journal
            .accept(&owner, request, &observed)
            .map_err(|e| e.to_string())?;
        let mut handle = {
            let domain = self
                .execution_domain
                .lock()
                .map_err(|_| "Execution domain lock poisoned")?;
            let preparation = winbridge_update::supervisor::prepare_for_worker(
                &mut journal,
                &owner,
                &update.update_id,
                &reg.config_path,
                &reg.image_sha256,
                domain.as_ref(),
                reg.binding.clone(),
            )?;
            let Preparation::New(p) = preparation else {
                return Err("Launch was already prepared; inspect it, no replay".into());
            };
            p.spawn(&mut journal, owner, domain.as_ref())?
        };
        handle.wait_ready().await?;
        handle.command(SupervisorAction::Handoff).await?;
        Ok(WorkerSupervisorLaunch::Started(Box::new(handle)))
    }
    fn require_control_current(&self, reg: &Registration) -> Result<()> {
        self.require_control_config_current(reg)?;
        if self.config.resource_scheduling.is_some() {
            return Err("Named-resource update reservations are not implemented; source/build grant deferred".into());
        }
        Ok(())
    }
    fn require_control_config_current(&self, reg: &Registration) -> Result<()> {
        if self.inspection
            || self.stop.load(Ordering::Acquire)
            || self.journal_recovery_required()?.is_some()
        {
            return Err("Worker update effects unavailable during recovery/stop".into());
        }
        let settings = self.runtime_settings()?;
        if settings.restart_required
            || settings.saved_sha256 != reg.binding.config_sha256
            || hash_config(&reg.config_path)? != reg.config_sha256
        {
            return Err("ConfigRestartPending: update bindings changed; no new grant".into());
        }
        Ok(())
    }
    fn control_authority(
        &self,
        reg: &Registration,
        request: &WorkerUpdateControlRequest,
        peer: u32,
    ) -> Result<UpdateRecord> {
        let version_matches = request.control_version == request.command.required_version();
        if !version_matches
            || request.binding != reg.binding
            || request.binding.generation != self.generation
        {
            return Err(
                "Worker update contract/build/context/generation changed; no rebind".into(),
            );
        }
        let (info, sup, update, current_owner) =
            UpdateJournal::supervisor_authority_snapshot(&reg.directory, &request.launch_id)
                .map_err(|e| e.to_string())?;
        if sup.spec.update_id != request.update_id
            || sup.spec.worker.as_ref() != Some(&reg.binding)
            || sup.phase != SupervisorPhase::Active
            || sup.process.as_ref() != Some(&request.supervisor)
            || peer != request.supervisor.process_id
            || !infrastructure::matches(&request.supervisor).map_err(|e| e.to_string())?
            || sup.owner_id.as_deref() != Some(request.updater_owner_id.as_str())
            || sup.owner_epoch != Some(request.updater_epoch)
            || current_owner != request.updater_owner_id
            || info.owner_epoch != request.updater_epoch
            || info.scope_sha256 != reg.scope
            || info.installation_id != update.request.expected.installation_id
            || info.scope_sha256 != update.request.expected.scope_sha256
            || update.request.expected.worker_build_id != reg.binding.build.build_id
            || update.request.expected.worker_config_sha256 != reg.binding.config_sha256
            || sup.spec.config_sha256 != reg.config_sha256
            || !matches!(
                request.command,
                WorkerUpdateControlCommand::Observe
                    | WorkerUpdateControlCommand::ObserveShutdownReceipt { .. }
            ) && matches!(
                update.phase,
                UpdatePhase::Committed
                    | UpdatePhase::Deferred
                    | UpdatePhase::FailedBeforeCutover
                    | UpdatePhase::ManualIntervention
            )
        {
            return Err(
                "Only the exact active supervisor/current updater epoch can control this worker"
                    .into(),
            );
        }
        Ok(update)
    }
    fn control_result(
        &self,
        reg: &Registration,
        request: &WorkerUpdateControlRequest,
        peer: u32,
    ) -> Result<WorkerUpdateControlResult> {
        if request.command.required_version() == 6 {
            return shutdown::observation(self, reg, request, peer);
        }
        if request.command == WorkerUpdateControlCommand::ObserveDrain {
            if request.binding.context != self.request_context()? {
                return Err("Worker journal context changed; no drain observation rebind".into());
            }
            let update = self.control_authority(reg, request, peer)?;
            let active = self
                .active_projects
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            let checks = self
                .checks
                .lock()
                .map_err(|_| "Readiness check lock poisoned")?
                .len() as u64;
            let in_flight = self.active_jobs.load(Ordering::Acquire) as u64
                + active.len() as u64
                + checks
                + u64::from(self.maintenance_running.load(Ordering::Acquire));
            let view = self.store()?.worker_drain_observation(
                &update.update_id,
                reg.binding.context.clone(),
                &self.generation,
                in_flight,
            )?;
            if let Some(lease) = &view.lease {
                require_saved_spec(reg, request, &update, &lease.spec)?;
            }
            return Ok(WorkerUpdateControlResult::DrainObservation {
                observation: Box::new(view),
            });
        }
        if let WorkerUpdateControlCommand::ObserveReceipt { command } = &request.command {
            if request.binding.context != self.request_context()? {
                return Err("Worker journal context changed; no receipt rebind".into());
            }
            let update = self.control_authority(reg, request, peer)?;
            let store = self.store()?;
            let receipt = if let Some(lease) = store.worker_update_observation(&update.update_id)? {
                let spec = &lease.spec;
                require_saved_spec(reg, request, &update, spec)?;
                store.worker_update_receipt(spec, command)?
            } else {
                None
            };
            return Ok(WorkerUpdateControlResult::CommandReceipt {
                command: command.clone(),
                receipt: receipt.map(Box::new),
                observed_at_ms: crate::store::now_ms(),
            });
        }
        if !matches!(
            request.command,
            WorkerUpdateControlCommand::ObserveRuntime
                | WorkerUpdateControlCommand::ObserveCompatibility
        ) {
            return self.control(reg, request, peer).map(|lease| {
                WorkerUpdateControlResult::Snapshot {
                    lease: lease.map(Box::new),
                }
            });
        }
        if request.binding.context != self.request_context()? {
            return Err("Worker journal context changed; no rebind".into());
        }
        let update = self.control_authority(reg, request, peer)?;
        self.require_control_config_current(reg)?;
        // Release the short guard mutex before file hashing. No Store mutex is held.
        let guard = self
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .clone()
            .ok_or("Bootstrap required: loaded runtime guard is not attached")?;
        guard.require_installation(&update.request.expected.installation_id, &reg.scope)?;
        let identity = guard.identity();
        if identity.process != reg.binding.process || identity.descriptor.build != reg.binding.build
        {
            return Err(
                "Loaded runtime process/build differs from immutable worker binding".into(),
            );
        }
        guard.revalidate()?;
        #[cfg(test)]
        if let Some(gate) = reg.runtime_verified_gate.lock().unwrap().take() {
            if let Some(entered) = gate.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            gate.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|e| format!("Runtime fixture checkpoint stopped: {e}"))?;
        }
        self.control_authority(reg, request, peer)?;
        self.require_control_config_current(reg)?;
        if request.binding.context != self.request_context()? {
            return Err("Worker journal context changed during runtime observation".into());
        }
        if request.command == WorkerUpdateControlCommand::ObserveCompatibility {
            let schema: u32 = self
                .store()?
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            if schema != crate::store::JOURNAL_SCHEMA_VERSION {
                return Err("Loaded worker journal schema changed; no compatibility grant".into());
            }
            let updater = UpdateJournal::inspect(&reg.directory).map_err(|e| e.to_string())?;
            self.control_authority(reg, request, peer)?;
            self.require_control_config_current(reg)?;
            return Ok(WorkerUpdateControlResult::RuntimeCompatibility {
                compatibility: Box::new(winbridge_protocol::WorkerRuntimeCompatibility {
                    schema_version: 1,
                    kind: "worker_runtime_compatibility".into(),
                    identity: identity.clone(),
                    context: reg.binding.context.clone(),
                    generation: self.generation.clone(),
                    worker_config_sha256: reg.binding.config_sha256.clone(),
                    worker_config_schema: self.config.schema_version,
                    worker_journal_schema: schema,
                    updater_journal_schema: updater.schema_version,
                    protocol_version: winbridge_protocol::PROTOCOL_VERSION,
                    observed_at_ms: crate::store::now_ms(),
                }),
            });
        }
        Ok(WorkerUpdateControlResult::RuntimeIdentity {
            identity: Box::new(identity.clone()),
        })
    }
    fn control(
        &self,
        reg: &Registration,
        request: &WorkerUpdateControlRequest,
        peer: u32,
    ) -> Result<Option<WorkerUpdateLease>> {
        if request.binding.context != self.request_context()? {
            return Err("Worker journal context changed; no rebind".into());
        }
        let update = self.control_authority(reg, request, peer)?;
        let spec = self.update_source_spec(
            &update.update_id,
            &request.updater_owner_id,
            request.updater_epoch,
            &reg.source,
            (
                &update.request.expected.baseline_manifest_sha256,
                &update.request.source.manifest_sha256,
            ),
        )?;
        match &request.command {
            WorkerUpdateControlCommand::Observe => self.store()?.existing_worker_update(&spec),
            WorkerUpdateControlCommand::ObserveRuntime
            | WorkerUpdateControlCommand::ObserveCompatibility
            | WorkerUpdateControlCommand::ObserveReceipt { .. }
            | WorkerUpdateControlCommand::ObserveDrain
            | WorkerUpdateControlCommand::ObserveShutdownState
            | WorkerUpdateControlCommand::RequestShutdown { .. }
            | WorkerUpdateControlCommand::ObserveShutdownReceipt { .. } => {
                Err("Runtime observation is not a lease request".into())
            }
            WorkerUpdateControlCommand::BeginSource => {
                if let Some(lease) = self.store()?.existing_worker_update(&spec)? {
                    return Ok(Some(lease));
                }
                if !matches!(
                    update.phase,
                    UpdatePhase::Requested | UpdatePhase::Preparing
                ) {
                    return Err("New source lease requires Requested/Preparing updater; no implicit reacquisition".into());
                }
                self.require_control_current(reg)?;
                self.begin_source_update(spec).map(Some)
            }
            WorkerUpdateControlCommand::Advance { command } => {
                // Existing receipts are observations; a new grant needs fresh config.
                if let Some(receipt) = self.store()?.worker_update_receipt(&spec, command)? {
                    return Ok(Some(receipt));
                }
                require_parent_phase(update.phase, command.action)?;
                require_abort_proof(reg, request, &spec, update.phase, command)?;
                if command.action != WorkerUpdateAction::Release {
                    self.require_control_current(reg)?;
                }
                self.advance_source_update_checked(&spec, command, &|| {
                    // File verification releases Store. Recheck the external
                    // authority/phase/config before the final grant transaction.
                    let fresh = self.control_authority(reg, request, peer)?;
                    require_parent_phase(fresh.phase, command.action)?;
                    require_abort_proof(reg, request, &spec, fresh.phase, command)?;
                    if command.action != WorkerUpdateAction::Release {
                        self.require_control_current(reg)?;
                    }
                    Ok(())
                })
                .map(Some)
            }
        }
    }
}

fn require_saved_spec(
    reg: &Registration,
    request: &WorkerUpdateControlRequest,
    update: &UpdateRecord,
    spec: &WorkerUpdateSpec,
) -> Result<()> {
    if spec.instance_id != reg.binding.context.instance_id
        || spec.journal_incarnation != reg.binding.context.journal_incarnation
        || spec.worker_generation != reg.binding.generation
        || spec.updater_owner_id != request.updater_owner_id
        || spec.updater_epoch != request.updater_epoch
        || std::path::Path::new(&spec.source_directory) != reg.source
        || spec.baseline_manifest_sha256 != update.request.expected.baseline_manifest_sha256
        || spec.target_manifest_sha256 != update.request.source.manifest_sha256
    {
        return Err("Saved worker command belongs to another source/scope/owner".into());
    }
    Ok(())
}
fn require_abort_proof(
    reg: &Registration,
    request: &WorkerUpdateControlRequest,
    spec: &WorkerUpdateSpec,
    phase: UpdatePhase,
    command: &WorkerUpdateCommand,
) -> Result<()> {
    if phase == UpdatePhase::Aborting
        && (command.action != WorkerUpdateAction::Release
            || !UpdateJournal::abort_release_authorized(
                &reg.directory,
                &request.update_id,
                &request.updater_owner_id,
                request.updater_epoch,
                spec,
                command,
            )
            .map_err(|e| e.to_string())?)
    {
        return Err(
            "Abort release requires a durable matching source/compiler cleanup receipt".into(),
        );
    }
    Ok(())
}

fn require_parent_phase(phase: UpdatePhase, action: WorkerUpdateAction) -> Result<()> {
    match action {
        WorkerUpdateAction::AcquireSource
            if !matches!(phase, UpdatePhase::Requested | UpdatePhase::Preparing) =>
        {
            return Err("Source grant requires Requested/Preparing updater".into());
        }
        WorkerUpdateAction::BeginDrain
        | WorkerUpdateAction::CloseDispatch
        | WorkerUpdateAction::ConfirmQuiesced
            if phase != UpdatePhase::Draining =>
        {
            return Err("Worker drain/closure requires committed updater Draining intent after candidate verification".into());
        }
        WorkerUpdateAction::Release
            if !matches!(
                phase,
                UpdatePhase::Requested
                    | UpdatePhase::Preparing
                    | UpdatePhase::Aborting
                    | UpdatePhase::CandidateVerified
                    | UpdatePhase::Draining
                    | UpdatePhase::Quiesced
            ) =>
        {
            return Err(
                "Source/compiler/cutover outcome must be reconciled before worker lease release"
                    .into(),
            );
        }
        _ => {}
    }
    Ok(())
}

pub(crate) struct ControlServer {
    task: tokio::task::JoinHandle<()>,
    reg: Arc<Registration>,
}
impl Drop for ControlServer {
    fn drop(&mut self) {
        self.reg.ready.store(false, Ordering::Release);
        self.task.abort();
    }
}
impl ControlServer {
    pub(crate) async fn close(mut self) {
        self.reg.ready.store(false, Ordering::Release);
        self.task.abort();
        let _ = (&mut self.task).await;
        // spawn_blocking cannot be aborted. Do not claim instance-lock release
        // while its authority callback still owns the worker.
        while self.reg.in_flight.load(Ordering::Acquire) != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}
pub(crate) fn start(worker: Arc<Worker>) -> Result<Option<ControlServer>> {
    let Some(reg) = worker
        .update_control
        .lock()
        .map_err(|_| "Update control lock poisoned")?
        .clone()
    else {
        return Ok(None);
    };
    let mut listener = ipc::Listener::bind(&reg.binding.endpoint).map_err(|e| e.to_string())?;
    let state = reg.clone();
    reg.ready.store(true, Ordering::Release);
    let task = tokio::spawn(async move {
        struct Ready(Arc<Registration>);
        impl Drop for Ready {
            fn drop(&mut self) {
                self.0.ready.store(false, Ordering::Release);
            }
        }
        let _ready = Ready(state.clone());
        loop {
            let (mut stream, peer) = match listener.accept().await {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    continue;
                }
                Err(e) => {
                    eprintln!("Worker update control unavailable: {e}");
                    break;
                }
            };
            let request: WorkerUpdateControlRequest = match ipc::read(&mut stream).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let (binding, launch_id, update_id) = (
                state.binding.clone(),
                request.launch_id.clone(),
                request.update_id.clone(),
            );
            let (w, r) = (worker.clone(), state.clone());
            #[cfg(any(test, feature = "update-validation"))]
            let drop_ack = {
                #[cfg(test)]
                if matches!(&request.command, WorkerUpdateControlCommand::Advance {command} if command.action == WorkerUpdateAction::BeginDrain)
                {
                    state.begin_drain_requests.fetch_add(1, Ordering::AcqRel);
                    state.drop_drain_ack.swap(false, Ordering::AcqRel)
                } else {
                    shutdown_validation_delivery(&state, &request.command)
                }
                #[cfg(not(test))]
                shutdown_validation_delivery(&state, &request.command)
            };
            let control_version = if (1..=6).contains(&request.control_version) {
                request.control_version
            } else {
                1
            };
            struct Callback(Arc<Registration>);
            impl Drop for Callback {
                fn drop(&mut self) {
                    self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
                }
            }
            state.in_flight.fetch_add(1, Ordering::AcqRel);
            let callback = Callback(state.clone());
            // Sequential authority, blocking verification off the async reactor.
            let result = tokio::task::spawn_blocking(move || {
                let _callback = callback;
                let worker = w;
                let registration = r;
                #[cfg(test)]
                if matches!(
                    request.command,
                    WorkerUpdateControlCommand::BeginSource
                        | WorkerUpdateControlCommand::ObserveRuntime
                        | WorkerUpdateControlCommand::ObserveCompatibility
                ) {
                    let gate = registration.test_gate.lock().unwrap().take();
                    if let Some(gate) = gate {
                        if let Some(entered) = gate.entered.lock().unwrap().take() {
                            let _ = entered.send(());
                        }
                        gate.release
                            .lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .map_err(|e| format!("Fixture control gate stopped: {e}"))?;
                    }
                }
                let reply = shutdown::prepare_reply(worker, &registration, &request, peer)?;
                #[cfg(any(test, feature = "update-validation"))]
                if matches!(
                    request.command,
                    WorkerUpdateControlCommand::RequestShutdown { .. }
                ) {
                    let gate = registration.shutdown_test_gate.lock().unwrap().take();
                    if let Some(gate) = gate {
                        if let Some(entered) = gate.entered.lock().unwrap().take() {
                            let _ = entered.send(());
                        }
                        gate.release
                            .lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .map_err(|e| format!("Shutdown validation checkpoint stopped: {e}"))?;
                    }
                }
                Ok::<_, String>(reply)
            })
            .await;
            let result = match result {
                Ok(Ok(reply)) => reply,
                Ok(Err(message)) => shutdown::ControlReply::error(message),
                Err(e) => shutdown::ControlReply::error(format!(
                    "Worker control outcome unconfirmed: {e}; observe original command"
                )),
            };
            let shutdown::ControlReply {
                result,
                stop_after_reply,
            } = result;
            let response = WorkerUpdateControlResponse {
                control_version,
                binding,
                launch_id,
                update_id,
                result,
            };
            // Native fault fixture: apply and retain receipt, then close the
            // channel without its ACK. Production builds have no such hook.
            #[cfg(any(test, feature = "update-validation"))]
            if drop_ack {
                if stop_after_reply.is_some() {
                    // Make the lost-shutdown-ACK case exercise the cold path:
                    // retire this listener before any live receipt lookup.
                    drop(stop_after_reply);
                    break;
                }
                continue;
            }
            let _ = ipc::write(&mut stream, &response).await;
            // Also runs when the reply is abandoned: acceptance already committed.
            drop(stop_after_reply);
        }
    });
    Ok(Some(ControlServer { task, reg }))
}

#[cfg(any(test, feature = "update-validation"))]
fn shutdown_validation_delivery(reg: &Registration, command: &WorkerUpdateControlCommand) -> bool {
    if matches!(command, WorkerUpdateControlCommand::RequestShutdown { .. }) {
        reg.shutdown_requests.fetch_add(1, Ordering::AcqRel);
        reg.drop_shutdown_ack.swap(false, Ordering::AcqRel)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[path = "health_control_cases.rs"]
    mod health;
    #[path = "runtime_control_cases.rs"]
    mod runtime;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    use winbridge_protocol::*;

    const BUILD_ACTOR_SCRIPT: &str = r#"[CmdletBinding()]
param([string]$SourceDirectory,[string]$ManifestSha256,[string]$StagingDirectory,[string]$VerifierExecutable)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$target=Join-Path $SourceDirectory 'target'
New-Item -ItemType Directory -Path $target -Force | Out-Null
[IO.File]::AppendAllText((Join-Path $target 'actor-runs.log'),"$PID`n")
Write-Output 'compiler actor stdout Привет ✓'
[Console]::Error.WriteLine('compiler actor diagnostic')
Start-Sleep -Milliseconds 1500
New-Item -ItemType Directory -Path $StagingDirectory | Out-Null
[IO.File]::WriteAllText((Join-Path $StagingDirectory 'fixture.json'),($PSBoundParameters | ConvertTo-Json -Compress))
[IO.File]::WriteAllText((Join-Path $StagingDirectory 'candidate-report.json'),'{"kind":"compiler-report-fixture","candidateVerified":false,"runtimeApplied":false}')
"#;
    // Real tiny executables validate metadata/process/report interoperability.
    // They are explicitly fixture artifacts, never replacements for WinBridge.
    const ARTIFACT_ACTOR_SCRIPT: &str = r##"[CmdletBinding()]
param([string]$SourceDirectory,[string]$ManifestSha256,[string]$StagingDirectory,[string]$VerifierExecutable)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$OutputEncoding=[Text.UTF8Encoding]::new($false)
$target=Join-Path $SourceDirectory 'target'
New-Item -ItemType Directory -Path $target -Force | Out-Null
[IO.File]::AppendAllText((Join-Path $target 'actor-runs.log'),"$PID`n")
New-Item -ItemType Directory -Path $StagingDirectory | Out-Null
$payload=Join-Path $StagingDirectory 'payload'
New-Item -ItemType Directory -Path $payload | Out-Null
$verifiedText=& $VerifierExecutable verify-source $SourceDirectory $ManifestSha256
if ($LASTEXITCODE -ne 0) { throw 'Fixture source verification failed' }
$verified=($verifiedText -join "`n") | ConvertFrom-Json
$os=if ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::Windows)) {'windows'} else {'linux'}
$arch=switch ([Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()) {'X64' {'x86_64'} 'Arm64' {'aarch64'} default {throw 'Unsupported fixture architecture'}}
$build=@{version='0.1.0';sourceCommit=$verified.sourceCommit;sourceDirty=$false;sourceManifestSha256=$ManifestSha256;buildId='artifact-actor-fixture'}
$commands=@(); $artifacts=@(); $queries=@()
$rustc=(Get-Command rustc -ErrorAction Stop).Source
foreach ($role in @('worker','fixture','desktop','updater')) {
  $name=switch ($role) {'updater' {'winbridge-update.exe'} default {"winbridge-$role.exe"}}
  $descriptor=@{schemaVersion=1;kind='winbridge_artifact';queryContract=1;role=$role;build=$build;targetOs=$os;targetArch=$arch} | ConvertTo-Json -Depth 8 -Compress
  $rust='fn main(){std::hint::black_box(b"\0WinBridge.ArtifactQuery.contract-1\0");let args:Vec<_>=std::env::args().skip(1).collect();if args!=["--artifact-info"]{std::process::exit(2)};println!("{}",r#"'+$descriptor+'"#);}'
  $file=Join-Path $target "artifact-$role.rs"
  [IO.File]::WriteAllText($file,$rust)
  $image=Join-Path $payload $name
  $compiled=Join-Path $target $name
  & $rustc --edition=2021 $file -o $compiled 2>&1 | ForEach-Object { Write-Output $_ }
  $code=$LASTEXITCODE
  $commands+=@{name="compile-$role";executable=$rustc;arguments=@('--edition=2021',$file,'-o',$compiled);exitCode=$code;log="compile-$role.log"}
  if ($code -ne 0) {throw "Fixture Rust compile failed: $role"}
  # Windows rustc/linker may emit auxiliary files. Only the four independent
  # exe copies belong to payload; preserve build outputs under ignored target.
  [IO.File]::Copy($compiled,$image,$false)
  $text=& $VerifierExecutable artifact-query $image $role
  if ($LASTEXITCODE -ne 0) {throw "Fixture query failed: $role"}
  $query=($text -join "`n") | ConvertFrom-Json
  $commands+=@{name="identity-$role";executable=$VerifierExecutable;arguments=@('artifact-query',$image,$role);exitCode=0;log="identity-$role.log"}
  $artifacts+=@{name=$name;bytes=$query.image.bytes;sha256=$query.image.sha256}
  $queries+=@{name=$name;query=$query}
}
$report=@{schemaVersion=1;kind='winbridge-update-candidate';status='built';runtimeApplied=$false;candidateVerified=$false;sourceDirectory=$SourceDirectory;manifestSha256=$ManifestSha256;storage=@{};commands=$commands;artifacts=$artifacts;error=$null;source=$verified;workerBuild=$build;artifactIdentities=$queries;artifactIdentityChecked=$true}
[IO.File]::WriteAllText((Join-Path $StagingDirectory 'candidate-report.json'),($report | ConvertTo-Json -Depth 24))
Write-Output 'Four fixture executables built and queried; no WinBridge runtime health claimed'
"##;
    struct TargetPackage {
        archive: PathBuf,
        pin: UpdateSourcePin,
    }
    struct Fixture {
        worker: Arc<Worker>,
        server: tokio::task::JoinHandle<Result<()>>,
        supervisor: Option<Box<SupervisorHandle>>,
        request: UpdateRequest,
        config: PathBuf,
        updater_config: PathBuf,
        baseline: String,
        _root: Option<tempfile::TempDir>,
    }
    impl Fixture {
        async fn new(actual: Option<PathBuf>) -> Self {
            Self::with_source_policy(actual, false).await
        }
        async fn with_source_policy(actual: Option<PathBuf>, enabled: bool) -> Self {
            Self::with_build_policy(actual, enabled, None).await
        }
        async fn with_build_policy(
            actual: Option<PathBuf>,
            enabled: bool,
            powershell: Option<PathBuf>,
        ) -> Self {
            Self::with_build_script(actual, enabled, powershell, BUILD_ACTOR_SCRIPT).await
        }
        async fn with_build_script(
            actual: Option<PathBuf>,
            enabled: bool,
            powershell: Option<PathBuf>,
            build_script: &str,
        ) -> Self {
            Self::with_build_script_at(actual, enabled, powershell, build_script, None, None).await
        }
        async fn with_build_script_at(
            actual: Option<PathBuf>,
            enabled: bool,
            powershell: Option<PathBuf>,
            build_script: &str,
            pinned_location: Option<(PathBuf, PathBuf)>,
            target_package: Option<TargetPackage>,
        ) -> Self {
            let owned_root = if pinned_location.is_some() {
                None
            } else {
                Some(if enabled {
                    // Keep the 6 GiB guard on local disk and Unix sockets below
                    // sockaddr_un's path limit; /tmp may be a small tmpfs.
                    #[cfg(windows)]
                    let parent = std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .to_owned();
                    #[cfg(not(windows))]
                    let parent = PathBuf::from("/var/tmp");
                    tempfile::tempdir_in(parent).unwrap()
                } else {
                    tempfile::tempdir().unwrap()
                })
            };
            let root = pinned_location
                .as_ref()
                .map(|(p, _)| p.as_path())
                .unwrap_or_else(|| owned_root.as_ref().unwrap().path());
            let source = actual.clone().unwrap_or_else(|| root.join("source"));
            let runtime = root.join("runtime");
            let other = root.join("other");
            for p in [&runtime, &other] {
                if p == &runtime && pinned_location.is_some() {
                    continue;
                }
                fs::create_dir(p).unwrap();
            }
            if actual.is_none() {
                fs::create_dir(&source).unwrap();
            }
            let cargo = b"[package]\nname='control-fixture'\nversion='0.1.0'\n";
            if actual.is_none() {
                fs::write(source.join("Cargo.toml"), cargo).unwrap();
            }
            let manifest = SourceManifest {
                schema_version: 1,
                source_commit: "a".repeat(40),
                dirty: false,
                created_at: "2026-10-17T00:00:00Z".into(),
                includes_built_ui: false,
                files: vec![SourceFile {
                    path: "Cargo.toml".into(),
                    bytes: cargo.len() as u64,
                    sha256: winbridge_update::sha256(cargo),
                }],
            };
            let bytes = if actual.is_none() {
                let b = serde_json::to_vec(&manifest).unwrap();
                fs::write(source.join("winbridge-source-manifest.json"), &b).unwrap();
                b
            } else {
                fs::read(source.join("winbridge-source-manifest.json")).unwrap()
            };
            let manifest: SourceManifest = serde_json::from_slice(&bytes).unwrap();
            let baseline = winbridge_update::sha256(&bytes);
            let config = runtime.join("winbridge-worker.local.json");
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"instanceId":format!("control-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"workScheduling":{"maxConcurrentWork":1},"projects":[{"id":"source","workspaceRoot":source,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]},{"id":"other","workspaceRoot":other,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
            let updater_config = runtime.join("winbridge-update.local.json");
            let mut update_config = serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":config});
            let source_pin = if enabled {
                use std::io::Write;
                let packages = root.join("packages");
                fs::create_dir(&packages).unwrap();
                update_config["schemaVersion"] = serde_json::json!(2);
                update_config["sourcePackagesDirectory"] = serde_json::json!(packages);
                if let Some(package) = target_package {
                    let executable = powershell.expect("Explicit private build adapter required");
                    update_config["schemaVersion"] = serde_json::json!(3);
                    update_config["build"] =
                        serde_json::json!({"powershellExecutable":executable,"timeoutMs":300000});
                    assert_eq!(
                        winbridge_update::sha256(&fs::read(&package.archive).unwrap()),
                        package.pin.archive_sha256
                    );
                    fs::copy(&package.archive, packages.join(&package.pin.archive_name)).unwrap();
                    Some(package.pin)
                } else {
                    let next = b"[package]\nname='control-fixture-updated'\nversion='0.2.0'\n";
                    let mut files = vec![SourceFile {
                        path: "Cargo.toml".into(),
                        bytes: next.len() as u64,
                        sha256: winbridge_update::sha256(next),
                    }];
                    if let Some(exe) = powershell {
                        update_config["schemaVersion"] = serde_json::json!(3);
                        update_config["build"] = serde_json::json!({"powershellExecutable":exe,"timeoutMs":if build_script==BUILD_ACTOR_SCRIPT {20000} else {60000}});
                        files.push(SourceFile {
                            path: "scripts/build-windows-update.ps1".into(),
                            bytes: build_script.len() as u64,
                            sha256: winbridge_update::sha256(build_script.as_bytes()),
                        });
                    }
                    let manifest = SourceManifest {
                        schema_version: 1,
                        source_commit: "b".repeat(40),
                        dirty: false,
                        created_at: "2026-10-18T00:00:00Z".into(),
                        includes_built_ui: false,
                        files,
                    };
                    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
                    let path = packages.join("fixture.zip");
                    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
                    for (name, bytes) in [
                        ("Cargo.toml", next.as_slice()),
                        ("winbridge-source-manifest.json", manifest_bytes.as_slice()),
                    ] {
                        zip.start_file(
                            format!("winbridge/{name}"),
                            zip::write::SimpleFileOptions::default(),
                        )
                        .unwrap();
                        zip.write_all(bytes).unwrap();
                    }
                    if update_config["schemaVersion"] == 3 {
                        zip.start_file(
                            "winbridge/scripts/build-windows-update.ps1",
                            zip::write::SimpleFileOptions::default(),
                        )
                        .unwrap();
                        zip.write_all(build_script.as_bytes()).unwrap();
                    }
                    zip.finish().unwrap();
                    Some(UpdateSourcePin {
                        archive_name: "fixture.zip".into(),
                        archive_sha256: winbridge_update::sha256(&fs::read(path).unwrap()),
                        manifest_sha256: winbridge_update::sha256(&manifest_bytes),
                        source_commit: manifest.source_commit,
                    })
                }
            } else {
                assert!(target_package.is_none());
                None
            };
            fs::write(&updater_config, serde_json::to_vec(&update_config).unwrap()).unwrap();
            let binary_directory = pinned_location
                .as_ref()
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| {
                    std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .to_owned()
                });
            let binary = binary_directory.join(if cfg!(windows) {
                "winbridge-update.exe"
            } else {
                "winbridge-update"
            });
            let image = runtime.join(binary.file_name().unwrap());
            fs::copy(&binary, &image).unwrap();
            let image_sha = winbridge_update::sha256(&fs::read(image).unwrap());
            if pinned_location.is_some() {
                runtime::write_known_fixture(&runtime, &updater_config);
            }
            let guard = if pinned_location.is_some() {
                crate::runtime_guard::prepare_runtime_guard(&config).unwrap()
            } else {
                None
            };
            let worker = Worker::open_file(&config).unwrap();
            if let Some(guard) = guard {
                worker.attach_runtime_guard(guard).unwrap();
            }
            let binding = worker
                .enable_update_control(&updater_config, &image_sha)
                .unwrap();
            let dir = runtime.join("update-state");
            let info = UpdateJournal::inspect(&dir).unwrap();
            let request = UpdateRequest {
                contract_version: 1,
                idempotency_key: "worker-launch".into(),
                source: source_pin.unwrap_or_else(|| UpdateSourcePin {
                    archive_name: "fixture.zip".into(),
                    archive_sha256: "b".repeat(64),
                    manifest_sha256: baseline.clone(),
                    source_commit: manifest.source_commit.clone(),
                }),
                expected: UpdateInstallationPin {
                    installation_id: info.installation_id,
                    scope_sha256: info.scope_sha256,
                    worker_build_id: binding.build.build_id,
                    worker_config_sha256: binding.config_sha256,
                    baseline_manifest_sha256: baseline.clone(),
                },
            };
            let server = tokio::spawn(crate::serve(worker.clone()));
            let deadline = Instant::now() + Duration::from_secs(5);
            while !worker
                .control_registration()
                .unwrap()
                .ready
                .load(Ordering::Acquire)
            {
                if server.is_finished() {
                    worker.stop();
                    panic!(
                        "Fixture worker serve failed before control readiness: {:?}",
                        server.await.unwrap()
                    );
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Self {
                worker,
                server,
                supervisor: None,
                request,
                config,
                updater_config,
                baseline,
                _root: owned_root,
            }
        }
        async fn launch(&mut self) {
            let WorkerSupervisorLaunch::Started(h) = self
                .worker
                .launch_supervisor_inner(&self.request, true)
                .await
                .unwrap_or_else(|message| {
                    use std::io::Read;
                    // TempDir teardown removes these files after a panic. Keep
                    // the bounded startup error in the exported test log first.
                    let diagnostic = self
                        .worker
                        .control_registration()
                        .ok()
                        .and_then(|r| UpdateJournal::read_snapshot(&r.directory, None).ok())
                        .and_then(|(_, update)| update)
                        .and_then(|r| {
                            self.config.parent().map(|runtime| {
                                runtime
                                    .join("update")
                                    .join(r.update_id)
                                    .join("supervisor/stderr.log")
                            })
                        })
                        .and_then(|p| {
                            let mut bytes = Vec::new();
                            fs::File::open(p)
                                .ok()?
                                .take(4096)
                                .read_to_end(&mut bytes)
                                .ok()?;
                            Some(String::from_utf8_lossy(&bytes).into_owned())
                        })
                        .unwrap_or_else(|| "Supervisor stderr unavailable".into());
                    panic!("{message}; supervisor startup stderr: {diagnostic}");
                })
            else {
                panic!("New fixture launch")
            };
            self.supervisor = Some(h);
        }
        async fn control(
            &mut self,
            command: WorkerUpdateControlCommand,
        ) -> WorkerUpdateControlResponse {
            self.supervisor
                .as_mut()
                .unwrap()
                .worker_control(command)
                .await
                .unwrap()
        }
        async fn advance(
            &mut self,
            id: &str,
            revision: u64,
            action: WorkerUpdateAction,
        ) -> WorkerUpdateControlResponse {
            self.control(WorkerUpdateControlCommand::Advance {
                command: WorkerUpdateCommand {
                    command_id: id.into(),
                    expected_revision: revision,
                    action,
                    source_manifest_sha256: if action == WorkerUpdateAction::Release {
                        Some(self.baseline.clone())
                    } else {
                        None
                    },
                    reason: format!("Fixture {id}"),
                },
            })
            .await
        }
        fn slots(&self) -> u32 {
            let context = self.worker.request_context().unwrap();
            self.worker
                .store()
                .unwrap()
                .work_scheduling_observation(context)
                .unwrap()
                .occupied_slots
        }
        async fn stop(&mut self) {
            let h = self.supervisor.as_mut().unwrap();
            let process = h.record.process.clone().unwrap();
            if h.record.phase != SupervisorPhase::Stopped {
                assert_eq!(
                    h.command(SupervisorAction::Stop).await.unwrap().phase,
                    SupervisorPhase::Stopped
                );
            }
            let until = Instant::now() + Duration::from_secs(5);
            while infrastructure::matches(&process).unwrap_or(false) {
                assert!(Instant::now() < until);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            self.supervisor.take();
            self.worker.stop();
            (&mut self.server).await.unwrap().unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Owned fixtures only. Wait for an admitted driver before considering
            // release; a failed assertion must not abandon a detached supervisor.
            let worker = self.worker.clone();
            let supervisor = self.supervisor.take();
            let _ = std::thread::spawn(move || {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    if let Some(mut h) = supervisor {
                        let deadline = Instant::now() + Duration::from_secs(15);
                        loop {
                            match h.preparation(false).await {
                                Ok(view) if view.driver_active && Instant::now() < deadline => {
                                    tokio::time::sleep(Duration::from_millis(50)).await
                                }
                                _ => break,
                            }
                        }
                        if h.record.phase != SupervisorPhase::Stopped {
                            let _ = h.abort_before_cutover().await;
                        }
                        let lease = worker
                            .store()
                            .ok()
                            .and_then(|s| s.active_worker_update().ok())
                            .flatten();
                        let compiler_known = UpdateJournal::preparation_snapshot(
                            &Path::new(&h.record.spec.runtime_directory).join("update-state"),
                            &h.record.spec.update_id,
                            false,
                            None,
                        )
                        .is_ok_and(|v| {
                            v.build.is_none_or(|b| {
                                b.phase == UpdateBuildPhase::Completed && b.cleanup_verified
                            })
                        });
                        if let Some(lease) = lease.filter(|_| compiler_known) {
                            let source = Path::new(&lease.spec.source_directory);
                            let verified = [
                                &lease.spec.target_manifest_sha256,
                                &lease.spec.baseline_manifest_sha256,
                            ]
                            .into_iter()
                            .find(|sha| winbridge_update::verify_source(source, sha).is_ok());
                            if let Some(sha) = verified {
                                let command = WorkerUpdateCommand {
                                    command_id: uuid::Uuid::new_v4().to_string(),
                                    expected_revision: lease.revision,
                                    action: WorkerUpdateAction::Release,
                                    source_manifest_sha256: Some(sha.clone()),
                                    reason: "Verified owned fixture cleanup".into(),
                                };
                                let _ = worker.advance_source_update(&lease.spec, &command);
                            }
                        }
                        if h.record.phase != SupervisorPhase::Stopped {
                            let _ = h.command(SupervisorAction::Stop).await;
                        }
                    }
                })
            })
            .join();
            self.worker.stop();
            self.server.abort();
        }
    }
    fn lease(r: WorkerUpdateControlResponse) -> WorkerUpdateLease {
        r.into_lease().unwrap().expect("Lease receipt")
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn candidate_drain_requires_health_and_preserves_all_legacy_views_without_effects() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let h = f.supervisor.as_mut().unwrap();
        let id = h.record.spec.update_id.clone();
        let before = UpdateJournal::read_snapshot(&reg.directory, Some(&id)).unwrap();
        let view = h.candidate_drain(false).await.unwrap();
        assert_eq!(view.schema_version, 5);
        assert_eq!(
            view.candidate_drain.as_ref().unwrap().phase,
            CandidateDrainPhase::NotAdmitted
        );
        assert!(view
            .candidate_drain
            .as_ref()
            .unwrap()
            .worker_observation
            .is_none());
        assert!(!view.driver_active);
        assert!(h.candidate_drain(true).await.is_err());
        assert!(h.command(SupervisorAction::DrainCandidate).await.is_err());
        for view in [
            h.preparation(false).await.unwrap(),
            h.runtime_baseline(false).await.unwrap(),
            h.runtime_preimages(false).await.unwrap(),
            h.candidate_health(false).await.unwrap(),
        ] {
            assert!(!serde_json::to_value(view)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("candidateDrain"));
        }
        assert_eq!(
            UpdateJournal::read_snapshot(&reg.directory, Some(&id)).unwrap(),
            before
        );
        assert!(UpdateJournal::inspect_drain(&reg.directory, &id)
            .unwrap()
            .intent
            .is_none());
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        assert_eq!(f.slots(), 0);
        f.stop().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn drain_observation_preserves_user_pause_and_reports_accepted_work_without_effects() {
        let mut f = Fixture::new(None).await;
        let context = f.worker.request_context().unwrap();
        let paused = f
            .worker
            .store()
            .unwrap()
            .queue_control(
                context.clone(),
                QueueControlCommand::Set {
                    command_id: "user-pause".into(),
                    expected_revision: "0".into(),
                    mode: QueueMode::DispatchPaused,
                },
            )
            .unwrap();
        assert!(matches!(paused, QueueControlResult::Applied { .. }));
        let LocalResponse::Task { task } = f.worker.handle(LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "paused-work".into(),
                idempotency_key: "paused-work".into(),
                project_id: "other".into(),
                timeout_ms: 5000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "never-executed-paused-fixture".into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                },
            },
        }) else {
            panic!("Accepted paused task expected")
        };
        assert_eq!(task.status, TaskStatus::Queued);
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let id = f.supervisor.as_ref().unwrap().record.spec.update_id.clone();
        let before = UpdateJournal::read_snapshot(&reg.directory, Some(&id)).unwrap();
        let v = f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_drain_observation()
            .await
            .unwrap();
        assert_eq!(v.context, context);
        assert!(v.lease.is_none());
        assert_eq!(
            (v.accepted_operations, v.logical_owners, v.in_flight_effects),
            (1, 0, 0)
        );
        assert_eq!(v.queue_mode, QueueMode::DispatchPaused);
        assert_eq!(v.queue_revision, "1");
        assert_eq!(
            UpdateJournal::read_snapshot(&reg.directory, Some(&id)).unwrap(),
            before
        );
        assert_eq!(f.slots(), 0);
        assert!(!f
            .worker
            .config
            .state_directory
            .join(crate::UPDATE_STARTUP_GUARD)
            .exists());
        lease(f.control(WorkerUpdateControlCommand::BeginSource).await);
        let held = lease(
            f.advance("source", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        let v = f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_drain_observation()
            .await
            .unwrap();
        assert_eq!(v.lease.as_deref(), Some(&held));
        assert_eq!(v.accepted_operations, 1);
        assert_eq!(v.queue_mode, QueueMode::DispatchPaused);
        assert_eq!(f.slots(), 1);
        let h = &f.supervisor.as_ref().unwrap().record;
        let mut q = WorkerUpdateControlRequest {
            control_version: 5,
            binding: reg.binding.clone(),
            update_id: id.clone(),
            launch_id: h.spec.launch_id.clone(),
            supervisor: h.process.clone().unwrap(),
            updater_owner_id: h.owner_id.clone().unwrap(),
            updater_epoch: h.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::ObserveDrain,
        };
        for version in [1, 2, 3, 4] {
            q.control_version = version;
            assert!(f
                .worker
                .control_result(&reg, &q, q.supervisor.process_id)
                .is_err());
        }
        q.control_version = 5;
        q.command = WorkerUpdateControlCommand::Advance {
            command: WorkerUpdateCommand {
                command_id: "forbidden-status-mutation".into(),
                expected_revision: 1,
                action: WorkerUpdateAction::Release,
                source_manifest_sha256: Some(f.baseline.clone()),
                reason: "Must not execute through a readonly contract".into(),
            },
        };
        assert!(f
            .worker
            .control_result(&reg, &q, q.supervisor.process_id)
            .is_err());
        assert_eq!(
            f.worker.store().unwrap().active_worker_update().unwrap(),
            Some(held)
        );
        assert!(f
            .supervisor
            .as_mut()
            .unwrap()
            .worker_control(WorkerUpdateControlCommand::ObserveDrain)
            .await
            .is_err());
        // An explicit user cancellation here is fixture cleanup, not updater policy.
        f.worker.handle(LocalRequest::Cancel {
            task_id: task.task_id.clone(),
        });
        let v = f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_drain_observation()
            .await
            .unwrap();
        assert_eq!(v.accepted_operations, 0);
        assert_eq!(v.queue_mode, QueueMode::DispatchPaused);
        assert_eq!(v.queue_revision, "1");
        assert_eq!(f.slots(), 1);
        let released = lease(f.advance("release", 1, WorkerUpdateAction::Release).await);
        let v = f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_drain_observation()
            .await
            .unwrap();
        assert_eq!(v.lease.as_deref(), Some(&released));
        assert_eq!(v.queue_mode, QueueMode::DispatchPaused);
        assert_eq!(v.queue_revision, "1");
        assert_eq!(f.slots(), 0);
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn shutdown_v6_observes_own_state_and_rejects_legacy_or_unadmitted_stop_without_effects()
    {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let h = &f.supervisor.as_ref().unwrap().record;
        let mut q = WorkerUpdateControlRequest {
            control_version: 6,
            binding: reg.binding.clone(),
            update_id: h.spec.update_id.clone(),
            launch_id: h.spec.launch_id.clone(),
            supervisor: h.process.clone().unwrap(),
            updater_owner_id: h.owner_id.clone().unwrap(),
            updater_epoch: h.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::ObserveShutdownState,
        };
        let before = UpdateJournal::read_snapshot(&reg.directory, Some(&q.update_id)).unwrap();
        let peer = q.supervisor.process_id;
        for version in 1..=5 {
            q.control_version = version;
            assert!(f.worker.control_result(&reg, &q, peer).is_err());
        }
        q.control_version = 6;
        let WorkerUpdateControlResult::ShutdownState { state } =
            f.worker.control_result(&reg, &q, peer).unwrap()
        else {
            panic!("Own state observation expected")
        };
        assert_eq!(
            state.lock_identity,
            winbridge_platform::file_identity(&f.worker._instance_lock).unwrap()
        );
        let original = q.binding.clone();
        q.binding.generation = uuid::Uuid::new_v4().to_string();
        assert!(f.worker.control_result(&reg, &q, peer).is_err());
        q.binding = original;
        let command = WorkerShutdownCommand {
            command_id: "shutdown-negative".into(),
            expected_lease_revision: 4,
            intent_sha256: "a".repeat(64),
            reason: "No committed intent; must remain live".into(),
        };
        q.command = WorkerUpdateControlCommand::ObserveShutdownReceipt {
            command: command.clone(),
        };
        assert!(matches!(
            f.worker.control_result(&reg, &q, peer).unwrap(),
            WorkerUpdateControlResult::ShutdownReceipt { receipt: None, .. }
        ));
        q.command = WorkerUpdateControlCommand::RequestShutdown {
            command: command.clone(),
        };
        // Both direct data handling and the real reply-owner path reject a stop
        // without the committed quiesced/issued updater intent.
        assert!(f.worker.control_result(&reg, &q, peer).is_err());
        assert!(shutdown::prepare_reply(f.worker.clone(), &reg, &q, peer).is_err());
        assert!(f
            .supervisor
            .as_mut()
            .unwrap()
            .worker_control(WorkerUpdateControlCommand::RequestShutdown { command })
            .await
            .is_err());
        assert!(!f.worker.stop.load(Ordering::Acquire));
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        assert_eq!(
            UpdateJournal::read_snapshot(&reg.directory, Some(&q.update_id)).unwrap(),
            before
        );
        f.stop().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn command_receipt_lookup_is_readonly_exact_and_never_falls_back() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let acquire = WorkerUpdateCommand {
            command_id: "receipt-acquire".into(),
            expected_revision: 0,
            action: WorkerUpdateAction::AcquireSource,
            source_manifest_sha256: None,
            reason: "Fixture receipt-acquire".into(),
        };
        let before = f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_receipt(&acquire)
            .await
            .unwrap();
        assert!(before.is_none());
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        assert!(!f
            .worker
            .config
            .state_directory
            .join(crate::UPDATE_STARTUP_GUARD)
            .exists());
        let pending = lease(f.control(WorkerUpdateControlCommand::BeginSource).await);
        assert_eq!(pending.phase, WorkerUpdatePhase::SourcePending);
        assert!(f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_receipt(&acquire)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            f.worker.store().unwrap().active_worker_update().unwrap(),
            Some(pending)
        );
        // Discard the original reply: reconciliation uses only the read endpoint.
        drop(
            f.advance("receipt-acquire", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        let held = f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .unwrap();
        for _ in 0..2 {
            assert_eq!(
                f.supervisor
                    .as_ref()
                    .unwrap()
                    .worker_receipt(&acquire)
                    .await
                    .unwrap(),
                Some(held.clone())
            );
        }
        let conflict = WorkerUpdateCommand {
            reason: "different payload".into(),
            ..acquire.clone()
        };
        assert!(f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_receipt(&conflict)
            .await
            .unwrap_err()
            .contains("another payload"));
        let missing = WorkerUpdateCommand {
            command_id: "never-executed-release".into(),
            expected_revision: 1,
            action: WorkerUpdateAction::Release,
            source_manifest_sha256: Some(f.baseline.clone()),
            reason: "Observe only".into(),
        };
        assert!(f
            .supervisor
            .as_ref()
            .unwrap()
            .worker_receipt(&missing)
            .await
            .unwrap()
            .is_none());
        assert!(f
            .worker
            .config
            .state_directory
            .join(crate::UPDATE_STARTUP_GUARD)
            .exists());
        let reg = f.worker.control_registration().unwrap();
        let h = f.supervisor.as_mut().unwrap();
        assert!(h
            .worker_control(WorkerUpdateControlCommand::ObserveReceipt {
                command: missing.clone()
            })
            .await
            .is_err());
        let mut q = WorkerUpdateControlRequest {
            control_version: 4,
            binding: reg.binding.clone(),
            update_id: h.record.spec.update_id.clone(),
            launch_id: h.record.spec.launch_id.clone(),
            supervisor: h.record.process.clone().unwrap(),
            updater_owner_id: h.record.owner_id.clone().unwrap(),
            updater_epoch: h.record.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::ObserveReceipt {
                command: acquire.clone(),
            },
        };
        let peer = q.supervisor.process_id;
        for version in [1, 2, 3] {
            q.control_version = version;
            assert!(f.worker.control_result(&reg, &q, peer).is_err());
        }
        q.control_version = 4;
        q.command = WorkerUpdateControlCommand::Advance { command: missing };
        assert!(f.worker.control_result(&reg, &q, peer).is_err());
        q.command = WorkerUpdateControlCommand::ObserveReceipt {
            command: acquire.clone(),
        };
        q.updater_epoch += 1;
        assert!(f.worker.control_result(&reg, &q, peer).is_err());
        assert_eq!(
            f.worker.store().unwrap().active_worker_update().unwrap(),
            Some(held.clone())
        );
        assert_eq!(f.slots(), 1);
        assert_eq!(
            f.worker
                .store()
                .unwrap()
                .connection
                .query_row("SELECT count(*) FROM worker_update_commands", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let released = lease(
            f.advance("receipt-release", 1, WorkerUpdateAction::Release)
                .await,
        );
        assert_eq!(released.phase, WorkerUpdatePhase::Released);
        // Historical reads cannot depend on source/project paths still existing.
        let moved_source = reg.source.with_file_name("historical-source");
        fs::rename(&reg.source, &moved_source).unwrap();
        // Return the original SourceHeld receipt even after the live lease moved.
        assert_eq!(
            f.supervisor
                .as_ref()
                .unwrap()
                .worker_receipt(&acquire)
                .await
                .unwrap(),
            Some(held)
        );
        fs::rename(&moved_source, &reg.source).unwrap();
        assert_eq!(f.slots(), 0);
        assert!(!f
            .worker
            .config
            .state_directory
            .join(crate::UPDATE_STARTUP_GUARD)
            .exists());
        f.stop().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn runtime_control_rejects_lease_versions_stale_authority_and_unpinned_worker() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let active = f.supervisor.as_ref().unwrap().record.clone();
        let q = WorkerUpdateControlRequest {
            control_version: 2,
            binding: reg.binding.clone(),
            update_id: active.spec.update_id.clone(),
            launch_id: active.spec.launch_id.clone(),
            supervisor: active.process.clone().unwrap(),
            updater_owner_id: active.owner_id.clone().unwrap(),
            updater_epoch: active.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::ObserveRuntime,
        };
        let before = UpdateJournal::read_snapshot(&reg.directory, Some(&q.update_id)).unwrap();
        assert!(f
            .supervisor
            .as_ref()
            .unwrap()
            .runtime_identity()
            .await
            .unwrap_err()
            .contains("Bootstrap required"));
        assert!(f
            .supervisor
            .as_mut()
            .unwrap()
            .worker_control(WorkerUpdateControlCommand::ObserveRuntime)
            .await
            .unwrap_err()
            .contains("supervisor v6"));
        let error = f
            .supervisor
            .as_ref()
            .unwrap()
            .runtime_compatibility()
            .await
            .unwrap_err();
        assert!(
            error.contains("Bootstrap required"),
            "Unexpected compatibility observation failure: {error}"
        );
        assert!(f
            .supervisor
            .as_mut()
            .unwrap()
            .worker_control(WorkerUpdateControlCommand::ObserveCompatibility)
            .await
            .unwrap_err()
            .contains("supervisor v6/v9"));
        for case in [
            "v1-runtime",
            "v2-compatibility",
            "v3-lease",
            "v2-lease",
            "version",
            "peer",
            "epoch",
            "process",
            "context",
            "build",
        ] {
            let mut forged = q.clone();
            let mut peer = q.supervisor.process_id;
            match case {
                "v1-runtime" => forged.control_version = 1,
                "v2-compatibility" => {
                    forged.command = WorkerUpdateControlCommand::ObserveCompatibility
                }
                "v3-lease" => {
                    forged.control_version = 3;
                    forged.command = WorkerUpdateControlCommand::BeginSource;
                }
                "v2-lease" => forged.command = WorkerUpdateControlCommand::BeginSource,
                "version" => forged.control_version = 3,
                "peer" => peer = std::process::id(),
                "epoch" => forged.updater_epoch += 1,
                "process" => forged.supervisor.birth.push('x'),
                "context" => {
                    forged.binding.context.journal_incarnation = uuid::Uuid::new_v4().to_string()
                }
                "build" => forged.binding.build.build_id.push('x'),
                _ => unreachable!(),
            }
            let error = f.worker.control_result(&reg, &forged, peer).unwrap_err();
            assert!(
                !error.contains("Bootstrap required"),
                "Authority must fail before reading identity: {case}: {error}"
            );
        }
        for path in [&f.config, &f.updater_config] {
            let bytes = fs::read(path).unwrap();
            fs::write(path, [bytes.as_slice(), b" "].concat()).unwrap();
            assert!(f
                .worker
                .control_result(&reg, &q, q.supervisor.process_id)
                .unwrap_err()
                .contains("ConfigRestartPending"));
            fs::write(path, bytes).unwrap();
        }
        let forged: WorkerUpdateControlResponse =
            ipc::call(&reg.binding.endpoint, std::process::id(), &q)
                .await
                .unwrap();
        assert_eq!(forged.control_version, 2);
        assert!(forged.result.is_error());
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        assert_eq!(f.slots(), 0);
        assert_eq!(
            UpdateJournal::read_snapshot(&reg.directory, Some(&q.update_id)).unwrap(),
            before
        );
        f.stop().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn runtime_observation_keeps_supervisor_mailbox_responsive_and_bounds_duplicates() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.test_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        {
            let h = f.supervisor.as_ref().unwrap();
            let observation = h.runtime_identity();
            tokio::pin!(observation);
            tokio::select! {
                result = &mut observation => panic!("Observation did not reach the held worker: {result:?}"),
                result = tokio::time::timeout(Duration::from_secs(5), entered_rx) => { result.unwrap().unwrap(); }
            }
            let status = tokio::time::timeout(Duration::from_secs(2), h.observe())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.phase, SupervisorPhase::Active);
            assert!(h
                .runtime_identity()
                .await
                .unwrap_err()
                .contains("Runtime observation active"));
            release_tx.send(()).unwrap();
            assert!(observation
                .await
                .unwrap_err()
                .contains("Bootstrap required"));
            // Receipt of the previous reply must mean its query slot is free.
            // No status roundtrip or delay may be required between observations.
            for _ in 0..3 {
                assert!(h
                    .runtime_identity()
                    .await
                    .unwrap_err()
                    .contains("Bootstrap required"));
            }
        }
        assert_eq!(f.slots(), 0);
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn runtime_baseline_actor_retains_failed_intent_and_blocks_source_without_replay() {
        let mut f = Fixture::with_source_policy(None, true).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let h = f.supervisor.as_mut().unwrap();
        let initial = h.runtime_baseline(false).await.unwrap();
        assert_eq!(initial.schema_version, 2);
        assert_eq!(
            initial.runtime_baseline,
            Some(RuntimeBaselineView::NotAdmitted)
        );
        assert!(h
            .command(SupervisorAction::CaptureRuntimeBaseline)
            .await
            .is_err());
        let legacy = h.preparation(false).await.unwrap();
        assert_eq!(legacy.schema_version, 1);
        assert!(!serde_json::to_value(legacy)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("runtimeBaseline"));
        let accepted = h.runtime_baseline(true).await.unwrap();
        assert!(matches!(
            accepted.runtime_baseline,
            Some(RuntimeBaselineView::Pending {
                intent_revision: 1,
                ..
            })
        ));
        assert!(accepted.driver_active);
        let id = h.record.spec.update_id.clone();
        let retained = UpdateJournal::inspect_runtime_baseline(&reg.directory, &id).unwrap();
        assert_eq!(retained.intent.as_ref().unwrap().parent_revision, 1);
        assert!(retained.receipt.is_none());
        let deadline = Instant::now() + Duration::from_secs(10);
        let failed = loop {
            let view = h.runtime_baseline(false).await.unwrap();
            if !view.driver_active {
                break view;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(failed
            .driver_error
            .as_ref()
            .unwrap()
            .contains("Bootstrap required"));
        assert!(h
            .runtime_baseline(true)
            .await
            .unwrap_err()
            .contains("must be inspected/aborted"));
        assert!(h
            .preparation(true)
            .await
            .unwrap_err()
            .contains("Runtime baseline pending"));
        assert_eq!(
            UpdateJournal::inspect_runtime_baseline(&reg.directory, &id).unwrap(),
            retained
        );
        let final_view = h.runtime_baseline(false).await.unwrap();
        assert_eq!(final_view.update.phase, UpdatePhase::Requested);
        assert_eq!(final_view.update.revision, 1);
        assert!(final_view.source.is_none());
        assert_eq!(f.slots(), 0);
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn preimages_actor_requires_verified_baseline_and_keeps_older_view_shapes() {
        let mut f = Fixture::with_source_policy(None, true).await;
        f.launch().await;
        let reg = f.worker.control_registration().unwrap();
        let h = f.supervisor.as_mut().unwrap();
        let initial = h.runtime_preimages(false).await.unwrap();
        assert_eq!(initial.schema_version, 3);
        assert_eq!(
            initial.runtime_baseline,
            Some(RuntimeBaselineView::NotAdmitted)
        );
        assert_eq!(
            initial.runtime_preimages,
            Some(RuntimePreimagesView::NotAdmitted)
        );
        let v7 = h.runtime_baseline(false).await.unwrap();
        assert_eq!(v7.schema_version, 2);
        assert!(!serde_json::to_value(v7)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("runtimePreimages"));
        let legacy = h.preparation(false).await.unwrap();
        let legacy = serde_json::to_value(legacy).unwrap();
        assert!(
            legacy.get("runtimePreimages").is_none() && legacy.get("runtimeBaseline").is_none()
        );
        assert!(h
            .command(SupervisorAction::ArchiveRuntimePreimages)
            .await
            .is_err());
        assert!(h
            .runtime_preimages(true)
            .await
            .unwrap_err()
            .contains("baseline"));
        let status = h.runtime_preimages(false).await.unwrap();
        assert_eq!(status.update.revision, 0);
        assert!(!status.driver_active);
        assert!(status.source.is_none());
        assert!(
            UpdateJournal::inspect_runtime_preimages(&reg.directory, &status.update.update_id)
                .unwrap()
                .intent
                .is_none()
        );
        let aborted = h.abort_before_cutover().await.unwrap();
        assert!(aborted.worker_before.is_none() && aborted.worker_released.is_none());
        assert!(aborted.reason.contains("отсутствие lease"));
        assert_eq!(f.slots(), 0);
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn worker_owned_launch_and_authenticated_source_control_roundtrip() {
        let mut f =
            Fixture::new(std::env::var_os("WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE").map(PathBuf::from))
                .await;
        // The production path must not treat the test runtime/enclosing Job as
        // verified installation policy. Rejection precedes update acceptance.
        assert!(f.worker.launch_update_supervisor(&f.request).await.is_err());
        let reg = f.worker.control_registration().unwrap();
        assert!(UpdateJournal::find_request(&reg.directory, &f.request)
            .unwrap()
            .is_none());
        let launch_started = Instant::now();
        f.launch().await;
        let launch_elapsed_ms = launch_started.elapsed().as_millis();
        let active = f.supervisor.as_ref().unwrap().record.clone();
        assert_eq!(active.phase, SupervisorPhase::Active);
        assert_eq!(active.spec.worker.as_ref(), Some(&reg.binding));
        assert_eq!(
            active.spec.excluded_domain,
            f.worker
                .execution_domain
                .lock()
                .unwrap()
                .as_ref()
                .map(|d| d.identity().job_name.clone())
        );
        let forged = WorkerUpdateControlRequest {
            control_version: 1,
            binding: reg.binding.clone(),
            update_id: active.spec.update_id.clone(),
            launch_id: active.spec.launch_id.clone(),
            supervisor: active.process.clone().unwrap(),
            updater_owner_id: active.owner_id.clone().unwrap(),
            updater_epoch: active.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::BeginSource,
        };
        let denied: WorkerUpdateControlResponse =
            ipc::call(&reg.binding.endpoint, std::process::id(), &forged)
                .await
                .unwrap();
        assert!(denied.result.is_error());
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        let empty = f.control(WorkerUpdateControlCommand::Observe).await;
        assert!(empty.into_lease().unwrap().is_none());
        let WorkerSupervisorLaunch::Existing { update, supervisor } =
            f.worker.launch_update_supervisor(&f.request).await.unwrap()
        else {
            panic!("Duplicate must only observe")
        };
        assert_eq!(update.update_id, active.spec.update_id);
        assert_eq!(supervisor.unwrap().process, active.process);
        let mut conflict = f.request.clone();
        conflict.source.archive_sha256 = "c".repeat(64);
        assert!(f.worker.launch_update_supervisor(&conflict).await.is_err());
        let pending = lease(f.control(WorkerUpdateControlCommand::BeginSource).await);
        assert_eq!(pending.phase, WorkerUpdatePhase::SourcePending);
        assert_eq!(f.slots(), 0);
        let grant_started = Instant::now();
        let held = lease(
            f.advance("acquire", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        let grant_elapsed_ms = grant_started.elapsed().as_millis();
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        assert_eq!(f.slots(), 1);
        let again = lease(
            f.advance("acquire", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        assert_eq!(again, held);
        assert_eq!(f.slots(), 1);
        let invalid = f
            .advance("new-invalid-acquire", 1, WorkerUpdateAction::AcquireSource)
            .await
            .into_lease()
            .unwrap_err();
        assert!(!invalid.starts_with("resource busy:"));
        assert!(f
            .supervisor
            .as_mut()
            .unwrap()
            .command(SupervisorAction::Stop)
            .await
            .is_err());
        assert!(infrastructure::matches(active.process.as_ref().unwrap()).unwrap());
        let fixture = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(if cfg!(windows) {
                "winbridge-fixture.exe"
            } else {
                "winbridge-fixture"
            });
        let LocalResponse::Task { task } = f.worker.handle(LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "other".into(),
                idempotency_key: "other".into(),
                project_id: "other".into(),
                timeout_ms: 10000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: fixture.to_string_lossy().into(),
                    arguments: vec!["success".into()],
                    working_directory: ".".into(),
                },
            },
        }) else {
            panic!("Other root accepted into queue")
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        let LocalResponse::Task { task: queued } = f.worker.handle(LocalRequest::GetTask {
            task_id: task.task_id.clone(),
        }) else {
            panic!("Queued task")
        };
        assert_eq!(queued.status, TaskStatus::Queued);
        assert!(queued.process_id.is_none());
        f.worker.handle(LocalRequest::Cancel {
            task_id: task.task_id.clone(),
        });
        for action in [
            WorkerUpdateAction::BeginDrain,
            WorkerUpdateAction::CloseDispatch,
            WorkerUpdateAction::ConfirmQuiesced,
        ] {
            let error = f
                .advance("premature-drain", 1, action)
                .await
                .into_lease()
                .unwrap_err();
            assert!(error.contains("updater Draining"));
        }
        let released = lease(f.advance("release", 1, WorkerUpdateAction::Release).await);
        assert_eq!(released.phase, WorkerUpdatePhase::Released);
        assert_eq!(f.slots(), 0);
        assert!(!f
            .worker
            .config
            .state_directory
            .join(crate::UPDATE_STARTUP_GUARD)
            .exists());
        let observed = f.control(WorkerUpdateControlCommand::Observe).await;
        assert_eq!(observed.into_lease().unwrap(), Some(released.clone()));
        let source = reg.source.clone();
        winbridge_update::verify_source(&source, &f.baseline).unwrap();
        f.stop().await;
        if let Some(path) = std::env::var_os("WINBRIDGE_UPDATE_CONTROL_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"worker_update_control_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"binding":reg.binding,"active":active,"stages":[pending,held,released],"duplicateObserved":true,"forgedPeerRejected":true,"prematureDrainRejected":true,"workSlots":[0,1,1,0],"queuedTask":queued,"sourceVerified":true,"sourceManifestSha256":f.baseline,"actualSourcePackage":std::env::var_os("WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE").is_some(),"productionEnvironmentDeferred":true,"launchElapsedMs":launch_elapsed_ms,"sourceGrantElapsedMs":grant_elapsed_ms,"runtimeApplied":false,"sourceEffectsEnabled":false,"windowsOpened":false})).unwrap()).unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn source_scan_keeps_info_responsive_and_rechecks_authority_before_grant() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let pending = lease(f.control(WorkerUpdateControlCommand::BeginSource).await);
        let worker = f.worker.clone();
        let spec = pending.spec.clone();
        let baseline = f.baseline.clone();
        let (entered, scanning) = std::sync::mpsc::channel();
        let (finish, resume) = std::sync::mpsc::channel();
        let verify = tokio::task::spawn_blocking(move || {
            let calls = AtomicUsize::new(0);
            worker.advance_source_update_test_verifier(
                &spec,
                &WorkerUpdateCommand {
                    command_id: "slow-scan".into(),
                    expected_revision: 0,
                    action: WorkerUpdateAction::AcquireSource,
                    source_manifest_sha256: None,
                    reason: "Controlled slow verification".into(),
                },
                &|| {
                    if calls.fetch_add(1, Ordering::AcqRel) == 0 {
                        Ok(())
                    } else {
                        Err("Updater authority changed during scan".into())
                    }
                },
                &|root, hash| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    assert_eq!(hash, baseline);
                    winbridge_update::verify_source(root, hash).map(|_| ())
                },
            )
        });
        tokio::task::spawn_blocking(move || scanning.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
        let address = winbridge_transport::endpoint(
            &f.worker.config.state_directory,
            &f.worker.config.instance_id,
        )
        .unwrap();
        // A real IPC Info must finish while the verifier is deliberately paused.
        let info = tokio::time::timeout(
            Duration::from_secs(2),
            winbridge_transport::call(&address, &LocalRequest::Info),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(info, LocalResponse::Info { .. }));
        assert_eq!(f.slots(), 0); // readonly scanning is not a source/build grant
        assert_eq!(
            f.worker.store().unwrap().active_worker_update().unwrap(),
            Some(pending.clone())
        );
        finish.send(()).unwrap();
        assert!(verify
            .await
            .unwrap()
            .unwrap_err()
            .contains("authority changed"));
        assert_eq!(
            f.worker.store().unwrap().active_worker_update().unwrap(),
            Some(pending)
        );
        assert_eq!(f.slots(), 0);
        lease(
            f.advance("fresh-acquire", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        lease(
            f.advance("fresh-release", 1, WorkerUpdateAction::Release)
                .await,
        );
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn source_preparation_actor_stays_responsive_and_never_replays_dispatch() {
        let mut f = Fixture::with_source_policy(None, true).await;
        f.launch().await;
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *f.worker
            .control_registration()
            .unwrap()
            .test_gate
            .lock()
            .unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        let h = f.supervisor.as_mut().unwrap();
        let accepted = h.preparation(true).await.unwrap();
        assert_eq!(accepted.update.phase, UpdatePhase::Preparing);
        assert!(accepted.driver_active);
        tokio::time::timeout(Duration::from_secs(5), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let busy = tokio::time::timeout(Duration::from_secs(2), h.preparation(false))
            .await
            .unwrap()
            .unwrap();
        assert!(busy.driver_active);
        assert!(busy.source.is_none());
        let again = h.preparation(true).await.unwrap();
        assert_eq!(again.update, accepted.update);
        assert!(h
            .command(SupervisorAction::Stop)
            .await
            .unwrap_err()
            .contains("driver active"));
        release_tx.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        let final_view = loop {
            let view = h.preparation(false).await.unwrap();
            if !view.driver_active {
                break view;
            }
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        assert!(
            final_view.driver_error.is_none(),
            "{:?}",
            final_view.driver_error
        );
        assert_eq!(final_view.update.phase, UpdatePhase::Building);
        assert_eq!(
            final_view.source.as_ref().unwrap().phase,
            SourceTransactionPhase::Verified
        );
        assert_eq!(final_view.source_applied_files, 2);
        assert!(final_view.build.is_none());
        let duplicate = h.preparation(true).await.unwrap();
        assert_eq!(duplicate.update, final_view.update);
        assert_eq!(duplicate.source, final_view.source);
        let held = lease(
            h.worker_control(WorkerUpdateControlCommand::Observe)
                .await
                .unwrap(),
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        let context = f.worker.request_context().unwrap();
        assert_eq!(
            f.worker
                .store()
                .unwrap()
                .work_scheduling_observation(context)
                .unwrap()
                .occupied_slots,
            1
        );
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        winbridge_update::verify_source(&cfg.source_directory, &f.request.source.manifest_sha256)
            .unwrap();
        assert!(h.build_candidate().await.unwrap_err().contains("schema 3"));
        assert!(h.preparation(false).await.unwrap().build.is_none());
        let aborted = h.abort_before_cutover().await.unwrap();
        assert_eq!(
            aborted.phase,
            winbridge_update::abort::AbortPhase::Completed
        );
        assert_eq!(
            aborted.worker_released.as_ref().unwrap().phase,
            WorkerUpdatePhase::Released
        );
        if let Some(path) = std::env::var_os("WINBRIDGE_SOURCE_PREPARATION_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"supervisor_source_preparation_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"accepted":accepted,"busy":busy,"final":final_view,"duplicate":duplicate,"mailboxResponsive":true,"stopWhileBusyRejected":true,"duplicateObserved":true,"fixtureSourceApplied":true,"workerLeaseRetained":true,"coordinatedAbortCompleted":true,"abort":aborted,"runtimeApplied":false,"windowsOpened":false})).unwrap()).unwrap();
        }
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    #[ignore = "Explicit local PowerShell executable required; isolated owned source/runtime only"]
    async fn compiler_actor_commits_before_dispatch_stays_responsive_and_aborts_cleanly() {
        let exe = PathBuf::from(
            std::env::var_os("WINBRIDGE_UPDATE_TEST_PWSH")
                .expect("Pinned local PowerShell fixture"),
        );
        let mut f = Fixture::with_build_policy(None, true, Some(exe)).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        h.preparation(true).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let accepted = h.build_candidate().await.unwrap();
        assert_eq!(
            accepted.build.as_ref().unwrap().phase,
            UpdateBuildPhase::Intent
        );
        assert!(accepted.driver_active);
        let duplicate = h.build_candidate().await.unwrap();
        assert_eq!(
            duplicate.build.as_ref().unwrap().spec,
            accepted.build.as_ref().unwrap().spec
        );
        let busy = loop {
            let v = tokio::time::timeout(Duration::from_secs(2), h.preparation(false))
                .await
                .unwrap()
                .unwrap();
            if v.build
                .as_ref()
                .is_some_and(|b| b.phase == UpdateBuildPhase::Running)
            {
                break v;
            }
            assert!(v.driver_active, "{:?}", v);
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(h
            .command(SupervisorAction::Stop)
            .await
            .unwrap_err()
            .contains("driver active"));
        assert!(h
            .abort_before_cutover()
            .await
            .unwrap_err()
            .contains("driver active"));
        let final_view = loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                break v;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        if let Some(error) = &final_view.driver_error {
            let build = final_view
                .build
                .as_ref()
                .expect("Failure keeps compiler receipt");
            let logs = Path::new(&build.spec.runtime_directory)
                .join("update")
                .join(&build.spec.update_id)
                .join("compiler")
                .join(&build.spec.attempt_id);
            let output = |name: &str| {
                use std::io::Read;
                let mut bytes = vec![];
                if let Ok(f) = fs::File::open(logs.join(format!("{name}.bin"))) {
                    let _ = f.take(65536).read_to_end(&mut bytes);
                }
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let (stdout, stderr) = (output("stdout"), output("stderr"));
            if let Some(path) = std::env::var_os("WINBRIDGE_COMPILER_ACTOR_REPORT") {
                let path = PathBuf::from(path).with_file_name("compiler-actor-failure.json");
                let bytes=serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"compiler_actor_failure","build":build_identity(),"view":final_view,"stdout":stdout,"stderr":stderr,"runtimeApplied":false,"windowsOpened":false})).unwrap();
                fs::write(path, bytes).unwrap();
            }
            // Report first; cleanup is the same coordinated production abort.
            let cleanup = h.abort_before_cutover().await;
            panic!("{error}; stdout={stdout}; stderr={stderr}; coordinated abort={cleanup:?}");
        }
        assert!(
            final_view.driver_error.is_none(),
            "{:?}",
            final_view.driver_error
        );
        let build = final_view.build.as_ref().unwrap();
        assert_eq!(build.phase, UpdateBuildPhase::Completed);
        assert_eq!(build.outcome, Some(UpdateBuildOutcome::Succeeded));
        assert!(build.cleanup_verified);
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        let (report_receipt, report_bytes) =
            winbridge_update::build_report::read_verified(&cfg, &build.spec.update_id).unwrap();
        assert!(String::from_utf8(report_bytes)
            .unwrap()
            .contains("compiler-report-fixture"));
        let held = lease(
            h.worker_control(WorkerUpdateControlCommand::Observe)
                .await
                .unwrap(),
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        let raw = h
            .worker_control(WorkerUpdateControlCommand::Advance {
                command: WorkerUpdateCommand {
                    command_id: "premature-build-release".into(),
                    expected_revision: held.revision,
                    action: WorkerUpdateAction::Release,
                    source_manifest_sha256: Some(f.request.source.manifest_sha256.clone()),
                    reason: "Raw release must not bypass compiler cleanup".into(),
                },
            })
            .await
            .unwrap();
        assert!(raw.into_lease().unwrap_err().contains("reconciled"));
        assert_eq!(h.build_candidate().await.unwrap().build, final_view.build);
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        assert_eq!(
            fs::read_to_string(cfg.source_directory.join("target/actor-runs.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let logs = cfg
            .runtime_directory
            .join("update")
            .join(&final_view.update.update_id)
            .join("compiler")
            .join(&build.spec.attempt_id);
        assert!(
            String::from_utf8(fs::read(logs.join("stdout.bin")).unwrap())
                .unwrap()
                .contains("Привет ✓")
        );
        assert!(
            String::from_utf8(fs::read(logs.join("stderr.bin")).unwrap())
                .unwrap()
                .contains("diagnostic")
        );
        let abort = h.abort_before_cutover().await.unwrap();
        assert_eq!(abort.phase, winbridge_update::abort::AbortPhase::Completed);
        assert!(abort.cleanup_verified);
        assert_eq!(
            abort.worker_released.as_ref().unwrap().phase,
            WorkerUpdatePhase::Released
        );
        assert_eq!(f.slots(), 0);
        winbridge_update::verify_source(&cfg.source_directory, &f.request.source.manifest_sha256)
            .unwrap();
        if let Some(path) = std::env::var_os("WINBRIDGE_COMPILER_ACTOR_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"supervisor_compiler_actor_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"accepted":accepted,"busy":busy,"duplicate":duplicate,"final":final_view,"abort":abort,"singleCompiler":true,"mailboxResponsive":true,"rawReleaseRejected":true,"busyAbortRejected":true,"coordinatedAbortCompleted":true,"compilerReportBound":true,"reportReceipt":report_receipt,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false})).unwrap()).unwrap();
        }
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    #[ignore = "Explicit local PowerShell and Rust compiler; owned fixture executables only"]
    async fn artifact_actor_records_real_fixture_queries_and_retains_health_barrier() {
        let exe = PathBuf::from(
            std::env::var_os("WINBRIDGE_UPDATE_TEST_PWSH")
                .expect("Pinned local PowerShell fixture"),
        );
        let mut f = Fixture::with_build_script(None, true, Some(exe), ARTIFACT_ACTOR_SCRIPT).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        assert!(h.verify_candidate_artifacts().await.is_err());
        h.preparation(true).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(h.verify_candidate_artifacts().await.is_err());
        h.build_candidate().await.unwrap();
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                assert_eq!(
                    v.build.as_ref().unwrap().outcome,
                    Some(UpdateBuildOutcome::Succeeded)
                );
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let accepted = h.verify_candidate_artifacts().await.unwrap();
        assert!(accepted.driver_active);
        assert_eq!(accepted.update.phase, UpdatePhase::Building);
        let duplicate = h.verify_candidate_artifacts().await.unwrap();
        let completed = loop {
            let v = tokio::time::timeout(Duration::from_secs(2), h.preparation(false))
                .await
                .unwrap()
                .unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                break v;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(completed.update.phase, UpdatePhase::ValidatingCandidate);
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        let record = UpdateJournal::inspect_candidate_artifacts(
            &cfg.runtime_directory.join("update-state"),
            &completed.update.update_id,
        )
        .unwrap();
        assert_eq!(record.inspection.artifacts.len(), 4);
        assert!(!record.inspection.candidate_verified);
        assert_eq!(
            h.verify_candidate_artifacts().await.unwrap().update,
            completed.update
        );
        assert_eq!(
            fs::read_to_string(cfg.source_directory.join("target/actor-runs.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let held = lease(
            h.worker_control(WorkerUpdateControlCommand::Observe)
                .await
                .unwrap(),
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        let release = h
            .worker_control(WorkerUpdateControlCommand::Advance {
                command: WorkerUpdateCommand {
                    command_id: "artifact-premature-release".into(),
                    expected_revision: held.revision,
                    action: WorkerUpdateAction::Release,
                    source_manifest_sha256: Some(f.request.source.manifest_sha256.clone()),
                    reason: "Artifact receipt is not health/install permission".into(),
                },
            })
            .await
            .unwrap();
        assert!(release.into_lease().is_err());
        let aborted = h.abort_before_cutover().await.unwrap();
        assert_eq!(
            aborted.phase,
            winbridge_update::abort::AbortPhase::Completed
        );
        assert!(aborted.spec.candidate_digest.is_some());
        assert_eq!(f.slots(), 0);
        if let Some(path) = std::env::var_os("WINBRIDGE_ARTIFACT_ACTOR_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"artifact_actor_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"accepted":accepted,"duplicate":duplicate,"final":completed,"record":record,"abort":aborted,"realFixtureQueries":true,"singleCompiler":true,"mailboxResponsive":true,"rawReleaseRejected":true,"candidateDigestBound":true,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false})).unwrap()).unwrap();
        }
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    #[ignore = "Explicit local PowerShell; intentionally incomplete owned report fixture"]
    async fn artifact_actor_rejects_incomplete_report_and_aborts_without_adoption() {
        let exe = PathBuf::from(
            std::env::var_os("WINBRIDGE_UPDATE_TEST_PWSH")
                .expect("Pinned local PowerShell fixture"),
        );
        let mut f = Fixture::with_build_policy(None, true, Some(exe)).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        h.preparation(true).await.unwrap();
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        h.build_candidate().await.unwrap();
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        h.verify_candidate_artifacts().await.unwrap();
        let failed = loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                break v;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(
            failed
                .driver_error
                .as_ref()
                .is_some_and(|e| e.contains("contract/JSON")
                    || e.contains("Candidate payload directory unavailable")),
            "{:?}",
            failed.driver_error
        );
        assert_eq!(failed.update.phase, UpdatePhase::Building);
        assert!(h.verify_candidate_artifacts().await.is_err());
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        assert!(UpdateJournal::inspect_candidate_artifacts(
            &cfg.runtime_directory.join("update-state"),
            &failed.update.update_id
        )
        .is_err());
        let held = lease(
            h.worker_control(WorkerUpdateControlCommand::Observe)
                .await
                .unwrap(),
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        let abort = h.abort_before_cutover().await.unwrap();
        assert_eq!(abort.phase, winbridge_update::abort::AbortPhase::Completed);
        assert!(abort.spec.candidate_digest.is_none());
        assert_eq!(f.slots(), 0);
        if let Some(path) = std::env::var_os("WINBRIDGE_ARTIFACT_ACTOR_REPORT") {
            let path = PathBuf::from(path).with_file_name("artifact-actor-rejection-proof.json");
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"artifact_actor_rejection_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"failed":failed,"abort":abort,"receiptAbsent":true,"retryRejected":true,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false})).unwrap()).unwrap();
        }
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn abort_without_file_effects_releases_only_its_pending_lease() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        let pending = lease(
            h.worker_control(WorkerUpdateControlCommand::BeginSource)
                .await
                .unwrap(),
        );
        assert_eq!(pending.phase, WorkerUpdatePhase::SourcePending);
        let aborted = h.abort_before_cutover().await.unwrap();
        assert_eq!(
            aborted.phase,
            winbridge_update::abort::AbortPhase::Completed
        );
        assert!(aborted.source_sha256.is_none());
        assert_eq!(
            aborted.worker_released.as_ref().unwrap().phase,
            WorkerUpdatePhase::Released
        );
        assert_eq!(f.slots(), 0);
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn changed_source_blocks_abort_and_retains_worker_barrier() {
        let mut f = Fixture::with_source_policy(None, true).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        h.preparation(true).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                assert!(v.driver_error.is_none());
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        let cargo = cfg.source_directory.join("Cargo.toml");
        let original = fs::read(&cargo).unwrap();
        fs::write(&cargo, b"user changed source").unwrap();
        assert!(h
            .abort_before_cutover()
            .await
            .unwrap_err()
            .contains("Abort остановлен"));
        let directory = cfg.runtime_directory.join("update-state");
        let blocked = UpdateJournal::inspect_abort(&directory, &h.record.spec.update_id).unwrap();
        assert_eq!(blocked.phase, winbridge_update::abort::AbortPhase::Blocked);
        assert_eq!(
            UpdateJournal::read_snapshot(&directory, Some(&h.record.spec.update_id))
                .unwrap()
                .1
                .unwrap()
                .phase,
            UpdatePhase::ManualIntervention
        );
        let held = lease(
            h.worker_control(WorkerUpdateControlCommand::Observe)
                .await
                .unwrap(),
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        assert!(h.abort_before_cutover().await.is_err());
        assert_eq!(
            UpdateJournal::inspect_abort(&directory, &h.record.spec.update_id).unwrap(),
            blocked
        );
        // Explicit restoration/cleanup of this owned fixture only. It is not a
        // production recovery path and does not erase the blocked receipt.
        fs::write(&cargo, original).unwrap();
        let command = WorkerUpdateCommand {
            command_id: "manual-owned-fixture-cleanup".into(),
            expected_revision: held.revision,
            action: WorkerUpdateAction::Release,
            source_manifest_sha256: Some(f.request.source.manifest_sha256.clone()),
            reason: "Restored and verified owned test source".into(),
        };
        f.worker
            .advance_source_update(&held.spec, &command)
            .unwrap();
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn source_preparation_rejects_unconfigured_policy_before_intent() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        assert!(h
            .preparation(true)
            .await
            .unwrap_err()
            .contains("sourcePackagesDirectory"));
        let view = h.preparation(false).await.unwrap();
        assert_eq!(view.update.phase, UpdatePhase::Requested);
        assert_eq!(view.update.revision, 0);
        assert!(!view.driver_active);
        assert!(view.source.is_none());
        assert!(h
            .worker_control(WorkerUpdateControlCommand::Observe)
            .await
            .unwrap()
            .into_lease()
            .unwrap()
            .is_none());
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn source_preparation_archive_failure_is_reported_and_never_retried() {
        let mut f = Fixture::with_source_policy(None, true).await;
        f.launch().await;
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        fs::write(
            cfg.source_packages_directory.unwrap().join("fixture.zip"),
            b"damaged fixture",
        )
        .unwrap();
        let h = f.supervisor.as_mut().unwrap();
        h.preparation(true).await.unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        let failed = loop {
            let v = h.preparation(false).await.unwrap();
            if !v.driver_active {
                break v;
            }
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(failed.driver_error.is_some());
        assert!(failed.source.is_none());
        assert_eq!(failed.update.phase, UpdatePhase::Preparing);
        let duplicate = h.preparation(true).await.unwrap();
        assert_eq!(duplicate.update, failed.update);
        assert_eq!(duplicate.driver_error, failed.driver_error);
        assert!(!duplicate.driver_active);
        winbridge_update::verify_source(&cfg.source_directory, &f.baseline).unwrap();
        assert!(h
            .worker_control(WorkerUpdateControlCommand::Observe)
            .await
            .unwrap()
            .into_lease()
            .unwrap()
            .is_none());
        f.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn stale_bindings_config_changes_and_ignored_writes_never_grant_source() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let pending = lease(f.control(WorkerUpdateControlCommand::BeginSource).await);
        let reg = f.worker.control_registration().unwrap();
        let active = f.supervisor.as_ref().unwrap().record.clone();
        let correct = WorkerUpdateControlRequest {
            control_version: 1,
            binding: reg.binding.clone(),
            update_id: active.spec.update_id.clone(),
            launch_id: active.spec.launch_id.clone(),
            supervisor: active.process.clone().unwrap(),
            updater_owner_id: active.owner_id.clone().unwrap(),
            updater_epoch: active.owner_epoch.unwrap(),
            command: WorkerUpdateControlCommand::Observe,
        };
        for field in [
            "epoch",
            "generation",
            "build",
            "config",
            "incarnation",
            "launch",
        ] {
            let mut q = correct.clone();
            match field {
                "epoch" => q.updater_epoch += 1,
                "generation" => q.binding.generation = uuid::Uuid::new_v4().to_string(),
                "build" => q.binding.build.build_id = "999999".into(),
                "config" => q.binding.config_sha256 = "d".repeat(64),
                "incarnation" => {
                    q.binding.context.journal_incarnation = uuid::Uuid::new_v4().to_string()
                }
                _ => q.launch_id = uuid::Uuid::new_v4().to_string(),
            };
            assert!(
                f.worker
                    .control(&reg, &q, correct.supervisor.process_id)
                    .is_err(),
                "{field}"
            );
        }
        for path in [f.config.clone(), f.updater_config.clone()] {
            let bytes = fs::read(&path).unwrap();
            let mut changed = bytes.clone();
            changed.push(b' ');
            fs::write(&path, changed).unwrap();
            assert!(f
                .advance("blocked-config", 0, WorkerUpdateAction::AcquireSource)
                .await
                .result
                .is_error());
            assert_eq!(
                f.control(WorkerUpdateControlCommand::Observe)
                    .await
                    .into_lease()
                    .unwrap(),
                Some(pending.clone())
            );
            fs::write(path, bytes).unwrap();
        }
        f.worker.store().unwrap().connection.execute_batch("CREATE TRIGGER control_fault BEFORE INSERT ON worker_update_commands BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(f
            .advance("ignored", 0, WorkerUpdateAction::AcquireSource)
            .await
            .result
            .is_error());
        assert_eq!(f.slots(), 0);
        assert_eq!(
            f.control(WorkerUpdateControlCommand::Observe)
                .await
                .into_lease()
                .unwrap(),
            Some(pending)
        );
        f.worker
            .store()
            .unwrap()
            .connection
            .execute_batch("DROP TRIGGER control_fault")
            .unwrap();
        let held = lease(
            f.advance("valid", 0, WorkerUpdateAction::AcquireSource)
                .await,
        );
        assert_eq!(held.phase, WorkerUpdatePhase::SourceHeld);
        let bytes = fs::read(&f.config).unwrap();
        fs::write(&f.config, [bytes.as_slice(), b" "].concat()).unwrap();
        // A saved config cannot issue new grants, but verified release is allowed.
        assert!(f
            .advance("drain-blocked", 1, WorkerUpdateAction::BeginDrain)
            .await
            .result
            .is_error());
        assert_eq!(
            lease(f.advance("release", 1, WorkerUpdateAction::Release).await).phase,
            WorkerUpdatePhase::Released
        );
        f.stop().await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn shutdown_v14_mailbox_rejects_unquiesced_start_and_keeps_older_views_frozen() {
        let mut f = Fixture::new(None).await;
        f.launch().await;
        let h = f.supervisor.as_mut().unwrap();
        let directory = Path::new(&h.record.spec.runtime_directory).join("update-state");
        let before =
            UpdateJournal::read_snapshot(&directory, Some(&h.record.spec.update_id)).unwrap();
        let view = h.candidate_shutdown(false).await.unwrap();
        assert_eq!(view.schema_version, 6);
        let phase = view.candidate_shutdown.unwrap();
        assert_eq!(phase.phase, CandidateShutdownPhase::NotAdmitted);
        assert!(!phase.physical_custody_held);
        assert!(h
            .candidate_shutdown(true)
            .await
            .unwrap_err()
            .contains("Quiesced"));
        assert!(h
            .command(SupervisorAction::ShutdownCandidate)
            .await
            .is_err());
        let old = h.candidate_drain(false).await.unwrap();
        assert!(old.candidate_shutdown.is_none());
        assert!(serde_json::to_value(old)
            .unwrap()
            .get("candidateShutdown")
            .is_none());
        assert_eq!(
            UpdateJournal::read_snapshot(&directory, Some(&h.record.spec.update_id)).unwrap(),
            before
        );
        assert!(!f.worker.stop.load(Ordering::Acquire));
        f.stop().await;
    }
}
