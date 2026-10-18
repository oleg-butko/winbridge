//! Trusted worker launcher and a separate supervisor-only local channel.
//! No network/CLI/MCP route enables this module. Runtime replacement is absent.
use crate::{Result, Worker};
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
}
#[cfg(test)]
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
        #[cfg(not(test))]
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
        if self.config.resource_scheduling.is_some() {
            return Err("Named-resource update reservations are not implemented; source/build grant deferred".into());
        }
        Ok(())
    }
    fn control_authority(
        &self,
        reg: &Registration,
        request: &WorkerUpdateControlRequest,
        peer: u32,
    ) -> Result<UpdateRecord> {
        if request.control_version != 1
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
            || update.request.expected.worker_build_id != reg.binding.build.build_id
            || update.request.expected.worker_config_sha256 != reg.binding.config_sha256
            || sup.spec.config_sha256 != reg.config_sha256
            || matches!(
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
                if command.action != WorkerUpdateAction::Release {
                    self.require_control_current(reg)?;
                }
                self.advance_source_update_checked(&spec, command, &|| {
                    // File verification releases Store. Recheck the external
                    // authority/phase/config before the final grant transaction.
                    let fresh = self.control_authority(reg, request, peer)?;
                    require_parent_phase(fresh.phase, command.action)?;
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
                if matches!(request.command, WorkerUpdateControlCommand::BeginSource) {
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
                worker.control(&registration, &request, peer)
            })
            .await;
            let (lease, error) = match result {
                Ok(Ok(lease)) => (lease, None),
                Ok(Err(e)) => (None, Some(e)),
                Err(e) => (
                    None,
                    Some(format!(
                        "Worker control outcome unconfirmed: {e}; observe original command"
                    )),
                ),
            };
            let response = WorkerUpdateControlResponse {
                control_version: 1,
                binding,
                launch_id,
                update_id,
                result: match error {
                    Some(message) => WorkerUpdateControlResult::Error { message },
                    None => WorkerUpdateControlResult::Snapshot {
                        lease: lease.map(Box::new),
                    },
                },
            };
            let _ = ipc::write(&mut stream, &response).await;
        }
    });
    Ok(Some(ControlServer { task, reg }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    use winbridge_protocol::*;

    struct Fixture {
        worker: Arc<Worker>,
        server: tokio::task::JoinHandle<Result<()>>,
        supervisor: Option<Box<SupervisorHandle>>,
        request: UpdateRequest,
        config: PathBuf,
        updater_config: PathBuf,
        baseline: String,
        _root: tempfile::TempDir,
    }
    impl Fixture {
        async fn new(actual: Option<PathBuf>) -> Self {
            Self::with_source_policy(actual, false).await
        }
        async fn with_source_policy(actual: Option<PathBuf>, enabled: bool) -> Self {
            let root = if enabled {
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
            };
            let source = actual.clone().unwrap_or_else(|| root.path().join("source"));
            let runtime = root.path().join("runtime");
            let other = root.path().join("other");
            for p in [&runtime, &other] {
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
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"instanceId":format!("control-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.path().join("state"),"worker":{"keepRunningOnUiExit":true},"workScheduling":{"maxConcurrentWork":1},"projects":[{"id":"source","workspaceRoot":source,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]},{"id":"other","workspaceRoot":other,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
            let updater_config = runtime.join("winbridge-update.local.json");
            let mut update_config = serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":config});
            let source_pin = if enabled {
                use std::io::Write;
                let packages = root.path().join("packages");
                fs::create_dir(&packages).unwrap();
                update_config["schemaVersion"] = serde_json::json!(2);
                update_config["sourcePackagesDirectory"] = serde_json::json!(packages);
                let next = b"[package]\nname='control-fixture-updated'\nversion='0.2.0'\n";
                let manifest = SourceManifest {
                    schema_version: 1,
                    source_commit: "b".repeat(40),
                    dirty: false,
                    created_at: "2026-10-18T00:00:00Z".into(),
                    includes_built_ui: false,
                    files: vec![SourceFile {
                        path: "Cargo.toml".into(),
                        bytes: next.len() as u64,
                        sha256: winbridge_update::sha256(next),
                    }],
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
                zip.finish().unwrap();
                Some(UpdateSourcePin {
                    archive_name: "fixture.zip".into(),
                    archive_sha256: winbridge_update::sha256(&fs::read(path).unwrap()),
                    manifest_sha256: winbridge_update::sha256(&manifest_bytes),
                    source_commit: manifest.source_commit,
                })
            } else {
                None
            };
            fs::write(&updater_config, serde_json::to_vec(&update_config).unwrap()).unwrap();
            let binary = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(if cfg!(windows) {
                    "winbridge-update.exe"
                } else {
                    "winbridge-update"
                });
            let image = runtime.join(binary.file_name().unwrap());
            fs::copy(&binary, &image).unwrap();
            let image_sha = winbridge_update::sha256(&fs::read(image).unwrap());
            let worker = Worker::open_file(&config).unwrap();
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
                _root: root,
            }
        }
        async fn launch(&mut self) {
            let WorkerSupervisorLaunch::Started(h) = self
                .worker
                .launch_supervisor_inner(&self.request, true)
                .await
                .unwrap()
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
            assert_eq!(
                h.command(SupervisorAction::Stop).await.unwrap().phase,
                SupervisorPhase::Stopped
            );
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
                        let lease = worker
                            .store()
                            .ok()
                            .and_then(|s| s.active_worker_update().ok())
                            .flatten();
                        if let Some(lease) = lease {
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
                        let _ = h.command(SupervisorAction::Stop).await;
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
        assert_eq!(f.slots(), 1);
        let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
        winbridge_update::verify_source(&cfg.source_directory, &f.request.source.manifest_sha256)
            .unwrap();
        // Explicit fixture cleanup after verified source and absent compiler. This
        // is not a production abort API or a release permitted through IPC.
        let release = WorkerUpdateCommand {
            command_id: "source-actor-fixture-release".into(),
            expected_revision: held.revision,
            action: WorkerUpdateAction::Release,
            source_manifest_sha256: Some(f.request.source.manifest_sha256.clone()),
            reason: "Verified source-only fixture cleanup".into(),
        };
        assert_eq!(
            f.worker
                .advance_source_update(&held.spec, &release)
                .unwrap()
                .phase,
            WorkerUpdatePhase::Released
        );
        if let Some(path) = std::env::var_os("WINBRIDGE_SOURCE_PREPARATION_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"supervisor_source_preparation_proof","passed":true,"platform":std::env::consts::OS,"build":build_identity(),"accepted":accepted,"busy":busy,"final":final_view,"duplicate":duplicate,"mailboxResponsive":true,"stopWhileBusyRejected":true,"duplicateObserved":true,"fixtureSourceApplied":true,"workerLeaseRetained":true,"fixtureLeaseReleasedExplicitly":true,"runtimeApplied":false,"windowsOpened":false})).unwrap()).unwrap();
        }
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
}
