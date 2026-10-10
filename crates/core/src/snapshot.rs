//! Platform-independent checks of untrusted snapshot metadata.
//! These checks do not verify payload bytes, archive entries or filesystem links.
use crate::config::{valid_id, validate_relative, Result};
use std::collections::{hash_map::Entry, HashMap};
use winbridge_protocol::{SnapshotManifest, SnapshotReady, SnapshotSource};

pub const SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_READY_BYTES: usize = 8192;
pub const MAX_FILES: usize = 10_000;
pub const MAX_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
pub const PAYLOAD_CHUNK_BYTES: u64 = 1024 * 1024;
pub const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_RELATIVE_PATH_UNITS: usize = 1024;
pub const MAX_PATH_DEPTH: usize = 64;

fn hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn identifier(value: &str) -> Result<()> {
    if !valid_id(value) {
        return Err("Snapshot project/id must use 1-80 letters, digits, '-' or '_'".into());
    }
    validate_relative(value)
}
fn version(value: u16) -> Result<()> {
    if !matches!(value, 1..=3) {
        return Err(
            "Unsupported snapshot schemaVersion; expected 1 (full), 2 (delta) or 3 (resumable)"
                .into(),
        );
    }
    Ok(())
}
fn payload(bytes: u64, sha256: &str) -> Result<()> {
    if bytes == 0 || bytes > MAX_PAYLOAD_BYTES {
        return Err("Snapshot payload must contain 1-536870912 bytes".into());
    }
    if !hex(sha256, 32) {
        return Err("Snapshot SHA-256 must use 64 lowercase hexadecimal characters".into());
    }
    Ok(())
}
fn file_path(path: &str) -> Result<()> {
    if path.contains('\\') || path.split('/').any(|part| part == ".") {
        return Err(
            "Snapshot paths must use canonical '/' separators without '.' components".into(),
        );
    }
    if path.encode_utf16().count() > MAX_RELATIVE_PATH_UNITS
        || path.split('/').count() > MAX_PATH_DEPTH
        || path
            .split('/')
            .any(|part| part.encode_utf16().count() > 255)
    {
        return Err("Snapshot path exceeds Windows component/path limits".into());
    }
    validate_relative(path)?;
    if path.split('/').any(|part| {
        let part = part.to_ascii_lowercase();
        matches!(
            part.as_str(),
            ".git" | ".winbridge" | "node_modules" | "target"
        ) || part.starts_with(".env")
            || part.ends_with(".local.json")
    }) {
        return Err(
            "Snapshot cannot contain runtime configs, environment files, Git data or build caches"
                .into(),
        );
    }
    Ok(())
}

pub fn parse_manifest(bytes: &[u8]) -> Result<SnapshotManifest> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("Snapshot manifest exceeds 1 MiB".into());
    }
    let value: SnapshotManifest = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "Invalid snapshot manifest JSON/schema ({:?}, line {}, column {})",
            error.classify(),
            error.line(),
            error.column()
        )
    })?;
    validate_manifest(&value)?;
    Ok(value)
}

