//! Local configuration editing; the running WorkerConfig remains immutable.
use crate::{Result, Worker};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{RuntimeSettings, SaveRuntimeSettings, SettingsSaved};

const MAX_BYTES: usize = 64 * 1024;
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path) -> Result<Vec<u8>> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Settings editor supports configurations up to 64 KiB; file retained".into());
    }
    Ok(bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    if read(path)? != bytes {
        return Err("Settings file readback mismatch; save not confirmed".into());
    }
    Ok(())
}
impl Worker {
    pub fn open_file(path: &Path) -> Result<std::sync::Arc<Self>> {
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
        let config = WorkerConfig::load(&path)?;
        Self::open_inner(config, Some(path), false)
    }
    pub fn open_recovery_file(path: &Path) -> Result<std::sync::Arc<Self>> {
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
        let config = WorkerConfig::load(&path)?;
        Self::open_inner(config, Some(path), true)
    }
    fn settings_at(&self, path: &Path) -> Result<RuntimeSettings> {
        let bytes = read(path)?;
        let saved_json = String::from_utf8(bytes.clone()).map_err(|_| "Config is not UTF-8")?;
        let saved = serde_json::from_slice::<WorkerConfig>(&bytes)
            .ok()
            .and_then(|c| serde_json::to_value(c).ok());
        let active = serde_json::to_value(&self.config).map_err(|e| e.to_string())?;
        Ok(RuntimeSettings {
            generation: self.generation.clone(),
            path: path.to_string_lossy().into(),
            active_json: serde_json::to_string_pretty(&active).map_err(|e| e.to_string())?,
            saved_json,
            saved_sha256: digest(&bytes),
            restart_required: saved.as_ref() != Some(&active),
        })
    }
    pub fn runtime_settings(&self) -> Result<RuntimeSettings> {
        let path = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?;
        self.settings_at(
            path.as_ref().ok_or(
                "Worker was opened without a config file; restart using --config PATH serve",
            )?,
        )
    }
    /// Updater SQL callbacks must not wait for a settings writer that may be
    /// waiting for Store. File/JSON checks are the same as the regular reader.
    #[cfg(any(windows, test))]
    pub(crate) fn try_runtime_settings(&self) -> Result<RuntimeSettings> {
        let path = self
            .config_path
            .try_lock()
            .map_err(|_| "Bootstrap config path busy/poisoned")?;
        self.settings_at(
            path.as_ref()
                .ok_or("Bootstrap worker config path missing")?,
        )
    }
    pub fn save_runtime_settings(&self, request: &SaveRuntimeSettings) -> Result<SettingsSaved> {
        // Serialize config writes with update admission/quiescence, including filesystem IO.
        let store = self.store()?;
        if request.generation != self.generation
            || !uuid::Uuid::parse_str(&request.request_id)
                .is_ok_and(|id| id.hyphenated().to_string() == request.request_id)
            || request.json.len() > MAX_BYTES
        {
            return Err("Settings generation/save ID/size is invalid; reload settings".into());
        }
        let next: WorkerConfig =
            serde_json::from_str(&request.json).map_err(|e| format!("Config JSON: {e}"))?;
        next.validate()?;
        if next.instance_id != self.config.instance_id
            || next.node_id != self.config.node_id
            || next.state_directory != self.config.state_directory
        {
            return Err("instanceId, nodeId and stateDirectory are installation identity; this editor cannot change them".into());
        }
        winbridge_platform::require_safe_path(&next.state_directory).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(&next.state_directory).map_err(|e| e.to_string())?;
        for project in &next.projects {
            winbridge_platform::require_safe_path(&project.workspace_root)
                .map_err(|e| e.to_string())?;
            winbridge_platform::require_local_disk(&project.workspace_root)
                .map_err(|e| e.to_string())?;
        }
        let path_guard = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?;
        let path = path_guard.as_ref().ok_or("Worker has no config path")?;
        let before = read(path)?;
        let bytes = request.json.as_bytes();
        let backup = path.with_file_name(format!(
            "{}.before-{}.json",
            path.file_name().ok_or("Config filename")?.to_string_lossy(),
            request.request_id
        ));
        if before == bytes {
            if backup.exists() && digest(&read(&backup)?) != request.expected_sha256 {
                return Err("Previous config backup differs from the expected version; save acknowledgement not confirmed".into());
            }
            return Ok(SettingsSaved {
                settings: self.settings_at(path)?,
                backup_path: backup.exists().then(|| backup.to_string_lossy().into()),
            });
        }
        if digest(&before) != request.expected_sha256 {
            return Err(
                "Config changed since preview; reload instead of overwriting another edit".into(),
            );
        }
        store.require_update_administration()?;
        match write_new(&backup, &before) {
            Ok(()) => {}
            Err(_) if backup.exists() && read(&backup)? == before => {}
            Err(error) => {
                return Err(format!(
                    "Config backup not confirmed: {error}; original retained"
                ))
            }
        }
        let temporary = path.with_file_name(format!(
            "{}.pending-{}.json",
            path.file_name().unwrap().to_string_lossy(),
            request.request_id
        ));
        if temporary.exists() {
            if read(&temporary)? != bytes {
                return Err(
                    "Save ID belongs to different pending bytes; reload with a new ID".into(),
                );
            }
        } else {
            write_new(&temporary, bytes)?;
        }
        // Recheck the disk version after staging; serialize WinBridge editors with
        // config_path mutex. External editors should also avoid simultaneous writes.
        if read(path)? != before {
            return Err("Config changed during staging; original and staged files retained".into());
        }
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        winbridge_platform::require_safe_path(&temporary).map_err(|e| e.to_string())?;
        fs::rename(&temporary, path)
            .map_err(|e| format!("Atomic config replacement failed: {e}"))?;
        if read(path)? != bytes {
            return Err("Config replacement readback mismatch; save not confirmed".into());
        }
        Ok(SettingsSaved {
            settings: self.settings_at(path)?,
            backup_path: Some(backup.to_string_lossy().into()),
        })
    }
}
