//! Bounded historical evidence after a successful build, before start admission.
use crate::{store::now_ms, Result};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use winbridge_protocol::{ArtifactSpec, BuildArtifact, VerifiedSnapshotRef};

fn safe_artifact(path: &Path) -> Result<()> {
    // Cargo intentionally hard-links its final executable to target/debug/deps.
    // This reader never modifies the file; writable journal/log guards stay stricter.
    winbridge_platform::require_safe_path(path.parent().ok_or("Artifact has no parent")?)
        .map_err(|e| e.to_string())?;
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Artifact reparse point refused".into());
        }
    }
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Artifact must be a regular file, without symbolic links".into());
    }
    Ok(())
}

pub(crate) fn capture(
    root: &Path,
    specs: &[ArtifactSpec],
    snapshot: &VerifiedSnapshotRef,
    task: &str,
) -> Result<Vec<BuildArtifact>> {
    let mut total = 0u64;
    specs
        .iter()
        .map(|spec| {
            let path = root.join(spec.path.replace('\\', "/"));
            safe_artifact(&path).map_err(|e| format!("{}: {e}", spec.path))?;
            let mut file = File::open(&path).map_err(|e| format!("{}: {e}", spec.path))?;
            let before = file.metadata().map_err(|e| e.to_string())?;
            if !before.is_file() || before.len() > 256 * 1024 * 1024 {
                return Err("Artifact must be a regular file up to 256 MiB".into());
            }
            total += before.len();
            if total > 512 * 1024 * 1024 {
                return Err("Artifact total exceeds 512 MiB".into());
            }
            let identity = winbridge_platform::file_identity(&file).map_err(|e| e.to_string())?;
            let mut hash = Sha256::new();
            let mut buffer = [0; 65536];
            let mut bytes = 0u64;
            loop {
                let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if size == 0 {
                    break;
                }
                bytes += size as u64;
                if bytes > before.len() {
                    return Err("Artifact grew during hashing".into());
                }
                hash.update(&buffer[..size]);
            }
            safe_artifact(&path)?;
            let after = file.metadata().map_err(|e| e.to_string())?;
            let current = File::open(&path).map_err(|e| e.to_string())?;
            if bytes != before.len()
                || after.len() != before.len()
                || before.modified().ok() != after.modified().ok()
                || winbridge_platform::file_identity(&current).map_err(|e| e.to_string())?
                    != identity
            {
                return Err("Artifact changed or was replaced during hashing".into());
            }
            Ok(BuildArtifact {
                name: spec.name.clone(),
                path: path.to_string_lossy().into(),
                bytes,
                sha256: format!("{:x}", hash.finalize()),
                source_commit: match &snapshot.source {
                    winbridge_protocol::SnapshotSource::GitCommit { commit } => commit.clone(),
                },
                manifest_sha256: snapshot.manifest_sha256.clone(),
                build_task_id: task.into(),
                recorded_at_ms: now_ms(),
            })
        })
        .collect()
}