pub fn validate_manifest(manifest: &SnapshotManifest) -> Result<()> {
    version(manifest.schema_version)?;
    identifier(&manifest.project_id)?;
    identifier(&manifest.snapshot_id)?;
    let SnapshotSource::GitCommit { commit } = &manifest.source;
    if !hex(commit, 20) && !hex(commit, 32) {
        return Err("Snapshot source must name a full lowercase Git commit hash".into());
    }
    match (manifest.schema_version, &manifest.delta) {
        (1 | 3, None) => {}
        (2 | 3, Some(delta)) => {
            identifier(&delta.snapshot_id)?;
            if delta.snapshot_id == manifest.snapshot_id || !hex(&delta.manifest_sha256, 32) {
                return Err(
                    "Delta requires a distinct base snapshot and valid manifest SHA-256".into(),
                );
            }
            if delta.reused_files.len() > manifest.files.len() {
                return Err("Delta reuse count exceeds target file count".into());
            }
            let targets: HashMap<_, _> = manifest
                .files
                .iter()
                .map(|f| (f.path.as_str(), f))
                .collect();
            let mut seen = std::collections::HashSet::new();
            for entry in &delta.reused_files {
                file_path(&entry.path)?;
                file_path(&entry.base_path)?;
                if !targets.contains_key(entry.path.as_str()) || !seen.insert(&entry.path) {
                    return Err("Delta reuse paths must be unique target manifest files".into());
                }
            }
        }
        _ => {
            return Err("Schema 1 requires a full package; schema 2 requires delta metadata".into())
        }
    }
    payload(manifest.payload_bytes, &manifest.payload_sha256)?;
    match (manifest.schema_version, &manifest.payload_chunks) {
        (1 | 2, None) => {}
        (3, Some(chunks)) if chunks.chunk_bytes == PAYLOAD_CHUNK_BYTES
            && chunks.sha256.len() as u64 == manifest.payload_bytes.div_ceil(PAYLOAD_CHUNK_BYTES)
            && chunks.sha256.iter().all(|h| hex(h, 32)) => {}
        _ => return Err("Schema 3 requires exactly one SHA-256 per 1 MiB payload block; schema 1/2 exclude chunk metadata".into()),
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_FILES {
        return Err("Snapshot must contain 1-10000 files".into());
    }
    let path_bytes = manifest.files.iter().try_fold(0_usize, |sum, file| {
        sum.checked_add(file.path.len())
            .filter(|sum| *sum <= MAX_MANIFEST_BYTES)
    });
    if path_bytes.is_none() {
        return Err("Snapshot paths exceed the manifest metadata budget".into());
    }
    // Each directory spelling and file has one canonical Windows name. This is
    // conservative for Unicode uppercase expansions; ambiguous trees fail closed.
    let mut names: HashMap<String, (&str, bool)> = HashMap::new();
    let mut total = 0_u64;
    for file in &manifest.files {
        file_path(&file.path)?;
        if !hex(&file.sha256, 32) {
            return Err("Snapshot file has an invalid SHA-256".into());
        }
        total = total
            .checked_add(file.bytes)
            .filter(|total| *total <= MAX_UNPACKED_BYTES)
            .ok_or("Snapshot files exceed the 1 GiB unpacked size limit")?;
        let prefix_ends = file
            .path
            .match_indices('/')
            .map(|(offset, _)| offset)
            .chain(std::iter::once(file.path.len()));
        for end in prefix_ends {
            let path = &file.path[..end];
            let is_file = end == file.path.len();
            match names.entry(path.to_uppercase()) {
                Entry::Vacant(entry) => {
                    entry.insert((path, is_file));
                }
                Entry::Occupied(entry) => {
                    let (prior, was_file) = entry.get();
                    if *prior != path || *was_file || is_file {
                        return Err("Snapshot has duplicate/case-colliding paths or a file/directory conflict".into());
                    }
                }
            }
        }
    }
    if total != manifest.total_bytes {
        return Err("Snapshot totalBytes does not match its file sizes".into());
    }
    Ok(())
}

pub fn parse_ready(bytes: &[u8]) -> Result<SnapshotReady> {
    if bytes.len() > MAX_READY_BYTES {
        return Err("Snapshot ready marker exceeds 8 KiB".into());
    }
    let value: SnapshotReady = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "Invalid snapshot ready JSON/schema ({:?}, line {}, column {})",
            error.classify(),
            error.line(),
            error.column()
        )
    })?;
    validate_ready(&value)?;
    Ok(value)
}

pub fn validate_ready(value: &SnapshotReady) -> Result<()> {
    version(value.schema_version)?;
    identifier(&value.project_id)?;
    identifier(&value.snapshot_id)?;
    payload(value.payload_bytes, &value.payload_sha256)?;
    if !hex(&value.manifest_sha256, 32) {
        return Err("Ready marker has an invalid manifest SHA-256".into());
    }
    Ok(())
}

