//! Pure source ownership policy and update lifecycle. No filesystem/process I/O.
use crate::config::validate_relative;
use std::collections::{BTreeMap, BTreeSet};
use winbridge_protocol::update::*;

pub const SOURCE_MANIFEST_NAME: &str = "winbridge-source-manifest.json";
pub const MAX_SOURCE_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_SOURCE_FILES: usize = 10_000;
pub const MAX_SOURCE_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;

pub fn valid_hash(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Preserved paths are not read recursively, hashed, overwritten or backed up.
pub fn preserved_source_path(path: &str, includes_built_ui: bool) -> bool {
    let normalized = path.to_lowercase();
    if normalized == "apps/desktop/src-tauri/gen"
        || normalized.starts_with("apps/desktop/src-tauri/gen/")
        || normalized == "release"
        || normalized.starts_with("release/")
    {
        return true;
    }
    path.split('/').any(|part| {
        let p = part.to_lowercase();
        matches!(
            p.as_str(),
            ".git" | ".winbridge" | "target" | "node_modules" | "dist" | "__pycache__"
        ) || p.starts_with(".env")
            || p.ends_with(".local.json")
            || p.ends_with(".pem")
            || p.ends_with(".key")
            || p.ends_with(".pfx")
            || p.ends_with(".p12")
            || p.ends_with(".pyc")
            || matches!(
                p.as_str(),
                "id_rsa" | "id_ed25519" | "credentials" | "credentials.json"
            )
            || p.starts_with("codex-session-")
            || (!includes_built_ui && p == "web-dist")
    }) || path.eq_ignore_ascii_case(SOURCE_MANIFEST_NAME)
}

pub fn validate_source_path(path: &str) -> Result<(), String> {
    if path.contains('\\')
        || path.split('/').any(|p| p == ".")
        || path.encode_utf16().count() > 1024
        || path.split('/').count() > 64
        || path.split('/').any(|p| p.encode_utf16().count() > 255)
    {
        return Err(format!("Noncanonical or oversized source path: {path}"));
    }
    validate_relative(path)?;
    if preserved_source_path(path, true) {
        return Err(format!(
            "Protected runtime/config/cache/secret path in source manifest: {path}"
        ));
    }
    Ok(())
}

pub fn parse_source_manifest(bytes: &[u8]) -> Result<SourceManifest, String> {
    if bytes.len() as u64 > MAX_SOURCE_MANIFEST_BYTES {
        return Err("Source manifest exceeds 2 MiB".into());
    }
    let manifest: SourceManifest = serde_json::from_slice(bytes)
        .map_err(|_| "Invalid source manifest JSON/fields".to_string())?;
    validate_source_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_source_manifest(manifest: &SourceManifest) -> Result<(), String> {
    if manifest.schema_version != 1
        || manifest.dirty
        || !valid_hash(&manifest.source_commit, &[40, 64])
        || manifest.created_at.is_empty()
        || manifest.created_at.len() > 80
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_SOURCE_FILES
    {
        return Err("Expected bounded schema-1 manifest for a clean, pinned source commit".into());
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in &manifest.files {
        validate_source_path(&file.path)?;
        if (!manifest.includes_built_ui && file.path.to_lowercase().starts_with("web-dist/"))
            || !valid_hash(&file.sha256, &[64])
            || file.bytes > MAX_SOURCE_FILE_BYTES
        {
            return Err(format!(
                "Invalid file size/hash/UI declaration: {}",
                file.path
            ));
        }
        total = total
            .checked_add(file.bytes)
            .ok_or("Source size overflow")?;
        if total > MAX_SOURCE_BYTES || !names.insert(file.path.to_lowercase()) {
            return Err("Source exceeds size limit or contains case-colliding paths".into());
        }
    }
    for path in &names {
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if names.contains(parent) {
                return Err(format!("Source file/directory collision: {path}"));
            }
            ancestor = parent;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CurrentSourceEntry {
    pub path: String,
    pub directory: bool,
    pub bytes: u64,
    /// Only manifest-owned regular files need content reads.
    pub sha256: Option<String>,
}

pub fn plan_source_update(
    previous: &SourceManifest,
    incoming: &SourceManifest,
    current: &[CurrentSourceEntry],
) -> Result<SourcePlan, String> {
    validate_source_manifest(previous)?;
    validate_source_manifest(incoming)?;
    let old: BTreeMap<_, _> = previous
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let new: BTreeMap<_, _> = incoming
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let owned: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    let mut seen = BTreeMap::new();
    let mut blockers = Vec::new();
    let mut block = |path: &str, code, message: &str| {
        blockers.push(SourceBlocker {
            path: path.into(),
            code,
            message: message.into(),
        })
    };
    for entry in current {
        validate_source_path(&entry.path)?;
        if seen.insert(entry.path.as_str(), entry).is_some() {
            return Err("Duplicate current source entry".into());
        }
        if entry.directory {
            if owned.contains(entry.path.as_str()) {
                block(
                    &entry.path,
                    SourceBlockerCode::PathTypeConflict,
                    "A directory occupies a source file path; preserved",
                );
            }
        } else if !old.contains_key(entry.path.as_str()) {
            block(
                &entry.path,
                SourceBlockerCode::UnmanagedFile,
                "File is not owned by the previous source manifest; preserved",
            );
        }
    }
    for (path, file) in &old {
        match seen.get(path) {
            None => block(
                path,
                SourceBlockerCode::LocalMissing,
                "Previously installed source file was removed locally; not recreated automatically",
            ),
            Some(actual)
                if !actual.directory
                    && (actual.bytes != file.bytes
                        || actual.sha256.as_ref() != Some(&file.sha256)) =>
            {
                block(
                    path,
                    SourceBlockerCode::LocalModified,
                    "Source differs from the previous package; local edit preserved",
                )
            }
            _ => {}
        }
    }
    let mut folded = BTreeMap::new();
    for path in owned
        .iter()
        .copied()
        .chain(current.iter().map(|f| f.path.as_str()))
    {
        if let Some(before) = folded.insert(path.to_lowercase(), path) {
            if before != path {
                block(
                    path,
                    SourceBlockerCode::CaseConflict,
                    "Case-only path change/collision requires explicit reconciliation",
                );
            }
        }
        let mut ancestor = path;
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if owned.contains(parent) {
                block(
                    path,
                    SourceBlockerCode::LayoutChange,
                    "File/directory layout changes require explicit reconciliation",
                );
            }
            ancestor = parent;
        }
    }
    blockers.sort_by(|a, b| a.path.cmp(&b.path).then(a.message.cmp(&b.message)));
    blockers.dedup();
    let changes = owned
        .into_iter()
        .map(|path| {
            let previous = old.get(path).map(|f| (*f).clone());
            let incoming = new.get(path).map(|f| (*f).clone());
            let action = match (&previous, &incoming) {
                (None, Some(_)) => SourceAction::Add,
                (Some(_), None) => SourceAction::Remove,
                (Some(a), Some(b)) if a == b => SourceAction::Keep,
                _ => SourceAction::Replace,
            };
            SourceChange {
                path: path.into(),
                action,
                previous,
                incoming,
            }
        })
        .collect();
    Ok(SourcePlan {
        changes,
        can_apply: blockers.is_empty(),
        blockers,
    })
}

pub use winbridge_protocol::update::UpdateEvent;

pub const MAX_UPDATE_COUNTER: u64 = 9_007_199_254_740_991;

pub fn validate_update_key(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err("Update key must contain 1–128 bytes without control characters".into());
    }
    Ok(())
}

pub fn validate_update_request(
    request: &winbridge_protocol::update::UpdateRequest,
) -> Result<(), String> {
    validate_update_key(&request.idempotency_key)?;
    if request.contract_version != 1 {
        return Err("Unsupported update request contract".into());
    }
    let source = &request.source;
    validate_source_path(&source.archive_name)?;
    if source.archive_name.contains('/') || !source.archive_name.ends_with(".zip") {
        return Err("Update archive must be a ZIP filename, without a directory".into());
    }
    if !valid_hash(&source.archive_sha256, &[64])
        || !valid_hash(&source.manifest_sha256, &[64])
        || !valid_hash(&source.source_commit, &[40, 64])
        || !valid_hash(&request.expected.scope_sha256, &[64])
        || !valid_hash(&request.expected.worker_config_sha256, &[64])
        || !valid_hash(&request.expected.baseline_manifest_sha256, &[64])
    {
        return Err("Update identities require lowercase SHA hashes".into());
    }
    validate_update_key(&request.expected.installation_id)?;
    validate_update_key(&request.expected.worker_build_id)?;
    Ok(())
}

/// Manual intervention keeps the installation blocked even though its FSM is terminal.
pub fn update_blocks_installation(phase: UpdatePhase) -> bool {
    !matches!(
        phase,
        UpdatePhase::Committed | UpdatePhase::Deferred | UpdatePhase::FailedBeforeCutover
    )
}

pub fn advance_update(phase: UpdatePhase, event: UpdateEvent) -> Result<UpdatePhase, String> {
    use UpdateEvent as E;
    use UpdatePhase as P;
    let next = match (phase, event) {
        (P::Requested, E::BeginPreparation) => P::Preparing,
        (P::Preparing, E::BeginSourceApply) => P::SourceApplying,
        (P::Preparing | P::SourceApplying, E::SourceVerified) => P::Building,
        (P::Building, E::BuildSucceeded) => P::ValidatingCandidate,
        (P::ValidatingCandidate, E::CandidateAccepted) => P::CandidateVerified,
        (P::CandidateVerified, E::BeginDrain) => P::Draining,
        (
            P::Draining,
            E::Quiesce {
                dispatch_closed: true,
                accepted_operations: 0,
                physical_owners: 0,
                uncertain_owners: 0,
            },
        ) => P::Quiesced,
        (P::Quiesced, E::BeginShutdown) => P::Stopping,
        (P::Stopping, E::ShutdownVerified) => P::Replacing,
        (P::Replacing, E::FilesVerified) => P::VerifyingLocal,
        (P::VerifyingLocal, E::LocalHealthVerified) => P::AwaitingController,
        (P::AwaitingController, E::ControllerConfirmed) => P::Committed,
        (P::Draining, E::Defer) => P::Deferred,
        (
            P::Requested
            | P::Preparing
            | P::Building
            | P::ValidatingCandidate
            | P::CandidateVerified
            | P::Draining
            | P::Quiesced,
            E::Fail,
        ) => P::FailedBeforeCutover,
        (
            P::SourceApplying
            | P::Stopping
            | P::Replacing
            | P::VerifyingLocal
            | P::AwaitingController,
            E::Fail,
        ) => P::ManualIntervention,
        _ => return Err(format!("Update transition rejected: {phase:?} + {event:?}")),
    };
    Ok(next)
}
