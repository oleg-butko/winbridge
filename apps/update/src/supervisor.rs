//! Trusted standalone launcher and authenticated source preparation mailbox.
//! Runtime installation and public update commands remain disabled.
use crate::{
    installation_scope,
    journal::{JournalErrorCode, UpdateAuthority, UpdateJournal},
    load_config, sha256,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use winbridge_platform::{
    execution_domain::ExecutionDomain,
    infrastructure::{self, InfrastructureChild},
};
use winbridge_protocol::update::*;
use winbridge_transport::supervisor as ipc;
type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_millis(),
    )
    .map_err(error)
}
fn local(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    path.canonicalize().map_err(error)
}
fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Updater paths must be UTF-8".into())
}
fn file(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut opt = OpenOptions::new();
    opt.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opt.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let f = opt.open(path).map_err(error)?;
    if !f.metadata().map_err(error)?.is_file() {
        return Err("Updater requires regular file".into());
    }
    Ok(f)
}
pub fn image_sha256(path: &Path) -> Result<String> {
    let mut f = file(path)?;
    if f.metadata().map_err(error)?.len() > 256 * 1024 * 1024 {
        return Err("Updater image exceeds 256 MiB".into());
    }
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut total = 0;
    loop {
        let n = f.read(&mut buf).map_err(error)?;
        if n == 0 {
            break;
        }
        total += n;
        if total > 256 * 1024 * 1024 {
            return Err("Updater image grew beyond limit".into());
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn config_hash(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    file(path)?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 65536 {
        return Err("Updater config too large".into());
    }
    Ok(sha256(&bytes))
}
fn create(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut opt = OpenOptions::new();
    opt.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opt.mode(0o700);
    }
    opt.open(path).map_err(error)
}
fn image_copy(from: &Path, to: &Path, expected: &str) -> Result<()> {
    let mut input = file(from)?;
    let mut out = create(to)?;
    let copied =
        std::io::copy(&mut (&mut input).take(256 * 1024 * 1024 + 1), &mut out).map_err(error)?;
    if copied > 256 * 1024 * 1024 {
        return Err("Image copy exceeds bound".into());
    }
    out.flush().map_err(error)?;
    out.sync_all().map_err(error)?;
    drop(out);
    winbridge_platform::source_files::sync_parent(to).map_err(error)?;
    if image_sha256(to)? != expected {
        return Err("Copied updater image SHA mismatch; staging retained".into());
    }
    Ok(())
}
fn exclusive_dir(path: &Path) -> Result<()> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    fs::create_dir(path).map_err(error)?;
    winbridge_platform::source_files::sync_parent(path).map_err(error)
}
fn validate_inputs(
    record: &SupervisorRecord,
    directory: &Path,
    full_verification: bool,
) -> Result<()> {
    let spec = &record.spec;
    let cfg_path = Path::new(&spec.config_path);
    if config_hash(cfg_path)? != spec.config_sha256 {
        return Err("Updater configuration changed; supervisor cannot acquire effects".into());
    }
    let cfg = load_config(cfg_path)?;
    let runtime = local(&cfg.runtime_directory)?;
    if path_text(&runtime)? != spec.runtime_directory
        || cfg_path.parent() != Some(runtime.as_path())
        || directory != runtime.join("update-state")
    {
        return Err("Supervisor runtime/config/state scope mismatch".into());
    }
    let (journal, update) =
        UpdateJournal::read_snapshot(directory, Some(&spec.update_id)).map_err(error)?;
    let update = update.ok_or("Update missing")?;
    if let Some(worker) = &spec.worker {
        if worker.process != spec.parent
            || worker.build.build_id != update.request.expected.worker_build_id
            || worker.config_sha256 != update.request.expected.worker_config_sha256
        {
            return Err("Worker launcher binding differs from accepted installation".into());
        }
    }
    if installation_scope(&cfg)? != journal.scope_sha256
        || journal.scope_sha256 != update.request.expected.scope_sha256
        || config_hash(&cfg.worker_config_path)? != update.request.expected.worker_config_sha256
        || update.phase != UpdatePhase::Requested
    {
        return Err("Supervisor accepted installation scope/phase changed".into());
    }
    if full_verification {
        crate::verify_source(
            &cfg.source_directory,
            &update.request.expected.baseline_manifest_sha256,
        )?;
    }
    let image = Path::new(&spec.image_path);
    let helper = runtime
        .join("update")
        .join(&spec.update_id)
        .join("supervisor");
    if image.parent() != Some(helper.as_path())
        || (full_verification && image_sha256(image)? != spec.image_sha256)
        || winbridge_platform::file_identity(&file(image)?).map_err(error)? != spec.image_identity
    {
        return Err("Supervisor image/path/identity changed".into());
    }
    Ok(())
}
pub struct PreparedSupervisor {
    record: SupervisorRecord,
    token: String,
    stdout: File,
    stderr: File,
    _image: File,
}
pub enum Preparation {
    New(Box<PreparedSupervisor>),
    Existing(Box<SupervisorRecord>),
}
/// This must be called by a trusted installation driver, never a command task.
pub fn prepare(
    journal: &mut UpdateJournal,
    owner: &UpdateAuthority,
    id: &str,
    config_path: &Path,
    expected_image_sha256: &str,
    domain: Option<&ExecutionDomain>,
) -> Result<Preparation> {
    prepare_bound(
        journal,
        owner,
        id,
        config_path,
        expected_image_sha256,
        domain,
        None,
    )
}
/// Only the worker launcher supplies this binding, never command JSON.
pub fn prepare_for_worker(
    journal: &mut UpdateJournal,
    owner: &UpdateAuthority,
    id: &str,
    config_path: &Path,
    expected_image_sha256: &str,
    domain: Option<&ExecutionDomain>,
    worker: SupervisorWorkerBinding,
) -> Result<Preparation> {
    prepare_bound(
        journal,
        owner,
        id,
        config_path,
        expected_image_sha256,
        domain,
        Some(worker),
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare_bound(
    journal: &mut UpdateJournal,
    owner: &UpdateAuthority,
    id: &str,
    config_path: &Path,
    expected_image_sha256: &str,
    domain: Option<&ExecutionDomain>,
    worker: Option<SupervisorWorkerBinding>,
) -> Result<Preparation> {
    journal.check_source_authority(owner).map_err(error)?;
    match journal.supervisor(id) {
        Ok(r) => return Ok(Preparation::Existing(Box::new(r))),
        Err(e) if e.code == JournalErrorCode::NotFound => {}
        Err(e) => return Err(error(e)),
    }
    infrastructure::require_parent_outside(domain).map_err(error)?;
    winbridge_platform::require_local_path_prefix(config_path).map_err(error)?;
    winbridge_platform::require_local_disk(config_path).map_err(error)?;
    let cfg = load_config(config_path)?;
    let runtime = local(&cfg.runtime_directory)?;
    let config_path = local(config_path)?;
    if config_path != local(&runtime.join("winbridge-update.local.json"))? {
        return Err("Supervisor uses winbridge-update.local.json beside installed updater".into());
    }
    let update = journal.get(id).map_err(error)?;
    let scope = journal.info().map_err(error)?;
    if update.phase != UpdatePhase::Requested || installation_scope(&cfg)? != scope.scope_sha256 {
        return Err("Supervisor needs the accepted Requested installation".into());
    }
    #[cfg(windows)]
    let filename = "winbridge-update.exe";
    #[cfg(not(windows))]
    let filename = "winbridge-update";
    let installed = runtime.join(filename);
    if image_sha256(&installed)? != expected_image_sha256 {
        return Err("Installed updater image differs from pinned release".into());
    }
    let updates = runtime.join("update");
    winbridge_platform::require_safe_path(&updates).map_err(error)?;
    if !updates.exists() {
        exclusive_dir(&updates)?;
    }
    let run = updates.join(id);
    if !run.exists() {
        exclusive_dir(&run)?;
    }
    let helper = run.join("supervisor");
    exclusive_dir(&helper)?;
    let image = helper.join(filename);
    image_copy(&installed, &image, expected_image_sha256)?;
    let stdout = create(&helper.join("stdout.log"))?;
    let stderr = create(&helper.join("stderr.log"))?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let launch_id = uuid::Uuid::new_v4().to_string();
    let spec = SupervisorSpec {
        launch_id: launch_id.clone(),
        update_id: id.into(),
        parent: infrastructure::stamp(std::process::id()).map_err(error)?,
        parent_owner_id: owner.owner_id().into(),
        parent_epoch: owner.epoch(),
        token_sha256: sha256(token.as_bytes()),
        config_path: path_text(&config_path)?,
        config_sha256: config_hash(&config_path)?,
        runtime_directory: path_text(&runtime)?,
        image_path: path_text(&local(&image)?)?,
        image_sha256: expected_image_sha256.into(),
        image_identity: winbridge_platform::file_identity(&file(&image)?).map_err(error)?,
        excluded_domain: domain.map(|d| d.identity().job_name.clone()),
        endpoint: ipc::endpoint(&runtime.join("update-state"), &launch_id).map_err(error)?,
        accepted_at_ms: now()?,
        worker,
    };
    let record = journal.prepare_supervisor(owner, spec).map_err(error)?;
    validate_inputs(&record, &runtime.join("update-state"), false)?;
    Ok(Preparation::New(Box::new(PreparedSupervisor {
        record,
        token,
        stdout,
        stderr,
        _image: file(&image)?,
    })))
}
pub struct SupervisorHandle {
    pub record: SupervisorRecord,
    child: InfrastructureChild,
    directory: PathBuf,
    token: String,
    handoff_sent: bool,
}
impl PreparedSupervisor {
    pub fn record(&self) -> &SupervisorRecord {
        &self.record
    }
    pub fn spawn(
        self,
        journal: &mut UpdateJournal,
        owner: UpdateAuthority,
        domain: Option<&ExecutionDomain>,
    ) -> Result<SupervisorHandle> {
        let spec = &self.record.spec;
        let directory = Path::new(&spec.runtime_directory).join("update-state");
        validate_inputs(&self.record, &directory, true)?;
        journal.check_source_authority(&owner).map_err(error)?;
        if journal.supervisor(&spec.launch_id).map_err(error)? != self.record
            || domain.map(|d| d.identity().job_name.as_str()) != spec.excluded_domain.as_deref()
        {
            return Err("Supervisor intent/domain changed; no spawn".into());
        }
        let args = vec![
            "--config".into(),
            spec.config_path.clone(),
            "supervisor-internal".into(),
            spec.launch_id.clone(),
            self.token.clone(),
        ];
        let child = match InfrastructureChild::spawn(
            Path::new(&spec.image_path),
            &args,
            Path::new(&spec.runtime_directory),
            self.stdout,
            self.stderr,
            domain,
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = journal.advance_supervisor(
                    &owner,
                    &spec.launch_id,
                    0,
                    SupervisorPhase::Indeterminate,
                    None,
                    "Запуск не подтверждён; повтор запрещён, требуется диагностика",
                );
                return Err(format!(
                    "Supervisor creation unconfirmed: {e}; do not spawn again"
                ));
            }
        };
        let record = journal
            .advance_supervisor(
                &owner,
                &spec.launch_id,
                0,
                SupervisorPhase::Spawned,
                Some(child.identity.clone()),
                "Отдельный процесс создан; ожидается его подтверждение, effects запрещены",
            )
            .map_err(error)?;
        drop(owner); // No inherited owner lock; child must acquire exactly the next epoch.
        Ok(SupervisorHandle {
            record,
            child,
            directory,
            token: self.token,
            handoff_sent: false,
        })
    }
}
impl SupervisorHandle {
    /// The child, not the caller, uses its own OS identity/epoch on the worker channel.
    pub async fn worker_control(
        &mut self,
        command: WorkerUpdateControlCommand,
    ) -> Result<WorkerUpdateControlResponse> {
        if self.record.phase != SupervisorPhase::Active {
            return Err("Worker control requires Active supervisor".into());
        }
        if self.record.spec.worker.is_none() {
            return Err("No worker bound to this supervisor".into());
        }
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 1,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::WorkerControl,
            token: self.token.clone(),
            worker_command: Some(command),
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 1
            || response.launch_id != request.launch_id
            || response.record.is_some()
        {
            return Err("Supervisor control response binding changed; observe, no replay".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let control = response
            .worker_control
            .ok_or("Worker control response missing")?;
        if Some(&control.binding) != self.record.spec.worker.as_ref()
            || control.update_id != self.record.spec.update_id
            || control.launch_id != self.record.spec.launch_id
        {
            return Err("Forwarded worker control response belongs to another scope".into());
        }
        Ok(control)
    }
    pub async fn preparation(&mut self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed; no command".into());
        }
        let q = SupervisorRequest {
            contract_version: 2,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::PrepareSource
            } else {
                SupervisorAction::ObservePreparation
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &q,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 2
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Preparation reply scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Preparation view missing")?;
        if view.schema_version != 1 || view.update.update_id != self.record.spec.update_id {
            return Err("Preparation view belongs to another update".into());
        }
        Ok(view)
    }
    pub async fn wait_ready(&mut self) -> Result<SupervisorRecord> {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if self.child.exited().map_err(error)? {
                return Err(
                    "Supervisor exited before Ready; inspect stderr.log and retained launch intent"
                        .into(),
                );
            }
            let r = UpdateJournal::inspect_supervisor(&self.directory, &self.record.spec.launch_id)
                .map_err(error)?;
            if r.phase == SupervisorPhase::Ready {
                if r.process.as_ref() != Some(&self.child.identity)
                    || r.owner_epoch != Some(r.spec.parent_epoch + 1)
                {
                    return Err("Supervisor ready identity/epoch mismatch".into());
                }
                self.record = r.clone();
                return Ok(r);
            }
            if matches!(
                r.phase,
                SupervisorPhase::Stopped | SupervisorPhase::Indeterminate
            ) {
                return Err("Supervisor startup stopped; no automatic retry".into());
            }
            if Instant::now() >= deadline {
                return Err(
                    "Supervisor Ready timeout; inspect both launch and process, no replay".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    pub async fn command(&mut self, action: SupervisorAction) -> Result<SupervisorRecord> {
        if matches!(
            action,
            SupervisorAction::PrepareSource | SupervisorAction::ObservePreparation
        ) {
            return Err("Use the typed v2 preparation method for this action".into());
        }
        if action == SupervisorAction::Handoff {
            if self.handoff_sent || self.record.phase != SupervisorPhase::Ready {
                return Err("Handoff requires Ready".into());
            }
            self.handoff_sent = true;
            self.child.detached();
        }
        let request = SupervisorRequest {
            contract_version: 1,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action,
            token: self.token.clone(),
            worker_command: None,
        };
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed; no command".into());
        }
        let response: SupervisorResponse = ipc::call(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 1 || response.launch_id != request.launch_id {
            return Err("Supervisor response contract/scope mismatch".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let r = response.record.ok_or("Supervisor record missing")?;
        if r.spec != self.record.spec || r.process.as_ref() != Some(&self.child.identity) {
            return Err("Supervisor response identity mismatch".into());
        }
        self.record = r.clone();
        Ok(r)
    }
}
pub(crate) async fn forward_worker(
    record: &SupervisorRecord,
    command: WorkerUpdateControlCommand,
) -> Result<WorkerUpdateControlResponse> {
    let binding = record
        .spec
        .worker
        .clone()
        .ok_or("No worker bound to this supervisor")?;
    if !infrastructure::matches(&binding.process).map_err(error)? {
        return Err("Bound worker process changed; no rebind".into());
    }
    let request = WorkerUpdateControlRequest {
        control_version: 1,
        binding: binding.clone(),
        update_id: record.spec.update_id.clone(),
        launch_id: record.spec.launch_id.clone(),
        supervisor: record.process.clone().ok_or("Supervisor process missing")?,
        updater_owner_id: record.owner_id.clone().ok_or("Supervisor owner missing")?,
        updater_epoch: record.owner_epoch.ok_or("Supervisor epoch missing")?,
        command,
    };
    let response: WorkerUpdateControlResponse = ipc::call_with_timeout(
        &binding.endpoint,
        binding.process.process_id,
        &request,
        ipc::CONTROL_TIMEOUT - ipc::TIMEOUT,
    )
    .await
    .map_err(error)?;
    if response.control_version != 1
        || response.binding != binding
        || response.update_id != request.update_id
        || response.launch_id != request.launch_id
    {
        return Err("Worker control response scope/build changed; outcome unconfirmed".into());
    }
    Ok(response)
}
fn authorize(r: &SupervisorRecord, token: &str, pid: u32, action: SupervisorAction) -> Result<()> {
    if token.len() != 64
        || sha256(token.as_bytes()) != r.spec.token_sha256
        || (action == SupervisorAction::Handoff
            && (pid != r.spec.parent.process_id
                || !infrastructure::matches(&r.spec.parent).map_err(error)?))
    {
        return Err("Supervisor caller process/nonce does not match launch authority".into());
    }
    Ok(())
}
/// Gated internal entry; source preparation requires a v2 action and frozen policy.
/// Runtime replacement/shutdown are not registered.
pub async fn run(config_path: &Path, id: &str, token: &str) -> Result<()> {
    if token.len() != 64 {
        return Err("Invalid supervisor startup token".into());
    }
    winbridge_platform::require_local_path_prefix(config_path).map_err(error)?;
    winbridge_platform::require_local_disk(config_path).map_err(error)?;
    let cfg = load_config(config_path)?;
    let runtime = local(&cfg.runtime_directory)?;
    let directory = runtime.join("update-state");
    let this = infrastructure::stamp(std::process::id()).map_err(error)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let r = loop {
        let r = UpdateJournal::inspect_supervisor(&directory, id).map_err(error)?;
        if sha256(token.as_bytes()) != r.spec.token_sha256 {
            return Err("Supervisor startup nonce mismatch".into());
        }
        match r.phase {
            SupervisorPhase::Intent => {}
            SupervisorPhase::Spawned if r.process.as_ref() == Some(&this) => break r,
            _ => return Err("Supervisor spawn receipt/process not authorized; no effects".into()),
        }
        if Instant::now() >= deadline {
            return Err(
                "No durable spawn receipt within deadline; no effects, do not replay".into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    validate_inputs(&r, &directory, true)?;
    if local(&std::env::current_exe().map_err(error)?)? != local(Path::new(&r.spec.image_path))? {
        return Err("Supervisor is not the pinned copied image".into());
    }
    let mut listener = ipc::Listener::bind(&r.spec.endpoint).map_err(error)?;
    let mut journal = UpdateJournal::open(&directory, &installation_scope(&cfg)?).map_err(error)?;
    let owner = loop {
        match journal.acquire_owner() {
            Ok(a) => break a,
            Err(e) if e.code == JournalErrorCode::OwnerBusy && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await
            }
            Err(e) => return Err(error(e)),
        }
    };
    let mut r = journal
        .advance_supervisor(
            &owner,
            id,
            r.revision,
            SupervisorPhase::Ready,
            None,
            "Supervisor владеет отдельным журналом; ожидает handoff, effects запрещены",
        )
        .map_err(error)?;
    let mut idle = Some((journal, owner));
    type Work = tokio::task::JoinHandle<(UpdateJournal, UpdateAuthority, Result<()>)>;
    let mut work: Option<Work> = None;
    let mut driver_error: Option<String> = None;
    let ready_deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if work.as_ref().is_some_and(|w| w.is_finished()) {
            let (journal, owner, outcome) = work.take().unwrap().await.map_err(error)?;
            driver_error = outcome
                .err()
                .map(|e| e.chars().take(1000).collect::<String>());
            if let Some(e) = &driver_error {
                eprintln!("Source preparation stopped: {e}; preserve both journals and barrier, no automatic retry");
            }
            idle = Some((journal, owner));
        }
        if let Some((journal, owner)) = &idle {
            journal.check_source_authority(owner).map_err(error)?;
        } else {
            let (info, saved, _, current_owner) =
                UpdateJournal::supervisor_authority_snapshot(&directory, id).map_err(error)?;
            if saved != r
                || Some(info.owner_epoch) != r.owner_epoch
                || Some(&current_owner) != r.owner_id.as_ref()
            {
                return Err("Preparation authority changed; no replay or takeover".into());
            }
        }
        if r.phase == SupervisorPhase::Ready && Instant::now() >= ready_deadline {
            let (journal, owner) = idle
                .as_mut()
                .ok_or("Ready supervisor cannot own an active source driver")?;
            journal
                .advance_supervisor(
                    owner,
                    id,
                    r.revision,
                    SupervisorPhase::Stopped,
                    None,
                    "Handoff не получен; supervisor завершён без source/runtime effects",
                )
                .map_err(error)?;
            return Ok(());
        }
        let accepted = tokio::time::timeout(Duration::from_millis(100), listener.accept()).await;
        let Ok(connection) = accepted else { continue };
        let (mut stream, pid) = match connection {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => return Err(error(e)),
        };
        let request: SupervisorRequest = match ipc::read(&mut stream).await {
            Ok(q) => q,
            Err(_) => continue,
        };
        if matches!(
            request.action,
            SupervisorAction::PrepareSource | SupervisorAction::ObservePreparation
        ) {
            let mut start = false;
            let outcome = (|| {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 2
                    || request.launch_id != r.spec.launch_id
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                    || request.expected_revision > r.revision
                {
                    return Err("Preparation needs exact Active worker-bound v2 supervisor".into());
                }
                if request.action == SupervisorAction::PrepareSource {
                    if request.expected_revision != r.revision {
                        return Err("Preparation supervisor revision changed".into());
                    }
                    let snapshot = UpdateJournal::preparation_snapshot(
                        &directory,
                        &r.spec.update_id,
                        work.is_some(),
                        driver_error.clone(),
                    )
                    .map_err(error)?;
                    if snapshot.update.phase == UpdatePhase::Requested && work.is_none() {
                        let cfg = crate::preparation::configuration(&r)?;
                        if cfg.source_packages_directory.is_none() {
                            return Err("Configure sourcePackagesDirectory with updater config schema 2 before preparation".into());
                        }
                        let (journal, owner) = idle
                            .as_mut()
                            .ok_or("Preparation driver already owns the journal")?;
                        journal.advance(owner,&r.spec.update_id,&UpdateCommand { idempotency_key: "supervisor-source-preparation-v1".into(), expected_revision: snapshot.update.revision, event: UpdateEvent::BeginPreparation, reason: "Подготовка исходников принята supervisor; установка runtime не выполняется".into() }).map_err(error)?;
                        start = true;
                    }
                }
                UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some() || start,
                    driver_error.clone(),
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 2,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = start && response.error.is_none();
            // Accepted work survives a lost ACK. Intent precedes dispatch even then.
            let _ = ipc::write(&mut stream, &response).await;
            if dispatch {
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Preparation owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = crate::preparation::source(&mut journal, &owner, &spec, io);
                    (journal, owner, result)
                }));
            }
            continue;
        }
        if work.is_some()
            && !(request.action == SupervisorAction::Observe
                || request.action == SupervisorAction::WorkerControl
                    && matches!(
                        request.worker_command,
                        Some(WorkerUpdateControlCommand::Observe)
                    ))
        {
            let busy_error = (|| -> Result<()> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 1 || request.launch_id != r.spec.launch_id {
                    return Err("Supervisor contract/scope mismatch".into());
                }
                Err(
                    "Source driver active; mutation/stop deferred, inspect preparation status"
                        .into(),
                )
            })()
            .unwrap_err();
            let response = SupervisorResponse {
                contract_version: 1,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error: Some(busy_error),
                worker_control: None,
                preparation: None,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if request.action == SupervisorAction::WorkerControl
            || (request.action == SupervisorAction::Stop
                && r.phase == SupervisorPhase::Active
                && r.spec.worker.is_some())
        {
            let result = async {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 1
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || r.phase != SupervisorPhase::Active
                {
                    return Err("Worker control requires exact Active supervisor revision/scope".into());
                }
                if request.action == SupervisorAction::Stop {
                    if request.worker_command.is_some() {
                        return Err("Stop cannot carry a worker command".into());
                    }
                    let observation = forward_worker(&r, WorkerUpdateControlCommand::Observe).await?;
                    if observation.into_lease().map_or(true, |lease| lease.is_some_and(|l| l.phase != WorkerUpdatePhase::Released))
                    {
                        return Err("Supervisor still owns a worker lease, or its release is unknown; preserve both journals".into());
                    }
                    return Ok(None);
                }
                let command = request.worker_command.clone().ok_or("Worker command missing")?;
                forward_worker(&r, command).await.map(Some)
            }.await;
            match result {
                Ok(None) => {} // Verified no outstanding worker lease; normal Stop follows.
                outcome => {
                    let (worker_control, error) = match outcome {
                        Ok(v) => (v, None),
                        Err(e) => (None, Some(e)),
                    };
                    let response = SupervisorResponse {
                        contract_version: 1,
                        launch_id: r.spec.launch_id.clone(),
                        // Avoid repeating the potentially large immutable launch spec
                        // alongside the worker lease in a bounded control reply.
                        record: None,
                        error,
                        worker_control,
                        preparation: None,
                    };
                    let _ = ipc::write(&mut stream, &response).await;
                    continue;
                }
            }
        }
        let result = (|| {
            authorize(&r, &request.token, pid, request.action)?;
            if request.contract_version != 1 || request.launch_id != r.spec.launch_id {
                return Err("Supervisor contract/scope mismatch".into());
            }
            if request.worker_command.is_some() {
                return Err("Unexpected worker command on supervisor lifecycle action".into());
            }
            if request.action == SupervisorAction::Observe {
                if request.expected_revision > r.revision {
                    return Err("Future supervisor revision".into());
                }
                return Ok(r.clone());
            }
            let (journal, owner) = idle
                .as_mut()
                .ok_or("Preparation driver active; mutation deferred")?;
            match request.action {
                SupervisorAction::Observe => {
                    if request.expected_revision > r.revision {
                        return Err("Future supervisor revision".into());
                    }
                    Ok(r.clone())
                }
                SupervisorAction::Handoff => {
                    if r.phase != SupervisorPhase::Active {
                        validate_inputs(&r, &directory, false)?;
                    }
                    journal.advance_supervisor(
                        owner, id, request.expected_revision, SupervisorPhase::Active, None,
                        "Handoff сохранён; supervisor продолжает работу после parent exit. Дальнейшие действия требуют отдельных проверенных команд",
                    ).map_err(error)
                }
                SupervisorAction::WorkerControl | SupervisorAction::PrepareSource | SupervisorAction::ObservePreparation => Err("Action needs its separate handler".into()),
                SupervisorAction::Stop => journal.advance_supervisor(
                    owner, id, request.expected_revision, SupervisorPhase::Stopped, None,
                    "Supervisor остановлен после проверки worker lease; установка runtime не выполнялась",
                ).map_err(error),
            }
        })();
        let response = match result {
            Ok(record) => {
                r = record.clone();
                SupervisorResponse {
                    contract_version: 1,
                    launch_id: r.spec.launch_id.clone(),
                    record: Some(record),
                    error: None,
                    worker_control: None,
                    preparation: None,
                }
            }
            Err(e) => SupervisorResponse {
                contract_version: 1,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error: Some(e),
                worker_control: None,
                preparation: None,
            },
        };
        // Lost ACK does not cancel committed Active. The next Observe is readonly.
        let _ = ipc::write(&mut stream, &response).await;
        if r.phase == SupervisorPhase::Stopped {
            return Ok(());
        }
    }
}

pub fn inspect(directory: &Path, id: &str) -> Result<serde_json::Value> {
    let (info, record, update) =
        UpdateJournal::supervisor_snapshot(directory, id).map_err(error)?;
    let physical = record
        .process
        .as_ref()
        .map(|p| match infrastructure::matches(p) {
            Ok(true) => "alive",
            Ok(false) => "different_process",
            Err(_) => "unavailable",
        });
    let external_job = record
        .process
        .as_ref()
        .and_then(|p| infrastructure::job_observation(p).ok().flatten());
    let source_preparation_configured = record.spec.worker.is_some()
        && crate::preparation::configuration(&record)
            .is_ok_and(|c| c.source_packages_directory.is_some());
    Ok(
        serde_json::json!({"sourcePreparationConfigured":source_preparation_configured,"publicUpdateCapabilityEnabled":false,"externalJobObserved":external_job,"schemaVersion":1,"kind":"supervisor_status","readOnly":true,"sourceEffectsEnabled":false,"runtimeApplied":false,"journalEpochCurrent":record.owner_epoch.is_some_and(|e|e==info.owner_epoch),"update":update,"journal":info,"physicalObservation":physical,"record":record}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::process::{Command, Stdio};
    struct Fixture {
        root: PathBuf,
        config: PathBuf,
        journal: UpdateJournal,
        owner: Option<UpdateAuthority>,
        id: String,
        hash: String,
        domain: Option<ExecutionDomain>,
    }
    impl Fixture {
        fn new(root: &Path) -> Self {
            Self::new_source(root, None)
        }
        fn new_source(root: &Path, actual: Option<&Path>) -> Self {
            let source = actual
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join("source"));
            let runtime = root.join("runtime");
            fs::create_dir_all(&runtime).unwrap();
            let bytes = if actual.is_some() {
                fs::read(source.join("winbridge-source-manifest.json")).unwrap()
            } else {
                fs::create_dir_all(&source).unwrap();
                fs::write(source.join("Cargo.toml"), b"fixture").unwrap();
                let m = SourceManifest {
                    schema_version: 1,
                    source_commit: "a".repeat(40),
                    dirty: false,
                    created_at: "2026-10-17T00:00:00Z".into(),
                    includes_built_ui: false,
                    files: vec![SourceFile {
                        path: "Cargo.toml".into(),
                        bytes: 7,
                        sha256: sha256(b"fixture"),
                    }],
                };
                let bytes = serde_json::to_vec(&m).unwrap();
                fs::write(source.join("winbridge-source-manifest.json"), &bytes).unwrap();
                bytes
            };
            let worker = runtime.join("worker.local.json");
            fs::write(&worker, b"{\"testFixture\":true}").unwrap();
            let config = runtime.join("winbridge-update.local.json");
            fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":worker})).unwrap()).unwrap();
            let cfg = load_config(&config).unwrap();
            let mut journal = UpdateJournal::open(
                &runtime.join("update-state"),
                &installation_scope(&cfg).unwrap(),
            )
            .unwrap();
            let owner = journal.acquire_owner().unwrap();
            let j = journal.info().unwrap();
            let req = UpdateRequest {
                contract_version: 1,
                idempotency_key: "supervisor-test".into(),
                source: UpdateSourcePin {
                    archive_name: "source.zip".into(),
                    archive_sha256: "b".repeat(64),
                    manifest_sha256: "c".repeat(64),
                    source_commit: "d".repeat(40),
                },
                expected: UpdateInstallationPin {
                    installation_id: j.installation_id,
                    scope_sha256: j.scope_sha256,
                    worker_build_id: "1234".into(),
                    worker_config_sha256: sha256(&fs::read(worker).unwrap()),
                    baseline_manifest_sha256: sha256(&bytes),
                },
            };
            let id = journal
                .accept(&owner, &req, &req.expected)
                .unwrap()
                .update_id;
            let image = std::env::current_exe()
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
            assert!(
                image.is_file(),
                "Cargo integration binary missing: {}",
                image.display()
            );
            let installed = runtime.join(image.file_name().unwrap());
            fs::copy(image, &installed).unwrap();
            let hash = image_sha256(&installed).unwrap();
            let domain = ExecutionDomain::identity_for(
                "supervisor-native-tests",
                &uuid::Uuid::new_v4().to_string(),
            )
            .unwrap()
            .map(|i| ExecutionDomain::open(&i).unwrap());
            Self {
                root: root.into(),
                config,
                journal,
                owner: Some(owner),
                id,
                hash,
                domain,
            }
        }
        fn prepare(&mut self) -> Box<PreparedSupervisor> {
            let Preparation::New(p) = prepare(
                &mut self.journal,
                self.owner.as_ref().unwrap(),
                &self.id,
                &self.config,
                &self.hash,
                self.domain.as_ref(),
            )
            .unwrap() else {
                panic!("New launch")
            };
            p
        }
        fn spawn(&mut self) -> SupervisorHandle {
            let p = self.prepare();
            let h = p
                .spawn(
                    &mut self.journal,
                    self.owner.take().unwrap(),
                    self.domain.as_ref(),
                )
                .unwrap();
            fs::write(
                self.root.join("test-control.json"),
                serde_json::to_vec(&serde_json::json!({"record":h.record,"token":h.token}))
                    .unwrap(),
            )
            .unwrap();
            h
        }
        fn state(&self) -> PathBuf {
            self.root.join("runtime/update-state")
        }
        fn db(&self) -> Connection {
            Connection::open(self.state().join("updates.sqlite")).unwrap()
        }
    }
    struct Parent(std::process::Child);
    impl Drop for Parent {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            cleanup(&self.0);
        }
    }
    fn cleanup(root: &Path) {
        let Ok(bytes) = fs::read(root.join("test-control.json")) else {
            return;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return;
        };
        let Ok(record) = serde_json::from_value::<SupervisorRecord>(value["record"].clone()) else {
            return;
        };
        let Some(token) = value["token"].as_str().map(str::to_owned) else {
            return;
        };
        let state = root.join("runtime/update-state");
        let _ = std::thread::spawn(move || {
            let Ok(current) = UpdateJournal::inspect_supervisor(&state, &record.spec.launch_id)
            else {
                return;
            };
            if !matches!(
                current.phase,
                SupervisorPhase::Ready | SupervisorPhase::Active
            ) {
                return;
            }
            let request = SupervisorRequest {
                contract_version: 1,
                launch_id: current.spec.launch_id,
                expected_revision: current.revision,
                action: SupervisorAction::Stop,
                token,
                worker_command: None,
            };
            if let Some(process) = current.process {
                let _ = tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(ipc::call::<_, SupervisorResponse>(
                        &record.spec.endpoint,
                        process.process_id,
                        &request,
                    ));
            }
        })
        .join();
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            cleanup(&self.root);
        }
    }
    async fn stopped(h: &mut SupervisorHandle) {
        let r = h.command(SupervisorAction::Stop).await.unwrap();
        assert_eq!(r.phase, SupervisorPhase::Stopped);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !h.child.exited().unwrap() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    #[tokio::test]
    async fn handoff_is_durable_unique_and_does_not_allow_source_effects() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(dir.path());
        let mut h = f.spawn();
        let ready = h.wait_ready().await.unwrap();
        assert_eq!(ready.owner_epoch, Some(ready.spec.parent_epoch + 1));
        assert!(matches!(f.journal.acquire_owner(),Err(e) if e.code==JournalErrorCode::OwnerBusy));
        let bad = SupervisorRequest {
            contract_version: 1,
            launch_id: ready.spec.launch_id.clone(),
            expected_revision: ready.revision,
            action: SupervisorAction::Handoff,
            token: "0".repeat(64),
            worker_command: None,
        };
        let rejected: SupervisorResponse =
            ipc::call(&ready.spec.endpoint, h.child.identity.process_id, &bad)
                .await
                .unwrap();
        assert!(rejected.error.is_some());
        assert_eq!(
            UpdateJournal::inspect_supervisor(&f.state(), &f.id)
                .unwrap()
                .phase,
            SupervisorPhase::Ready
        );
        let active = h.command(SupervisorAction::Handoff).await.unwrap();
        assert_eq!(active.phase, SupervisorPhase::Active);
        assert_eq!(h.command(SupervisorAction::Observe).await.unwrap(), active);
        assert!(h.command(SupervisorAction::Handoff).await.is_err());
        let out = Command::new(f.root.join("runtime").join(if cfg!(windows) {
            "winbridge-update.exe"
        } else {
            "winbridge-update"
        }))
        .arg("--config")
        .arg(&f.config)
        .args(["journal", "supervisor-status", &f.id])
        .output()
        .unwrap();
        assert!(out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["sourceEffectsEnabled"], false);
        assert_eq!(v["journalEpochCurrent"], true);
        assert_eq!(v["physicalObservation"], "alive");
        stopped(&mut h).await;
        let owner = f.journal.acquire_owner().unwrap();
        assert_eq!(
            f.journal.get(&f.id).unwrap().phase,
            UpdatePhase::FailedBeforeCutover
        );
        assert!(matches!(
            prepare(
                &mut f.journal,
                &owner,
                &f.id,
                &f.config,
                &f.hash,
                f.domain.as_ref()
            )
            .unwrap(),
            Preparation::Existing(_)
        ));
        assert_eq!(
            fs::read(f.root.join("source/Cargo.toml")).unwrap(),
            b"fixture"
        );
    }
    #[tokio::test]
    async fn lost_handoff_ack_is_observed_without_replay_or_killing_supervisor() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(dir.path());
        let mut h = f.spawn();
        h.wait_ready().await.unwrap();
        let request = SupervisorRequest {
            contract_version: 1,
            launch_id: h.record.spec.launch_id.clone(),
            expected_revision: h.record.revision,
            action: SupervisorAction::Handoff,
            token: h.token.clone(),
            worker_command: None,
        };
        let wrong = ipc::call::<_, SupervisorResponse>(
            &h.record.spec.endpoint,
            std::process::id(),
            &request,
        )
        .await;
        assert!(wrong.is_err());
        h.child.detached();
        let mut stream = ipc::connect(&h.record.spec.endpoint, h.child.identity.process_id)
            .await
            .unwrap();
        winbridge_transport::write_frame(&mut stream, &serde_json::to_vec(&request).unwrap())
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if UpdateJournal::inspect_supervisor(&f.state(), &f.id)
                .unwrap()
                .phase
                == SupervisorPhase::Active
            {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(stream); // Deliberately never consume the already committed ACK.
        assert_eq!(
            h.command(SupervisorAction::Observe).await.unwrap().phase,
            SupervisorPhase::Active
        );
        assert!(infrastructure::matches(&h.child.identity).unwrap());
        stopped(&mut h).await;
    }
    #[tokio::test]
    async fn ignored_launch_and_handoff_writes_never_ack_or_duplicate() {
        for point in ["intent", "spawned", "active"] {
            let dir = tempfile::tempdir().unwrap();
            let mut f = Fixture::new(dir.path());
            if point == "intent" {
                f.db().execute_batch("CREATE TRIGGER fail BEFORE INSERT ON supervisor_receipts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
                assert!(prepare(
                    &mut f.journal,
                    f.owner.as_ref().unwrap(),
                    &f.id,
                    &f.config,
                    &f.hash,
                    f.domain.as_ref()
                )
                .is_err());
                assert!(
                    matches!(f.journal.supervisor(&f.id),Err(e) if e.code==JournalErrorCode::NotFound)
                );
                continue;
            }
            let p = f.prepare();
            if point == "spawned" {
                f.db().execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON supervisor_launches BEGIN SELECT RAISE(IGNORE); END;").unwrap();
                assert!(p
                    .spawn(&mut f.journal, f.owner.take().unwrap(), f.domain.as_ref())
                    .is_err());
                assert_eq!(
                    f.journal.supervisor(&f.id).unwrap().phase,
                    SupervisorPhase::Intent
                );
                let a = f.journal.acquire_owner().unwrap();
                assert!(matches!(
                    prepare(
                        &mut f.journal,
                        &a,
                        &f.id,
                        &f.config,
                        &f.hash,
                        f.domain.as_ref()
                    )
                    .unwrap(),
                    Preparation::Existing(_)
                ));
            } else {
                let mut h = p
                    .spawn(&mut f.journal, f.owner.take().unwrap(), f.domain.as_ref())
                    .unwrap();
                h.wait_ready().await.unwrap();
                f.db().execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON supervisor_launches WHEN json_extract(NEW.record,'$.phase')='active' BEGIN SELECT RAISE(IGNORE); END;").unwrap();
                assert!(h.command(SupervisorAction::Handoff).await.is_err());
                assert_eq!(
                    UpdateJournal::inspect_supervisor(&f.state(), &f.id)
                        .unwrap()
                        .phase,
                    SupervisorPhase::Ready
                );
                stopped(&mut h).await;
            }
        }
    }
    #[test]
    fn changed_config_and_image_block_spawn_and_preserve_intent() {
        for which in ["config", "image"] {
            let dir = tempfile::tempdir().unwrap();
            let mut f = Fixture::new(dir.path());
            let p = f.prepare();
            assert!(f
                .journal
                .advance(
                    f.owner.as_ref().unwrap(),
                    &f.id,
                    &UpdateCommand {
                        idempotency_key: "premature-source".into(),
                        expected_revision: 0,
                        event: UpdateEvent::BeginPreparation,
                        reason: "Must wait for handoff".into()
                    }
                )
                .is_err());
            #[cfg(windows)]
            let mut p = p;
            if which == "config" {
                fs::write(&f.config, b"{}").unwrap();
            } else {
                let image = PathBuf::from(&p.record.spec.image_path);
                #[cfg(windows)]
                {
                    assert!(
                        fs::write(&image, b"tamper").is_err(),
                        "Prepared image must deny writes"
                    );
                    drop(std::mem::replace(
                        &mut p._image,
                        File::open(&f.config).unwrap(),
                    ));
                }
                fs::write(&image, b"tamper").unwrap();
            }
            assert!(p
                .spawn(&mut f.journal, f.owner.take().unwrap(), f.domain.as_ref())
                .is_err());
            assert_eq!(
                f.journal.supervisor(&f.id).unwrap().phase,
                SupervisorPhase::Intent
            );
            assert_eq!(
                fs::read(f.root.join("source/Cargo.toml")).unwrap(),
                b"fixture"
            );
        }
    }
    #[tokio::test]
    async fn parent_crash_after_handoff_keeps_supervisor_alive_and_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let _cleanup = Cleanup(dir.path().into());
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "supervisor::tests::parent_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("WINBRIDGE_SUPERVISOR_TEST_ROOT", dir.path())
            .env("WINBRIDGE_SUPERVISOR_TEST_MODE", "active")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        let mut parent = Parent(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = parent.0.try_wait().unwrap() {
                assert_eq!(status.code(), Some(41));
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        // Test-only private handoff to its creator; never copied to proof/shared logs.
        let data: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("test-control.json")).unwrap())
                .unwrap();
        let record: SupervisorRecord = serde_json::from_value(data["record"].clone()).unwrap();
        let token = data["token"].as_str().unwrap().to_owned();
        assert!(infrastructure::matches(record.process.as_ref().unwrap()).unwrap());
        let cfg = load_config(&dir.path().join("runtime/winbridge-update.local.json")).unwrap();
        let state = cfg.runtime_directory.join("update-state");
        let scope = installation_scope(&cfg).unwrap();
        let mut journal = UpdateJournal::open(&state, &scope).unwrap();
        assert!(matches!(journal.acquire_owner(),Err(e) if e.code==JournalErrorCode::OwnerBusy));
        let request = SupervisorRequest {
            contract_version: 1,
            launch_id: record.spec.launch_id.clone(),
            expected_revision: record.revision,
            action: SupervisorAction::Observe,
            token: token.clone(),
            worker_command: None,
        };
        let observed: SupervisorResponse = ipc::call(
            &record.spec.endpoint,
            record.process.as_ref().unwrap().process_id,
            &request,
        )
        .await
        .unwrap();
        assert_eq!(
            observed.record.as_ref().unwrap().phase,
            SupervisorPhase::Active
        );
        let external_job =
            infrastructure::job_observation(record.process.as_ref().unwrap()).unwrap();
        let stop = SupervisorRequest {
            action: SupervisorAction::Stop,
            ..request
        };
        let response: SupervisorResponse = ipc::call(
            &record.spec.endpoint,
            record.process.as_ref().unwrap().process_id,
            &stop,
        )
        .await
        .unwrap();
        assert_eq!(response.record.unwrap().phase, SupervisorPhase::Stopped);
        let until = Instant::now() + Duration::from_secs(5);
        while matches!(journal.acquire_owner(),Err(e) if e.code==JournalErrorCode::OwnerBusy) {
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let process_deadline = Instant::now() + Duration::from_secs(5);
        while infrastructure::matches(record.process.as_ref().unwrap()).unwrap_or(false) {
            assert!(Instant::now() < process_deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            journal.get(&record.spec.update_id).unwrap().phase,
            UpdatePhase::FailedBeforeCutover
        );
        if let Ok(path) = std::env::var("WINBRIDGE_SUPERVISOR_REPORT") {
            fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"supervisor_native_proof","passed":true,"platform":std::env::consts::OS,"build":winbridge_protocol::build_identity(),"parentExited":true,"survivedParent":true,"exclusiveOwner":true,"runtimeApplied":false,"sourceEffectsEnabled":false,"windowsOpened":false,"externalJobObserved":external_job,"active":record,"stopped":UpdateJournal::inspect_supervisor(&state,&record.spec.launch_id).unwrap()})).unwrap()).unwrap();
        }
    }
    #[tokio::test]
    async fn crashes_before_receipt_or_handoff_never_start_effects_or_replay() {
        for mode in ["before_receipt", "ready"] {
            let dir = tempfile::tempdir().unwrap();
            let _cleanup = Cleanup(dir.path().into());
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "supervisor::tests::parent_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("WINBRIDGE_SUPERVISOR_TEST_ROOT", dir.path())
                .env("WINBRIDGE_SUPERVISOR_TEST_MODE", mode)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
            }
            let mut parent = Parent(command.spawn().unwrap());
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                if let Some(code) = parent.0.try_wait().unwrap() {
                    assert_eq!(code.code(), Some(42));
                    break;
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let data: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.path().join("test-control.json")).unwrap())
                    .unwrap();
            let witness: SupervisorRecord = serde_json::from_value(data["record"].clone()).unwrap();
            let state = dir.path().join("runtime/update-state");
            if mode == "ready" {
                let ready =
                    UpdateJournal::inspect_supervisor(&state, &witness.spec.launch_id).unwrap();
                let wrong_parent = SupervisorRequest {
                    contract_version: 1,
                    launch_id: ready.spec.launch_id.clone(),
                    expected_revision: ready.revision,
                    action: SupervisorAction::Handoff,
                    token: data["token"].as_str().unwrap().into(),
                    worker_command: None,
                };
                let reply: SupervisorResponse = ipc::call(
                    &ready.spec.endpoint,
                    witness.process.as_ref().unwrap().process_id,
                    &wrong_parent,
                )
                .await
                .unwrap();
                assert!(reply.error.is_some());
            }
            let until = Instant::now() + Duration::from_secs(16);
            loop {
                if !infrastructure::matches(witness.process.as_ref().unwrap()).unwrap_or(false) {
                    break;
                }
                assert!(Instant::now() < until, "Unaccepted supervisor did not exit");
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let r = UpdateJournal::inspect_supervisor(&state, &witness.spec.launch_id).unwrap();
            assert_eq!(
                r.phase,
                if mode == "ready" {
                    SupervisorPhase::Stopped
                } else {
                    SupervisorPhase::Intent
                }
            );
            let config = dir.path().join("runtime/winbridge-update.local.json");
            let cfg = load_config(&config).unwrap();
            let mut j = UpdateJournal::open(&state, &installation_scope(&cfg).unwrap()).unwrap();
            let owner = j.acquire_owner().unwrap();
            assert!(matches!(
                prepare(
                    &mut j,
                    &owner,
                    &witness.spec.update_id,
                    &config,
                    &witness.spec.image_sha256,
                    None
                )
                .unwrap(),
                Preparation::Existing(_)
            ));
            assert_eq!(
                fs::read(dir.path().join("source/Cargo.toml")).unwrap(),
                b"fixture"
            );
        }
    }
    #[tokio::test]
    #[ignore = "Explicit actual staged source required; native gate only"]
    async fn actual_staged_source_reaches_ready_without_mutation() {
        let source = PathBuf::from(
            std::env::var_os("WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE").expect("Pinned staged source"),
        );
        let manifest = fs::read(source.join("winbridge-source-manifest.json")).unwrap();
        let hash = sha256(&manifest);
        let before = crate::verify_source(&source, &hash).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut f = Fixture::new_source(dir.path(), Some(&source));
        let started = Instant::now();
        let mut h = f.spawn();
        let ready = h.wait_ready().await.unwrap();
        let ready_ms = started.elapsed().as_millis();
        let active = h.command(SupervisorAction::Handoff).await.unwrap();
        assert_eq!(active.phase, SupervisorPhase::Active);
        stopped(&mut h).await;
        let after = crate::verify_source(&source, &hash).unwrap();
        assert_eq!(before.manifest_sha256, after.manifest_sha256);
        let path = std::env::var_os("WINBRIDGE_SUPERVISOR_SOURCE_REPORT")
            .expect("Native readonly source report");
        fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"supervisor_staged_source_proof","passed":true,"platform":std::env::consts::OS,"build":winbridge_protocol::build_identity(),"runtimeApplied":false,"sourceEffectsEnabled":false,"windowsOpened":false,"readyElapsedMs":ready_ms,"sourceBefore":before,"sourceAfter":after,"ready":ready,"active":active,"stopped":h.record})).unwrap()).unwrap();
    }
    #[test]
    #[ignore = "Owned parent crash helper; native gate invokes only through its test"]
    fn parent_helper() {
        let root = PathBuf::from(std::env::var_os("WINBRIDGE_SUPERVISOR_TEST_ROOT").unwrap());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut f = Fixture::new(&root);
            let mode = std::env::var("WINBRIDGE_SUPERVISOR_TEST_MODE").unwrap_or_default();
            if mode == "before_receipt" {
                let p = f.prepare();
                let spec = &p.record.spec;
                let args = vec![
                    "--config".into(),
                    spec.config_path.clone(),
                    "supervisor-internal".into(),
                    spec.launch_id.clone(),
                    p.token.clone(),
                ];
                let child = InfrastructureChild::spawn(
                    Path::new(&spec.image_path),
                    &args,
                    Path::new(&spec.runtime_directory),
                    p.stdout.try_clone().unwrap(),
                    p.stderr.try_clone().unwrap(),
                    f.domain.as_ref(),
                )
                .unwrap();
                let mut private = p.record.clone();
                private.process = Some(child.identity.clone());
                fs::write(
                    root.join("test-control.json"),
                    serde_json::to_vec(&serde_json::json!({"record":private,"token":p.token}))
                        .unwrap(),
                )
                .unwrap();
                std::process::exit(42);
            }
            let mut h = f.spawn();
            h.wait_ready().await.unwrap();
            if mode == "ready" {
                std::process::exit(42);
            }
            h.command(SupervisorAction::Handoff).await.unwrap();
            fs::write(
                root.join("test-control.json"),
                serde_json::to_vec(&serde_json::json!({"record":h.record,"token":h.token}))
                    .unwrap(),
            )
            .unwrap();
            drop(f.domain.take());
            assert_eq!(
                h.command(SupervisorAction::Observe).await.unwrap().phase,
                SupervisorPhase::Active
            );
            std::process::exit(41);
        });
    }
}
