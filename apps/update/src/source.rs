//! Trusted source transaction driver. No source-write CLI/RPC is registered.
use crate::{
    archive, installation_scope,
    journal::{JournalErrorCode, UpdateAuthority, UpdateJournal},
    sha256, verify_source, UpdateConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use winbridge_core::update::*;
use winbridge_platform::source_files::{
    directory_identity, move_new, require_same_volume, sync_parent,
};
use winbridge_protocol::{update::*, WorkerInfo};
use winbridge_workspace_sync::source_tree::{inventory, read_manifest};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceCheckpoint {
    Prepared,
    Applying,
    IntentCommitted,
    OldPreserved,
    NewPlaced,
    ReceiptCommitted,
    Verified,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProgress {
    pub update_id: String,
    pub checkpoint: SourceCheckpoint,
    pub ordinal: Option<u32>,
    pub path: Option<String>,
}
/// Implement with a fresh trusted worker observation, never a supplied/cached JSON
/// proof. The future local IPC adapter must authenticate its peer and bound calls.
pub trait SourceControl {
    fn observe_worker(&mut self) -> Result<WorkerInfo>;
    fn progress(&mut self, _event: &SourceProgress) -> Result<()> {
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Coordination {
    schema_version: u16,
    lease: WorkerUpdateLease,
    read_only: bool,
    observed_at_ms: u64,
}
fn live_lease(
    control: &mut impl SourceControl,
    update: &UpdateRecord,
    expected: Option<&SourceTransactionRecord>,
) -> Result<WorkerUpdateLease> {
    let info = control.observe_worker()?;
    if info.journal_recovery_required.is_some()
        || info.maintenance_error.is_some()
        || info.journal_inspection.is_some()
    {
        return Err("Worker is not available for source effects; preserve source lease".into());
    }
    let v: Coordination = serde_json::from_value(
        info.update_coordination
            .ok_or("Live source lease missing")?,
    )
    .map_err(err)?;
    let lease = v.lease;
    let spec = &lease.spec;
    let context = info
        .fenced_requests
        .as_ref()
        .map(|r| &r.context)
        .ok_or("Worker journal context missing")?;
    if v.schema_version != 1
        || v.read_only
        || v.observed_at_ms < lease.updated_at_ms
        || lease.phase != WorkerUpdatePhase::SourceHeld
        || spec.update_id != update.update_id
        || spec.instance_id != info.instance_id
        || spec.worker_generation != info.generation
        || spec.instance_id != context.instance_id
        || spec.journal_incarnation != context.journal_incarnation
        || spec.baseline_manifest_sha256 != update.request.expected.baseline_manifest_sha256
        || spec.target_manifest_sha256 != update.request.source.manifest_sha256
        || info
            .build
            .as_ref()
            .is_none_or(|b| b.build_id != update.request.expected.worker_build_id)
        || expected.is_some_and(|r| {
            r.worker_source_spec != *spec || r.worker_lease_revision != lease.revision
        })
    {
        return Err(
            "Live worker source grant/scope/build changed; no file effect permitted".into(),
        );
    }
    Ok(lease)
}
fn local(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_local_path_prefix(path).map_err(err)?;
    winbridge_platform::require_safe_path(path).map_err(err)?;
    winbridge_platform::require_local_disk(path).map_err(err)?;
    if !path.is_dir() {
        return Err(format!(
            "Local source directory unavailable: {}",
            path.display()
        ));
    }
    path.canonicalize().map_err(err)
}
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Source path must be UTF-8".into())
}
fn input(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(err)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        options.share_mode(FILE_SHARE_READ);
    }
    let file = options.open(path).map_err(err)?;
    if !file.metadata().map_err(err)?.is_file() {
        return Err("Expected a regular source file".into());
    }
    Ok(file)
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > limit {
        return Err("Source file exceeds pinned limit".into());
    }
    Ok(bytes)
}
fn matches_file(path: &Path, pin: &SourceFile) -> Result<()> {
    let mut file = input(path)?;
    if file.metadata().map_err(err)?.len() != pin.bytes {
        return Err(format!("Source size mismatch: {}", path.display()));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > pin.bytes {
            return Err("Source changed during verification".into());
        }
        hash.update(&buffer[..n]);
    }
    if total != pin.bytes || format!("{:x}", hash.finalize()) != pin.sha256 {
        return Err(format!("Source SHA-256 mismatch: {}", path.display()));
    }
    Ok(())
}
fn copy_verified(from: &Path, to: &Path, pin: &SourceFile) -> Result<()> {
    let mut source = input(from)?;
    if source.metadata().map_err(err)?.len() != pin.bytes {
        return Err("Source preimage/input size changed".into());
    }
    winbridge_platform::require_safe_path(to).map_err(err)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(to).map_err(err)?;
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buf = [0u8; 65536];
    loop {
        let n = source.read(&mut buf).map_err(err)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > pin.bytes {
            return Err("Source preimage/input grew during copy".into());
        }
        out.write_all(&buf[..n]).map_err(err)?;
        hash.update(&buf[..n]);
    }
    out.sync_all().map_err(err)?;
    drop(out);
    sync_parent(to).map_err(err)?;
    if total != pin.bytes || format!("{:x}", hash.finalize()) != pin.sha256 {
        return Err("Source preimage/input SHA changed; partial staging preserved".into());
    }
    matches_file(to, pin)
}
fn private_dir(path: &Path) -> Result<()> {
    winbridge_platform::require_safe_path(path).map_err(err)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| {
        format!(
            "Create-only transaction directory {}: {e}; existing data preserved",
            path.display()
        )
    })?;
    sync_parent(path).map_err(err)
}
fn artifact(root: &Path, kind: &str, ordinal: u32) -> PathBuf {
    root.join(kind).join(format!("{ordinal:05}"))
}
fn progress(
    control: &mut impl SourceControl,
    id: &str,
    checkpoint: SourceCheckpoint,
    file: Option<(u32, &str)>,
) -> Result<()> {
    control.progress(&SourceProgress {
        update_id: id.into(),
        checkpoint,
        ordinal: file.map(|f| f.0),
        path: file.map(|f| f.1.into()),
    })
}
fn identities(r: &SourceTransactionRecord) -> Result<()> {
    for (path, expected) in [
        (&r.source_directory, &r.source_directory_identity),
        (&r.stage_directory, &r.stage_directory_identity),
        (&r.transaction_directory, &r.transaction_directory_identity),
    ] {
        if directory_identity(Path::new(path)).map_err(err)? != *expected {
            return Err(
                "Source/staging/transaction directory identity changed; stop effects".into(),
            );
        }
    }
    Ok(())
}
fn guards(
    journal: &mut UpdateJournal,
    authority: &UpdateAuthority,
    config: &UpdateConfig,
    r: &SourceTransactionRecord,
    control: &mut impl SourceControl,
    phase: SourceTransactionPhase,
) -> Result<()> {
    let actual = journal
        .source_guard(authority, &r.update_id, phase)
        .map_err(err)?;
    if actual.worker_source_spec != r.worker_source_spec || actual.plan_sha256 != r.plan_sha256 {
        return Err("Source transaction binding changed".into());
    }
    let update = journal.get(&r.update_id).map_err(err)?;
    if installation_scope(config)? != update.request.expected.scope_sha256
        || text(&local(&config.source_directory)?)? != r.source_directory
        || text(&local(&config.runtime_directory)?)? != r.runtime_directory
        || sha256(&read(&config.worker_config_path, 64 * 1024)?)
            != update.request.expected.worker_config_sha256
    {
        return Err("Installation paths/config changed during source transaction".into());
    }
    identities(r)?;
    live_lease(control, &update, Some(r))?;
    Ok(())
}

/// Prepares bytes/preimages only in runtime/update/<id>. The existing source tree
/// is unchanged. Repeating this call returns stored metadata, not a write grant.
pub fn prepare_source(
    journal: &mut UpdateJournal,
    authority: &UpdateAuthority,
    config: &UpdateConfig,
    id: &str,
    control: &mut impl SourceControl,
) -> Result<SourceTransactionRecord> {
    journal.check_source_authority(authority).map_err(err)?;
    match journal.source_header(id) {
        Ok(r) => return Ok(r),
        Err(e) if e.code == JournalErrorCode::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    let update = journal.get(id).map_err(err)?;
    if update.phase != UpdatePhase::Preparing {
        return Err("Source preparation requires Preparing".into());
    }
    if installation_scope(config)? != update.request.expected.scope_sha256
        || sha256(&read(&config.worker_config_path, 64 * 1024)?)
            != update.request.expected.worker_config_sha256
    {
        return Err("Installation config/scope differs from the accepted update".into());
    }
    let lease = live_lease(control, &update, None)?;
    if lease.spec.updater_owner_id != authority.owner_id()
        || lease.spec.updater_epoch != authority.epoch()
    {
        return Err("Source grant belongs to another updater owner/epoch".into());
    }
    let source = local(&config.source_directory)?;
    let runtime = local(&config.runtime_directory)?;
    if source.starts_with(&runtime)
        || runtime.starts_with(&source)
        || text(&source)? != lease.spec.source_directory
    {
        return Err("Source/runtime/lease directories overlap or differ".into());
    }
    let run = local(&runtime.join("update").join(id))?;
    let staged = local(&run.join("archive"))?;
    let incoming = staged.join("source");
    if !run.starts_with(&runtime) || !staged.starts_with(&run) {
        return Err("Staging escapes installation scope".into());
    }
    require_same_volume(&source, &run).map_err(err)?;
    verify_source(&source, &update.request.expected.baseline_manifest_sha256)?;
    archive::verify_archive(
        &staged.join("source.zip"),
        &update.request.source.archive_sha256,
        &update.request.source.manifest_sha256,
    )?;
    let target_check = verify_source(&incoming, &update.request.source.manifest_sha256)?;
    if target_check.source_commit != update.request.source.source_commit {
        return Err("Incoming source commit differs from accepted request".into());
    }
    let (baseline, _) = read_manifest(&source)?;
    let (target, _) = read_manifest(&incoming)?;
    let plan = plan_source_update(
        &baseline,
        &target,
        &inventory(
            &source,
            &baseline,
            baseline.includes_built_ui || target.includes_built_ui,
        )?,
    )?;
    if !plan.can_apply {
        return Err(format!(
            "Source conflicts: {}",
            serde_json::to_string(&plan.blockers).map_err(err)?
        ));
    }
    let mut changes: Vec<_> = plan
        .changes
        .into_iter()
        .filter(|c| c.action != SourceAction::Keep)
        .collect();
    if update.request.expected.baseline_manifest_sha256 != update.request.source.manifest_sha256 {
        let previous = read(
            &source.join(SOURCE_MANIFEST_NAME),
            MAX_SOURCE_MANIFEST_BYTES,
        )?;
        let next = read(
            &incoming.join(SOURCE_MANIFEST_NAME),
            MAX_SOURCE_MANIFEST_BYTES,
        )?;
        changes.push(SourceChange {
            path: SOURCE_MANIFEST_NAME.into(),
            action: SourceAction::Replace,
            previous: Some(SourceFile {
                path: SOURCE_MANIFEST_NAME.into(),
                bytes: previous.len() as u64,
                sha256: sha256(&previous),
            }),
            incoming: Some(SourceFile {
                path: SOURCE_MANIFEST_NAME.into(),
                bytes: next.len() as u64,
                sha256: sha256(&next),
            }),
        });
    }
    let bytes = serde_json::to_vec(&changes).map_err(err)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Source plan exceeds limit".into());
    }
    let transaction = run.join("source-transaction");
    private_dir(&transaction)?;
    for name in ["preimages", "prepared", "held"] {
        private_dir(&transaction.join(name))?;
    }
    for (i, change) in changes.iter().enumerate() {
        live_lease(control, &update, None)?;
        journal.check_source_authority(authority).map_err(err)?;
        if let Some(pin) = &change.previous {
            copy_verified(
                &source.join(&change.path),
                &artifact(&transaction, "preimages", i as u32),
                pin,
            )?;
        }
        if let Some(pin) = &change.incoming {
            copy_verified(
                &incoming.join(&change.path),
                &artifact(&transaction, "prepared", i as u32),
                pin,
            )?;
        }
    }
    verify_source(&source, &update.request.expected.baseline_manifest_sha256)?;
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(err)?
            .as_millis(),
    )
    .map_err(err)?;
    let r = SourceTransactionRecord {
        schema_version: 1,
        update_id: id.into(),
        source_directory: text(&source)?,
        runtime_directory: text(&runtime)?,
        stage_directory: text(&staged)?,
        transaction_directory: text(&transaction)?,
        source_directory_identity: directory_identity(&source).map_err(err)?,
        stage_directory_identity: directory_identity(&staged).map_err(err)?,
        transaction_directory_identity: directory_identity(&transaction).map_err(err)?,
        worker_source_spec: lease.spec,
        worker_lease_revision: lease.revision,
        plan_sha256: sha256(&bytes),
        total_files: changes.len() as u32,
        revision: 0,
        phase: SourceTransactionPhase::Prepared,
        accepted_at_ms: now,
        updated_at_ms: now,
        reason: None,
    };
    guards_for_prepared(&update, &r, authority, control)?;
    journal
        .store_source_plan(authority, &r, &changes)
        .map_err(err)?;
    if let Err(error) = progress(control, id, SourceCheckpoint::Prepared, None) {
        journal.change_source_phase(authority,id,SourceTransactionPhase::Blocked,"Подготовка остановлена до изменения исходников; требуется проверка сохранённого результата").map_err(err)?;
        return Err(error);
    }
    Ok(r)
}
fn guards_for_prepared(
    update: &UpdateRecord,
    r: &SourceTransactionRecord,
    authority: &UpdateAuthority,
    control: &mut impl SourceControl,
) -> Result<()> {
    identities(r)?;
    let lease = live_lease(control, update, Some(r))?;
    if lease.spec.updater_owner_id != authority.owner_id()
        || lease.spec.updater_epoch != authority.epoch()
    {
        return Err("Prepared source owner changed".into());
    }
    Ok(())
}

