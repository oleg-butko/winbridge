//! Out-of-place preparation and explicit activation. Old history never resumes.
use crate::{
    domain::{self, Coverage},
    recovery,
    store::{now_ms, Store},
    Result,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};
use winbridge_core::config::WorkerConfig;
use winbridge_platform::{execution_domain::observe_domain, process_identity::ProcessObservation};
use winbridge_protocol::RequestContext;

const PREPARATION: &str = "recovery-pair.json";
const ACTIVATION: &str = "recovery-activated.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationStage {
    Prepared,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryPreparation {
    pub format_version: u16,
    pub stage: PreparationStage,
    pub blocked_reason: Option<String>,
    pub source_context: RequestContext,
    pub target_context: RequestContext,
    pub source_state_directory: PathBuf,
    pub target_state_directory: PathBuf,
    pub worker_config_path: PathBuf,
    pub archive_path: PathBuf,
    pub archive_sha256: String,
    pub source_witness_sha256: Option<String>,
    pub target_witness_sha256: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryActivation {
    pub format_version: u16,
    pub target_context: RequestContext,
    pub preparation_sha256: String,
    pub checked_at_ms: u64,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&winbridge_workspace_sync::read_bounded(path, 32768)?).map_err(|_| {
        format!(
            "Invalid recovery record {}; activation is blocked",
            path.display()
        )
    })
}
fn hash(path: &Path) -> Result<String> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn lock_state(state: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(state).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(state).map_err(|e| e.to_string())?;
    let path = state.join("worker.lock");
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("Cannot lock existing worker state: {e}"))?;
    file.try_lock_exclusive().map_err(|e| {
        format!(
            "Worker/inspection still owns {}; close it before recovery preparation/activation: {e}",
            state.display()
        )
    })?;
    Ok(file)
}
fn require_marker(state: &Path) -> Result<()> {
    let marker = state.join(recovery::RECOVERY_MARKER);
    winbridge_platform::require_safe_path(&marker).map_err(|e| e.to_string())?;
    if !marker.is_file() {
        return Err("Explicit read-only recovery marker is required; ordinary live state is not implicitly restored".into());
    }
    Ok(())
}
fn require_retired_source(state: &Path) -> Result<()> {
    require_marker(state)?;
    let marker = state.join(recovery::SOURCE_RETIRED_MARKER);
    winbridge_platform::require_safe_path(&marker).map_err(|e| e.to_string())?;
    if !marker.is_file() {
        return Err("Source must be explicitly retired with serve-recovery; its previous activation receipt is not a recovery block".into());
    }
    Ok(())
}
fn mkdir_private(path: &Path) -> Result<()> {
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| e.to_string())
}
fn source_empty(
    state: &Path,
    expected: &RequestContext,
    expected_witness: Option<&str>,
) -> Result<()> {
    require_retired_source(state)?;
    let (authority, _) = Store::read_admission_header(&state.join("journal.sqlite"))?;
    if authority.incarnation != expected.journal_incarnation {
        return Err("Source incarnation changed; prepare a separate recovery pair".into());
    }
    let witness = domain::load(state, expected)?;
    if !witness.complete() {
        return Err(witness.info().reason);
    }
    if expected_witness != Some(domain::fingerprint(state, expected)?.as_str()) {
        return Err("Source physical witness changed; activation is blocked".into());
    }
    for identity in domain::all_identities(&witness)? {
        match observe_domain(&identity) {
            ProcessObservation::Empty(_) => {}
            ProcessObservation::Occupied(reason) | ProcessObservation::Unknown(reason) => {
                return Err(reason)
            }
        }
    }
    Ok(())
}

pub fn prepare_recovery(source: &WorkerConfig, destination: &Path) -> Result<RecoveryPreparation> {
    source.validate()?;
    winbridge_platform::require_safe_path(&source.state_directory).map_err(|e| e.to_string())?;
    if destination
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Recovery destination must be absolute without parent traversal".into());
    }
    let source_state = fs::canonicalize(&source.state_directory).map_err(|e| e.to_string())?;
    let _source_lock = lock_state(&source_state)?;
    require_retired_source(&source_state)?;
    let source_store = Store::open_read_only(&source_state.join("journal.sqlite"))?;
    source_store.check_authority()?;
    let source_context = domain::context(source, &source_store);
    winbridge_platform::require_safe_path(destination).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(destination).map_err(|e| e.to_string())?;
    let parent = fs::canonicalize(
        destination
            .parent()
            .ok_or("Recovery destination has no parent")?,
    )
    .map_err(|e| e.to_string())?;
    let destination = parent.join(
        destination
            .file_name()
            .ok_or("Recovery destination has no name")?,
    );
    for project in &source.projects {
        if let Ok(root) = fs::canonicalize(&project.workspace_root) {
            if destination.starts_with(root) {
                return Err(
                    "Recovery state must be separate from registered project workspaces".into(),
                );
            }
        }
    }
    if destination.starts_with(&source_state)
        || source_state.starts_with(&destination)
        || destination.try_exists().map_err(|e| e.to_string())?
    {
        return Err("Recovery destination must be a new separate local directory; source/history are never overwritten".into());
    }
    mkdir_private(&destination)?;
    let state = destination.join("state");
    mkdir_private(&state)?;
    // Persist the gate before creating any journal. Even a partial preparation
    // remains blocked; a receipt, not marker deletion, permits normal startup.
    recovery::latch(&state)?;
    let lock_path = state.join("worker.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive().map_err(|e| e.to_string())?;
    let archive_path = destination.join("archived-journal.sqlite");
    source_store.archive_read_only(&archive_path)?;
    let archive_sha256 = hash(&archive_path)?;
    let mut target = source.clone();
    target.instance_id = format!("recovery-{}", uuid::Uuid::new_v4());
    target.state_directory = fs::canonicalize(&state).map_err(|e| e.to_string())?;
    target.worker.controller_url = None; // Explicit pair/reconnect, no old session.
    target.validate()?;
    let mut target_store = Store::open(&state.join("journal.sqlite"))?;
    target_store.require_fenced_requests()?;
    let target_context = domain::context(&target, &target_store);
    let witness = domain::fresh_witness(&target, &target_store, Coverage::FreshJournal)?;
    recovery::persist_new_json(&state.join(domain::WITNESS), &witness)?;
    let worker_config_path = destination.join("winbridge-worker.local.json");
    recovery::persist_new_json(&worker_config_path, &target)?;
    let source_witness_sha256 = domain::fingerprint(&source_state, &source_context).ok();
    let blocked_reason = source_empty(
        &source_state,
        &source_context,
        source_witness_sha256.as_deref(),
    )
    .err();
    let preparation = RecoveryPreparation {
        format_version: 1,
        stage: if blocked_reason.is_some() {
            PreparationStage::Blocked
        } else {
            PreparationStage::Prepared
        },
        blocked_reason,
        source_context,
        target_context,
        source_state_directory: source_state,
        target_state_directory: target.state_directory,
        worker_config_path,
        archive_path,
        archive_sha256,
        source_witness_sha256,
        target_witness_sha256: hash(&state.join(domain::WITNESS))?,
    };
    recovery::persist_new_json(&state.join(PREPARATION), &preparation)?;
    Ok(preparation)
}

