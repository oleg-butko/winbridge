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
    sync::Arc,
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
fn shutdown_view(
    directory: &Path,
    id: &str,
    active: bool,
    failure: Option<String>,
    custody: &crate::shutdown::Custody,
    preparing: bool,
) -> Result<UpdatePreparationView> {
    let mut view = UpdateJournal::shutdown_preparation_snapshot(directory, id, active, failure)
        .map_err(error)?;
    if let Some(shutdown) = &mut view.candidate_shutdown {
        if preparing && shutdown.attempt_id.is_none() {
            shutdown.phase = CandidateShutdownPhase::Preparing;
        }
        shutdown.physical_custody_held =
            shutdown.phase == CandidateShutdownPhase::Exited && custody.present()?;
    }
    Ok(view)
}
fn backup_view(
    directory: &Path,
    id: &str,
    active: bool,
    failure: Option<String>,
    custody: &crate::shutdown::Custody,
    shutdown_preparing: bool,
    backup_preparing: bool,
) -> Result<UpdatePreparationView> {
    let mut view = UpdateJournal::backup_preparation_snapshot(directory, id, active, failure)
        .map_err(error)?;
    if let Some(shutdown) = &mut view.candidate_shutdown {
        if shutdown_preparing && shutdown.attempt_id.is_none() {
            shutdown.phase = CandidateShutdownPhase::Preparing;
        }
        shutdown.physical_custody_held =
            shutdown.phase == CandidateShutdownPhase::Exited && custody.present()?;
    }
    if let Some(backup) = &mut view.candidate_state_backup {
        if backup_preparing && backup.attempt_id.is_none() {
            backup.phase = CandidateStateBackupPhase::Preparing;
        }
    }
    Ok(view)
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
fn runtime_files_view(
    directory: &Path,
    id: &str,
    active: bool,
    failure: Option<String>,
    custody: &crate::shutdown::Custody,
    admitting: bool,
) -> Result<UpdatePreparationView> {
    let mut view =
        UpdateJournal::runtime_files_preparation_snapshot(directory, id, active, failure)
            .map_err(error)?;
    if let Some(shutdown) = &mut view.candidate_shutdown {
        shutdown.physical_custody_held =
            shutdown.phase == CandidateShutdownPhase::Exited && custody.present()?;
    }
    if let Some(preparation) = &mut view.runtime_preparation {
        if admitting && preparation.attempt_id.is_none() {
            preparation.phase = RuntimePreparationPhase::Admitting;
        }
    }
    Ok(view)
}
fn support_view(
    directory: &Path,
    id: &str,
    active: bool,
    error: Option<String>,
    admitting: bool,
) -> Result<UpdatePreparationView> {
    let mut view = UpdateJournal::candidate_support_snapshot(directory, id, active, error)
        .map_err(crate::supervisor::error)?;
    if let Some(support) = &mut view.candidate_support {
        if admitting && support.attempt_id.is_none() {
            support.phase = CandidateSupportViewPhase::Admitting;
        }
    }
    Ok(view)
}

fn compatible_view(
    directory: &Path,
    id: &str,
    active: bool,
    error: Option<String>,
    admitting: bool,
) -> Result<UpdatePreparationView> {
    let mut view = UpdateJournal::compatible_drain_snapshot(directory, id, active, error)
        .map_err(crate::supervisor::error)?;
    if let Some(gate) = &mut view.compatible_drain {
        if admitting && gate.attempt_id.is_none() {
            gate.phase = CompatibleDrainPhase::Admitting;
        }
    }
    Ok(view)
}

fn activation_view(
    directory: &Path,
    id: &str,
    active: bool,
    error: Option<String>,
    preparing: bool,
    signal: &crate::runtime_replacement::CustodySignal,
) -> Result<UpdatePreparationView> {
    let mut view =
        UpdateJournal::activation_preparation_snapshot(directory, id, active, error.clone())
            .map_err(crate::supervisor::error)?;
    if let Some(p) = &mut view.runtime_replacement {
        p.physical_custody_held = signal.held();
    }
    if let Some(p) = &mut view.activation_preparation {
        p.physical_custody_held = signal.held();
        if p.attempt_id.is_none() {
            p.phase = if preparing {
                ActivationPreparationPhase::Preparing
            } else if error.is_some() {
                ActivationPreparationPhase::Blocked
            } else {
                ActivationPreparationPhase::NotPrepared
            };
            p.reason = error;
        }
    }
    Ok(view)
}

mod activation_files;
mod finalization_control;
mod keeper_control;
mod keeper_launch;
mod keeper_probe;
pub use keeper_probe::{observe_original_keeper, KeeperProbeCapture};
mod resident_control;

fn local(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    path.canonicalize().map_err(error)
}
fn replacement_view(
    directory: &Path,
    id: &str,
    active: bool,
    error: Option<String>,
    custody: &crate::shutdown::Custody,
    signal: &crate::runtime_replacement::CustodySignal,
    admitting: bool,
) -> Result<UpdatePreparationView> {
    let mut view = UpdateJournal::runtime_replacement_snapshot(directory, id, active, error)
        .map_err(crate::supervisor::error)?;
    if let Some(shutdown) = &mut view.candidate_shutdown {
        shutdown.physical_custody_held =
            shutdown.phase == CandidateShutdownPhase::Exited && custody.present()?;
    }
    if let Some(replacement) = &mut view.runtime_replacement {
        replacement.physical_custody_held = signal.held();
        if admitting && replacement.attempt_id.is_none() {
            replacement.phase = RuntimeReplacementPhase::Admitting;
        }
    }
    Ok(view)
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
    /// Readonly v6/worker-control-v2 identity. No implicit compatibility upgrade.
    pub async fn runtime_identity(&self) -> Result<winbridge_protocol::LoadedWorkerIdentity> {
        self.runtime_query(false).await?.into_runtime_identity()
    }
    /// V9/worker-control-v3: current schemas/config from the exact bound process.
    pub async fn runtime_compatibility(
        &self,
    ) -> Result<winbridge_protocol::WorkerRuntimeCompatibility> {
        let control = self.runtime_query(true).await?;
        let binding = control.binding.clone();
        let compatibility = control.into_runtime_compatibility()?;
        compatibility.validate_binding(&binding)?;
        Ok(compatibility)
    }
    /// The supervisor authenticates its own process on worker control v2.
    /// This is a readonly observation, never a lease or installation receipt.
    async fn runtime_query(&self, compatibility: bool) -> Result<WorkerUpdateControlResponse> {
        let version = if compatibility { 9 } else { 6 };
        let action = if compatibility {
            SupervisorAction::ObserveWorkerCompatibility
        } else {
            SupervisorAction::ObserveWorkerRuntime
        };
        self.worker_observation(version, action, None).await
    }
    /// Read a historical command receipt without sending Advance. Absence is
    /// only an observation, never permission to repeat an unknown command.
    pub async fn worker_receipt(
        &self,
        command: &WorkerUpdateCommand,
    ) -> Result<Option<WorkerUpdateLease>> {
        let response = self
            .worker_observation(
                11,
                SupervisorAction::ObserveWorkerReceipt,
                Some(WorkerUpdateControlCommand::ObserveReceipt {
                    command: command.clone(),
                }),
            )
            .await?;
        let receipt = response.into_command_receipt(command)?;
        if receipt.as_ref().is_some_and(|r| {
            Some(&r.spec.updater_owner_id) != self.record.owner_id.as_ref()
                || Some(r.spec.updater_epoch) != self.record.owner_epoch
        }) {
            return Err("Worker command receipt belongs to another owner/epoch".into());
        }
        Ok(receipt)
    }
    /// Logical blockers/user pause only. Even all-zero counters are NOT an OS
    /// quiescence proof and never replace the ConfirmQuiesced command receipt.
    pub async fn worker_drain_observation(&self) -> Result<WorkerDrainObservation> {
        let view = self
            .worker_observation(12, SupervisorAction::ObserveWorkerDrain, None)
            .await?
            .into_drain_observation()?;
        if view.lease.as_ref().is_some_and(|r| {
            Some(&r.spec.updater_owner_id) != self.record.owner_id.as_ref()
                || Some(r.spec.updater_epoch) != self.record.owner_epoch
        }) {
            return Err("Drain observation belongs to another owner/epoch".into());
        }
        Ok(view)
    }
    async fn worker_observation(
        &self,
        version: u16,
        action: SupervisorAction,
        worker_command: Option<WorkerUpdateControlCommand>,
    ) -> Result<WorkerUpdateControlResponse> {
        if self.record.phase != SupervisorPhase::Active
            || self.record.spec.worker.is_none()
            || !infrastructure::matches(&self.child.identity).map_err(error)?
        {
            return Err(
                "Runtime observation requires the exact active worker-bound supervisor".into(),
            );
        }
        let request = SupervisorRequest {
            contract_version: version,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action,
            token: self.token.clone(),
            worker_command,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != version
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.preparation.is_some()
        {
            return Err("Supervisor runtime observation scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let control = response
            .worker_control
            .ok_or("Runtime observation missing")?;
        if Some(&control.binding) != self.record.spec.worker.as_ref()
            || control.update_id != self.record.spec.update_id
            || control.launch_id != self.record.spec.launch_id
        {
            return Err("Runtime observation belongs to another worker/install scope".into());
        }
        Ok(control)
    }
    /// V14 starts exact shutdown once; reading status never dispatches work.
    pub async fn candidate_shutdown(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 14,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ShutdownCandidate
            } else {
                SupervisorAction::ObserveCandidateShutdown
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 14
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Shutdown reply scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Shutdown view missing")?;
        if view.schema_version != 6
            || view.candidate_shutdown.is_none()
            || view.candidate_drain.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Shutdown reply belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Generated fixture authority for an owned external validation parent.
    /// Never include this request/token in reports, logs or shared delivery.
    #[cfg(feature = "update-validation")]
    pub fn validation_private_client(&self) -> Result<SupervisorRequest> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Validation supervisor process changed".into());
        }
        Ok(SupervisorRequest {
            contract_version: 15,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::ObserveStateBackup,
            token: self.token.clone(),
            worker_command: None,
        })
    }
    /// V15 only: final export support is required before admitting a stop.
    pub async fn shutdown_for_state_backup(&self) -> Result<UpdatePreparationView> {
        self.backup_request(SupervisorAction::ShutdownCandidateForBackup)
            .await
    }
    pub async fn final_state_backup(&self, start: bool) -> Result<UpdatePreparationView> {
        self.backup_request(if start {
            SupervisorAction::BackupWorkerState
        } else {
            SupervisorAction::ObserveStateBackup
        })
        .await
    }
    async fn backup_request(&self, action: SupervisorAction) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 15,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action,
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 15
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Final backup response scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Final backup view missing")?;
        if view.schema_version != 7
            || view.candidate_state_backup.is_none()
            || view.candidate_shutdown.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Final backup response belongs to another contract/update".into());
        }
        Ok(view)
    }
    pub async fn runtime_files(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 16,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::PrepareRuntimeFiles
            } else {
                SupervisorAction::ObserveRuntimeFiles
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 16
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Runtime preparation response scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        let view = response
            .preparation
            .ok_or("Runtime preparation view missing")?;
        if view.schema_version != 8
            || view.runtime_preparation.is_none()
            || view.candidate_state_backup.is_none()
            || view.candidate_shutdown.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Runtime preparation response belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Internal v17/view9 only. A saved attempt is observed, never replayed.
    pub async fn replace_runtime(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 17,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ReplaceRuntime
            } else {
                SupervisorAction::ObserveRuntimeReplacement
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 17
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Runtime replacement response scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        let view = response
            .preparation
            .ok_or("Runtime replacement view missing")?;
        if view.schema_version != 9
            || view.runtime_replacement.is_none()
            || view.runtime_preparation.is_none()
            || view.candidate_state_backup.is_none()
            || view.candidate_shutdown.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Runtime replacement response belongs to another contract/update".into());
        }
        Ok(view)
    }
    pub async fn candidate_drain(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 13,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::DrainCandidate
            } else {
                SupervisorAction::ObserveCandidateDrain
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 13
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Drain reply scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Drain view missing")?;
        if view.schema_version != 5
            || view.candidate_drain.is_none()
            || view.candidate_health.is_none()
            || view.runtime_preimages.is_none()
            || view.runtime_baseline.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Drain reply belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// V20 prepares metadata under resident custody; it cannot launch a worker.
    pub async fn activation_preparation(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 20,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::PrepareActivation
            } else {
                SupervisorAction::ObserveActivationPreparation
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 20
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Activation preparation reply scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response
            .preparation
            .ok_or("Activation preparation view missing")?;
        if view.schema_version != 12
            || view.activation_preparation.is_none()
            || view.runtime_replacement.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Activation preparation view belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Internal21 binds read handles only. No worker process or state transfer.
    pub async fn activation_files(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 21,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::BindActivationFiles
            } else {
                SupervisorAction::ObserveActivationFiles
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 21
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Activation files reply scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response
            .preparation
            .ok_or("Activation files view missing")?;
        if view.schema_version != 13
            || view.activation_files.is_none()
            || view.activation_preparation.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Activation files view belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Internal22 launches one pre-writer receiver. No state/lifetime handoff.
    pub async fn activation_process(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 22,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::LaunchActivationReceiver
            } else {
                SupervisorAction::ObserveActivationProcess
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 22
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Activation process reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        let view = response
            .preparation
            .ok_or("Activation process view missing")?;
        if view.schema_version != 14
            || view.activation_process.is_none()
            || view.activation_files.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Activation process view belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Internal23 transfers the exact physical state lock. No writable Store.
    pub async fn activation_state(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 23,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::HandoffActivationState
            } else {
                SupervisorAction::ObserveActivationState
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 23
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("State handoff reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        let view = response.preparation.ok_or("State handoff view missing")?;
        if view.schema_version != 15
            || view.activation_state.is_none()
            || view.activation_process.is_none()
            || view.activation_files.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("State handoff view belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// Internal24: None is only volatile dispatch ACK / driver busy. It never
    /// asserts durable admission or completion; observe again after retirement.
    pub async fn activation_writer(&self, start: bool) -> Result<Option<UpdatePreparationView>> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 24,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ActivateJournalWriter
            } else {
                SupervisorAction::ObserveJournalWriter
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 24
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Journal writer reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        if let Some(view) = response.preparation {
            if view.schema_version != 16
                || view.activation_writer.is_none()
                || view.update.update_id != self.record.spec.update_id
            {
                return Err("Journal writer view belongs to another contract/update".into());
            }
            Ok(Some(view))
        } else {
            Ok(None)
        }
    }
    /// Internal25: None is volatile dispatch/busy, never admission or readiness.
    pub async fn activation_service(&self, start: bool) -> Result<Option<UpdatePreparationView>> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 25,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ActivateLocalService
            } else {
                SupervisorAction::ObserveLocalService
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 25
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Local service reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        if let Some(view) = response.preparation {
            if view.schema_version != 17
                || view.activation_service.is_none()
                || view.update.update_id != self.record.spec.update_id
            {
                return Err("Local service view belongs to another contract/update".into());
            }
            Ok(Some(view))
        } else {
            Ok(None)
        }
    }
    /// Internal26: None is volatile dispatch/busy, never admission or readiness.
    pub async fn activation_controller(
        &self,
        start: bool,
    ) -> Result<Option<UpdatePreparationView>> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 26,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ConnectActivationController
            } else {
                SupervisorAction::ObserveActivationController
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 26
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Controller reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        if let Some(view) = response.preparation {
            if view.schema_version != 18
                || view.activation_controller.is_none()
                || view.update.update_id != self.record.spec.update_id
            {
                return Err("Controller view belongs to another contract/update".into());
            }
            Ok(Some(view))
        } else {
            Ok(None)
        }
    }
    /// Internal27: None is volatile dispatch/busy, never admission or readiness.
    pub async fn activation_lifetime(&self, start: bool) -> Result<Option<UpdatePreparationView>> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 27,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::EnterActivationLifetime
            } else {
                SupervisorAction::ObserveActivationLifetime
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 27
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Lifetime reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        if let Some(view) = response.preparation {
            if view.schema_version != 19
                || view.activation_lifetime.is_none()
                || view.update.update_id != self.record.spec.update_id
            {
                return Err("Lifetime view belongs to another contract/update".into());
            }
            Ok(Some(view))
        } else {
            Ok(None)
        }
    }
    /// V19 is the production prerequisite route; there is no fallback to v13.
    pub async fn compatible_drain(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 19,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::DrainCompatibleCandidate
            } else {
                SupervisorAction::ObserveCompatibleDrain
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 19
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Production drain response scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response
            .preparation
            .ok_or("Production drain view missing")?;
        if view.schema_version != 11
            || view.compatible_drain.is_none()
            || view.candidate_support.is_none()
            || view.candidate_drain.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Production drain view belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// V18 captures journal-bound updater evidence; it never authorizes drain.
    pub async fn candidate_support(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 18,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::CaptureCandidateSupport
            } else {
                SupervisorAction::ObserveCandidateSupport
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 18
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Support response scope/version changed; no fallback".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Support view missing")?;
        if view.schema_version != 10
            || view.candidate_support.is_none()
            || view.candidate_health.is_none()
            || view.runtime_preimages.is_none()
            || view.runtime_baseline.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Support response belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// V10 admission is duplicate-first and never replays a saved pending health check.
    pub async fn candidate_health(&self, start: bool) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let q = SupervisorRequest {
            contract_version: 10,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start {
                SupervisorAction::ValidateCandidateHealth
            } else {
                SupervisorAction::ObserveCandidateHealth
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
        if response.contract_version != 10
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Health reply scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Health view missing")?;
        if view.schema_version != 4
            || view.candidate_health.is_none()
            || view.runtime_preimages.is_none()
            || view.runtime_baseline.is_none()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Health reply belongs to another contract/update".into());
        }
        Ok(view)
    }
    /// V7 keeps its legacy view; v8 adds saved preimages without new runtime authority.
    pub async fn runtime_baseline(&self, start: bool) -> Result<UpdatePreparationView> {
        self.runtime_preparation(start, false).await
    }
    pub async fn runtime_preimages(&self, start: bool) -> Result<UpdatePreparationView> {
        self.runtime_preparation(start, true).await
    }
    async fn runtime_preparation(
        &self,
        start: bool,
        preimages: bool,
    ) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let (version, view_schema, begin, observe) = if preimages {
            (
                8,
                3,
                SupervisorAction::ArchiveRuntimePreimages,
                SupervisorAction::ObserveRuntimePreimages,
            )
        } else {
            (
                7,
                2,
                SupervisorAction::CaptureRuntimeBaseline,
                SupervisorAction::ObserveRuntimeBaseline,
            )
        };
        let q = SupervisorRequest {
            contract_version: version,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if start { begin } else { observe },
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
        if response.contract_version != version
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Runtime preparation reply scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response
            .preparation
            .ok_or("Runtime preparation view missing")?;
        if view.schema_version != view_schema
            || view.runtime_baseline.is_none()
            || preimages != view.runtime_preimages.is_some()
            || view.update.update_id != self.record.spec.update_id
        {
            return Err("Runtime preparation reply belongs to another contract/update".into());
        }
        Ok(view)
    }
    pub async fn observe(&self) -> Result<SupervisorRecord> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed; no observation".into());
        }
        let request = SupervisorRequest {
            contract_version: 1,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::Observe,
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 1
            || response.launch_id != request.launch_id
            || response.worker_control.is_some()
            || response.preparation.is_some()
        {
            return Err("Supervisor status scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let record = response.record.ok_or("Supervisor status record missing")?;
        if record.spec != self.record.spec
            || record.process.as_ref() != Some(&self.child.identity)
            || record.revision < self.record.revision
        {
            return Err("Supervisor status identity/revision changed".into());
        }
        Ok(record)
    }
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
    pub async fn build_candidate(&mut self) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let q = SupervisorRequest {
            contract_version: 3,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::BuildCandidate,
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
        if response.contract_version != 3
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Compiler reply scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Compiler view missing")?;
        if view.schema_version != 1 || view.update.update_id != self.record.spec.update_id {
            return Err("Compiler view belongs to another update".into());
        }
        Ok(view)
    }
    /// Readonly driver dispatch. A saved receipt is observed without revalidation
    /// or process effects; this does not approve candidate health/installation.
    pub async fn verify_candidate_artifacts(&mut self) -> Result<UpdatePreparationView> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let q = SupervisorRequest {
            contract_version: 5,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::VerifyCandidateArtifacts,
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
        if response.contract_version != 5
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Artifact reply scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response
            .preparation
            .ok_or("Artifact preparation view missing")?;
        if view.schema_version != 1 || view.update.update_id != self.record.spec.update_id {
            return Err("Artifact reply belongs to another update".into());
        }
        Ok(view)
    }
    /// Authenticated admission, then readonly receipt polling. Lost ACK never
    /// grants a second execution; terminal failure retains the worker barrier.
    pub async fn abort_before_cutover(&mut self) -> Result<crate::abort::AbortRecord> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let q = SupervisorRequest {
            contract_version: 4,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: SupervisorAction::AbortBeforeCutover,
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
        if response.contract_version != 4
            || response.launch_id != q.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Abort response scope/version changed".into());
        }
        if let Some(e) = response.error {
            return Err(e);
        }
        let view = response.preparation.ok_or("Abort admission view missing")?;
        if view.schema_version != 1 || view.update.update_id != self.record.spec.update_id {
            return Err("Abort view belongs to another update".into());
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let receipt =
                UpdateJournal::inspect_abort(&self.directory, &self.record.spec.update_id)
                    .map_err(error)?;
            match receipt.phase {
                crate::abort::AbortPhase::Completed => {
                    self.record = UpdateJournal::inspect_supervisor(&self.directory, &q.launch_id)
                        .map_err(error)?;
                    if self.record.phase != SupervisorPhase::Stopped {
                        return Err("Abort completion/supervisor stop differs".into());
                    }
                    return Ok(receipt);
                }
                crate::abort::AbortPhase::Blocked => return Err(receipt.reason),
                _ => {}
            }
            if Instant::now() >= deadline || self.child.exited().map_err(error)? {
                return Err(
                    "Abort outcome unconfirmed; inspect saved receipt, do not replay".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
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
            SupervisorAction::PrepareSource
                | SupervisorAction::ObservePreparation
                | SupervisorAction::BuildCandidate
                | SupervisorAction::AbortBeforeCutover
                | SupervisorAction::VerifyCandidateArtifacts
                | SupervisorAction::ObserveWorkerRuntime
                | SupervisorAction::ObserveWorkerCompatibility
                | SupervisorAction::ObserveWorkerReceipt
                | SupervisorAction::ObserveWorkerDrain
                | SupervisorAction::DrainCandidate
                | SupervisorAction::ObserveCandidateDrain
                | SupervisorAction::ShutdownCandidate
                | SupervisorAction::ObserveCandidateShutdown
                | SupervisorAction::ShutdownCandidateForBackup
                | SupervisorAction::BackupWorkerState
                | SupervisorAction::ObserveStateBackup
                | SupervisorAction::PrepareRuntimeFiles
                | SupervisorAction::ObserveRuntimeFiles
                | SupervisorAction::ReplaceRuntime
                | SupervisorAction::ObserveRuntimeReplacement
                | SupervisorAction::CaptureCandidateSupport
                | SupervisorAction::ObserveCandidateSupport
                | SupervisorAction::DrainCompatibleCandidate
                | SupervisorAction::ObserveCompatibleDrain
                | SupervisorAction::PrepareActivation
                | SupervisorAction::ObserveActivationPreparation
                | SupervisorAction::BindActivationFiles
                | SupervisorAction::ObserveActivationFiles
                | SupervisorAction::LaunchActivationReceiver
                | SupervisorAction::ObserveActivationProcess
                | SupervisorAction::HandoffActivationState
                | SupervisorAction::ObserveActivationState
                | SupervisorAction::ActivateJournalWriter
                | SupervisorAction::ObserveJournalWriter
                | SupervisorAction::ActivateLocalService
                | SupervisorAction::ObserveLocalService
                | SupervisorAction::ConnectActivationController
                | SupervisorAction::ObserveActivationController
                | SupervisorAction::EnterActivationLifetime
                | SupervisorAction::ObserveActivationLifetime
                | SupervisorAction::ObserveResidentConnection
                | SupervisorAction::FinalizeActivation
                | SupervisorAction::ObserveActivationFinalization
                | SupervisorAction::ValidateCandidateHealth
                | SupervisorAction::ObserveCandidateHealth
                | SupervisorAction::CaptureRuntimeBaseline
                | SupervisorAction::ObserveRuntimeBaseline
                | SupervisorAction::ArchiveRuntimePreimages
                | SupervisorAction::ObserveRuntimePreimages
        ) {
            return Err("Use the typed preparation/compiler method for this action".into());
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
        control_version: command.required_version(),
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
    if response.control_version != request.control_version
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
/// Public installation stays disabled; effects use their versioned internal drivers.
/// Allocate before polling, outside the caller's executor frame. The Windows
/// Main thread must not retain a second large mailbox future on its stack while
/// synchronous admission/serde calls use that same stack. Keep construction in
/// a separate frame even in optimized builds.
#[inline(never)]
pub fn run<'a>(
    config_path: &'a Path,
    id: &'a str,
    token: &'a str,
) -> impl std::future::Future<Output = Result<()>> + 'a {
    Box::pin(run_inner(config_path, id, token))
}
async fn run_inner(config_path: &Path, id: &str, token: &str) -> Result<()> {
    if token.len() != 64 {
        return Err("Invalid supervisor startup token".into());
    }
    winbridge_platform::require_local_path_prefix(config_path).map_err(error)?;
    winbridge_platform::require_local_disk(config_path).map_err(error)?;
    let cfg = load_config(config_path)?;
    let runtime = local(&cfg.runtime_directory)?;
    let directory = runtime.join("update-state");
    let this = infrastructure::stamp(std::process::id()).map_err(error)?;
    #[cfg(windows)]
    let incoming = UpdateJournal::inspect_bootstrap(&directory, id).map_err(error)?;
    #[cfg(windows)]
    let startup_remaining = match &incoming {
        Some(b) => keeper_launch::startup_remaining(b.intent.prepared_at_ms, now()?)?,
        None => Duration::from_secs(10),
    };
    #[cfg(not(windows))]
    let startup_remaining = Duration::from_secs(10);
    let deadline = Instant::now() + startup_remaining;
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
    #[cfg(windows)]
    let predecessor =
        crate::runtime_replacement::KeeperPredecessor::capture(&directory, &r, incoming.as_ref())?;
    validate_inputs(&r, &directory, true)?;
    if Instant::now() >= deadline {
        return Err("Original supervisor startup budget expired before listener/owner".into());
    }
    if local(&std::env::current_exe().map_err(error)?)? != local(Path::new(&r.spec.image_path))? {
        return Err("Supervisor is not the pinned copied image".into());
    }
    let mut listener = ipc::Listener::bind(&r.spec.endpoint).map_err(error)?;
    let mut journal = UpdateJournal::open(&directory, &installation_scope(&cfg)?).map_err(error)?;
    let owner = loop {
        if Instant::now() >= deadline {
            return Err("Original supervisor startup budget expired before ownership".into());
        }
        match journal.acquire_owner() {
            Ok(a) => break a,
            Err(e) if e.code == JournalErrorCode::OwnerBusy && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await
            }
            Err(e) => return Err(error(e)),
        }
    };
    if Instant::now() >= deadline {
        return Err("Original supervisor startup budget expired before Ready".into());
    }
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
    // A single physical owner is shared only inside this supervisor process.
    // A copy callback retains its Arc if the mailbox exits; cancellation uses a
    // separate CAS-checked connection and never claims a new epoch or lock.
    let control_authority = Arc::new(owner);
    let mut idle = Some((journal, control_authority.clone()));
    enum DriverCompletion {
        Complete(Result<()>),
        ShutdownPrepared(Result<Box<crate::shutdown::Prepared>>),
        BackupAdmitted(Result<crate::state_backup::StateBackupAdmission>),
        RuntimeAdmitted(Result<crate::runtime_preparation::RuntimePreparationAdmission>),
        ReplacementAdmitted(Result<crate::runtime_replacement::RuntimeReplacementAdmission>),
        ReplacementDone(crate::runtime_replacement::ReplacementOutcome),
        SupportAdmitted(Result<crate::candidate_support::CandidateSupportAdmission>),
        CompatibleAdmitted(Result<crate::compatible_drain::CompatibleDrainAdmission>),
        #[cfg(windows)]
        ActivationPrepared {
            guard: Box<crate::runtime_replacement::RuntimeReplacementGuard>,
            result: Result<crate::activation::ActivationPreparationAdmission>,
        },
        #[cfg(windows)]
        ActivationFiles(
            std::result::Result<
                Box<crate::activation::ActivationFilesGuard>,
                crate::activation::ActivationFilesFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationProcess(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationState(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationWriter(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationService(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationController(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationLifetime(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        ActivationResident(
            std::result::Result<
                Box<crate::activation::ActivationProcessGuard>,
                crate::activation::ActivationLaunchFailure,
            >,
        ),
        #[cfg(windows)]
        KeeperPrepared(Box<crate::activation::ActivationKeeperAttempt>),
        #[cfg(windows)]
        KeeperPreparationFailed(crate::activation::ActivationFinalizationFailure),
        #[cfg(windows)]
        FinalizationPreparationFailed(Box<crate::activation::ActivationLaunchFailure>),
        #[cfg(windows)]
        Finalization(
            std::result::Result<
                Box<crate::activation::ActivationFinalizationGuard>,
                crate::activation::ActivationFinalizationFailure,
            >,
        ),
    }
    type Work = tokio::task::JoinHandle<(UpdateJournal, Arc<UpdateAuthority>, DriverCompletion)>;
    let mut work: Option<Work> = None;
    let mut artifacts_active = false;
    let mut preimages_active = false;
    let mut health_active = false;
    let mut support_active = false;
    let mut support_admitting = false;
    let mut support_reply: Option<ipc::Stream> = None;
    let mut compatible_admitting = false;
    let mut compatible_reply: Option<ipc::Stream> = None;
    let mut activation_preparing = false;
    let mut activation_reply: Option<ipc::Stream> = None;
    let mut lifetime_reply: Option<ipc::Stream> = None;
    let mut resident_reply: Option<ipc::Stream> = None;
    let mut finalization_reply: Option<ipc::Stream> = None;
    let mut keeper_reply: Option<(ipc::Stream, KeeperControlRequest)> = None;
    let files_actor = activation_files::FilesActor::default();
    #[cfg(windows)]
    let mut files_actor = files_actor;
    #[cfg(windows)]
    let mut activation_preparation: Option<Box<crate::activation::ActivationPreparation>> = None;
    let mut drain_active = false;
    let mut shutdown_preparing = false;
    let shutdown_custody = crate::shutdown::Custody::default();
    let mut shutdown_reply: Option<(ipc::Stream, bool)> = None;
    let mut backup_reply: Option<ipc::Stream> = None;
    let mut backup_preparing = false;
    let mut runtime_reply: Option<ipc::Stream> = None;
    let mut runtime_admitting = false;
    let mut replacement_reply: Option<ipc::Stream> = None;
    let mut replacement_admitting = false;
    let mut replacement_signal = crate::runtime_replacement::CustodySignal::default();
    #[cfg(windows)]
    let mut replacement_guard: Option<
        Box<crate::runtime_replacement::RuntimeReplacementGuard>,
    > = None;
    let mut drain_telemetry = crate::drain::Telemetry::default();
    let mut pending_abort: Option<Box<crate::abort::AbortPermit>> = None;
    // One readonly query at a time; dropping the set aborts its IO task. The
    // mailbox and journal owner stay available while worker verifies file hashes.
    let mut runtime_reads = tokio::task::JoinSet::new();
    let mut driver_error: Option<String> = None;
    let ready_deadline = Instant::now() + Duration::from_secs(12);
    loop {
        files_actor.require_scope(&r)?;
        #[cfg(windows)]
        if let Some(prepared) = &activation_preparation {
            if prepared.record().intent.source.update_id != r.spec.update_id
                || Some(prepared.record().intent.source.owner_epoch) != r.owner_epoch
            {
                return Err("Resident activation preparation changed scope; no adoption".into());
            }
        }
        #[cfg(windows)]
        if let Some(guard) = &replacement_guard {
            if guard.permit().intent().preparation.plan.update_id != r.spec.update_id {
                return Err("Resident replacement custody changed scope".into());
            }
        }
        // Retire the completed query before publishing its response. Otherwise
        // a caller receiving that response can race the next accept and get a
        // false Busy from a completed but not-yet-collected JoinSet entry.
        while let Some(done) = runtime_reads.try_join_next() {
            let (mut stream, response) = done.map_err(error)?;
            let _ = ipc::write(&mut stream, &response).await;
        }
        if work.as_ref().is_some_and(|w| w.is_finished()) {
            let (journal, owner, outcome) = work.take().unwrap().await.map_err(error)?;
            #[cfg(windows)]
            if let DriverCompletion::KeeperPrepared(attempt) = outcome {
                // No old coordinator frame may retain a writer/authority or
                // verification owner while the new keeper loop is awaited.
                drop(activation_preparation);
                drop(replacement_guard);
                drop(pending_abort);
                drop(shutdown_custody);
                drop(replacement_signal);
                drop(files_actor);
                drop(runtime_reads);
                let resources =
                    attempt.bind_coordinator(journal, owner, control_authority, predecessor);
                return keeper_control::run(listener, r, resources, keeper_reply.take()).await;
            }
            let mut prepared = None;
            let mut backup_permit = None;
            let mut runtime_permit = None;
            let mut replacement_permit = None;
            let mut support_permit = None;
            let mut compatible_permit = None;
            let outcome = match outcome {
                DriverCompletion::Complete(outcome) => outcome,
                DriverCompletion::ShutdownPrepared(Ok(value)) => {
                    prepared = Some(value);
                    Ok(())
                }
                DriverCompletion::ShutdownPrepared(Err(e)) => Err(e),
                DriverCompletion::BackupAdmitted(Ok(
                    crate::state_backup::StateBackupAdmission::Fresh(p),
                )) => {
                    backup_permit = Some(p);
                    Ok(())
                }
                DriverCompletion::BackupAdmitted(Ok(
                    crate::state_backup::StateBackupAdmission::Existing(_),
                )) => Ok(()),
                DriverCompletion::BackupAdmitted(Err(e)) => Err(e),
                DriverCompletion::RuntimeAdmitted(Ok(
                    crate::runtime_preparation::RuntimePreparationAdmission::Fresh(p),
                )) => {
                    runtime_permit = Some(p);
                    Ok(())
                }
                DriverCompletion::RuntimeAdmitted(Ok(
                    crate::runtime_preparation::RuntimePreparationAdmission::Existing(_),
                )) => Ok(()),
                DriverCompletion::RuntimeAdmitted(Err(e)) => Err(e),
                DriverCompletion::ReplacementAdmitted(Ok(
                    crate::runtime_replacement::RuntimeReplacementAdmission::Fresh(p),
                )) => {
                    replacement_permit = Some(p);
                    Ok(())
                }
                DriverCompletion::ReplacementAdmitted(Ok(
                    crate::runtime_replacement::RuntimeReplacementAdmission::Existing(_),
                )) => Ok(()),
                DriverCompletion::ReplacementAdmitted(Err(e)) => Err(e),
                DriverCompletion::SupportAdmitted(Ok(
                    crate::candidate_support::CandidateSupportAdmission::Fresh(p),
                )) => {
                    support_permit = Some(p);
                    Ok(())
                }
                DriverCompletion::SupportAdmitted(Ok(
                    crate::candidate_support::CandidateSupportAdmission::Existing(_),
                )) => Ok(()),
                DriverCompletion::SupportAdmitted(Err(e)) => Err(e),
                DriverCompletion::CompatibleAdmitted(Ok(
                    crate::compatible_drain::CompatibleDrainAdmission::Fresh(p),
                )) => {
                    compatible_permit = Some(p);
                    Ok(())
                }
                DriverCompletion::CompatibleAdmitted(Ok(
                    crate::compatible_drain::CompatibleDrainAdmission::Blocked(_)
                    | crate::compatible_drain::CompatibleDrainAdmission::Observed(_),
                )) => Ok(()),
                DriverCompletion::CompatibleAdmitted(Err(e)) => Err(e),
                #[cfg(windows)]
                DriverCompletion::ActivationPrepared { guard, result } => {
                    replacement_guard = Some(guard);
                    match result {
                        Ok(crate::activation::ActivationPreparationAdmission::Prepared(value)) => {
                            activation_preparation = Some(value);
                            Ok(())
                        }
                        Ok(crate::activation::ActivationPreparationAdmission::Existing(_)) => {
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                #[cfg(windows)]
                DriverCompletion::ActivationFiles(result) => files_actor.complete(result),
                #[cfg(windows)]
                DriverCompletion::ActivationProcess(result) => files_actor.complete_launch(result),
                #[cfg(windows)]
                DriverCompletion::ActivationState(result) => files_actor.complete_handoff(result),
                #[cfg(windows)]
                DriverCompletion::ActivationWriter(result) => {
                    let saved =
                        UpdateJournal::inspect_activation_writer(&directory, &r.spec.update_id)
                            .map_err(error);
                    files_actor.complete_writer(result, saved)
                }
                #[cfg(windows)]
                DriverCompletion::ActivationService(result) => {
                    let saved =
                        UpdateJournal::inspect_activation_service(&directory, &r.spec.update_id)
                            .map_err(error);
                    files_actor.complete_service(result, saved)
                }
                #[cfg(windows)]
                DriverCompletion::ActivationController(result) => {
                    let saved =
                        UpdateJournal::inspect_activation_controller(&directory, &r.spec.update_id)
                            .map_err(error);
                    files_actor.complete_controller(result, saved)
                }
                #[cfg(windows)]
                DriverCompletion::ActivationLifetime(result) => {
                    let saved =
                        UpdateJournal::inspect_activation_lifetime(&directory, &r.spec.update_id)
                            .map_err(error);
                    files_actor.complete_lifetime(result, saved)
                }
                #[cfg(windows)]
                DriverCompletion::ActivationResident(result) => {
                    files_actor.complete_resident(result, &directory, &r.spec.update_id)
                }
                #[cfg(windows)]
                DriverCompletion::KeeperPrepared(_) => {
                    unreachable!("Keeper transferred before coordinator completion")
                }
                #[cfg(windows)]
                DriverCompletion::KeeperPreparationFailed(failure) => {
                    files_actor.fail_keeper_preparation(failure)
                }
                #[cfg(windows)]
                DriverCompletion::FinalizationPreparationFailed(failure) => {
                    files_actor.fail_finalization_preparation(*failure)
                }
                #[cfg(windows)]
                DriverCompletion::Finalization(result) => files_actor.complete_finalization(result),
                DriverCompletion::ReplacementDone(outcome) => {
                    #[cfg(windows)]
                    {
                        replacement_guard = outcome.guard;
                    }
                    outcome.error.map_or(Ok(()), Err)
                }
            };
            shutdown_preparing = false;
            backup_preparing = false;
            runtime_admitting = false;
            replacement_admitting = false;
            artifacts_active = false;
            preimages_active = false;
            health_active = false;
            support_active = false;
            support_admitting = false;
            compatible_admitting = false;
            activation_preparing = false;
            drain_active = false;
            driver_error = outcome
                .err()
                .map(|e| e.chars().take(1000).collect::<String>());
            if let Some(e) = &driver_error {
                if pending_abort.is_none() {
                    eprintln!("Update preparation stopped: {e}; preserve both journals and barrier, no automatic retry");
                }
            }
            let saved = UpdateJournal::inspect_supervisor(&directory, id).map_err(error)?;
            if saved.phase == SupervisorPhase::Stopped {
                return Ok(());
            }
            if saved != r {
                return Err("Preparation supervisor record changed unexpectedly".into());
            }
            idle = Some((journal, owner));
            if let Some((mut stream, q)) = keeper_reply.take() {
                let result = KeeperControlResult::Rejected {
                    message: driver_error.clone().unwrap_or_else(|| {
                        "Keeper preparation did not produce an original retirement owner".into()
                    }),
                };
                let _ =
                    ipc::write(&mut stream, &keeper_control::response(&r, Some(&q), result)).await;
            }
            if let Some(mut stream) = finalization_reply.take() {
                let (preparation, error) = match files_actor.finalization_view(
                    &directory,
                    &r.spec.update_id,
                    false,
                    driver_error.clone(),
                ) {
                    Ok(view) => (Some(view), None),
                    Err(error) => (None, Some(error)),
                };
                let response = SupervisorResponse {
                    contract_version: 29,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            if let Some(mut stream) = resident_reply.take() {
                let snapshot = files_actor.resident_view(
                    &directory,
                    &r.spec.update_id,
                    false,
                    driver_error.clone(),
                );
                let (preparation, error) = match snapshot {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 28,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            if let Some(mut stream) = lifetime_reply.take() {
                let snapshot = files_actor.lifetime_view(
                    &directory,
                    &r.spec.update_id,
                    false,
                    driver_error.clone(),
                    &replacement_signal,
                );
                let (preparation, error) = match snapshot {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 27,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            if let Some(mut stream) = activation_reply.take() {
                let snapshot = activation_view(
                    &directory,
                    &r.spec.update_id,
                    false,
                    driver_error.clone(),
                    false,
                    &replacement_signal,
                );
                let (preparation, error) = match snapshot {
                    Ok(value) => (Some(value), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                if error.is_some() {
                    driver_error = error.clone();
                }
                let response = SupervisorResponse {
                    contract_version: 20,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                // The committed preparation and nonce remain resident after a lost ACK.
                let _ = ipc::write(&mut stream, &response).await;
            }
            if let Some(mut stream) = compatible_reply.take() {
                let snapshot = compatible_view(
                    &directory,
                    &r.spec.update_id,
                    compatible_permit.is_some(),
                    driver_error.clone(),
                    false,
                );
                let (preparation, error) = match snapshot {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 19,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                if let Some(permit) = compatible_permit.filter(|_| dispatch) {
                    let (mut journal, owner) =
                        idle.take().ok_or("Production drain owner missing")?;
                    let spec = r.clone();
                    let io = tokio::runtime::Handle::current();
                    let telemetry = crate::drain::Telemetry::default();
                    drain_telemetry = telemetry.clone();
                    drain_active = true;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::preparation::drain(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            io,
                            telemetry,
                        );
                        (journal, owner, DriverCompletion::Complete(result))
                    }));
                }
            }
            if let Some(mut stream) = support_reply.take() {
                let snapshot = support_view(
                    &directory,
                    &r.spec.update_id,
                    support_permit.is_some(),
                    driver_error.clone(),
                    false,
                );
                let (preparation, error) = match snapshot {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 18,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                // A lost ACK does not revoke this already-committed fresh permit.
                if let Some(permit) = support_permit.filter(|_| dispatch) {
                    let (mut journal, owner) =
                        idle.take().ok_or("Support owner missing after admission")?;
                    let spec = r.clone();
                    let io = tokio::runtime::Handle::current();
                    support_active = true;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::preparation::candidate_support(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            io,
                        );
                        (journal, owner, DriverCompletion::Complete(result))
                    }));
                }
            }
            if let Some((mut stream, backup_required)) = shutdown_reply.take() {
                let outcome = if backup_required {
                    backup_view(
                        &directory,
                        &r.spec.update_id,
                        prepared.is_some(),
                        driver_error.clone(),
                        &shutdown_custody,
                        false,
                        false,
                    )
                } else {
                    shutdown_view(
                        &directory,
                        &r.spec.update_id,
                        prepared.is_some(),
                        driver_error.clone(),
                        &shutdown_custody,
                        false,
                    )
                };
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: if backup_required { 15 } else { 14 },
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                // Admission committed before ACK. Losing the response does not
                // revoke the original one-use permission or spawn another request.
                if let Some(permit) = prepared.filter(|_| dispatch) {
                    let (mut journal, owner) = idle
                        .take()
                        .ok_or("Shutdown owner missing after admission")?;
                    let spec = r.clone();
                    let io = tokio::runtime::Handle::current();
                    let custody = shutdown_custody.clone();
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::shutdown::execute(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            io,
                            custody,
                        );
                        (journal, owner, DriverCompletion::Complete(result))
                    }));
                }
            }
            if let Some(mut stream) = backup_reply.take() {
                let outcome = backup_view(
                    &directory,
                    &r.spec.update_id,
                    backup_permit.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    false,
                    false,
                );
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 15,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                if let Some(permit) = backup_permit.filter(|_| dispatch) {
                    let guard = shutdown_custody
                        .loan()?
                        .ok_or("Backup original custody missing after admission")?;
                    let (mut journal, owner) = idle.take().ok_or("Backup driver owner missing")?;
                    let spec = r.clone();
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::state_backup::capture_final_state(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            &guard,
                        )
                        .map(|_| ());
                        (journal, owner, DriverCompletion::Complete(result))
                    }));
                }
            }
            if let Some(mut stream) = runtime_reply.take() {
                let outcome = runtime_files_view(
                    &directory,
                    &r.spec.update_id,
                    runtime_permit.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    false,
                );
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 16,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                if let Some(permit) = runtime_permit.filter(|_| dispatch) {
                    let guard = shutdown_custody
                        .loan()?
                        .ok_or("Runtime preparation original custody missing")?;
                    let (mut journal, owner) =
                        idle.take().ok_or("Runtime preparation owner missing")?;
                    let spec = r.clone();
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::runtime_preparation::prepare_runtime_files(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            &guard,
                        )
                        .map(|_| ());
                        (journal, owner, DriverCompletion::Complete(result))
                    }));
                }
            }
            if let Some(mut stream) = replacement_reply.take() {
                let snapshot = replacement_view(
                    &directory,
                    &r.spec.update_id,
                    replacement_permit.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    &replacement_signal,
                    false,
                );
                let (preparation, error) = match snapshot {
                    Ok(v) => (Some(v), driver_error.clone()),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 17,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let dispatch = response.error.is_none();
                if response.error.is_some() {
                    driver_error = response.error.clone();
                }
                let _ = ipc::write(&mut stream, &response).await;
                // Lost ACK does not revoke this already-committed one-use permit.
                if let Some(permit) = replacement_permit.filter(|_| dispatch) {
                    let (mut journal, owner) =
                        idle.take().ok_or("Replacement driver owner missing")?;
                    let spec = r.clone();
                    let custody = shutdown_custody.clone();
                    replacement_signal = crate::runtime_replacement::CustodySignal::default();
                    let signal = replacement_signal.clone();
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = crate::runtime_replacement::execute(
                            &mut journal,
                            &owner,
                            &spec,
                            *permit,
                            &custody,
                            signal,
                        );
                        (journal, owner, DriverCompletion::ReplacementDone(result))
                    }));
                }
            }
        }
        if work.is_none() && pending_abort.is_some() {
            let permit = pending_abort.take().unwrap();
            let (mut journal, owner) = idle
                .take()
                .ok_or("Abort owner missing after copy quiescence")?;
            let spec = r.clone();
            let io = tokio::runtime::Handle::current();
            driver_error = None;
            work = Some(tokio::task::spawn_blocking(move || {
                let result = permit.execute(&mut journal, &owner, &spec, io).map(|_| ());
                (journal, owner, DriverCompletion::Complete(result))
            }));
        }
        if let Some((journal, owner)) = &idle {
            journal.check_source_authority(owner).map_err(error)?;
        } else if !files_actor.keeper_preparing() {
            // The original retirement driver may already have committed Keeper
            // while its JoinHandle is not yet marked finished. Its transaction
            // owns all checks. Never kill its original Job by applying the old
            // coordinator's Active-record comparison in this narrow window.
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
        let bytes = match ipc::read_bytes(&mut stream).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("keeperLaunchVersion").is_some() {
            let q = serde_json::from_slice::<KeeperLaunchRequest>(&bytes).ok();
            let reply = keeper_launch::rejected(
                q.as_ref(),
                "Original keeper custody unavailable; no coordinator creation",
            );
            let _ = ipc::write(&mut stream, &reply).await;
            continue;
        }
        if value.get("keeperProbeVersion").is_some() {
            let query = serde_json::from_slice::<KeeperProbeRequest>(&bytes).ok();
            // No current original custody in this coordinator loop. Never
            // adopt SQL rows or begin retirement in response to a probe.
            let reply = keeper_probe::respond(None, query.as_ref(), pid, |_, _| {
                Err("Original keeper custody unavailable".into())
            });
            let _ = ipc::write(&mut stream, &reply).await;
            continue;
        }
        if value.get("keeperControlVersion").is_some() {
            let q: KeeperControlRequest = match serde_json::from_slice(&bytes) {
                Ok(q) => q,
                Err(_) => {
                    let _ = ipc::write(
                        &mut stream,
                        &keeper_control::response(
                            &r,
                            None,
                            KeeperControlResult::Rejected {
                                message: "Invalid keeper request; no fallback".into(),
                            },
                        ),
                    )
                    .await;
                    continue;
                }
            };
            let mut dispatch = false;
            let result = (|| -> Result<KeeperControlResult> {
                keeper_control::validate(&r, &q, pid, false)?;
                if r.phase != SupervisorPhase::Active || r.spec.worker.is_none() {
                    return Err(
                        "Keeper retirement requires the original Active worker-bound coordinator"
                            .into(),
                    );
                }
                if work.is_some() || !runtime_reads.is_empty() {
                    return Ok(KeeperControlResult::Busy {
                        message: "Original coordinator is busy; no second retirement accepted"
                            .into(),
                    });
                }
                if q.command == KeeperControlCommand::Observe {
                    return Ok(KeeperControlResult::Status {
                        status: Box::new(keeper_control::not_selected(driver_error.clone())),
                    });
                }
                if q.command != KeeperControlCommand::RetireCoordinator {
                    return Err(
                        "Writer-close retry requires an already selected original retirement owner"
                            .into(),
                    );
                }
                if pending_abort.is_some() || !files_actor.can_retire_keeper() {
                    return Err("Keeper retirement requires original confirmed finalization custody and no pending admission".into());
                }
                dispatch = true;
                Ok(KeeperControlResult::Busy {
                    message: "Original keeper retirement accepted".into(),
                })
            })();
            if dispatch && result.is_ok() {
                #[cfg(windows)]
                {
                    let guard = files_actor.take_for_keeper()?;
                    let (mut journal, owner) =
                        idle.take().ok_or("Original keeper journal owner missing")?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let done = match guard.prepare_keeper_blocking(&mut journal, &owner, &spec)
                        {
                            Ok(v) => DriverCompletion::KeeperPrepared(Box::new(v)),
                            Err(e) => DriverCompletion::KeeperPreparationFailed(e),
                        };
                        (journal, owner, done)
                    }));
                    keeper_reply = Some((stream, q));
                    continue;
                }
            }
            let result = match result {
                Ok(v) => v,
                Err(message) => KeeperControlResult::Rejected { message },
            };
            let _ = ipc::write(&mut stream, &keeper_control::response(&r, Some(&q), result)).await;
            continue;
        }
        let request: SupervisorRequest = match serde_json::from_slice(&bytes) {
            Ok(q) => q,
            Err(_) => continue,
        };
        if matches!(
            request.action,
            SupervisorAction::ShutdownCandidate
                | SupervisorAction::ObserveCandidateShutdown
                | SupervisorAction::ShutdownCandidateForBackup
        ) {
            let mut start = false;
            let backup_required = request.action == SupervisorAction::ShutdownCandidateForBackup;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != if backup_required { 15 } else { 14 }
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(format!(
                        "Shutdown requires exact Active worker-bound v{} supervisor",
                        if backup_required { 15 } else { 14 }
                    ));
                }
                let view = if backup_required {
                    backup_view(
                        &directory,
                        &r.spec.update_id,
                        work.is_some(),
                        driver_error.clone(),
                        &shutdown_custody,
                        shutdown_preparing,
                        backup_preparing,
                    )
                } else {
                    shutdown_view(
                        &directory,
                        &r.spec.update_id,
                        work.is_some(),
                        driver_error.clone(),
                        &shutdown_custody,
                        shutdown_preparing,
                    )
                }?;
                if request.action == SupervisorAction::ObserveCandidateShutdown
                    || view
                        .candidate_shutdown
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                    || shutdown_preparing
                {
                    return Ok(view);
                }
                if work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::Quiesced
                {
                    return Err("Shutdown requires fresh Quiesced, idle owner and no unresolved driver failure".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let (mut journal, owner) = idle.take().ok_or("Shutdown preflight owner missing")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                shutdown_preparing = true;
                shutdown_reply = Some((stream, backup_required));
                driver_error = None;
                // The original reply waits for durable admission. Status and
                // coordinated abort stay responsive during readonly preflight.
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = if backup_required {
                        crate::shutdown::prepare_for_backup(&mut journal, &owner, &spec, io)
                    } else {
                        crate::shutdown::prepare(&mut journal, &owner, &spec, io)
                    }
                    .map(Box::new);
                    (journal, owner, DriverCompletion::ShutdownPrepared(result))
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: if backup_required { 15 } else { 14 },
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ReplaceRuntime | SupervisorAction::ObserveRuntimeReplacement
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 17
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Runtime replacement requires exact Active bound v17 supervisor".into(),
                    );
                }
                let view = replacement_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    &replacement_signal,
                    replacement_admitting,
                )?;
                if request.action == SupervisorAction::ObserveRuntimeReplacement
                    || replacement_admitting
                    || view
                        .runtime_replacement
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                {
                    return Ok(view);
                }
                if !cfg!(windows)
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::Replacing
                    || !shutdown_custody.present()?
                    || view
                        .runtime_preparation
                        .as_ref()
                        .is_none_or(|p| p.phase != RuntimePreparationPhase::Prepared)
                    || view
                        .candidate_state_backup
                        .as_ref()
                        .is_none_or(|b| b.phase != CandidateStateBackupPhase::Verified)
                {
                    return Err("Replacement needs native Windows, idle original custody and verified preparation/backup".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let loan = shutdown_custody
                    .loan()?
                    .ok_or("Replacement original custody missing")?;
                let (mut journal, owner) =
                    idle.take().ok_or("Replacement admission owner missing")?;
                let spec = r.clone();
                replacement_admitting = true;
                replacement_reply = Some(stream);
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = journal
                        .admit_runtime_replacement(&owner, &spec, Some(&loan))
                        .map_err(error);
                    (
                        journal,
                        owner,
                        DriverCompletion::ReplacementAdmitted(result),
                    )
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 17,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::PrepareRuntimeFiles | SupervisorAction::ObserveRuntimeFiles
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 16
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Runtime preparation requires exact Active bound v16 supervisor".into(),
                    );
                }
                let view = runtime_files_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    runtime_admitting,
                )?;
                if request.action == SupervisorAction::ObserveRuntimeFiles
                    || view
                        .runtime_preparation
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                    || runtime_admitting
                {
                    return Ok(view);
                }
                if work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::Replacing
                    || !shutdown_custody.present()?
                    || view
                        .candidate_state_backup
                        .as_ref()
                        .is_none_or(|b| b.phase != CandidateStateBackupPhase::Verified)
                {
                    return Err("Runtime preparation needs verified final backup, original custody and idle Replacing owner".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let guard = shutdown_custody
                    .loan()?
                    .ok_or("Runtime preparation custody missing")?;
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Runtime preparation admission owner missing")?;
                let spec = r.clone();
                runtime_admitting = true;
                runtime_reply = Some(stream);
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = journal
                        .admit_runtime_preparation(&owner, &spec, Some(&guard))
                        .map_err(error);
                    (journal, owner, DriverCompletion::RuntimeAdmitted(result))
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 16,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::BackupWorkerState | SupervisorAction::ObserveStateBackup
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 15
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Final backup requires exact Active bound v15 supervisor".into());
                }
                let view = backup_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &shutdown_custody,
                    shutdown_preparing,
                    backup_preparing,
                )?;
                if request.action == SupervisorAction::ObserveStateBackup
                    || view
                        .candidate_state_backup
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                    || backup_preparing
                {
                    return Ok(view);
                }
                if work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::Replacing
                    || !shutdown_custody.present()?
                {
                    return Err("Final backup requires original live exit custody, idle owner and unchanged Replacing root".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let guard = shutdown_custody
                    .loan()?
                    .ok_or("Final backup original custody missing")?;
                let (mut journal, owner) =
                    idle.take().ok_or("Final backup admission owner missing")?;
                let spec = r.clone();
                backup_preparing = true;
                backup_reply = Some(stream);
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = journal
                        .admit_state_backup(&owner, &spec, Some(&guard))
                        .map_err(error);
                    (journal, owner, DriverCompletion::BackupAdmitted(result))
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 15,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    error,
                    worker_control: None,
                    preparation,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::LaunchActivationReceiver | SupervisorAction::ObserveActivationProcess
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 22
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Activation process requires exact Active bound v22 supervisor".into(),
                    );
                }
                let view = files_actor.process_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &replacement_signal,
                )?;
                if request.action == SupervisorAction::ObserveActivationProcess
                    || files_actor.process_started()
                {
                    return Ok(view);
                }
                if !files_actor.can_launch()
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::VerifyingLocal
                {
                    return Err("Activation receiver needs the idle original Bound guard; no replay/adoption".into());
                }
                start = true;
                Ok(view)
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) =
                        idle.take().ok_or("Activation process owner missing")?;
                    let guard = files_actor.take_for_launch()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = guard.launch_receiver_blocking(&mut journal, &owner, &spec);
                        (journal, owner, DriverCompletion::ActivationProcess(result))
                    }));
                    // Volatile dispatch ACK, never an assertion that CreateProcess
                    // or Ready happened. The library persists intent before OS.
                    files_actor.process_view(
                        &directory,
                        &r.spec.update_id,
                        true,
                        None,
                        &replacement_signal,
                    )
                }
                #[cfg(not(windows))]
                {
                    Err("Activation process requires native original custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 22,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ActivateJournalWriter | SupervisorAction::ObserveJournalWriter
        ) {
            let mut start = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 24
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Journal writer requires exact Active bound v24 supervisor".into());
                }
                if request.action == SupervisorAction::ObserveJournalWriter
                    || files_actor.writer_started()
                {
                    if work.is_some() {
                        return Ok(None);
                    }
                    return files_actor
                        .writer_view(
                            &directory,
                            &r.spec.update_id,
                            false,
                            driver_error.clone(),
                            &replacement_signal,
                        )
                        .map(Some);
                }
                if !files_actor.can_activate_writer() || work.is_some() || driver_error.is_some() {
                    return Err("Journal writer needs the idle original StateOwned guard; no replay/adoption".into());
                }
                start = true;
                Ok(None) // Do not spend the remaining original window on a status audit.
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle.take().ok_or("Journal writer owner missing")?;
                    let (guard, key) = files_actor.take_for_writer()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result =
                            guard.activate_journal_blocking(&mut journal, &owner, &spec, &key);
                        (journal, owner, DriverCompletion::ActivationWriter(result))
                    }));
                    Ok(None)
                }
                #[cfg(not(windows))]
                {
                    Err("Journal writer requires native original custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 24,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::FinalizeActivation | SupervisorAction::ObserveActivationFinalization
        ) {
            let observe = request.action == SupervisorAction::ObserveActivationFinalization;
            let mut dispatch = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 29
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Finalization requires exact Active bound v29 supervisor".into());
                }
                if work.is_some() {
                    return Ok(None);
                }
                if !(if observe {
                    files_actor.can_observe_finalization()
                } else {
                    files_actor.can_finalize()
                }) {
                    return files_actor
                        .finalization_view(
                            &directory,
                            &r.spec.update_id,
                            false,
                            driver_error.clone(),
                        )
                        .map(Some);
                }
                dispatch = true;
                Ok(None)
            })();
            if dispatch && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle
                        .take()
                        .ok_or("Original finalization journal owner missing")?;
                    let spec = r.clone();
                    driver_error = None;
                    if observe {
                        let guard = files_actor.take_finalization_observation()?;
                        work = Some(tokio::task::spawn_blocking(move || {
                            let result = guard.observe_blocking(&mut journal, &owner, &spec);
                            (journal, owner, DriverCompletion::Finalization(result))
                        }));
                    } else {
                        let (guard, lifetime, key) = files_actor.take_for_finalization()?;
                        work = Some(tokio::task::spawn_blocking(move || {
                            let completion = match guard.prepare_finalization_blocking(
                                &mut journal,
                                &owner,
                                &spec,
                                &lifetime,
                                &key,
                            ) {
                                Ok(prepared) => DriverCompletion::Finalization(
                                    prepared.deliver_blocking(&mut journal, &owner, &spec),
                                ),
                                Err(failure) => DriverCompletion::FinalizationPreparationFailed(
                                    Box::new(failure),
                                ),
                            };
                            (journal, owner, completion)
                        }));
                    }
                    finalization_reply = Some(stream);
                    continue;
                }
            }
            let (preparation, error) = match outcome {
                Ok(view) => (view, None),
                Err(error) => (None, Some(error)),
            };
            let response = SupervisorResponse {
                contract_version: 29,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if request.action == SupervisorAction::ObserveResidentConnection {
            let mut dispatch = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 28
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Resident observation requires exact Active bound v28 supervisor".into(),
                    );
                }
                if work.is_some() {
                    return Ok(None);
                }
                if !files_actor.can_observe_resident() {
                    return files_actor
                        .resident_view(&directory, &r.spec.update_id, false, driver_error.clone())
                        .map(Some);
                }
                dispatch = true;
                Ok(None)
            })();
            if dispatch && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) =
                        idle.take().ok_or("Resident observation owner missing")?;
                    let (guard, key) = files_actor.take_for_resident()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result =
                            guard.observe_resident_blocking(&mut journal, &owner, &spec, &key);
                        (journal, owner, DriverCompletion::ActivationResident(result))
                    }));
                    resident_reply = Some(stream);
                    continue;
                }
            }
            let (preparation, error) = match outcome {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 28,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::EnterActivationLifetime | SupervisorAction::ObserveActivationLifetime
        ) {
            let observe = request.action == SupervisorAction::ObserveActivationLifetime;
            let mut dispatch = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 27
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Lifetime requires exact Active bound v27 supervisor".into());
                }
                if work.is_some() {
                    return Ok(None);
                }
                if (!observe && files_actor.lifetime_started())
                    || (observe && !files_actor.can_observe_lifetime())
                {
                    return files_actor
                        .lifetime_view(
                            &directory,
                            &r.spec.update_id,
                            false,
                            driver_error.clone(),
                            &replacement_signal,
                        )
                        .map(Some);
                }
                if !observe && (!files_actor.can_enter_lifetime() || driver_error.is_some()) {
                    return Err(
                        "Lifetime needs original confirmed controller custody; no replay/adoption"
                            .into(),
                    );
                }
                dispatch = true;
                Ok(None)
            })();
            if dispatch && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle.take().ok_or("Lifetime owner missing")?;
                    let (guard, key) = files_actor.take_for_lifetime(observe)?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = if observe {
                            guard.observe_lifetime_blocking(&mut journal, &owner, &spec, &key)
                        } else {
                            guard
                                .prepare_lifetime_blocking(&mut journal, &owner, &spec, &key)
                                .and_then(|prepared| {
                                    prepared.enter_lifetime_blocking(&mut journal, &owner, &spec)
                                })
                        };
                        (journal, owner, DriverCompletion::ActivationLifetime(result))
                    }));
                    if observe {
                        // Return the fresh observation when this same driver is
                        // collected. A disconnected caller cannot cancel it.
                        lifetime_reply = Some(stream);
                        continue;
                    }
                }
            }
            let (preparation, error) = match outcome {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 27,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ConnectActivationController
                | SupervisorAction::ObserveActivationController
        ) {
            let mut start = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 26
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Controller requires exact Active bound v26 supervisor".into());
                }
                if request.action == SupervisorAction::ObserveActivationController
                    || files_actor.controller_started()
                {
                    if work.is_some() {
                        return Ok(None);
                    }
                    return files_actor
                        .controller_view(
                            &directory,
                            &r.spec.update_id,
                            false,
                            driver_error.clone(),
                            &replacement_signal,
                        )
                        .map(Some);
                }
                if !files_actor.can_connect_controller() || work.is_some() || driver_error.is_some()
                {
                    return Err("Controller needs the idle original confirmed local-service owner; no replay/adoption".into());
                }
                start = true;
                Ok(None) // Do not spend the remaining original window on a status audit.
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle.take().ok_or("Controller owner missing")?;
                    let (guard, key) = files_actor.take_for_controller()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = guard
                            .prepare_controller_blocking(&mut journal, &owner, &spec, &key, None)
                            .and_then(|prepared| {
                                prepared.start_controller_blocking(&mut journal, &owner, &spec)
                            });
                        (
                            journal,
                            owner,
                            DriverCompletion::ActivationController(result),
                        )
                    }));
                    Ok(None)
                }
                #[cfg(not(windows))]
                {
                    Err("Controller requires native original custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 26,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ActivateLocalService | SupervisorAction::ObserveLocalService
        ) {
            let mut start = false;
            let outcome = (|| -> Result<Option<UpdatePreparationView>> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 25
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Local service requires exact Active bound v25 supervisor".into());
                }
                if request.action == SupervisorAction::ObserveLocalService
                    || files_actor.service_started()
                {
                    if work.is_some() {
                        return Ok(None);
                    }
                    return files_actor
                        .service_view(
                            &directory,
                            &r.spec.update_id,
                            false,
                            driver_error.clone(),
                            &replacement_signal,
                        )
                        .map(Some);
                }
                if !files_actor.can_start_service() || work.is_some() || driver_error.is_some() {
                    return Err("Local service needs the idle original confirmed writer guard; no replay/adoption".into());
                }
                start = true;
                Ok(None) // Do not spend the remaining original window on a status audit.
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle.take().ok_or("Local service owner missing")?;
                    let (guard, key) = files_actor.take_for_service()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = guard
                            .prepare_local_service_blocking(&mut journal, &owner, &spec, &key)
                            .and_then(|prepared| {
                                prepared.start_local_service_blocking(&mut journal, &owner, &spec)
                            });
                        (journal, owner, DriverCompletion::ActivationService(result))
                    }));
                    Ok(None)
                }
                #[cfg(not(windows))]
                {
                    Err("Local service requires native original custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 25,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::HandoffActivationState | SupervisorAction::ObserveActivationState
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 23
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("State handoff requires exact Active bound v23 supervisor".into());
                }
                let view = files_actor.state_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &replacement_signal,
                )?;
                if request.action == SupervisorAction::ObserveActivationState
                    || files_actor.handoff_started()
                {
                    return Ok(view);
                }
                if !files_actor.can_handoff()
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::VerifyingLocal
                {
                    return Err(
                        "State handoff needs the idle original Ready guard; no replay/adoption"
                            .into(),
                    );
                }
                start = true;
                Ok(view)
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) = idle.take().ok_or("State handoff owner missing")?;
                    let guard = files_actor.take_for_handoff()?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = guard.handoff_state_blocking(&mut journal, &owner, &spec);
                        (journal, owner, DriverCompletion::ActivationState(result))
                    }));
                    // Volatile dispatch ACK, never an assertion of lock transfer.
                    // HandoffIssued precedes the single native release effect.
                    files_actor.state_view(
                        &directory,
                        &r.spec.update_id,
                        true,
                        None,
                        &replacement_signal,
                    )
                }
                #[cfg(not(windows))]
                {
                    Err("State handoff requires native original custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 23,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::BindActivationFiles | SupervisorAction::ObserveActivationFiles
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 21
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Activation files require exact Active bound v21 supervisor".into());
                }
                let view = files_actor.view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    &replacement_signal,
                )?;
                if request.action == SupervisorAction::ObserveActivationFiles
                    || !files_actor.is_idle()
                {
                    return Ok(view);
                }
                #[cfg(windows)]
                let present = replacement_guard.is_some() && activation_preparation.is_some();
                #[cfg(not(windows))]
                let present = false;
                if !present
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::VerifyingLocal
                {
                    return Err("Activation files need fresh resident preparation and idle original custody".into());
                }
                start = true;
                Ok(view)
            })();
            let outcome = if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (journal, owner) = idle.take().ok_or("Activation files owner missing")?;
                    let preparation = activation_preparation
                        .take()
                        .ok_or("Fresh preparation missing")?;
                    let original = replacement_guard
                        .take()
                        .ok_or("Original replacement custody missing")?;
                    files_actor.begin(preparation.record())?;
                    let spec = r.clone();
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result =
                            journal.bind_activation_files(&owner, &spec, *preparation, original);
                        (journal, owner, DriverCompletion::ActivationFiles(result))
                    }));
                    // This ACK reports volatile resource binding, not an effect
                    // grant. Prepared is already durable; cold data never binds.
                    files_actor.view(
                        &directory,
                        &r.spec.update_id,
                        true,
                        None,
                        &replacement_signal,
                    )
                }
                #[cfg(not(windows))]
                {
                    Err("Activation file binding requires native custody".into())
                }
            } else {
                outcome
            };
            let (preparation, error) = match outcome {
                Ok(value) => (Some(value), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 21,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                worker_control: None,
                preparation,
                error,
            };
            let _ = ipc::write(&mut stream, &response).await;
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::PrepareActivation | SupervisorAction::ObserveActivationPreparation
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 20
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Activation preparation requires exact Active bound v20 supervisor".into(),
                    );
                }
                let view = activation_view(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                    activation_preparing,
                    &replacement_signal,
                )?;
                if request.action == SupervisorAction::ObserveActivationPreparation
                    || activation_preparing
                    || view
                        .activation_preparation
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                {
                    return Ok(view);
                }
                #[cfg(windows)]
                let present = replacement_guard.is_some();
                #[cfg(not(windows))]
                let present = false;
                if !present
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::VerifyingLocal
                {
                    return Err("Activation preparation needs idle original replacement custody and VerifyingLocal root".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                #[cfg(windows)]
                {
                    let (mut journal, owner) =
                        idle.take().ok_or("Activation preparation owner missing")?;
                    let mut guard = replacement_guard
                        .take()
                        .ok_or("Original replacement guard missing")?;
                    let spec = r.clone();
                    activation_preparing = true;
                    activation_reply = Some(stream);
                    driver_error = None;
                    work = Some(tokio::task::spawn_blocking(move || {
                        let result = journal
                            .prepare_activation(&owner, &spec, &mut guard)
                            .map_err(error);
                        (
                            journal,
                            owner,
                            DriverCompletion::ActivationPrepared { guard, result },
                        )
                    }));
                }
                #[cfg(not(windows))]
                return Err("Activation preparation requires native Windows custody".into());
            } else {
                let (preparation, error) = match outcome {
                    Ok(value) => (Some(value), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 20,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::DrainCompatibleCandidate | SupervisorAction::ObserveCompatibleDrain
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 19
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Production drain requires exact Active bound v19 supervisor".into(),
                    );
                }
                let view = compatible_view(
                    &directory,
                    &r.spec.update_id,
                    drain_active,
                    driver_error.clone(),
                    compatible_admitting,
                )?;
                if request.action == SupervisorAction::ObserveCompatibleDrain
                    || compatible_admitting
                    || view
                        .compatible_drain
                        .as_ref()
                        .is_some_and(|v| v.attempt_id.is_some())
                {
                    return Ok(view);
                }
                if !cfg!(windows)
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::CandidateVerified
                {
                    return Err("Production drain needs idle native verified candidate and terminal support".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Production drain admission owner missing")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                compatible_admitting = true;
                compatible_reply = Some(stream);
                drain_active = true;
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result =
                        crate::preparation::compatible_drain(&mut journal, &owner, &spec, io);
                    (journal, owner, DriverCompletion::CompatibleAdmitted(result))
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 19,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::DrainCandidate | SupervisorAction::ObserveCandidateDrain
        ) {
            let mut admitted = None;
            let outcome = async {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 13
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Drain requires exact Active worker-bound v13 supervisor".into());
                }
                let mut view = UpdateJournal::drain_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                if request.action == SupervisorAction::ObserveCandidateDrain
                    || view
                        .candidate_drain
                        .as_ref()
                        .is_some_and(|v| v.phase != CandidateDrainPhase::NotAdmitted)
                {
                    view.candidate_drain.as_mut().unwrap().worker_observation =
                        drain_telemetry.sample()?;
                    return Ok(view);
                }
                if work.is_some() || driver_error.is_some() {
                    return Err(
                        "Active driver or unresolved failure prevents drain admission".into(),
                    );
                }
                let lease = forward_worker(&r, WorkerUpdateControlCommand::Observe)
                    .await?
                    .into_lease()?
                    .ok_or("Drain source lease missing")?;
                let (journal, owner) = idle.as_mut().ok_or("Drain owner unavailable")?;
                if let crate::drain::DrainAdmission::Fresh(permit) =
                    journal.admit_drain(owner, &r, &lease).map_err(error)?
                {
                    admitted = Some(permit);
                    drain_telemetry = crate::drain::Telemetry::default();
                }
                UpdateJournal::drain_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    admitted.is_some(),
                    None,
                )
                .map_err(error)
            }
            .await;
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 13,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if let Some(permit) = admitted.filter(|_| dispatch) {
                let (mut journal, owner) =
                    idle.take().ok_or("Drain owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                let telemetry = drain_telemetry.clone();
                driver_error = None;
                drain_active = true;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = crate::preparation::drain(
                        &mut journal,
                        &owner,
                        &spec,
                        *permit,
                        io,
                        telemetry,
                    );
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::CaptureCandidateSupport | SupervisorAction::ObserveCandidateSupport
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 18
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Support requires exact Active worker-bound v18 supervisor".into());
                }
                let view = support_view(
                    &directory,
                    &r.spec.update_id,
                    support_active,
                    driver_error.clone(),
                    support_admitting,
                )?;
                if request.action == SupervisorAction::ObserveCandidateSupport
                    || support_admitting
                    || view
                        .candidate_support
                        .as_ref()
                        .is_some_and(|s| s.attempt_id.is_some())
                {
                    return Ok(view);
                }
                if !cfg!(windows)
                    || work.is_some()
                    || driver_error.is_some()
                    || view.update.phase != UpdatePhase::CandidateVerified
                {
                    return Err("Support requires native Windows, idle verified candidate and no unresolved failure".into());
                }
                start = true;
                Ok(view)
            })();
            if start && outcome.is_ok() {
                let (mut journal, owner) = idle.take().ok_or("Support admission owner missing")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                support_active = true;
                support_admitting = true;
                support_reply = Some(stream);
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = crate::preparation::admit_candidate_support(
                        &mut journal,
                        &owner,
                        &spec,
                        io,
                    );
                    (journal, owner, DriverCompletion::SupportAdmitted(result))
                }));
            } else {
                let (preparation, error) = match outcome {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                let response = SupervisorResponse {
                    contract_version: 18,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation,
                    error,
                };
                let _ = ipc::write(&mut stream, &response).await;
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ValidateCandidateHealth | SupervisorAction::ObserveCandidateHealth
        ) {
            let mut admitted = None;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 10
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Health requires exact Active worker-bound v10 supervisor".into());
                }
                let saved = UpdateJournal::health_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                if request.action == SupervisorAction::ObserveCandidateHealth
                    || saved
                        .candidate_health
                        .as_ref()
                        .is_some_and(|h| h.phase != CandidateHealthPhase::NotAdmitted)
                {
                    return Ok(saved);
                }
                if work.is_some() || driver_error.is_some() {
                    return Err(
                        "Another driver or unresolved failure prevents health admission".into(),
                    );
                }
                let (journal, owner) = idle.as_mut().ok_or("Health owner unavailable")?;
                if let crate::health::HealthAdmission::Fresh(permit) =
                    crate::health::admit(journal, owner, &r)?
                {
                    admitted = Some(permit);
                }
                UpdateJournal::health_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    admitted.is_some(),
                    None,
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 10,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if let Some(permit) = admitted.filter(|_| dispatch) {
                let (mut journal, owner) =
                    idle.take().ok_or("Health owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                driver_error = None;
                health_active = true;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result =
                        crate::preparation::health(&mut journal, &owner, &spec, *permit, io);
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ArchiveRuntimePreimages | SupervisorAction::ObserveRuntimePreimages
        ) {
            let mut admitted = None;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 8
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Runtime preimages require exact Active worker-bound v8 supervisor".into(),
                    );
                }
                let saved = UpdateJournal::preimages_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                if request.action == SupervisorAction::ObserveRuntimePreimages
                    || matches!(
                        saved.runtime_preimages,
                        Some(RuntimePreimagesView::Verified { .. })
                    )
                {
                    return Ok(saved);
                }
                if work.is_some() {
                    if preimages_active {
                        return Ok(saved);
                    }
                    return Err("Another preparation driver is active".into());
                }
                if driver_error.is_some() {
                    return Err("Preimages failure must be inspected/aborted; no replay".into());
                }
                let (journal, owner) = idle.as_mut().ok_or("Preimages owner unavailable")?;
                match crate::runtime_preimages::admit(journal, owner, &r)? {
                    crate::runtime_preimages::RuntimePreimagesAdmission::Fresh(p) => {
                        admitted = Some(p)
                    }
                    crate::runtime_preimages::RuntimePreimagesAdmission::Existing(_) => {}
                }
                UpdateJournal::preimages_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    admitted.is_some(),
                    None,
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 8,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if let Some(permit) = admitted.filter(|_| dispatch) {
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Preimages owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                preimages_active = true;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result =
                        crate::runtime_preimages::execute(&mut journal, &owner, &spec, *permit, io)
                            .map(|_| ());
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::CaptureRuntimeBaseline | SupervisorAction::ObserveRuntimeBaseline
        ) {
            let mut start = false;
            let outcome = (|| -> Result<UpdatePreparationView> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 7
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Runtime baseline requires exact Active worker-bound v7 supervisor".into(),
                    );
                }
                let saved = UpdateJournal::runtime_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                if request.action == SupervisorAction::ObserveRuntimeBaseline
                    || matches!(
                        saved.runtime_baseline,
                        Some(RuntimeBaselineView::Verified { .. })
                    )
                {
                    return Ok(saved);
                }
                if work.is_some() {
                    if saved.update.phase == UpdatePhase::Requested
                        && matches!(
                            saved.runtime_baseline,
                            Some(RuntimeBaselineView::Pending { .. })
                        )
                    {
                        return Ok(saved);
                    }
                    return Err("Another preparation driver is active".into());
                }
                if driver_error.is_some() {
                    return Err("Runtime baseline failure must be inspected/aborted; status never replays it".into());
                }
                crate::preparation::configuration(&r)?;
                let (journal, owner) = idle.as_mut().ok_or("Runtime baseline owner unavailable")?;
                journal.admit_runtime_baseline(owner, &r).map_err(error)?;
                start = true;
                UpdateJournal::runtime_preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    true,
                    None,
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 7,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = start && response.error.is_none();
            // Lost ACK does not revoke admission; only readonly work is dispatched.
            let _ = ipc::write(&mut stream, &response).await;
            if dispatch {
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Runtime baseline owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = crate::runtime_baseline::capture(&mut journal, &owner, &spec, io)
                        .map(|_| ());
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::ObserveWorkerRuntime
                | SupervisorAction::ObserveWorkerCompatibility
                | SupervisorAction::ObserveWorkerReceipt
                | SupervisorAction::ObserveWorkerDrain
        ) {
            let required_version = match request.action {
                SupervisorAction::ObserveWorkerCompatibility => 9,
                SupervisorAction::ObserveWorkerReceipt => 11,
                SupervisorAction::ObserveWorkerDrain => 12,
                _ => 6,
            };
            let command = match required_version {
                11 => request
                    .worker_command
                    .clone()
                    .filter(|c| matches!(c, WorkerUpdateControlCommand::ObserveReceipt { .. })),
                9 => Some(WorkerUpdateControlCommand::ObserveCompatibility),
                12 => Some(WorkerUpdateControlCommand::ObserveDrain),
                _ => Some(WorkerUpdateControlCommand::ObserveRuntime),
            };
            let version = if request.contract_version == required_version {
                required_version
            } else {
                1
            };
            let allowed = (|| -> Result<()> {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != required_version
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || (required_version != 11 && request.worker_command.is_some())
                    || command.is_none()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        format!("Runtime observation requires exact Active worker-bound v{required_version} supervisor"),
                    );
                }
                if !runtime_reads.is_empty() {
                    return Err("Runtime observation active; retry readonly status".into());
                }
                Ok(())
            })();
            if let Err(message) = allowed {
                let response = SupervisorResponse {
                    contract_version: version,
                    launch_id: r.spec.launch_id.clone(),
                    record: None,
                    worker_control: None,
                    preparation: None,
                    error: Some(message),
                };
                let _ = ipc::write(&mut stream, &response).await;
            } else {
                let record = r.clone();
                let command = command.expect("Authorized readonly worker command");
                runtime_reads.spawn(async move {
                    let (worker_control, error) = match forward_worker(&record, command).await {
                        Ok(result) => (Some(result), None),
                        Err(message) => (None, Some(message)),
                    };
                    let response = SupervisorResponse {
                        contract_version: required_version,
                        launch_id: record.spec.launch_id,
                        record: None,
                        worker_control,
                        preparation: None,
                        error,
                    };
                    (stream, response)
                });
            }
            continue;
        }
        if request.action == SupervisorAction::AbortBeforeCutover {
            let mut admitted = None;
            let outcome = (|| {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 4
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Abort needs exact Active worker-bound v4 supervisor".into());
                }
                match UpdateJournal::inspect_abort(&directory, &r.spec.update_id) {
                    Ok(_) => {}
                    Err(e) if e.code == JournalErrorCode::NotFound => {
                        if work.is_some()
                            && !preimages_active
                            && !health_active
                            && !support_active
                            && !drain_active
                            && !shutdown_preparing
                        {
                            return Err("Preparation driver active; wait for confirmed source/compiler cleanup".into());
                        }
                        if preimages_active
                            || health_active
                            || support_active
                            || drain_active
                            || shutdown_preparing
                        {
                            // Same physical owner/epoch; Root Aborting fences the
                            // copier. Cleanup waits for its blocking callback.
                            let cfg = crate::preparation::configuration(&r)?;
                            let mut control =
                                UpdateJournal::open(&directory, &installation_scope(&cfg)?)
                                    .map_err(error)?;
                            if let crate::abort::AbortAdmission::New(permit) =
                                crate::abort::admit(&mut control, &control_authority, &r)?
                            {
                                pending_abort = Some(permit);
                            }
                        } else {
                            let (journal, owner) =
                                idle.as_mut().ok_or("Abort owner unavailable")?;
                            match crate::abort::admit(journal, owner, &r)? {
                                crate::abort::AbortAdmission::New(permit) => {
                                    admitted = Some(permit)
                                }
                                crate::abort::AbortAdmission::Observed(_) => {}
                            }
                        }
                    }
                    Err(e) => return Err(error(e)),
                }
                UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some() || admitted.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 4,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if let Some(permit) = admitted.filter(|_| dispatch) {
                let (mut journal, owner) =
                    idle.take().ok_or("Abort owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                work = Some(tokio::task::spawn_blocking(move || {
                    let result = permit.execute(&mut journal, &owner, &spec, io).map(|_| ());
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if request.action == SupervisorAction::VerifyCandidateArtifacts {
            let mut start = false;
            let outcome = (|| {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 5
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err(
                        "Artifact verification requires exact Active worker-bound v5 supervisor"
                            .into(),
                    );
                }
                let saved = UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                match UpdateJournal::inspect_candidate_artifacts(&directory, &r.spec.update_id) {
                    Ok(_) => return Ok(saved),
                    Err(e) if e.code == JournalErrorCode::NotFound => {}
                    Err(e) => return Err(error(e)),
                }
                if work.is_some() {
                    if artifacts_active {
                        return Ok(saved);
                    }
                    return Err("Another preparation driver is active".into());
                }
                if driver_error.is_some() {
                    return Err("Preparation failure must be inspected/aborted before artifact verification".into());
                }
                let cfg = crate::preparation::configuration(&r)?;
                let (journal, owner) = idle.as_mut().ok_or("Artifact owner unavailable")?;
                crate::build::saved_source_guard(journal, owner, &cfg, &r.spec.update_id)?;
                if saved.build.as_ref().is_none_or(|b| {
                    b.phase != UpdateBuildPhase::Completed
                        || b.outcome != Some(UpdateBuildOutcome::Succeeded)
                        || !b.cleanup_verified
                        || b.spec.owner_id != owner.owner_id()
                        || b.spec.owner_epoch != owner.epoch()
                }) {
                    return Err(
                        "Successful current compiler required before artifact verification".into(),
                    );
                }
                start = true;
                UpdateJournal::preparation_snapshot(&directory, &r.spec.update_id, true, None)
                    .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 5,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if start && dispatch {
                let (mut journal, owner) =
                    idle.take().ok_or("Artifact owner missing after dispatch")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                driver_error = None;
                artifacts_active = true;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result =
                        crate::preparation::candidate_artifacts(&mut journal, &owner, &spec, io);
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if request.action == SupervisorAction::BuildCandidate {
            let mut admitted = None;
            let outcome = (|| {
                authorize(&r, &request.token, pid, request.action)?;
                if request.contract_version != 3
                    || request.launch_id != r.spec.launch_id
                    || request.expected_revision != r.revision
                    || request.worker_command.is_some()
                    || r.phase != SupervisorPhase::Active
                    || r.spec.worker.is_none()
                {
                    return Err("Compiler needs exact Active worker-bound v3 supervisor".into());
                }
                let saved = UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)?;
                if saved.update.phase == UpdatePhase::Aborting
                    || saved
                        .source
                        .as_ref()
                        .is_some_and(|s| s.phase == SourceTransactionPhase::Aborted)
                {
                    return Err("Abort status requires supervisor contract v4".into());
                }
                if saved.build.is_some() {
                    return Ok(saved);
                }
                if work.is_some()
                    || saved.update.phase != UpdatePhase::Building
                    || driver_error.is_some()
                {
                    return Err(
                        "Verified source, idle driver and no unresolved preparation error required"
                            .into(),
                    );
                }
                let (journal, owner) = idle.as_mut().ok_or("Compiler owner unavailable")?;
                let (cfg, cmd) = crate::preparation::build_command(journal, &r)?;
                match crate::build::admit_build(
                    journal,
                    owner,
                    &cfg,
                    &r.spec.update_id,
                    "supervisor-candidate-build-v1",
                    &cmd,
                )? {
                    crate::build::BuildAdmission::New(permit) => admitted = Some(permit),
                    crate::build::BuildAdmission::Observed(_) => {}
                }
                UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    admitted.is_some(),
                    driver_error.clone(),
                )
                .map_err(error)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: 3,
                launch_id: r.spec.launch_id.clone(),
                record: None,
                error,
                worker_control: None,
                preparation,
            };
            let dispatch = response.error.is_none();
            let _ = ipc::write(&mut stream, &response).await;
            if let Some(permit) = admitted.filter(|_| dispatch) {
                let (mut journal, owner) = idle
                    .take()
                    .ok_or("Compiler owner missing after admission")?;
                let spec = r.clone();
                let io = tokio::runtime::Handle::current();
                driver_error = None;
                work = Some(tokio::task::spawn_blocking(move || {
                    let result =
                        crate::preparation::compiler(&mut journal, &owner, &spec, *permit, io);
                    (journal, owner, DriverCompletion::Complete(result))
                }));
            }
            continue;
        }
        if matches!(
            request.action,
            SupervisorAction::PrepareSource | SupervisorAction::ObservePreparation
        ) {
            let mut start = false;
            let outcome = (|| {
                authorize(&r, &request.token, pid, request.action)?;
                if !(request.contract_version == 2
                    || matches!(request.contract_version, 4 | 5)
                        && request.action == SupervisorAction::ObservePreparation)
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
                    if snapshot.update.phase == UpdatePhase::Requested && work.is_some() {
                        return Err(
                            "Old runtime preparation active; source preparation deferred".into(),
                        );
                    }
                    if snapshot.update.phase == UpdatePhase::Requested {
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
                let view = UpdateJournal::preparation_snapshot(
                    &directory,
                    &r.spec.update_id,
                    work.is_some() || start,
                    driver_error.clone(),
                )
                .map_err(error)?;
                if request.contract_version == 2
                    && (view.update.phase == UpdatePhase::Aborting
                        || view
                            .source
                            .as_ref()
                            .is_some_and(|s| s.phase == SourceTransactionPhase::Aborted))
                {
                    return Err("Abort status requires supervisor contract v4".into());
                }
                Ok(view)
            })();
            let (preparation, error) = match outcome {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let response = SupervisorResponse {
                contract_version: request.contract_version,
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
                    (journal, owner, DriverCompletion::Complete(result))
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
                if command.required_version()!=1 {
                    return Err("Versioned worker command requires supervisor v6/v9/v11/v12 for observations or an explicit shutdown contract; v1 forwarding is forbidden".into());
                }
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
                SupervisorAction::WorkerControl | SupervisorAction::PrepareSource | SupervisorAction::ObservePreparation | SupervisorAction::BuildCandidate | SupervisorAction::AbortBeforeCutover | SupervisorAction::VerifyCandidateArtifacts | SupervisorAction::ObserveWorkerRuntime | SupervisorAction::CaptureRuntimeBaseline | SupervisorAction::ObserveRuntimeBaseline | SupervisorAction::ArchiveRuntimePreimages | SupervisorAction::ObserveRuntimePreimages | SupervisorAction::ObserveWorkerCompatibility | SupervisorAction::ValidateCandidateHealth | SupervisorAction::ObserveCandidateHealth | SupervisorAction::ObserveWorkerReceipt | SupervisorAction::ObserveWorkerDrain | SupervisorAction::DrainCandidate | SupervisorAction::ObserveCandidateDrain | SupervisorAction::ShutdownCandidate | SupervisorAction::ObserveCandidateShutdown | SupervisorAction::ShutdownCandidateForBackup | SupervisorAction::BackupWorkerState | SupervisorAction::ObserveStateBackup | SupervisorAction::PrepareRuntimeFiles | SupervisorAction::ObserveRuntimeFiles
                | SupervisorAction::ReplaceRuntime | SupervisorAction::ObserveRuntimeReplacement
                | SupervisorAction::CaptureCandidateSupport | SupervisorAction::ObserveCandidateSupport
                | SupervisorAction::DrainCompatibleCandidate | SupervisorAction::ObserveCompatibleDrain
                | SupervisorAction::PrepareActivation | SupervisorAction::ObserveActivationPreparation
                | SupervisorAction::BindActivationFiles | SupervisorAction::ObserveActivationFiles
                | SupervisorAction::LaunchActivationReceiver | SupervisorAction::ObserveActivationProcess
                | SupervisorAction::HandoffActivationState | SupervisorAction::ObserveActivationState
                | SupervisorAction::ActivateJournalWriter | SupervisorAction::ObserveJournalWriter
                | SupervisorAction::ActivateLocalService | SupervisorAction::ObserveLocalService
                | SupervisorAction::ConnectActivationController | SupervisorAction::ObserveActivationController
                | SupervisorAction::EnterActivationLifetime | SupervisorAction::ObserveActivationLifetime
                | SupervisorAction::ObserveResidentConnection | SupervisorAction::FinalizeActivation | SupervisorAction::ObserveActivationFinalization => Err("Action needs its separate handler".into()),
                SupervisorAction::Stop if files_actor.service_started() => Err("Cannot stop a supervisor retaining the resident worker Job; keeper conversion/confirmed cleanup is required".into()),
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
        serde_json::json!({"sourcePreparationConfigured":source_preparation_configured,"publicUpdateCapabilityEnabled":false,"externalJobObserved":external_job,"schemaVersion":2,"kind":"supervisor_status","readOnly":true,"sourceEffectsEnabled":false,"runtimeApplied":crate::runtime_replacement::applied_observation(Some(update.phase)),"journalEpochCurrent":record.owner_epoch.is_some_and(|e|e==info.owner_epoch),"update":update,"journal":info,"physicalObservation":physical,"record":record}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::process::{Command, Stdio};
    #[test]
    fn replacement_view_v9_does_not_add_fields_to_frozen_views() {
        let temp = tempfile::tempdir().unwrap();
        let f = Fixture::new(temp.path());
        let directory = temp.path().join("runtime/update-state");
        let snapshots = [
            UpdateJournal::preparation_snapshot,
            UpdateJournal::runtime_preparation_snapshot,
            UpdateJournal::preimages_preparation_snapshot,
            UpdateJournal::health_preparation_snapshot,
            UpdateJournal::drain_preparation_snapshot,
            UpdateJournal::shutdown_preparation_snapshot,
            UpdateJournal::backup_preparation_snapshot,
            UpdateJournal::runtime_files_preparation_snapshot,
            UpdateJournal::runtime_replacement_snapshot,
            UpdateJournal::candidate_support_snapshot,
            UpdateJournal::compatible_drain_snapshot,
            UpdateJournal::activation_preparation_snapshot,
            UpdateJournal::activation_files_snapshot,
            UpdateJournal::activation_process_snapshot,
            UpdateJournal::activation_state_snapshot,
            UpdateJournal::activation_writer_snapshot,
            UpdateJournal::activation_service_snapshot,
            UpdateJournal::activation_controller_snapshot,
            UpdateJournal::activation_lifetime_snapshot,
        ];
        for (index, snapshot) in snapshots.into_iter().enumerate() {
            let view = snapshot(&directory, &f.id, false, None).unwrap();
            assert_eq!(view.schema_version as usize, index + 1);
            let value = serde_json::to_value(&view).unwrap();
            assert_eq!(value.get("compatibleDrain").is_some(), index >= 10);
            assert_eq!(value.get("activationPreparation").is_some(), index >= 11);
            assert_eq!(value.get("activationFiles").is_some(), index >= 12);
            assert_eq!(value.get("activationProcess").is_some(), index >= 13);
            assert_eq!(value.get("activationState").is_some(), index >= 14);
            assert_eq!(value.get("activationWriter").is_some(), index >= 15);
            assert_eq!(value.get("activationService").is_some(), index >= 16);
            assert_eq!(value.get("activationController").is_some(), index >= 17);
            assert_eq!(value.get("activationLifetime").is_some(), index >= 18);
            assert!(value.get("activationResident").is_none());
            if index < 9 {
                assert!(value.get("candidateSupport").is_none());
            } else {
                let support = view.candidate_support.as_ref().unwrap();
                assert_eq!(support.phase, CandidateSupportViewPhase::NotAdmitted);
                assert!(!support.cleanup_verified);
                assert!(support.attempt_id.is_none());
            }
            if index < 8 {
                assert!(value.get("runtimeReplacement").is_none());
            } else {
                let replacement = view.runtime_replacement.unwrap();
                assert_eq!(replacement.phase, RuntimeReplacementPhase::NotAdmitted);
                assert!(!replacement.physical_custody_held);
                assert!(replacement.attempt_id.is_none());
            }
        }
    }
    #[test]
    fn resident_cold_view20_is_separate_bounded_readonly_data_without_a_prefix_grant() {
        let temp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(temp.path());
        let before = f.journal.get(&f.id).unwrap();
        let actor = activation_files::FilesActor::default();
        let view = actor.resident_view(&f.state(), &f.id, false, None).unwrap();
        assert_eq!(view.schema_version, 20);
        assert_eq!(view.update, before);
        let resident = view.activation_resident.as_ref().unwrap();
        assert_eq!(resident.phase, ActivationResidentPhase::NotResident);
        assert!(
            resident.observation.is_none()
                && resident.resident_process.is_none()
                && resident.original_custody_held.is_none()
        );
        let raw = serde_json::to_value(&view).unwrap();
        for key in [
            "activationLifetime",
            "activationController",
            "activationService",
            "activationWriter",
            "runtimeReplacement",
            "compatibleDrain",
        ] {
            assert!(raw.get(key).is_none(), "unexpected historical prefix {key}");
        }
        assert_eq!(f.journal.get(&f.id).unwrap(), before);
        assert!(serde_json::to_vec(&view).unwrap().len() < 32768);
    }
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
    async fn resident_mailbox_rejects_unbound_or_rebound_requests_without_a_worker_launch() {
        let temp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(temp.path());
        let mut handle = f.spawn();
        handle.wait_ready().await.unwrap();
        assert!(handle.observe_resident_connection().await.is_err());
        handle.command(SupervisorAction::Handoff).await.unwrap();
        let before = f.journal.get(&f.id).unwrap();
        for mutation in 0..5 {
            let mut request = SupervisorRequest {
                contract_version: 28,
                launch_id: handle.record.spec.launch_id.clone(),
                expected_revision: handle.record.revision,
                action: SupervisorAction::ObserveResidentConnection,
                token: handle.token.clone(),
                worker_command: None,
            };
            match mutation {
                0 => {}
                1 => request.contract_version = 27,
                2 => request.launch_id = uuid::Uuid::new_v4().to_string(),
                3 => request.expected_revision += 1,
                _ => request.token = "0".repeat(64),
            }
            let response: SupervisorResponse = ipc::call(
                &handle.record.spec.endpoint,
                handle.child.identity.process_id,
                &request,
            )
            .await
            .unwrap();
            assert_eq!(response.contract_version, 28);
            assert!(response.error.is_some() && response.preparation.is_none());
        }
        assert_eq!(f.journal.get(&f.id).unwrap(), before);
        assert!(UpdateJournal::inspect_activation(&f.state(), &f.id)
            .unwrap()
            .is_none());
        assert!(
            UpdateJournal::inspect_activation_lifetime(&f.state(), &f.id)
                .unwrap()
                .is_none()
        );
        stopped(&mut handle).await;
    }
    #[tokio::test]
    async fn keeper_control_rejects_cold_unbound_and_malformed_requests_without_retiring() {
        let temp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(temp.path());
        let mut handle = f.spawn();
        handle.wait_ready().await.unwrap();
        assert!(matches!(
            handle
                .keeper_control(KeeperControlCommand::RetireCoordinator)
                .await
                .unwrap(),
            KeeperControlResult::Rejected { .. }
        ));
        handle.command(SupervisorAction::Handoff).await.unwrap();
        let before = UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap();
        let root = f.journal.get(&f.id).unwrap();
        let request = KeeperControlRequest {
            keeper_control_version: 1,
            update_id: handle.record.spec.update_id.clone(),
            launch_id: handle.record.spec.launch_id.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            expected_coordinator_revision: handle.record.revision,
            token: handle.token.clone(),
            command: KeeperControlCommand::RetireCoordinator,
        };
        for mutation in 0..9 {
            let mut wire = serde_json::to_value(&request).unwrap();
            match mutation {
                0 => {} // Authentic but no original worker/finalization custody.
                1 => wire["keeperControlVersion"] = 2.into(),
                2 => wire["updateId"] = uuid::Uuid::new_v4().to_string().into(),
                3 => wire["launchId"] = uuid::Uuid::new_v4().to_string().into(),
                4 => wire["requestId"] = "invalid".into(),
                5 => {
                    wire["expectedCoordinatorRevision"] =
                        (request.expected_coordinator_revision + 1).into()
                }
                6 => wire["token"] = "0".repeat(64).into(),
                7 => wire["executable"] = "forbidden.exe".into(),
                _ => wire["command"] = "adopt_saved_keeper".into(),
            }
            let reply: KeeperControlResponse = ipc::call(
                &handle.record.spec.endpoint,
                handle.child.identity.process_id,
                &wire,
            )
            .await
            .unwrap();
            assert!(matches!(reply.result, KeeperControlResult::Rejected { .. }));
        }
        assert_eq!(
            UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap(),
            before
        );
        assert_eq!(f.journal.get(&f.id).unwrap(), root);
        assert!(infrastructure::matches(&handle.child.identity).unwrap());
        assert!(
            matches!(f.journal.acquire_owner(), Err(e) if e.code == JournalErrorCode::OwnerBusy)
        );
        stopped(&mut handle).await;
    }
    #[tokio::test]
    async fn supervisor_dispatch_preserves_duplicate_field_rejection_for_both_wire_contracts() {
        struct Duplicate(serde_json::Value, &'static str);
        impl serde::Serialize for Duplicate {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let object = self.0.as_object().unwrap();
                let mut map = serializer.serialize_map(Some(object.len() + 1))?;
                for (key, value) in object {
                    map.serialize_entry(key, value)?;
                }
                map.serialize_entry(self.1, &object[self.1])?;
                map.end()
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(temp.path());
        let mut handle = f.spawn();
        handle.wait_ready().await.unwrap();
        handle.command(SupervisorAction::Handoff).await.unwrap();
        let before = UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap();
        let legacy = SupervisorRequest {
            contract_version: 1,
            launch_id: handle.record.spec.launch_id.clone(),
            expected_revision: handle.record.revision,
            action: SupervisorAction::Observe,
            token: handle.token.clone(),
            worker_command: None,
        };
        let valid: SupervisorResponse = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &legacy,
        )
        .await
        .unwrap();
        assert_eq!(valid.record.as_ref(), Some(&before));
        let duplicated = Duplicate(serde_json::to_value(&legacy).unwrap(), "expectedRevision");
        let bad: std::io::Result<SupervisorResponse> = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &duplicated,
        )
        .await;
        assert!(
            bad.is_err(),
            "Duplicate legacy fields must not become an authorized request"
        );
        let keeper = KeeperControlRequest {
            keeper_control_version: 1,
            update_id: f.id.clone(),
            launch_id: before.spec.launch_id.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            expected_coordinator_revision: before.revision,
            token: handle.token.clone(),
            command: KeeperControlCommand::RetireCoordinator,
        };
        let duplicated = Duplicate(
            serde_json::to_value(&keeper).unwrap(),
            "keeperControlVersion",
        );
        let bad: KeeperControlResponse = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &duplicated,
        )
        .await
        .unwrap();
        assert!(matches!(bad.result, KeeperControlResult::Rejected { .. }));
        assert!(
            bad.request_id.is_none(),
            "Malformed keeper DTO must fail before authorization/scope handling"
        );
        let root_before = f.journal.get(&f.id).unwrap();
        let helper = fs::read(&before.spec.image_path).unwrap();
        assert!(memchr::memmem::find(&helper, KEEPER_PROBE_MARKER).is_some(),
            "Stale supervisor fixture has no keeper-probe decoder; run cargo build -p winbridge-update --bin winbridge-update --locked before these IPC tests");
        let probe = KeeperProbeRequest {
            keeper_probe_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: crate::bootstrap::tests::fixture().previous,
        };
        let reply: KeeperProbeResponse = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &probe,
        )
        .await
        .unwrap();
        assert_eq!(reply.request_id.as_ref(), Some(&probe.request_id));
        assert!(
            matches!(reply.result, KeeperProbeResult::Rejected { ref message }
            if message.contains("custody is unavailable"))
        );
        let duplicated = Duplicate(serde_json::to_value(&probe).unwrap(), "requestId");
        let reply: KeeperProbeResponse = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &duplicated,
        )
        .await
        .unwrap();
        assert!(
            reply.request_id.is_none(),
            "Probe duplicates must fail in the typed decoder"
        );
        assert!(matches!(reply.result, KeeperProbeResult::Rejected { .. }));
        let mut mixed = serde_json::to_value(&probe).unwrap();
        mixed["keeperControlVersion"] = 1.into();
        let reply: KeeperProbeResponse = ipc::call(
            &handle.record.spec.endpoint,
            handle.child.identity.process_id,
            &mixed,
        )
        .await
        .unwrap();
        assert!(reply.request_id.is_none());
        assert!(matches!(reply.result, KeeperProbeResult::Rejected { .. }));
        assert_eq!(f.journal.get(&f.id).unwrap(), root_before);
        assert_eq!(
            UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap(),
            before
        );
        stopped(&mut handle).await;
    }
    #[tokio::test]
    async fn keeper_launch_cold_duplicate_and_mixed_frames_cannot_create_or_retire() {
        struct Duplicate(serde_json::Value);
        impl serde::Serialize for Duplicate {
            fn serialize<S: serde::Serializer>(
                &self,
                s: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let object = self.0.as_object().unwrap();
                let mut map = s.serialize_map(Some(object.len() + 1))?;
                for (k, v) in object {
                    map.serialize_entry(k, v)?;
                }
                map.serialize_entry("keeperLaunchVersion", &1)?;
                map.end()
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(temp.path());
        let mut h = f.spawn();
        h.wait_ready().await.unwrap();
        h.command(SupervisorAction::Handoff).await.unwrap();
        let before = UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap();
        let root = f.journal.get(&f.id).unwrap();
        assert!(
            memchr::memmem::find(
                &fs::read(&before.spec.image_path).unwrap(),
                KEEPER_LAUNCH_MARKER
            )
            .is_some(),
            "Rebuild winbridge-update support binary before testing new keeper-launch IPC"
        );
        let q = KeeperLaunchRequest {
            keeper_launch_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: crate::bootstrap::tests::fixture().previous,
            command: KeeperLaunchCommand::Observe {
                launch_id: uuid::Uuid::new_v4().to_string(),
                ticket_sha256: "a".repeat(64),
            },
        };
        let reply: KeeperLaunchResponse =
            ipc::call(&h.record.spec.endpoint, h.child.identity.process_id, &q)
                .await
                .unwrap();
        assert_eq!(reply.request_id.as_ref(), Some(&q.request_id));
        assert!(matches!(reply.result, KeeperLaunchResult::Rejected { .. }));
        let duplicate = Duplicate(serde_json::to_value(&q).unwrap());
        let reply: KeeperLaunchResponse = ipc::call(
            &h.record.spec.endpoint,
            h.child.identity.process_id,
            &duplicate,
        )
        .await
        .unwrap();
        assert!(reply.request_id.is_none());
        assert!(matches!(reply.result, KeeperLaunchResult::Rejected { .. }));
        let mut mixed = serde_json::to_value(&q).unwrap();
        mixed["keeperProbeVersion"] = 1.into();
        let reply: KeeperLaunchResponse =
            ipc::call(&h.record.spec.endpoint, h.child.identity.process_id, &mixed)
                .await
                .unwrap();
        assert!(reply.request_id.is_none());
        assert!(matches!(reply.result, KeeperLaunchResult::Rejected { .. }));
        assert_eq!(
            UpdateJournal::inspect_supervisor(&f.state(), &f.id).unwrap(),
            before
        );
        assert_eq!(f.journal.get(&f.id).unwrap(), root);
        assert!(infrastructure::matches(&h.child.identity).unwrap());
        assert!(matches!(f.journal.acquire_owner(),Err(e) if e.code==JournalErrorCode::OwnerBusy));
        stopped(&mut h).await;
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