fn parent_dirs(root: &Path, target: &Path, mut before: impl FnMut() -> Result<()>) -> Result<()> {
    let relative = target
        .parent()
        .ok_or("Source parent missing")?
        .strip_prefix(root)
        .map_err(err)?;
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        before()?;
        winbridge_platform::require_safe_path(&path).map_err(err)?;
        if !path.try_exists().map_err(err)? {
            fs::create_dir(&path).map_err(err)?;
            sync_parent(&path).map_err(err)?;
        }
        if !path.is_dir() {
            return Err("Source parent changed to a file".into());
        }
    }
    Ok(())
}
fn target_files_match(source: &Path, target: &SourceManifest) -> Result<()> {
    let plan = plan_source_update(
        target,
        target,
        &inventory(source, target, target.includes_built_ui)?,
    )?;
    if !plan.can_apply {
        return Err(format!(
            "Target source tree conflicts before manifest commit: {}",
            serde_json::to_string(&plan.blockers).map_err(err)?
        ));
    }
    Ok(())
}
fn apply_inner(
    journal: &mut UpdateJournal,
    authority: &UpdateAuthority,
    config: &UpdateConfig,
    id: &str,
    control: &mut impl SourceControl,
) -> Result<SourceTransactionRecord> {
    let r = journal
        .source_guard(authority, id, SourceTransactionPhase::Prepared)
        .map_err(err)?;
    guards(
        journal,
        authority,
        config,
        &r,
        control,
        SourceTransactionPhase::Prepared,
    )?;
    verify_source(
        Path::new(&r.source_directory),
        &r.worker_source_spec.baseline_manifest_sha256,
    )?;
    let changes = journal.source_plan(id).map_err(err)?;
    let transaction = Path::new(&r.transaction_directory);
    let source = Path::new(&r.source_directory);
    let incoming = Path::new(&r.stage_directory).join("source");
    verify_source(&incoming, &r.worker_source_spec.target_manifest_sha256)?;
    let (target, _) = read_manifest(&incoming)?;
    for (i, change) in changes.iter().enumerate() {
        if let Some(pin) = &change.previous {
            matches_file(&artifact(transaction, "preimages", i as u32), pin)?;
        }
        if let Some(pin) = &change.incoming {
            matches_file(&artifact(transaction, "prepared", i as u32), pin)?;
        }
    }
    journal
        .change_source_phase(
            authority,
            id,
            SourceTransactionPhase::Applying,
            "Применение исходников: намерения и прежние файлы сохранены",
        )
        .map_err(err)?;
    progress(control, id, SourceCheckpoint::Applying, None)?;
    for (i, change) in changes.iter().enumerate() {
        let ordinal = i as u32;
        let path = source.join(&change.path);
        let held = artifact(transaction, "held", ordinal);
        guards(
            journal,
            authority,
            config,
            &r,
            control,
            SourceTransactionPhase::Applying,
        )?;
        if change.path == SOURCE_MANIFEST_NAME {
            target_files_match(source, &target)?;
        }
        if let Some(pin) = &change.previous {
            matches_file(&path, pin)?;
        } else if path.try_exists().map_err(err)? {
            return Err("New source path already exists; user data preserved".into());
        }
        journal
            .source_file_intent(authority, id, ordinal)
            .map_err(err)?;
        progress(
            control,
            id,
            SourceCheckpoint::IntentCommitted,
            Some((ordinal, &change.path)),
        )?;
        if let Some(previous) = &change.previous {
            guards(
                journal,
                authority,
                config,
                &r,
                control,
                SourceTransactionPhase::Applying,
            )?;
            move_new(&path, &held).map_err(err)?;
            matches_file(&held, previous)?;
            progress(
                control,
                id,
                SourceCheckpoint::OldPreserved,
                Some((ordinal, &change.path)),
            )?;
        }
        if let Some(pin) = &change.incoming {
            parent_dirs(source, &path, || {
                guards(
                    journal,
                    authority,
                    config,
                    &r,
                    control,
                    SourceTransactionPhase::Applying,
                )
            })?;
            guards(
                journal,
                authority,
                config,
                &r,
                control,
                SourceTransactionPhase::Applying,
            )?;
            let prepared = artifact(transaction, "prepared", ordinal);
            matches_file(&prepared, pin)?;
            move_new(&prepared, &path).map_err(err)?;
            matches_file(&path, pin)?;
            progress(
                control,
                id,
                SourceCheckpoint::NewPlaced,
                Some((ordinal, &change.path)),
            )?;
        } else if path.try_exists().map_err(err)? {
            return Err("Removed source path reappeared; no overwrite or automatic retry".into());
        }
        guards(
            journal,
            authority,
            config,
            &r,
            control,
            SourceTransactionPhase::Applying,
        )?;
        journal
            .source_file_receipt(authority, id, ordinal)
            .map_err(err)?;
        progress(
            control,
            id,
            SourceCheckpoint::ReceiptCommitted,
            Some((ordinal, &change.path)),
        )?;
    }
    guards(
        journal,
        authority,
        config,
        &r,
        control,
        SourceTransactionPhase::Applying,
    )?;
    verify_source(source, &r.worker_source_spec.target_manifest_sha256)?;
    let verified = journal
        .change_source_phase(
            authority,
            id,
            SourceTransactionPhase::Verified,
            "Исходники полностью проверены; установка binaries ещё не выполнялась",
        )
        .map_err(err)?;
    progress(control, id, SourceCheckpoint::Verified, None).map_err(|e| {
        format!("Source is verified; progress delivery failed: {e}; inspect the retained result")
    })?;
    Ok(verified)
}
/// Attempts each pinned effect once. An unfinished transaction is never resumed by
/// calling this again; inspect its saved preimages/held files and both journals.
pub fn apply_source(
    journal: &mut UpdateJournal,
    authority: &UpdateAuthority,
    config: &UpdateConfig,
    id: &str,
    control: &mut impl SourceControl,
) -> Result<SourceTransactionRecord> {
    journal.check_source_authority(authority).map_err(err)?;
    let current = journal.source_header(id).map_err(err)?;
    if current.phase == SourceTransactionPhase::Verified {
        return Ok(current);
    }
    if current.phase != SourceTransactionPhase::Prepared {
        return Err("Unfinished/blocked source transaction cannot replay effects; manual reconciliation required".into());
    }
    match apply_inner(journal, authority, config, id, control) {
        Ok(r) => Ok(r),
        Err(error) => {
            if journal.source_header(id).is_ok_and(|r| {
                matches!(
                    r.phase,
                    SourceTransactionPhase::Prepared | SourceTransactionPhase::Applying
                )
            }) {
                let reason=format!("Изменение исходников остановлено: {error}. Сохраните preimages/held и оба журнала; не снимайте блокировку и не повторяйте применение.");
                let reason = if reason.len() > 4000 {
                    "Изменение исходников остановлено; сохраните staging, прежние файлы и журналы. Требуется ручная проверка."
                } else {
                    &reason
                };
                if let Err(e) = journal.change_source_phase(
                    authority,
                    id,
                    SourceTransactionPhase::Blocked,
                    reason,
                ) {
                    return Err(format!("{error}; failure record unconfirmed: {e}; durable source barrier must be preserved"));
                }
            }
            Err(error)
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceImage {
    Absent,
    Baseline,
    Target,
    Other,
    Unavailable,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryFile {
    pub ordinal: u32,
    pub path: String,
    pub source: SourceImage,
    pub preimage: SourceImage,
    pub held: SourceImage,
    pub preimage_path: String,
    pub held_path: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub can_replay: bool,
    pub status: SourceTransactionStatus,
    pub source_matches_baseline: bool,
    pub source_matches_target: bool,
    pub files: Vec<RecoveryFile>,
    pub instruction: &'static str,
}
fn image(path: &Path, change: &SourceChange) -> SourceImage {
    match path.try_exists() {
        Ok(false) => SourceImage::Absent,
        Err(_) => SourceImage::Unavailable,
        Ok(true) => {
            if winbridge_platform::require_safe_path(path).is_err() {
                return SourceImage::Unavailable;
            }
            if change
                .previous
                .as_ref()
                .is_some_and(|p| matches_file(path, p).is_ok())
            {
                SourceImage::Baseline
            } else if change
                .incoming
                .as_ref()
                .is_some_and(|p| matches_file(path, p).is_ok())
            {
                SourceImage::Target
            } else {
                SourceImage::Other
            }
        }
    }
}
/// Explicit bounded readonly forensic inspection. It neither repairs files nor
/// turns a missing receipt into success, even when target bytes happen to match.
pub fn inspect_source(
    directory: &Path,
    id: &str,
    offset: u32,
    limit: u16,
) -> Result<SourceInspection> {
    let status = UpdateJournal::read_source_status(directory, id, offset, limit).map_err(err)?;
    let r = &status.transaction;
    identities(r)?;
    let root = Path::new(&r.source_directory);
    let tx = Path::new(&r.transaction_directory);
    let baseline = verify_source(root, &r.worker_source_spec.baseline_manifest_sha256).is_ok();
    let target = verify_source(root, &r.worker_source_spec.target_manifest_sha256).is_ok();
    let mut files = Vec::new();
    for f in &status.files {
        let preimage = artifact(tx, "preimages", f.ordinal);
        let held = artifact(tx, "held", f.ordinal);
        files.push(RecoveryFile {
            ordinal: f.ordinal,
            path: f.change.path.clone(),
            source: image(&root.join(&f.change.path), &f.change),
            preimage: image(&preimage, &f.change),
            held: image(&held, &f.change),
            preimage_path: text(&preimage)?,
            held_path: text(&held)?,
        });
    }
    Ok(SourceInspection {schema_version:1,kind:"source_transaction_inspection",read_only:true,can_replay:false,status,source_matches_baseline:baseline,source_matches_target:target,files,instruction:"Сохраните оба журнала, preimages и held. Intent без квитанции не означает повтор: проверьте сохранённые файлы. Совпадение target само по себе не завершает обновление и не снимает worker guard."})
}