fn validate_preparation(state: &Path) -> Result<RecoveryPreparation> {
    let preparation: RecoveryPreparation = read_json(&state.join(PREPARATION))?;
    if preparation.format_version != 1
        || fs::canonicalize(state).map_err(|e| e.to_string())? != preparation.target_state_directory
        || preparation.source_state_directory == preparation.target_state_directory
        || preparation
            .source_context
            .instance_id
            .eq_ignore_ascii_case(&preparation.target_context.instance_id)
        || preparation.source_context.journal_incarnation
            == preparation.target_context.journal_incarnation
    {
        return Err("Recovery pair identity/path boundary is invalid".into());
    }
    if hash(&preparation.archive_path)? != preparation.archive_sha256
        || hash(&state.join(domain::WITNESS))? != preparation.target_witness_sha256
    {
        return Err("Archive/target witness checksum mismatch; activation is blocked".into());
    }
    let (archive, _) = Store::read_admission_header(&preparation.archive_path)?;
    if archive.incarnation != preparation.source_context.journal_incarnation {
        return Err("Archive belongs to another source incarnation".into());
    }
    let (target, requires_fenced) = Store::read_admission_header(&state.join("journal.sqlite"))?;
    if target.incarnation != preparation.target_context.journal_incarnation || !requires_fenced {
        return Err("Target journal request boundary changed; no activation allowed".into());
    }
    domain::load(state, &preparation.target_context)?;
    Ok(preparation)
}
pub fn activate_recovery(target: &WorkerConfig) -> Result<RecoveryActivation> {
    target.validate()?;
    let _target_lock = lock_state(&target.state_directory)?;
    recovery::reject_retired_source(&target.state_directory)?;
    require_marker(&target.state_directory)?;
    let preparation = validate_preparation(&target.state_directory)?;
    if target.instance_id != preparation.target_context.instance_id {
        return Err("Use the explicitly prepared target config; instance ID mismatch".into());
    }
    let _source_lock = lock_state(&preparation.source_state_directory)?;
    source_empty(
        &preparation.source_state_directory,
        &preparation.source_context,
        preparation.source_witness_sha256.as_deref(),
    )?;
    let receipt = RecoveryActivation {
        format_version: 1,
        target_context: preparation.target_context,
        preparation_sha256: hash(&target.state_directory.join(PREPARATION))?,
        checked_at_ms: now_ms(),
    };
    let path = target.state_directory.join(ACTIVATION);
    if path.try_exists().map_err(|e| e.to_string())? {
        let existing: RecoveryActivation = read_json(&path)?;
        if existing.format_version != receipt.format_version
            || existing.target_context != receipt.target_context
            || existing.preparation_sha256 != receipt.preparation_sha256
        {
            return Err("Existing activation belongs to another preparation; no overwrite".into());
        }
        return Ok(existing);
    }
    recovery::persist_new_json(&path, &receipt)?;
    Ok(receipt)
}

pub(crate) fn check_active(target: &WorkerConfig) -> Result<()> {
    let state = &target.state_directory;
    let receipt: RecoveryActivation = read_json(&state.join(ACTIVATION))?;
    let preparation = validate_preparation(state)?;
    if receipt.format_version != 1
        || receipt.target_context != preparation.target_context
        || target.instance_id != receipt.target_context.instance_id
        || receipt.preparation_sha256 != hash(&state.join(PREPARATION))?
    {
        return Err("Recovery activation receipt does not match this pair".into());
    }
    let _source_lock = lock_state(&preparation.source_state_directory)?;
    source_empty(
        &preparation.source_state_directory,
        &preparation.source_context,
        preparation.source_witness_sha256.as_deref(),
    )
}
