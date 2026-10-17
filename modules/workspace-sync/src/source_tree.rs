//! Manifest-owned source verification shared by worker coordination and updater.
use crate::{error, sha256, Result};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use winbridge_core::update::*;
use winbridge_protocol::update::SourceManifest;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedSource {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub source_commit: String,
    pub manifest_sha256: String,
    pub includes_built_ui: bool,
    pub file_count: usize,
    pub bytes: u64,
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
    winbridge_platform::require_local_path_prefix(path).map_err(error)?;
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    if !path.is_dir() {
        return Err(format!("Local directory unavailable: {}", path.display()));
    }
    path.canonicalize().map_err(error)
}

pub fn read_manifest(root: &Path) -> Result<(SourceManifest, String)> {
    let bytes = read(&root.join(SOURCE_MANIFEST_NAME), MAX_SOURCE_MANIFEST_BYTES).map_err(|e| {
        format!("Source baseline/manifest unavailable; bootstrap or repair is required: {e}")
    })?;
    Ok((parse_source_manifest(&bytes)?, sha256(&bytes)))
}

pub fn inventory(
    root: &Path,
    known: &SourceManifest,
    includes_built_ui: bool,
) -> Result<Vec<CurrentSourceEntry>> {
    let known: BTreeSet<_> = known.files.iter().map(|f| f.path.as_str()).collect();
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        winbridge_platform::require_safe_path(&directory).map_err(error)?;
        for entry in fs::read_dir(&directory).map_err(error)? {
            let path = entry.map_err(error)?.path();
            let relative = path
                .strip_prefix(root)
                .map_err(error)?
                .to_str()
                .ok_or("Source path is not UTF-8")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if preserved_source_path(&relative, includes_built_ui) {
                continue;
            }
            validate_source_path(&relative)?;
            winbridge_platform::require_safe_path(&path).map_err(error)?;
            let metadata = fs::symlink_metadata(&path).map_err(error)?;
            if !metadata.is_file() && !metadata.is_dir() {
                return Err("Special source entry refused".into());
            }
            let digest = if metadata.is_file() && known.contains(relative.as_str()) {
                Some(sha256(&read(&path, MAX_SOURCE_FILE_BYTES)?))
            } else {
                None
            };
            if metadata.is_dir() {
                pending.push(path);
            }
            files.push(CurrentSourceEntry {
                path: relative,
                directory: metadata.is_dir(),
                bytes: metadata.len(),
                sha256: digest,
            });
            if files.len() > MAX_SOURCE_FILES * 2 {
                return Err("Source inventory exceeds limit".into());
            }
        }
    }
    Ok(files)
}

pub fn verify_source(root: &Path, expected_manifest_sha256: &str) -> Result<VerifiedSource> {
    if !valid_hash(expected_manifest_sha256, &[64]) {
        return Err("Expected manifest SHA-256 must use 64 lowercase hex characters".into());
    }
    let root = local_directory(root)?;
    let (manifest, digest) = read_manifest(&root)?;
    if digest != expected_manifest_sha256 {
        return Err("Pinned source manifest SHA-256 mismatch".into());
    }
    let entries = inventory(&root, &manifest, manifest.includes_built_ui)?;
    let plan = plan_source_update(&manifest, &manifest, &entries)?;
    if !plan.can_apply {
        return Err(format!(
            "Source verification failed: {}",
            serde_json::to_string(&plan.blockers).map_err(error)?
        ));
    }
    Ok(VerifiedSource {
        schema_version: 1,
        kind: "source_verified",
        read_only: true,
        source_commit: manifest.source_commit,
        manifest_sha256: digest,
        includes_built_ui: manifest.includes_built_ui,
        file_count: manifest.files.len(),
        bytes: manifest.files.iter().map(|f| f.bytes).sum(),
    })
}
