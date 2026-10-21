//! Exact old-worker SQLite exporter, never normal startup or journal recovery.
use crate::{store::Store, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};
use winbridge_protocol::update::*;
use winbridge_update::state_backup::prepare_export;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn authority(store: &Store) -> Result<StateBackupAuthority> {
    store.check_authority()?;
    let a = store.journal_authority();
    let mut raw = b"winbridge.worker-state-authority.v1\n".to_vec();
    raw.extend(serde_json::to_vec(&(&a.incarnation, a.epoch, &a.authority_id)).map_err(error)?);
    Ok(StateBackupAuthority {
        incarnation: a.incarnation.clone(),
        epoch: a.epoch,
        fingerprint_sha256: winbridge_update::sha256(&raw),
    })
}
fn backup_file(path: &Path) -> Result<(File, StateBackupFileEvidence)> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0x1); // FILE_SHARE_READ; deny writes/deletion while hashing.
    }
    let mut file = options.open(path).map_err(error)?;
    let meta = file.metadata().map_err(error)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > 9_007_199_254_740_991 {
        return Err("Invalid backup file size/type".into());
    }
    let identity = winbridge_platform::file_identity(&file).map_err(error)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > meta.len() {
            return Err("Backup grew while hashing".into());
        }
        hash.update(&buffer[..n]);
    }
    winbridge_platform::require_safe_path(path).map_err(error)?;
    if bytes != meta.len()
        || file.metadata().map_err(error)?.len() != bytes
        || winbridge_platform::file_identity(&File::open(path).map_err(error)?).map_err(error)?
            != identity
    {
        return Err("Backup changed while hashing".into());
    }
    let evidence = StateBackupFileEvidence {
        path: path.to_str().ok_or("Backup path UTF-8")?.into(),
        bytes,
        sha256: format!("{:x}", hash.finalize()),
        file_identity: identity,
    };
    Ok((file, evidence))
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
    source.archive_read_only(&path)?;
    let (_file_pin, backup) = backup_file(&path)?;
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
    guard.revalidate()?;
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
    report.validate_envelope()?;
    Ok(report)
}
