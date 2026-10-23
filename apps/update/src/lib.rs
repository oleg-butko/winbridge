//! Update preparation and its independent durable journal.
//! Effects require trusted internal drivers; public runtime installation remains disabled.
pub mod abort;
pub mod archive;
pub mod artifact;
pub mod build;
pub mod build_output;
pub mod build_report;
pub mod candidate;
pub mod candidate_artifacts;
pub mod candidate_support;
pub mod drain;
pub mod health;
pub mod installed_set;
pub mod journal;
mod preparation;
pub mod runtime_baseline;
pub mod runtime_exchange;
pub mod runtime_preimages;
pub mod runtime_preparation;
pub mod runtime_replacement;
pub mod shutdown;
pub mod source;
pub mod state_backup;
pub mod supervisor;
pub mod updater_support;
pub mod worker_compatibility;
pub mod worker_rehearsal;
pub mod worker_source;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use winbridge_core::update::*;
use winbridge_protocol::update::SourcePlan;
use winbridge_workspace_sync::source_tree::{inventory, read_manifest};
pub use winbridge_workspace_sync::source_tree::{verify_source, VerifiedSource};

type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConfig {
    pub schema_version: u16,
    pub source_directory: PathBuf,
    pub runtime_directory: PathBuf,
    pub worker_config_path: PathBuf,
    #[serde(default)]
    pub source_packages_directory: Option<PathBuf>,
    #[serde(default)]
    pub build: Option<PowerShellBuildPolicy>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PowerShellBuildPolicy {
    pub powershell_executable: PathBuf,
    pub timeout_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReport {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub runtime_update_ready: bool,
    pub source_directory: PathBuf,
    pub runtime_directory: PathBuf,
    pub worker_config_path: PathBuf,
    pub worker_config_sha256: String,
    pub baseline_manifest_sha256: String,
    pub incoming: VerifiedSource,
    pub plan: SourcePlan,
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(error)?.is_file() {
        return Err("Expected a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds byte limit", path.display()));
    }
    Ok(bytes)
}

fn local_directory(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    if !path.is_dir() {
        return Err(format!("Local directory unavailable: {}", path.display()));
    }
    path.canonicalize().map_err(error)
}

pub fn load_config(path: &Path) -> Result<UpdateConfig> {
    let config: UpdateConfig = serde_json::from_slice(&read(path, 64 * 1024)?)
        .map_err(|_| "Invalid updater local configuration/fields")?;
    if !matches!(
        (
            config.schema_version,
            config.source_packages_directory.is_some(),
            config.build.is_some()
        ),
        (1, false, false) | (2, true, false) | (3, true, true)
    ) {
        return Err("Unsupported updater config schema".into());
    }
    if let Some(build) = &config.build {
        if !build.powershell_executable.is_absolute() {
            return Err("PowerShell build executable must use an absolute local path".into());
        }
        if build.timeout_ms == 0 || build.timeout_ms > 3_600_000 {
            return Err("Build timeout must be 1..3600000 ms".into());
        }
        winbridge_platform::require_local_path_prefix(&build.powershell_executable)
            .map_err(error)?;
        winbridge_platform::require_safe_path(&build.powershell_executable).map_err(error)?;
        winbridge_platform::require_local_disk(&build.powershell_executable).map_err(error)?;
        if !build.powershell_executable.is_file() {
            return Err("PowerShell build executable must be an existing local file".into());
        }
    }
    if let Some(directory) = &config.source_packages_directory {
        // Delivery may be HGFS. It is read-only input, never journal/build storage.
        winbridge_platform::require_local_path_prefix(directory).map_err(error)?;
        winbridge_platform::require_safe_path(directory).map_err(error)?;
    }
    let source = local_directory(&config.source_directory)?;
    let runtime = local_directory(&config.runtime_directory)?;
    if source.starts_with(&runtime) || runtime.starts_with(&source) {
        return Err("Source and runtime directories must not overlap".into());
    }
    winbridge_platform::require_safe_path(&config.worker_config_path).map_err(error)?;
    let worker = config.worker_config_path.canonicalize().map_err(error)?;
    if !worker.starts_with(runtime) || !worker.is_file() {
        return Err("Worker config must be an existing regular file inside runtime".into());
    }
    Ok(config)
}

/// Stable path binding. Mutable worker config/source identities belong to request preconditions.
pub fn installation_scope(config: &UpdateConfig) -> Result<String> {
    let mut paths = Vec::new();
    for path in [
        &config.source_directory,
        &config.runtime_directory,
        &config.worker_config_path,
    ] {
        let canonical = path.canonicalize().map_err(error)?;
        let value = canonical
            .to_str()
            .ok_or("Installation paths must be UTF-8")?;
        #[cfg(windows)]
        let value = value.to_lowercase();
        paths.push(value.to_owned());
    }
    Ok(sha256(&serde_json::to_vec(&(1, paths)).map_err(error)?))
}

pub fn source_plan(
    config: &UpdateConfig,
    incoming: &Path,
    expected_sha256: &str,
) -> Result<PlanReport> {
    let verified = verify_source(incoming, expected_sha256)?;
    let source = local_directory(&config.source_directory)?;
    let (previous, baseline_hash) = read_manifest(&source)?;
    let (target, incoming_hash) = read_manifest(incoming)?;
    if incoming_hash != verified.manifest_sha256 {
        return Err("Incoming manifest changed during verification".into());
    }
    let current = inventory(
        &source,
        &previous,
        previous.includes_built_ui || target.includes_built_ui,
    )?;
    let plan = plan_source_update(&previous, &target, &current)?;
    let config_hash = sha256(&read(&config.worker_config_path, 64 * 1024)?);
    Ok(PlanReport {
        schema_version: 1,
        kind: "source_plan",
        read_only: true,
        runtime_update_ready: false,
        source_directory: config.source_directory.clone(),
        runtime_directory: config.runtime_directory.clone(),
        worker_config_path: config.worker_config_path.clone(),
        worker_config_sha256: config_hash,
        baseline_manifest_sha256: baseline_hash,
        incoming: verified,
        plan,
    })
}
