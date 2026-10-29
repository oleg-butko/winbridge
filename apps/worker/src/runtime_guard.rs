//! Early known-runtime validation. Does not migrate either journal or enable update actions.
use crate::{Result, Worker};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use winbridge_update::{
    installed_set::{LoadedWorkerIdentity, PinnedInstalledSet, MANIFEST_NAME},
    journal::UpdateJournal,
};

pub struct RuntimeGuard {
    pin: PinnedInstalledSet,
    identity: LoadedWorkerIdentity,
    worker_config: PathBuf,
}
impl RuntimeGuard {
    #[cfg(windows)]
    pub(crate) fn resident_updater_source(&self) -> Result<(PathBuf, String)> {
        self.revalidate()?;
        let updater = self
            .pin
            .observation()
            .manifest
            .artifacts
            .iter()
            .find(|a| a.descriptor.role == winbridge_protocol::ArtifactRole::Updater)
            .ok_or("Loaded installed set has no updater image")?;
        let directory = Path::new(&self.identity.image.path)
            .parent()
            .ok_or("Runtime parent missing")?;
        Ok((
            directory.join("winbridge-update.local.json"),
            updater.sha256.clone(),
        ))
    }
    pub fn identity(&self) -> &LoadedWorkerIdentity {
        &self.identity
    }
    pub fn revalidate(&self) -> Result<()> {
        self.pin.revalidate()
    }
    pub(crate) fn require_installation(&self, installation: &str, scope: &str) -> Result<()> {
        let manifest = &self.pin.observation().manifest;
        if manifest.installation_id != installation || manifest.scope_sha256 != scope {
            return Err("Loaded runtime belongs to another updater installation/scope".into());
        }
        Ok(())
    }
}
/// A known manifest is an explicit installed-runtime marker. Development/legacy
/// binaries without this marker retain normal serve, with no update authority.
/// Keep this guard alive for the complete worker lifetime.
pub fn prepare_runtime_guard(worker_config: &Path) -> Result<Option<RuntimeGuard>> {
    prepare_guard(worker_config, false)
}
/// Ordinary CLI serve must not cold-start through the SQL-release/Active-ACK
/// gap. Verification/private activation only bind identity and use their own
/// original custody; they must not call this normal-start factory.
pub fn prepare_normal_runtime_guard(worker_config: &Path) -> Result<Option<RuntimeGuard>> {
    prepare_guard(worker_config, true)
}
fn prepare_guard(worker_config: &Path, normal_start: bool) -> Result<Option<RuntimeGuard>> {
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let runtime = exe.parent().ok_or("Worker executable has no parent")?;
    let manifest = runtime.join(MANIFEST_NAME);
    winbridge_platform::require_safe_path(&manifest).map_err(|e| e.to_string())?;
    if !manifest.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let updater_path = runtime.join("winbridge-update.local.json");
    let cfg = winbridge_update::load_config(&updater_path)?;
    let worker_config = if worker_config.is_absolute() {
        worker_config.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(worker_config)
    };
    for path in [&worker_config, &cfg.worker_config_path] {
        winbridge_platform::require_local_path_prefix(path).map_err(|e| e.to_string())?;
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(path).map_err(|e| e.to_string())?;
    }
    let worker_config = worker_config.canonicalize().map_err(|e| e.to_string())?;
    if cfg
        .runtime_directory
        .canonicalize()
        .map_err(|e| e.to_string())?
        != runtime
        || cfg
            .worker_config_path
            .canonicalize()
            .map_err(|e| e.to_string())?
            != worker_config
    {
        return Err("Known installed runtime points to another executable directory/worker config; no journal opened".into());
    }
    // Read compatible updater metadata, never call open/migrate here: a previous
    // supervisor may still own its older supported schema during activation.
    let info = UpdateJournal::inspect(&runtime.join("update-state")).map_err(|e| e.to_string())?;
    let scope = winbridge_update::installation_scope(&cfg)?;
    if info.scope_sha256 != scope {
        return Err("Known runtime updater installation scope differs; no journal opened".into());
    }
    if normal_start && info.active_update_id.is_some() {
        return Err(format!("Обычный запуск worker заблокирован: обновление WinBridge {} ещё не завершено, включая подтверждение release/Active; worker journal не открыт. Проверьте updater journal status, не удаляйте guards", info.active_update_id.as_deref().unwrap_or("unknown")));
    }
    let pin =
        winbridge_update::installed_set::pin_installed_set(runtime, &info.installation_id, &scope)?;
    let identity = pin.bind_running_worker()?;
    Ok(Some(RuntimeGuard {
        pin,
        identity,
        worker_config,
    }))
}
impl Worker {
    pub fn attach_runtime_guard(&self, guard: RuntimeGuard) -> Result<()> {
        let path = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?
            .clone();
        if path
            .as_ref()
            .map(|p| p.canonicalize().map_err(|e| e.to_string()))
            .transpose()?
            .as_ref()
            != Some(&guard.worker_config)
            || guard.identity.process.process_id != std::process::id()
            || guard.identity.descriptor.build != winbridge_protocol::build_identity()
        {
            return Err("Runtime guard belongs to another worker/config/process".into());
        }
        guard.revalidate()?;
        let mut slot = self
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?;
        if slot.is_some() {
            return Err("Worker runtime identity cannot be rebound".into());
        }
        *slot = Some(Arc::new(guard));
        Ok(())
    }
}
