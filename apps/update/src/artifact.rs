//! Bounded owned probes. This never approves a candidate or changes installed runtime.
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
    pub(crate) file: File,
    path: PathBuf,
    pub(crate) evidence: ImageEvidence,
}
impl Pinned {
    /// Internal generation writer: create_new only, never copy metadata from a
    /// caller-selected file. Both original and returned read pins stay live.
    pub(crate) fn copy_new(&self, destination: &Path) -> Result<Self> {
        self.copy_new_checked(destination, || Ok(()))
    }
    /// The owner checks fencing before create and each bounded write. This does
    /// not grant authority; the caller must commit its intent before calling.
    pub(crate) fn copy_new_checked(
        &self,
        destination: &Path,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
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
        checkpoint()?;
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
            checkpoint()?;
            output.write_all(&buffer[..n]).map_err(error)?;
            digest.update(&buffer[..n]);
        }
        output.sync_all().map_err(error)?;
        checkpoint()?;
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
    let marker = memchr::memmem::Finder::new(ARTIFACT_QUERY_MARKER);
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
        // The digest still covers every byte. Marker presence, once known,
        // does not need another sliding-window scan of a large debug image.
        if !supported {
            tail.extend_from_slice(&buffer[..n]);
            supported = marker.find(&tail).is_some();
            let retain = ARTIFACT_QUERY_MARKER.len() - 1;
            if tail.len() > retain {
                tail.drain(..tail.len() - retain);
            }
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
    let (stdout, stderr, process) = run_probe(&pinned, &["--artifact-info".into()], 10000)?;
    if !stderr.is_empty() {
        return Err("Artifact descriptor wrote unexpected stderr".into());
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

/// Process evidence for a future journal-backed observer. It is not candidate
/// acceptance. `no_process_created` is distinct from unknown creation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeCleanup {
    pub job_name: String,
    pub process: Option<winbridge_protocol::update::SupervisorProcess>,
    pub creation_confirmed: bool,
    pub no_process_created: bool,
    pub resume_requested: bool,
    pub tree_empty: bool,
    pub capture_complete: bool,
    pub output_exceeded: bool,
    pub exit_code: Option<i32>,
    pub failure: Option<String>,
}
/// Only the trusted driver implements this; callbacks never come from wire JSON.
/// before_create must persist intent; before_resume must save exact PID/Job and
/// resume authorization. A callback error never permits automatic process replay.
pub(crate) trait ProbeObserver {
    fn before_create(&mut self) -> Result<()>;
    fn before_resume(
        &mut self,
        process: &winbridge_protocol::update::SupervisorProcess,
        job: &str,
    ) -> Result<()>;
    fn resumed(&mut self) -> Result<()>;
    fn poll(&mut self) -> Result<()>;
    fn finished(&mut self, receipt: &ProbeCleanup) -> Result<()>;
}
struct UnjournaledProbe;
impl ProbeObserver for UnjournaledProbe {
    fn before_create(&mut self) -> Result<()> {
        Ok(())
    }
    fn before_resume(
        &mut self,
        _: &winbridge_protocol::update::SupervisorProcess,
        _: &str,
    ) -> Result<()> {
        Ok(())
    }
    fn resumed(&mut self) -> Result<()> {
        Ok(())
    }
    fn poll(&mut self) -> Result<()> {
        Ok(())
    }
    fn finished(&mut self, _: &ProbeCleanup) -> Result<()> {
        Ok(())
    }
}
pub(crate) fn run_probe(
    pinned: &Pinned,
    arguments: &[String],
    timeout_ms: u64,
) -> Result<(
    Vec<u8>,
    Vec<u8>,
    winbridge_protocol::update::SupervisorProcess,
)> {
    run_probe_observed(
        pinned,
        arguments,
        timeout_ms,
        &format!("Global\\WinBridge-OwnedProbe-{}", uuid::Uuid::new_v4()),
        &mut UnjournaledProbe,
    )
}
pub(crate) fn run_probe_observed(
    pinned: &Pinned,
    arguments: &[String],
    timeout_ms: u64,
    job: &str,
    observer: &mut impl ProbeObserver,
) -> Result<(
    Vec<u8>,
    Vec<u8>,
    winbridge_protocol::update::SupervisorProcess,
)> {
    let command = ResolvedCommand {
        executable: pinned.evidence.path.clone(),
        arguments: arguments.to_vec(),
        working_directory: pinned
            .path
            .parent()
            .ok_or("Artifact parent missing")?
            .to_str()
            .ok_or("Artifact cwd must be UTF-8")?
            .into(),
        timeout_ms,
    };
    let mut evidence = ProbeCleanup {
        job_name: job.into(),
        process: None,
        creation_confirmed: false,
        no_process_created: false,
        resume_requested: false,
        tree_empty: false,
        capture_complete: false,
        output_exceeded: false,
        exit_code: None,
        failure: None,
    };
    if let Err(error) = observer.before_create() {
        evidence.no_process_created = true;
        evidence.tree_empty = true;
        evidence.capture_complete = true;
        evidence.failure = Some(format!("Probe stopped before process creation: {error}"));
        observer.finished(&evidence)?;
        return Err(evidence.failure.unwrap());
    }
    let (mut child, stdout, stderr) =
        match ManagedChild::prepare_atomic_executable(&command, Some(job), None) {
            Ok(child) => child,
            Err(error) => {
                evidence.no_process_created = error.no_process_created;
                evidence.tree_empty = error.no_process_created;
                evidence.capture_complete = error.no_process_created;
                evidence.failure = Some(format!("Artifact query creation unconfirmed: {error}"));
                observer.finished(&evidence)?;
                return Err(evidence.failure.unwrap());
            }
        };
    evidence.creation_confirmed = true;
    let process = match winbridge_platform::infrastructure::stamp(child.id()) {
        Ok(p) => p,
        Err(error) => {
            child.close_unconfirmed();
            evidence.failure = Some(format!("Artifact query PID unavailable: {error}"));
            observer.finished(&evidence)?;
            return Err(evidence.failure.unwrap());
        }
    };
    evidence.process = Some(process.clone());
    // The atomic Windows adapter has already assigned the suspended child to
    // this exact Job. Never publish a different name as a durable PID receipt.
    let assigned = !cfg!(windows)
        || child.identity().is_some_and(|id| {
            id.job_name == job
                && id.process_id == process.process_id
                && id.host_id == process.host_id
                && id.session_id == process.session_id
                && id.creation_time.to_string() == process.birth
        });
    let excess = Arc::new(AtomicBool::new(false));
    let readers = (
        capture(stdout, excess.clone()),
        capture(stderr, excess.clone()),
    );
    let mut failure = if assigned {
        observer.before_resume(&process, job).err()
    } else {
        Some("Probe Job/PID receipt inconsistent; no resume".into())
    };
    let start = Instant::now();
    if failure.is_none() {
        // Set before the OS call: a failed resume can have an unknown outcome.
        evidence.resume_requested = true;
        failure = child.resume().err().map(error);
        if failure.is_none() {
            failure = observer.resumed().err();
        }
    }
    let mut exit = None;
    while failure.is_none() {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit = Some(status);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
        if let Err(error) = observer.poll() {
            failure = Some(error);
            break;
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
    evidence.tree_empty = empty;
    evidence.exit_code = exit.and_then(|status| status.code());
    evidence.output_exceeded = excess.load(Ordering::Acquire);
    if !empty || !readers.0.is_finished() || !readers.1.is_finished() {
        evidence.failure =
            Some("Artifact query cleanup/capture unconfirmed; no candidate approval".into());
        observer.finished(&evidence)?;
        return Err(evidence.failure.unwrap());
    }
    let stdout = readers
        .0
        .join()
        .map_err(|_| "Artifact stdout capture panicked".to_string())
        .and_then(|r| r);
    let stderr = readers
        .1
        .join()
        .map_err(|_| "Artifact stderr capture panicked".to_string())
        .and_then(|r| r);
    evidence.capture_complete = stdout.is_ok() && stderr.is_ok();
    let bytes = match (stdout, stderr) {
        (Ok(stdout), Ok(stderr)) => {
            if failure.is_none()
                && (evidence.output_exceeded || !exit.is_some_and(|status| status.success()))
            {
                failure = Some(format!(
                    "Artifact query failed; stderr={}",
                    String::from_utf8_lossy(&stderr)
                        .chars()
                        .take(1000)
                        .collect::<String>()
                ));
            }
            Some((stdout, stderr))
        }
        (Err(error), _) | (_, Err(error)) => {
            failure = Some(error);
            None
        }
    };
    evidence.failure = failure;
    observer.finished(&evidence)?;
    if let Some(error) = evidence.failure {
        return Err(error);
    }
    let (stdout, stderr) = bytes.ok_or("Probe capture missing")?;
    Ok((stdout, stderr, process))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
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
    #[test]
    fn marker_detection_never_shortcuts_suffix_hash_and_spans_read_boundaries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("image");
        for offset in [0, 65536 - ARTIFACT_QUERY_MARKER.len() / 2, 2 * 65536 + 17] {
            let mut bytes = vec![0x5a; 512 * 1024];
            bytes[offset..offset + ARTIFACT_QUERY_MARKER.len()]
                .copy_from_slice(ARTIFACT_QUERY_MARKER);
            std::fs::write(&path, &bytes).unwrap();
            let before = inspect_image(&path).unwrap();
            assert!(before.query_supported);
            assert_eq!(before.bytes, bytes.len() as u64);
            assert_eq!(before.sha256, crate::sha256(&bytes));
            // The pin was released. A change after an early marker must still
            // change the observed SHA; finding a capability never trusts a tail.
            *bytes.last_mut().unwrap() ^= 1;
            std::fs::write(&path, &bytes).unwrap();
            let after = inspect_image(&path).unwrap();
            assert!(after.query_supported);
            assert_ne!(after.sha256, before.sha256);
            assert_eq!(after.sha256, crate::sha256(&bytes));
        }
        std::fs::write(&path, vec![0x5a; 128 * 1024]).unwrap();
        assert!(!inspect_image(&path).unwrap().query_supported);
    }
    #[test]
    fn marker_detection_large_image_keeps_exact_hash() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large-image");
        let mut bytes = vec![0x5a; 16 * 1024 * 1024];
        let offset = bytes.len() - ARTIFACT_QUERY_MARKER.len() - 17;
        bytes[offset..offset + ARTIFACT_QUERY_MARKER.len()].copy_from_slice(ARTIFACT_QUERY_MARKER);
        std::fs::write(&path, &bytes).unwrap();
        let started = Instant::now();
        let marked = inspect_image(&path).unwrap();
        eprintln!(
            "Large artifact marker search: present {}ms",
            started.elapsed().as_millis()
        );
        assert!(marked.query_supported);
        assert_eq!(marked.bytes, bytes.len() as u64);
        assert_eq!(marked.sha256, crate::sha256(&bytes));
        // A near match at EOF must not authorize an executable query. Hashing
        // continues over the entire suffix for both supported and absent cases.
        bytes[offset + ARTIFACT_QUERY_MARKER.len() - 1] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        let started = Instant::now();
        let absent = inspect_image(&path).unwrap();
        eprintln!(
            "Large artifact marker search: absent {}ms",
            started.elapsed().as_millis()
        );
        assert!(!absent.query_supported);
        assert_ne!(absent.sha256, marked.sha256);
        assert_eq!(absent.sha256, crate::sha256(&bytes));
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod probe_tests;
