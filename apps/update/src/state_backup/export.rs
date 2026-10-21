//! Physical readonly export boundary. It cannot construct shutdown custody,
//! resume a helper, replace runtime files or restore a journal.
use super::*;
use crate::{
    artifact::{self, Pinned},
    installed_set::{pin_installed_set, PinnedInstalledSet},
    journal::{UpdateJournal, UpdateJournalInfo},
    shutdown::ShutdownStateObservation,
};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
use winbridge_core::config::WorkerConfig;
use winbridge_platform::infrastructure::PinnedProcess;

pub struct StateBackupExportGuard {
    record: StateBackupRecord,
    installation: UpdateJournalInfo,
    supervisor: SupervisorRecord,
    owner: PinnedProcess,
    runtime: PinnedInstalledSet,
    state: ShutdownStateObservation,
    parent: File,
    input: Pinned,
    config: Pinned,
    updater_config: Pinned,
    configuration: WorkerConfig,
    process: SupervisorProcess,
    journal_directory: PathBuf,
    input_evidence: StateBackupFileEvidence,
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(super) fn parent_pin(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let file = options.open(path).map_err(error)?;
    if !file.metadata().map_err(error)?.is_dir() {
        return Err("Expected existing backup policy directory".into());
    }
    Ok(file)
}
impl StateBackupExportGuard {
    pub fn intent(&self) -> &StateBackupIntent {
        &self.record.intent
    }
    pub fn worker_configuration(&self) -> &WorkerConfig {
        &self.configuration
    }
    pub fn process(&self) -> &SupervisorProcess {
        &self.process
    }
    pub fn input_sha256(&self) -> &str {
        &self.input_evidence.sha256
    }
    pub fn shutdown_receipt(&self) -> &WorkerShutdownReceipt {
        self.record.intent.receipt()
    }
    pub fn revalidate(&self) -> Result<()> {
        let intent = &self.record.intent;
        self.owner.require_live().map_err(error)?;
        self.runtime.revalidate()?;
        self.state.revalidate().map_err(error)?;
        let parent = Path::new(&intent.binding.parent_directory);
        if winbridge_platform::file_identity(&self.parent).map_err(error)?
            != intent.binding.parent_identity
            || winbridge_platform::file_identity(&parent_pin(parent)?).map_err(error)?
                != intent.binding.parent_identity
            || winbridge_platform::infrastructure::stamp(std::process::id()).map_err(error)?
                != self.process
        {
            return Err("Export physical parent/process changed".into());
        }
        for pin in [&self.input, &self.config, &self.updater_config] {
            if artifact::inspect_image(Path::new(&pin.evidence.path))? != pin.evidence {
                return Err("Export pinned input/config changed".into());
            }
        }
        let (info, sup, saved) = UpdateJournal::inspect_state_backup_export(
            &self.journal_directory,
            intent.update_id(),
            &self.process,
            &self.input_evidence,
        )
        .map_err(error)?;
        if info.installation_id != self.installation.installation_id
            || info.scope_sha256 != self.installation.scope_sha256
            || sup != self.supervisor
            || saved.intent != *intent
        {
            return Err("Export installation/owner/intent changed".into());
        }
        self.owner.require_live().map_err(error)
    }
}
/// Uses the current executable's runtime as the only journal/policy root.
/// The caller supplies a pinned-intent filename and digest, never a destination.
/// A saved PID/grant alone is insufficient: live supervisor and physical files
/// remain pinned until the returned guard is dropped.
pub fn prepare_export(input_path: &Path, expected_sha256: &str) -> Result<StateBackupExportGuard> {
    if !valid_hash(expected_sha256, &[64]) {
        return Err("Expected final backup input SHA-256".into());
    }
    let input = artifact::pin(input_path)?;
    if input.evidence.bytes > 65536 || input.evidence.sha256 != expected_sha256 {
        return Err("Export input bytes/SHA differ".into());
    }
    let raw = winbridge_workspace_sync::read_bounded(input_path, 65536)?;
    if crate::sha256(&raw) != expected_sha256 {
        return Err("Export input changed".into());
    }
    let intent: StateBackupIntent =
        serde_json::from_slice(&raw).map_err(|_| "Invalid export intent/fields")?;
    intent.validate()?;
    let exe = std::env::current_exe()
        .map_err(error)?
        .canonicalize()
        .map_err(error)?;
    let runtime_path = exe.parent().ok_or("Export executable parent missing")?;
    if artifact::inspect_image(&exe)? != intent.binding.shutdown.intent.binding.loaded_worker.image
        || winbridge_protocol::artifact_descriptor(winbridge_protocol::ArtifactRole::Worker)
            != intent
                .binding
                .shutdown
                .intent
                .binding
                .loaded_worker
                .descriptor
        || Path::new(&input.evidence.path) != intent.input_path()
        || Path::new(&intent.binding.parent_directory)
            != runtime_path.join("update").join(intent.update_id())
    {
        return Err(
            "Export requires exact old worker image and runtime-derived input/directory".into(),
        );
    }
    let process = winbridge_platform::infrastructure::stamp(std::process::id()).map_err(error)?;
    let input_evidence = StateBackupFileEvidence {
        path: input.evidence.path.clone(),
        bytes: input.evidence.bytes,
        sha256: input.evidence.sha256.clone(),
        file_identity: input.evidence.file_identity.clone(),
    };
    let journal_directory = runtime_path.join("update-state");
    let (installation, supervisor, record) = UpdateJournal::inspect_state_backup_export(
        &journal_directory,
        intent.update_id(),
        &process,
        &input_evidence,
    )
    .map_err(error)?;
    if record.intent != intent {
        return Err("Export file differs from saved immutable attempt".into());
    }
    let updater_config = artifact::pin(Path::new(&supervisor.spec.config_path))?;
    if updater_config.evidence.sha256 != supervisor.spec.config_sha256 {
        return Err("Export updater policy changed".into());
    }
    let policy = crate::preparation::configuration(&supervisor)?;
    if policy.runtime_directory.canonicalize().map_err(error)? != runtime_path
        || crate::installation_scope(&policy)? != installation.scope_sha256
    {
        return Err("Export policy/installation scope differs".into());
    }
    let config = artifact::pin(&policy.worker_config_path)?;
    if config.evidence != intent.binding.worker_config {
        return Err("Export worker config changed".into());
    }
    let configuration: WorkerConfig = serde_json::from_slice(
        &winbridge_workspace_sync::read_bounded(Path::new(&config.evidence.path), 65536)?,
    )
    .map_err(|_| "Invalid export worker configuration")?;
    configuration.validate()?;
    if configuration.instance_id
        != intent
            .binding
            .shutdown
            .intent
            .binding
            .worker
            .context
            .instance_id
    {
        return Err("Export worker instance changed".into());
    }
    let state = ShutdownStateObservation::capture(&configuration.state_directory).map_err(error)?;
    if state.observation() != &intent.binding.shutdown.intent.binding.worker_state {
        return Err("Export source state/journal/lock changed".into());
    }
    let parent = parent_pin(Path::new(&intent.binding.parent_directory))?;
    let runtime = pin_installed_set(
        runtime_path,
        &installation.installation_id,
        &installation.scope_sha256,
    )?;
    let observed = runtime.observation();
    let loaded = &intent.binding.shutdown.intent.binding.loaded_worker;
    if observed.runtime_identity != loaded.runtime_identity
        || observed.manifest.set_id != loaded.set_id
        || observed.manifest_file.sha256 != loaded.manifest_sha256
        || observed.images.first() != Some(&loaded.image)
    {
        return Err("Export installed runtime differs from original shutdown".into());
    }
    let owner = PinnedProcess::live(
        supervisor
            .process
            .as_ref()
            .ok_or("Export owner process missing")?,
    )
    .map_err(error)?;
    let guard = StateBackupExportGuard {
        record,
        installation,
        supervisor,
        owner,
        runtime,
        state,
        parent,
        input,
        config,
        updater_config,
        configuration,
        process,
        journal_directory,
        input_evidence,
    };
    guard.revalidate()?;
    Ok(guard)
}