/// Check identity/metadata agreement only. Import must also hash manifest/payload
/// bytes, validate every archive entry and inspect the actual destination paths.
pub fn validate_ready_matches_manifest(
    ready: &SnapshotReady,
    manifest: &SnapshotManifest,
) -> Result<()> {
    validate_manifest(manifest)?;
    validate_ready(ready)?;
    if ready.schema_version != manifest.schema_version
        || ready.project_id != manifest.project_id
        || ready.snapshot_id != manifest.snapshot_id
        || ready.payload_bytes != manifest.payload_bytes
        || ready.payload_sha256 != manifest.payload_sha256
    {
        return Err("Ready marker does not describe the selected manifest/payload".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_protocol::SnapshotFile;

    fn manifest() -> SnapshotManifest {
        SnapshotManifest {
            schema_version: 1,
            project_id: "fixture".into(),
            snapshot_id: "commit-012345".into(),
            source: SnapshotSource::GitCommit {
                commit: "a".repeat(40),
            },
            payload_bytes: 20,
            payload_sha256: "b".repeat(64),
            payload_chunks: None,
            total_bytes: 3,
            delta: None,
            files: vec![SnapshotFile {
                path: "src/Привет.txt".into(),
                bytes: 3,
                sha256: "c".repeat(64),
                executable: false,
            }],
        }
    }
    #[test]
    fn resumable_blocks_are_pinned_bounded_and_cover_the_exact_payload() {
        use winbridge_protocol::SnapshotPayloadChunks;
        let mut value = manifest();
        value.schema_version = 3;
        assert!(validate_manifest(&value).is_err());
        value.payload_chunks = Some(SnapshotPayloadChunks {
            chunk_bytes: PAYLOAD_CHUNK_BYTES,
            sha256: vec!["d".repeat(64)],
        });
        assert!(validate_manifest(&value).is_ok());
        for size in [0, 65536, u64::MAX] {
            let mut bad = value.clone();
            bad.payload_chunks.as_mut().unwrap().chunk_bytes = size;
            assert!(validate_manifest(&bad).is_err());
        }
        let mut bad = value.clone();
        bad.payload_chunks
            .as_mut()
            .unwrap()
            .sha256
            .push("e".repeat(64));
        assert!(validate_manifest(&bad).is_err());
        bad.payload_chunks.as_mut().unwrap().sha256 = vec!["A".repeat(64)];
        assert!(validate_manifest(&bad).is_err());
        value.payload_bytes = PAYLOAD_CHUNK_BYTES + 1;
        assert!(validate_manifest(&value).is_err());
        value
            .payload_chunks
            .as_mut()
            .unwrap()
            .sha256
            .push("e".repeat(64));
        assert!(validate_manifest(&value).is_ok());
        value.schema_version = 1;
        assert!(validate_manifest(&value).is_err());
    }
    #[test]
    fn delta_reuse_paths_and_schema_agreement_fail_closed() {
        use winbridge_protocol::{SnapshotDelta, SnapshotReuse};
        let mut value = manifest();
        value.schema_version = 2;
        assert!(validate_manifest(&value).is_err());
        value.delta = Some(SnapshotDelta {
            snapshot_id: "base".into(),
            manifest_sha256: "e".repeat(64),
            reused_files: vec![SnapshotReuse {
                path: value.files[0].path.clone(),
                base_path: "original.txt".into(),
            }],
        });
        assert!(validate_manifest(&value).is_ok());
        let ready = SnapshotReady {
            schema_version: 1,
            project_id: value.project_id.clone(),
            snapshot_id: value.snapshot_id.clone(),
            manifest_sha256: "d".repeat(64),
            payload_sha256: value.payload_sha256.clone(),
            payload_bytes: value.payload_bytes,
        };
        assert!(validate_ready_matches_manifest(&ready, &value).is_err());
        for path in [
            "../escape",
            "CON.txt",
            ".winbridge/verified.json",
            "other.txt",
        ] {
            let mut bad = value.clone();
            bad.delta.as_mut().unwrap().reused_files[0].path = path.into();
            assert!(validate_manifest(&bad).is_err());
        }
        for path in [
            "../escape",
            "CON.txt",
            ".winbridge/verified.json",
            "dir\\file",
        ] {
            let mut bad = value.clone();
            bad.delta.as_mut().unwrap().reused_files[0].base_path = path.into();
            assert!(validate_manifest(&bad).is_err());
        }
        let mut bad = value.clone();
        bad.delta.as_mut().unwrap().snapshot_id = bad.snapshot_id.clone();
        assert!(validate_manifest(&bad).is_err());
        let mut bad = value.clone();
        let reused = bad.delta.as_ref().unwrap().reused_files[0].clone();
        bad.delta.as_mut().unwrap().reused_files.push(reused);
        assert!(validate_manifest(&bad).is_err());
        value.schema_version = 1;
        assert!(validate_manifest(&value).is_err());
    }
    #[test]
    fn bounded_strict_metadata_and_ready_identity() {
        let value = manifest();
        assert_eq!(
            parse_manifest(&serde_json::to_vec(&value).unwrap()).unwrap(),
            value
        );
        let mut two = value.clone();
        let mut file = two.files[0].clone();
        file.path = "src/World.txt".into();
        two.files.push(file);
        two.total_bytes *= 2;
        assert!(validate_manifest(&two).is_ok());
        let mut ready = SnapshotReady {
            schema_version: 1,
            project_id: value.project_id.clone(),
            snapshot_id: value.snapshot_id.clone(),
            manifest_sha256: "d".repeat(64),
            payload_sha256: value.payload_sha256.clone(),
            payload_bytes: value.payload_bytes,
        };
        assert!(validate_ready_matches_manifest(&ready, &value).is_ok());
        ready.snapshot_id = "another".into();
        assert!(validate_ready_matches_manifest(&ready, &value).is_err());
        assert!(parse_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
        assert!(parse_ready(&vec![b' '; MAX_READY_BYTES + 1]).is_err());
        let mut json = serde_json::to_value(&value).unwrap();
        json["ignored"] = true.into();
        assert!(parse_manifest(&serde_json::to_vec(&json).unwrap()).is_err());
        json.as_object_mut().unwrap().remove("ignored");
        json["files"][0]["linkTarget"] = "../secret".into();
        assert!(parse_manifest(&serde_json::to_vec(&json).unwrap()).is_err());
    }
    #[test]
    fn rejects_windows_escapes_configs_and_noncanonical_paths() {
        for path in [
            "../escape",
            "src/../escape",
            "/absolute",
            "C:/absolute",
            "C:relative",
            "\\\\host\\share",
            "src\\file",
            "./file",
            "src/./file",
            "a//b",
            "file:stream",
            "CON",
            "src/COM¹.txt",
            "NUL.json",
            "x. ",
            "x.",
            "src/a?",
            "src/a\0b",
            ".git/config",
            "nested/.env",
            ".env.production",
            ".envrc",
            ".env-production",
            "winbridge-worker.local.json",
            "target/output",
            "node_modules/pkg/index.js",
        ] {
            let mut value = manifest();
            value.files[0].path = path.into();
            assert!(validate_manifest(&value).is_err(), "{path:?}");
        }
        let mut value = manifest();
        value.files[0].path = "a".repeat(256);
        assert!(validate_manifest(&value).is_err());
        value.files[0].path = "a".repeat(100_000) + "?";
        let error = validate_manifest(&value).unwrap_err();
        assert!(
            error.len() < 256,
            "An oversized path was echoed into diagnostics"
        );
        value.files[0].path = "a/".repeat(MAX_PATH_DEPTH) + "file";
        assert!(validate_manifest(&value).is_err());
        value.snapshot_id = "NUL".into();
        assert!(validate_manifest(&value).is_err());
    }
    #[test]
    fn rejects_case_collisions_duplicate_files_and_directory_conflicts_in_any_order() {
        for paths in [
            ["src/A", "src/a"],
            ["src/a", "src/a"],
            ["src/a", "SRC/b"],
            ["src", "src/a"],
            ["src/a", "src"],
            ["Ä/a", "ä/b"],
        ] {
            let mut value = manifest();
            value.files = paths
                .iter()
                .map(|path| SnapshotFile {
                    path: (*path).into(),
                    bytes: 1,
                    sha256: "c".repeat(64),
                    executable: false,
                })
                .collect();
            value.total_bytes = 2;
            assert!(validate_manifest(&value).is_err(), "{paths:?}");
        }
    }
    #[test]
    fn rejects_inconsistent_size_overflow_and_invalid_hashes() {
        let mut value = manifest();
        value.total_bytes = 2;
        assert!(validate_manifest(&value).is_err());
        value.total_bytes = 3;
        value.files[0].bytes = u64::MAX;
        assert!(validate_manifest(&value).is_err());
        value.files[0].bytes = 3;
        value.payload_bytes = MAX_PAYLOAD_BYTES + 1;
        assert!(validate_manifest(&value).is_err());
        value.payload_bytes = 20;
        value.payload_sha256 = "B".repeat(64);
        assert!(validate_manifest(&value).is_err());
        value.payload_sha256 = "b".repeat(64);
        value.source = SnapshotSource::GitCommit {
            commit: "HEAD".into(),
        };
        assert!(validate_manifest(&value).is_err());
    }
}
