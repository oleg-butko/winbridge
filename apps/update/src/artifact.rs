//! Readonly identity probes. This never approves a candidate or changes runtime.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use winbridge_platform::{ManagedChild, OutputReader};
use winbridge_protocol::{
    ArtifactDescriptor, ArtifactRole, ResolvedCommand, ARTIFACT_QUERY_MARKER,
};
type Result<T> = std::result::Result<T, String>;
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_QUERY_BYTES: usize = 64 * 1024;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub use winbridge_protocol::RuntimeImageEvidence as ImageEvidence;
pub(crate) struct Pinned {
    file: File,
    path: PathBuf,
    pub(crate) evidence: ImageEvidence,
}
impl Pinned {
    /// Internal generation writer: create_new only, never copy metadata from a
    /// caller-selected file. Both original and returned read pins stay live.
    pub(crate) fn copy_new(&self, destination: &Path) -> Result<Self> {
        winbridge_platform::require_safe_path(&self.path).map_err(error)?;
        winbridge_platform::require_local_path_prefix(destination).map_err(error)?;
        winbridge_platform::require_safe_path(destination).map_err(error)?;
        winbridge_platform::require_local_disk(destination).map_err(error)?;
        let mut source = open(&self.path)?;
        if winbridge_platform::file_identity(&source).map_err(error)? != self.evidence.file_identity
            || source.metadata().map_err(error)?.len() != self.evidence.bytes
        {
            return Err("Pinned preimage source identity/size changed".into());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(destination).map_err(error)?;
        let created = winbridge_platform::file_identity(&output).map_err(error)?;
        let mut digest = Sha256::new();
        let mut total = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = source.read(&mut buffer).map_err(error)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > self.evidence.bytes {
                return Err("Preimage source grew; partial copy preserved".into());
            }
            output.write_all(&buffer[..n]).map_err(error)?;
            digest.update(&buffer[..n]);
        }
        output.sync_all().map_err(error)?;
        drop(output);
        winbridge_platform::source_files::sync_parent(destination).map_err(error)?;
        if total != self.evidence.bytes
            || format!("{:x}", digest.finalize()) != self.evidence.sha256
        {
            return Err("Preimage source bytes changed; partial copy preserved".into());
        }
        let copied = pin(destination)?;
        if copied.evidence.file_identity != created
            || copied.evidence.file_identity == self.evidence.file_identity
            || copied.evidence.bytes != self.evidence.bytes
            || copied.evidence.sha256 != self.evidence.sha256
            || copied.evidence.query_supported != self.evidence.query_supported
        {
            return Err("Preimage copy changed before pinning; preserve for inspection".into());
        }
        Ok(copied)
    }
}
fn open(path: &Path) -> Result<File> {
    let mut o = OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    o.open(path).map_err(error)
}
pub(crate) fn pin(path: &Path) -> Result<Pinned> {
    if !path.is_absolute() {
        return Err("Artifact image needs an absolute local path".into());
    }
    winbridge_platform::require_local_path_prefix(path).map_err(error)?;
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    let path = path.canonicalize().map_err(error)?;
    let mut file = open(&path)?;
    let meta = file.metadata().map_err(error)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
        return Err("Artifact image is empty, oversized or not a regular file".into());
    }
    let identity = winbridge_platform::file_identity(&file).map_err(error)?;
    let mut digest = Sha256::new();
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    let mut tail = vec![];
    let mut supported = false;
    loop {
        let n = file.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_IMAGE_BYTES {
            return Err("Artifact grew beyond the byte limit".into());
        }
        digest.update(&buffer[..n]);
        tail.extend_from_slice(&buffer[..n]);
        supported |= tail
            .windows(ARTIFACT_QUERY_MARKER.len())
            .any(|w| w == ARTIFACT_QUERY_MARKER);
        let retain = ARTIFACT_QUERY_MARKER.len() - 1;
        if tail.len() > retain {
            tail.drain(..tail.len() - retain);
        }
    }
    winbridge_platform::require_safe_path(&path).map_err(error)?;
    if total != meta.len()
        || file.metadata().map_err(error)?.len() != total
        || winbridge_platform::file_identity(&open(&path)?).map_err(error)? != identity
    {
        return Err("Artifact changed while reading identity".into());
    }
    let evidence = ImageEvidence {
        path: path.to_str().ok_or("Artifact path must be UTF-8")?.into(),
        bytes: total,
        sha256: format!("{:x}", digest.finalize()),
        file_identity: identity,
        query_supported: supported,
    };
    Ok(Pinned {
        file,
        path,
        evidence,
    })
}
fn same_image(before: &ImageEvidence, after: &ImageEvidence) -> bool {
    before.file_identity == after.file_identity
        && before.bytes == after.bytes
        && before.sha256 == after.sha256
}
pub fn inspect_image(path: &Path) -> Result<ImageEvidence> {
    Ok(pin(path)?.evidence)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryReport {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub candidate_verified: bool,
    pub image: ImageEvidence,
    pub descriptor: ArtifactDescriptor,
    pub process: winbridge_protocol::update::SupervisorProcess,
    pub cleanup_verified: bool,
}
fn capture(
    mut reader: OutputReader,
    excess: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut out = vec![];
        let mut buf = [0u8; 4096];
        loop {
            let n = reader.read(&mut buf).map_err(error)?;
            if n == 0 {
                break;
            }
            let keep = n.min(MAX_QUERY_BYTES.saturating_sub(out.len()));
            out.extend_from_slice(&buf[..keep]);
            if keep < n {
                excess.store(true, Ordering::Release);
            }
        }
        Ok(out)
    })
}
/// The capability marker prevents executing an unsupported older desktop. It
/// is not a signature: the caller still owns the trusted build/source policy.
pub fn query_image(path: &Path, role: ArtifactRole) -> Result<QueryReport> {
    let pinned = pin(path)?;
    if !pinned.evidence.query_supported {
        return Err(
            "UnsupportedArtifactQuery: capability marker absent; executable was not started".into(),
        );
    }
    let command = ResolvedCommand {
        executable: pinned.evidence.path.clone(),
        arguments: vec!["--artifact-info".into()],
        working_directory: pinned
            .path
            .parent()
            .ok_or("Artifact parent missing")?
            .to_str()
            .ok_or("Artifact cwd must be UTF-8")?
            .into(),
        timeout_ms: 10000,
    };
    let job = format!("Global\\WinBridge-ArtifactQuery-{}", uuid::Uuid::new_v4());
    let (mut child, stdout, stderr) =
        ManagedChild::prepare_atomic_executable(&command, Some(&job), None)
            .map_err(|e| format!("Artifact query creation unconfirmed: {e}"))?;
    let process = match winbridge_platform::infrastructure::stamp(child.id()) {
        Ok(p) => p,
        Err(e) => {
            child.close_unconfirmed();
            return Err(format!("Artifact query PID unavailable: {e}"));
        }
    };
    let excess = Arc::new(AtomicBool::new(false));
    let readers = (
        capture(stdout, excess.clone()),
        capture(stderr, excess.clone()),
    );
    let start = Instant::now();
    let mut failure = child.resume().err().map(error);
    let mut exit = None;
    while failure.is_none() {
        match child.try_wait() {
            Ok(Some(s)) => {
                exit = Some(s);
                break;
            }
            Ok(None) => {}
            Err(e) => {
                failure = Some(error(e));
                break;
            }
        }
        if excess.load(Ordering::Acquire)
            || start.elapsed() > Duration::from_millis(command.timeout_ms)
        {
            failure = Some("Artifact query timeout/output limit".into());
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let stop = child.terminate_tree();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut empty = false;
    if stop.is_ok() {
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) && child.tree_is_empty().unwrap_or(false) {
                empty = true;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    if empty {
        drop(child);
    } else {
        child.close_unconfirmed();
    }
    while !(readers.0.is_finished() && readers.1.is_finished()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !empty || !readers.0.is_finished() || !readers.1.is_finished() {
        return Err("Artifact query cleanup/capture unconfirmed; no candidate approval".into());
    }
    let stdout = readers
        .0
        .join()
        .map_err(|_| "Artifact stdout capture panicked")??;
    let stderr = readers
        .1
        .join()
        .map_err(|_| "Artifact stderr capture panicked")??;
    if let Some(e) = failure {
        return Err(e);
    }
    if excess.load(Ordering::Acquire) || !exit.is_some_and(|s| s.success()) || !stderr.is_empty() {
        return Err(format!(
            "Artifact query failed; stderr={}",
            String::from_utf8_lossy(&stderr)
                .chars()
                .take(1000)
                .collect::<String>()
        ));
    }
    let descriptor: ArtifactDescriptor =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid artifact descriptor/fields")?;
    if descriptor.schema_version != 1
        || descriptor.query_contract != 1
        || descriptor.kind != "winbridge_artifact"
        || descriptor.role != role
        || descriptor.target_os != std::env::consts::OS
        || descriptor.target_arch != std::env::consts::ARCH
    {
        return Err("Artifact descriptor role/contract/target mismatch".into());
    }
    let after = pin(&pinned.path)?;
    if winbridge_platform::file_identity(&pinned.file).map_err(error)?
        != pinned.evidence.file_identity
        || !same_image(&pinned.evidence, &after.evidence)
    {
        return Err("Artifact changed during query".into());
    }
    Ok(QueryReport {
        schema_version: 1,
        kind: "artifact_query",
        read_only: true,
        candidate_verified: false,
        image: pinned.evidence,
        descriptor,
        process,
        cleanup_verified: true,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn same_bytes_at_a_replaced_path_never_rebind_the_pinned_image() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("image");
        std::fs::write(&path, b"original").unwrap();
        let before = pin(&path).unwrap();
        std::fs::rename(&path, root.path().join("original-image")).unwrap();
        std::fs::write(&path, b"original").unwrap();
        let replaced = pin(&path).unwrap();
        assert_eq!(before.evidence.sha256, replaced.evidence.sha256);
        assert_ne!(
            before.evidence.file_identity,
            replaced.evidence.file_identity
        );
        assert!(!same_image(&before.evidence, &replaced.evidence));
        std::fs::write(&path, b"modified").unwrap();
        let edited = pin(&path).unwrap();
        assert_eq!(
            replaced.evidence.file_identity,
            edited.evidence.file_identity
        );
        assert!(!same_image(&replaced.evidence, &edited.evidence));
    }
}
