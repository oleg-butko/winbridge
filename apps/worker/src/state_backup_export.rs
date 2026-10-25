//! Exact old-worker SQLite exporter, never normal startup or journal recovery.
use crate::{store::Store, Result};
use std::path::Path;
use winbridge_protocol::update::*;
use winbridge_update::state_backup::{pin_backup_file, prepare_export};
fn authority(store: &Store) -> Result<StateBackupAuthority> {
    store.state_backup_authority()
}
pub fn export(input_path: &Path, expected_sha256: &str) -> Result<WorkerStateBackupReport> {
    std::hint::black_box(WORKER_STATE_BACKUP_MARKER);
    let guard = prepare_export(input_path, expected_sha256)?;
    let intent = guard.intent();
    if intent.binding.worker_journal_schema != crate::store::JOURNAL_SCHEMA_VERSION {
        return Err("Final backup requires exact old worker journal schema; no migration".into());
    }
    let config = guard.worker_configuration();
    let source = Store::open_read_only(&config.state_directory.join("journal.sqlite"))?;
    let original = authority(&source)?;
    if original.incarnation
        != intent
            .binding
            .shutdown
            .intent
            .binding
            .worker
            .context
            .journal_incarnation
    {
        return Err("Final backup source incarnation differs".into());
    }
    let receipt = source
        .worker_shutdown_receipt(intent.update_id(), &intent.binding.shutdown.intent.command)?
        .ok_or("Final backup source shutdown receipt missing")?;
    if &receipt != guard.shutdown_receipt() {
        return Err("Final backup shutdown acceptance differs".into());
    }
    let source_header = source.archive_sqlite_metadata()?;
    guard.revalidate()?;
    let path = intent.backup_path();
    #[cfg(feature = "update-validation")]
    let fault = crate::update_validation::export_before_copy(&guard, &path)?;
    source.archive_read_only(&path)?;
    let file_pin = pin_backup_file(&path)?;
    let backup = file_pin.evidence().clone();
    let copied = Store::open_read_only(&path)?;
    let copied_header = copied.archive_sqlite_metadata()?;
    let backup_authority = authority(&copied)?;
    if source_header != copied_header
        || backup_authority != original
        || copied
            .worker_shutdown_receipt(intent.update_id(), &intent.binding.shutdown.intent.command)?
            .as_ref()
            != Some(&receipt)
        || authority(&source)? != original
    {
        return Err("Final backup SQLite header/authority/receipt differs".into());
    }
    file_pin.revalidate()?;
    guard.revalidate()?;
    #[cfg(feature = "update-validation")]
    crate::update_validation::export_after_copy(fault);
    let report = WorkerStateBackupReport {
        schema_version: 1,
        kind: "worker_state_backup".into(),
        input_sha256: guard.input_sha256().into(),
        update_id: intent.update_id().into(),
        attempt_id: intent.attempt_id.clone(),
        helper_descriptor: winbridge_protocol::artifact_descriptor(
            winbridge_protocol::ArtifactRole::Worker,
        ),
        helper_process: guard.process().clone(),
        helper_image: intent
            .binding
            .shutdown
            .intent
            .binding
            .loaded_worker
            .image
            .clone(),
        worker_config: intent.binding.worker_config.clone(),
        state: intent.binding.shutdown.intent.binding.worker_state.clone(),
        worker_journal_schema: crate::store::JOURNAL_SCHEMA_VERSION,
        source_authority: original,
        backup_authority,
        shutdown_receipt: receipt,
        backup,
        page_size: copied_header.0,
        page_count: copied_header.1,
        completed_at_ms: crate::store::now_ms(),
        integrity_verified: true,
        backup_created_new: true,
        journal_opened_read_only: true,
        worker_constructed: false,
        scheduler_started: false,
        recovery_marker_changed: false,
        runtime_applied: false,
    };
    #[cfg(feature = "update-validation")]
    let report = crate::update_validation::export_report(fault, report);
    report.validate_envelope()?;
    Ok(report)
}
