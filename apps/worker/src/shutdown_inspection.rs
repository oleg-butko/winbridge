//! Read an exact acceptance after exit without Worker construction, startup,
//! recovery latching, migration, scheduler, IPC listener or a writer connection.
use crate::{store::Store, Result};
use std::path::Path;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{artifact_descriptor, update::*, ArtifactRole};
use winbridge_update::{
    artifact::inspect_image,
    shutdown::{ShutdownInspectionRequest, ShutdownStateObservation},
};
pub fn inspect(input_path: &Path, expected_sha256: &str) -> Result<WorkerShutdownInspectionReport> {
    std::hint::black_box(WORKER_SHUTDOWN_INSPECTION_MARKER);
    if !winbridge_core::update::valid_hash(expected_sha256, &[64]) {
        return Err("Expected shutdown input SHA-256".into());
    }
    winbridge_platform::require_local_path_prefix(input_path).map_err(|e| e.to_string())?;
    winbridge_platform::require_safe_path(input_path).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(input_path).map_err(|e| e.to_string())?;
    let input_pin = inspect_image(input_path)?;
    let raw = winbridge_workspace_sync::read_bounded(input_path, 65536)?;
    if input_pin.bytes == 0
        || input_pin.bytes > 65536
        || input_pin.sha256 != expected_sha256
        || winbridge_update::sha256(&raw) != expected_sha256
    {
        return Err("Shutdown input bytes/SHA differ; no journal opened".into());
    }
    let request: ShutdownInspectionRequest =
        serde_json::from_slice(&raw).map_err(|_| "Invalid shutdown input fields")?;
    request.validate()?;
    if request.worker_journal_schema != crate::store::JOURNAL_SCHEMA_VERSION {
        return Err(
            "Cold inspection requires the exact original worker journal schema; no migration"
                .into(),
        );
    }
    let helper = std::env::current_exe().map_err(|e| e.to_string())?;
    let descriptor = artifact_descriptor(ArtifactRole::Worker);
    if descriptor != request.intent.binding.loaded_worker.descriptor
        || inspect_image(&helper)? != request.helper_image
    {
        return Err("Shutdown inspection must run the exact pinned old worker image/build".into());
    }
    let config_path = Path::new(&request.worker_config.path);
    winbridge_platform::require_local_path_prefix(config_path).map_err(|e| e.to_string())?;
    if inspect_image(config_path)? != request.worker_config {
        return Err("Shutdown worker config file was rebound".into());
    }
    let config_bytes = winbridge_workspace_sync::read_bounded(config_path, 65536)?;
    if winbridge_update::sha256(&config_bytes) != request.worker_config.sha256 {
        return Err("Shutdown worker config changed".into());
    }
    let config: WorkerConfig =
        serde_json::from_slice(&config_bytes).map_err(|_| "Invalid shutdown worker config")?;
    config.validate()?;
    if config.instance_id != request.intent.binding.worker.context.instance_id {
        return Err("Shutdown config instance differs".into());
    }
    let state =
        ShutdownStateObservation::capture(&config.state_directory).map_err(|e| e.to_string())?;
    if state.observation() != &request.intent.binding.worker_state {
        return Err("Shutdown state/journal/lock was rebound".into());
    }
    // This API has no recovery marker path and never calls Store::open.
    let store = Store::open_read_only(&config.state_directory.join("journal.sqlite"))?;
    if store.journal_authority().incarnation
        != request.intent.binding.worker.context.journal_incarnation
    {
        return Err("Shutdown journal incarnation differs".into());
    }
    let receipt = store
        .worker_shutdown_receipt(&request.intent.binding.update_id, &request.intent.command)?;
    if let Some(r) = &receipt {
        request.intent.validate_receipt(r)?;
    }
    state.revalidate().map_err(|e| e.to_string())?;
    if inspect_image(config_path)? != request.worker_config
        || inspect_image(input_path)? != input_pin
        || inspect_image(&helper)? != request.helper_image
    {
        return Err("Shutdown input/config/helper changed during readonly inspection".into());
    }
    Ok(WorkerShutdownInspectionReport {
        schema_version: 1,
        kind: "worker_shutdown_inspection".into(),
        request_sha256: expected_sha256.into(),
        descriptor,
        helper_process: winbridge_platform::infrastructure::stamp(std::process::id())
            .map_err(|e| e.to_string())?,
        helper_image: request.helper_image,
        worker_config: request.worker_config,
        state: state.observation().clone(),
        receipt: receipt.map(Box::new),
        worker_journal_schema: crate::store::JOURNAL_SCHEMA_VERSION,
        journal_opened_read_only: true,
        worker_constructed: false,
        scheduler_started: false,
        recovery_marker_changed: false,
        runtime_applied: false,
    })
}
