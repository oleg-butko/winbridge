//! Feature-only fixture layout. A declaration is never native update custody.
use std::{
    fs::{File, OpenOptions},
    io::Read,
    ops::Deref,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use winbridge_protocol::BuildIdentity;

type Result<T> = std::result::Result<T, String>;
const MARKER: &str = "winbridge-activation-runtime.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureRuntimePin {
    pub directory: PathBuf,
    pub marker_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reservation {
    schema_version: u16,
    kind: String,
    reservation_id: String,
    runtime_directory: PathBuf,
    run_directory: PathBuf,
    installed_worker: PathBuf,
    source_commit: String,
    source_manifest_sha256: String,
    initial_worker_sha256: String,
    created_at_utc: String,
}

/// Holds one immutable resolved layout throughout setup, Main, and post-exit
/// observations. No ambient resolver or mutable JSON redirects later reads.
pub struct FixtureLayout {
    root: PathBuf,
    runtime: PathBuf,
    initial_worker_sha256: Option<String>,
    _marker_pin: Option<File>,
}
impl Deref for FixtureLayout {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.root
    }
}
impl AsRef<Path> for FixtureLayout {
    fn as_ref(&self) -> &Path {
        &self.root
    }
}
impl FixtureLayout {
    pub fn runtime(&self) -> &Path {
        &self.runtime
    }
    pub fn initial_worker_sha256(&self) -> Option<&str> {
        self.initial_worker_sha256.as_deref()
    }
    pub fn bind(
        root: &Path,
        declared: Option<&FixtureRuntimePin>,
        build: &BuildIdentity,
    ) -> Result<Self> {
        let root = directory(root)?;
        let Some(declared) = declared else {
            return Ok(Self {
                runtime: directory(&root.join("runtime"))?,
                root,
                initial_worker_sha256: None,
                _marker_pin: None,
            });
        };
        if !sha(&declared.marker_sha256) {
            return Err("Fixture reservation requires an exact marker SHA".into());
        }
        let runtime = directory(&declared.directory)?;
        if overlap(&runtime, &root) {
            return Err("Stable fixture runtime must be disjoint from case evidence".into());
        }
        let marker = runtime.join(MARKER);
        safe(&marker)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut pin = options.open(&marker).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        (&mut pin)
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65536 || winbridge_update::sha256(&bytes) != declared.marker_sha256 {
            return Err("Fixture reservation is oversized or its SHA changed".into());
        }
        let r: Reservation = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        safe(&r.installed_worker)?;
        let production = directory(
            r.installed_worker
                .parent()
                .ok_or("Fixture production parent missing")?,
        )?;
        let run = directory(&r.run_directory)?;
        if r.schema_version != 1
            || r.kind != "winbridge_activation_test_reservation"
            || uuid::Uuid::parse_str(&r.reservation_id).is_err()
            || directory(&r.runtime_directory)? != runtime
            || root.parent() != Some(run.as_path())
            || overlap(&runtime, &production)
            || overlap(&run, &production)
            || overlap(&runtime, &run)
            || build.source_dirty
            || build.source_commit.as_deref() != Some(r.source_commit.as_str())
            || build.source_manifest_sha256.as_deref() != Some(r.source_manifest_sha256.as_str())
            || !sha(&r.source_manifest_sha256)
            || r.source_commit.len() != 40
            || !r
                .source_commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || !sha(&r.initial_worker_sha256)
            || r.created_at_utc.is_empty()
        {
            return Err("Fixture reservation source/runtime/run/production binding differs".into());
        }
        require_busy_lease(&runtime)?;
        Ok(Self {
            root,
            runtime,
            initial_worker_sha256: Some(r.initial_worker_sha256),
            _marker_pin: Some(pin),
        })
    }
}

fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn overlap(a: &Path, b: &Path) -> bool {
    // Windows paths are case-insensitive even when serialized with different case.
    #[cfg(windows)]
    let (a, b) = (
        PathBuf::from(a.as_os_str().to_string_lossy().to_lowercase()),
        PathBuf::from(b.as_os_str().to_string_lossy().to_lowercase()),
    );
    #[cfg(windows)]
    let (a, b) = (a.as_path(), b.as_path());
    a.starts_with(b) || b.starts_with(a)
}
fn safe(path: &Path) -> Result<()> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Fixture paths cannot contain traversal".into());
    }
    winbridge_platform::require_local_path_prefix(path).map_err(|e| e.to_string())?;
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(path).map_err(|e| e.to_string())
}
fn directory(path: &Path) -> Result<PathBuf> {
    safe(path)?;
    if !path.is_dir() {
        return Err("Existing fixture directory required".into());
    }
    path.canonicalize().map_err(|e| e.to_string())
}
fn require_busy_lease(runtime: &Path) -> Result<()> {
    let path = runtime.join(".winbridge-test-runtime.lock");
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, fs::OpenOptionsExt};
        // Do not call require_safe_path(file): its single-link check opens for
        // DATA read, which the expected FileShare::None lease correctly denies.
        // Check the safe directory plus exact file metadata, with no data access.
        safe(runtime)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err("Regular non-reparse fixture lease required".into());
        }
        let attributes = OpenOptions::new()
            .read(true)
            .access_mode(0x80) // FILE_READ_ATTRIBUTES, unaffected by data sharing flags
            .share_mode(7)
            .open(&path)
            .map_err(|e| e.to_string())?;
        winbridge_platform::require_single_link_handle(&attributes).map_err(|e| e.to_string())?;
        // PowerShell holds FileShare::None. Only sharing violation proves busy;
        // missing files, ACL failures and unrelated I/O errors are not evidence.
        match OpenOptions::new().read(true).write(true).open(path) {
            Err(e) if e.raw_os_error() == Some(32) => Ok(()),
            _ => Err("Original fixture script must retain the exclusive slot lease".into()),
        }
    }
    #[cfg(not(windows))]
    {
        safe(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            _ => Err("Original fixture script must retain the exclusive slot lease".into()),
        }
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
