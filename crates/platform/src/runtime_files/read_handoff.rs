//! Convert a live move owner to a strict read pin without releasing write denial.
//! No cold constructor, process startup, filesystem rename or permission change.
use super::*;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;

/// Exact file held for reading while denying new writers and deleters.
/// This resource alone grants no updater or worker startup authority.
pub struct RuntimeReadPin {
    file: File,
    path: PathBuf,
    identity: String,
    bytes: u64,
    sha256: String,
}

enum FailedCustody {
    Move { _owner: Box<OwnedRuntimeFile> },
    Read { _hold: File },
}
/// An error preserves the original owner before release, or the same readonly
/// file object after release. The latter still denies writers but may allow
/// rename/delete; it is deliberately not exposed as a successful strict pin.
/// No method turns this failure into a new move owner or resumes the handoff.
pub struct RuntimeReadHandoffFailure {
    reason: io::Error,
    custody: FailedCustody,
}
impl std::fmt::Debug for RuntimeReadHandoffFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeReadHandoffFailure")
            .field("reason", &self.reason)
            .field("move_owner_retained", &self.move_owner_retained())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for RuntimeReadHandoffFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Runtime read-pin handoff stopped: {}", self.reason)
    }
}
impl std::error::Error for RuntimeReadHandoffFailure {}
impl RuntimeReadHandoffFailure {
    pub fn reason(&self) -> &io::Error {
        &self.reason
    }
    pub fn move_owner_retained(&self) -> bool {
        matches!(self.custody, FailedCustody::Move { .. })
    }
    fn owned(reason: io::Error, owner: OwnedRuntimeFile) -> Self {
        Self {
            reason,
            custody: FailedCustody::Move {
                _owner: Box::new(owner),
            },
        }
    }
    fn held(reason: io::Error, hold: File) -> Self {
        Self {
            reason,
            custody: FailedCustody::Read { _hold: hold },
        }
    }
}
fn exact(file: &mut File, path: &Path, identity: &str, bytes: u64, sha256: &str) -> io::Result<()> {
    crate::require_safe_path(path)?;
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.len() != bytes
        || crate::file_identity(file)? != identity
        || crate::file_identity(&File::open(path)?)? != identity
        || hash(file, bytes)? != sha256
    {
        return Err(error("Runtime read handoff path/id/size/hash changed"));
    }
    Ok(())
}
impl RuntimeReadPin {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn revalidate(&mut self) -> io::Result<()> {
        exact(
            &mut self.file,
            &self.path,
            &self.identity,
            self.bytes,
            &self.sha256,
        )
    }
}
impl OwnedRuntimeFile {
    /// The caller retains directory/state/owner guards and verifies the entire
    /// installed set again before launch. A filename rebound during conversion
    /// is rejected rather than adopted. All error resources must be retained.
    pub fn into_read_pin(self) -> Result<RuntimeReadPin, RuntimeReadHandoffFailure> {
        self.into_read_pin_checked(|| Ok(()))
    }
    /// Check live authority before acquisition, before releasing DELETE access,
    /// and after that release while the same read-only object denies writers.
    /// A failed checkpoint never returns a partially verified successful pin.
    pub fn into_read_pin_checked(
        mut self,
        mut checkpoint: impl FnMut() -> io::Result<()>,
    ) -> Result<RuntimeReadPin, RuntimeReadHandoffFailure> {
        if let Err(reason) = checkpoint().and_then(|_| self.revalidate()) {
            return Err(RuntimeReadHandoffFailure::owned(reason, self));
        }
        // Must share DELETE with the still-live move owner. Deny WRITE at all
        // times, including the short interval before a no-delete pin is opened.
        let mut hold = match OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(reason) => return Err(RuntimeReadHandoffFailure::owned(reason, self)),
        };
        if let Err(reason) = exact(
            &mut hold,
            &self.path,
            &self.identity,
            self.bytes,
            &self.sha256,
        )
        .and_then(|_| checkpoint())
        .and_then(|_| self.revalidate())
        {
            return Err(RuntimeReadHandoffFailure::owned(reason, self));
        }
        let Self {
            file,
            path,
            identity,
            bytes,
            sha256,
            ..
        } = self;
        drop(file);
        // A foreign rename may now be possible. No action can start a worker
        // until the final handle and original path prove the expected File ID.
        let pinned = (|| {
            checkpoint()?;
            crate::require_local_path_prefix(&path)?;
            crate::require_safe_path(&path)?;
            crate::require_local_disk(&path)?;
            let mut file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(&path)?;
            exact(&mut file, &path, &identity, bytes, &sha256)?;
            exact(&mut hold, &path, &identity, bytes, &sha256)?;
            checkpoint()?;
            Ok(file)
        })();
        match pinned {
            Ok(file) => Ok(RuntimeReadPin {
                file,
                path,
                identity,
                bytes,
                sha256,
            }),
            Err(reason) => Err(RuntimeReadHandoffFailure::held(reason, hold)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(path: &Path) -> OwnedRuntimeFile {
        std::fs::write(path, b"exact runtime read handoff").unwrap();
        let file = File::open(path).unwrap();
        let id = crate::file_identity(&file).unwrap();
        drop(file);
        let bytes = std::fs::read(path).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        OwnedRuntimeFile::acquire(path, &id, bytes.len() as u64, &sha).unwrap()
    }
    #[test]
    fn runtime_read_handoff_preserves_exact_file_and_denies_write_delete() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker.exe");
        let owner = fixture(&path);
        let identity = owner.identity().to_owned();
        let mut pin = owner.into_read_pin().unwrap();
        assert_eq!(pin.identity(), identity);
        assert_eq!(pin.path(), path.canonicalize().unwrap());
        pin.revalidate().unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        assert!(File::open(&path).is_ok());
        drop(pin);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }
    #[test]
    fn runtime_read_handoff_failure_before_release_retains_original_move_owner() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker.exe");
        let failure = fixture(&path)
            .into_read_pin_checked(|| Err(error("stale owner")))
            .err()
            .unwrap();
        assert!(failure.move_owner_retained());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(failure);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }
    #[test]
    fn runtime_read_handoff_failure_after_release_keeps_write_denial_without_claiming_pin() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker.exe");
        let mut point = 0;
        let failure = fixture(&path)
            .into_read_pin_checked(|| {
                point += 1;
                if point == 3 {
                    Err(error("owner retired during handoff"))
                } else {
                    Ok(())
                }
            })
            .err()
            .unwrap();
        assert!(!failure.move_owner_retained());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(failure);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }
    #[test]
    fn runtime_read_handoff_rejects_foreign_file_even_with_identical_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("worker.exe");
        let displaced = root.path().join("foreign-retired.exe");
        let owner = fixture(&path);
        let bytes = std::fs::read(&path).unwrap();
        let mut point = 0;
        let failure = owner
            .into_read_pin_checked(|| {
                point += 1;
                if point == 3 {
                    std::fs::rename(&path, &displaced)?;
                    std::fs::write(&path, &bytes)?;
                }
                Ok(())
            })
            .err()
            .unwrap();
        assert!(!failure.move_owner_retained());
        assert!(OpenOptions::new().write(true).open(&displaced).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        drop(failure);
        assert!(OpenOptions::new().write(true).open(&displaced).is_ok());
    }
}
