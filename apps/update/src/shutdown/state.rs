//! Handles to existing local state, never a create/migrate/cleanup operation.
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use winbridge_protocol::update::WorkerShutdownState;

pub struct ShutdownStatePins {
    path: PathBuf,
    directory: File,
    journal: File,
    instance: File,
    observation: WorkerShutdownState,
}
pub struct LockedShutdownState {
    pins: ShutdownStatePins,
}
/// Readonly inspection cannot acquire the instance lock or turn into a writer.
pub struct ShutdownStateObservation {
    pins: ShutdownStatePins,
}
fn open(path: &Path, directory: bool, write: bool) -> io::Result<File> {
    winbridge_platform::require_safe_path(path)?;
    winbridge_platform::require_local_disk(path)?;
    let mut opts = OpenOptions::new();
    opts.read(true).write(write);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        // Retain names throughout the proof; SQLite can still read/write WAL.
        opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        if directory {
            opts.custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .access_mode(FILE_READ_ATTRIBUTES);
        }
    }
    let file = opts.open(path)?;
    let meta = file.metadata()?;
    if (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err(io::Error::other("Unexpected shutdown state file type"));
    }
    winbridge_platform::require_safe_path(path)?;
    Ok(file)
}
impl ShutdownStatePins {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Self::capture_with(path, true)
    }
    fn capture_with(path: &Path, lock_write: bool) -> io::Result<Self> {
        winbridge_platform::require_local_path_prefix(path)?;
        winbridge_platform::require_safe_path(path)?;
        winbridge_platform::require_local_disk(path)?;
        let path = path.canonicalize()?;
        let directory = open(&path, true, false)?;
        let journal = open(&path.join("journal.sqlite"), false, false)?;
        // Missing worker.lock is an error, never a reason to create a new lock.
        let instance = open(&path.join("worker.lock"), false, lock_write)?;
        let observation = WorkerShutdownState {
            state_directory: path
                .to_str()
                .ok_or_else(|| io::Error::other("Shutdown state path must be UTF-8"))?
                .into(),
            directory_identity: winbridge_platform::file_identity(&directory)?,
            journal_identity: winbridge_platform::file_identity(&journal)?,
            lock_identity: winbridge_platform::file_identity(&instance)?,
        };
        if observation.directory_identity == observation.journal_identity
            || observation.directory_identity == observation.lock_identity
            || observation.journal_identity == observation.lock_identity
        {
            return Err(io::Error::other(
                "Shutdown state identities must be distinct",
            ));
        }
        let pins = Self {
            path,
            directory,
            journal,
            instance,
            observation,
        };
        pins.revalidate()?;
        Ok(pins)
    }
    pub fn observation(&self) -> &WorkerShutdownState {
        &self.observation
    }
    pub fn revalidate(&self) -> io::Result<()> {
        for (path, held, expected, directory) in [
            (
                self.path.clone(),
                &self.directory,
                &self.observation.directory_identity,
                true,
            ),
            (
                self.path.join("journal.sqlite"),
                &self.journal,
                &self.observation.journal_identity,
                false,
            ),
            (
                self.path.join("worker.lock"),
                &self.instance,
                &self.observation.lock_identity,
                false,
            ),
        ] {
            winbridge_platform::require_safe_path(&path)?;
            if winbridge_platform::file_identity(held)? != *expected
                || winbridge_platform::file_identity(&open(&path, directory, false)?)? != *expected
            {
                return Err(io::Error::other(
                    "Shutdown state path or held file was rebound",
                ));
            }
        }
        Ok(())
    }
    /// The caller must separately prove exact process exit. This guard alone
    /// neither stops a worker nor grants runtime replacement/startup authority.
    pub fn try_lock(self) -> io::Result<LockedShutdownState> {
        self.revalidate()?;
        fs2::FileExt::try_lock_exclusive(&self.instance)?;
        self.revalidate()?;
        Ok(LockedShutdownState { pins: self })
    }
}
impl LockedShutdownState {
    pub fn observation(&self) -> &WorkerShutdownState {
        self.pins.observation()
    }
    pub fn revalidate(&self) -> io::Result<()> {
        self.pins.revalidate()
    }
}
impl ShutdownStateObservation {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Ok(Self {
            pins: ShutdownStatePins::capture_with(path, false)?,
        })
    }
    pub fn observation(&self) -> &WorkerShutdownState {
        self.pins.observation()
    }
    pub fn revalidate(&self) -> io::Result<()> {
        self.pins.revalidate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn state_pin_never_creates_files_and_existing_owner_blocks_lock_until_release() {
        let root = tempfile::tempdir().unwrap();
        assert!(ShutdownStatePins::capture(root.path()).is_err());
        assert!(!root.path().join("journal.sqlite").exists());
        fs::write(
            root.path().join("journal.sqlite"),
            b"not read as SQLite by this adapter",
        )
        .unwrap();
        assert!(ShutdownStatePins::capture(root.path()).is_err());
        assert!(!root.path().join("worker.lock").exists());
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(root.path().join("worker.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&owner).unwrap();
        let pins = ShutdownStatePins::capture(root.path()).unwrap();
        let expected = pins.observation().clone();
        assert!(pins.try_lock().is_err());
        drop(owner);
        let guard = ShutdownStatePins::capture(root.path())
            .unwrap()
            .try_lock()
            .unwrap();
        assert_eq!(guard.observation(), &expected);
        guard.revalidate().unwrap();
        // Acquiring the same name again must not bypass the held instance lock.
        assert!(ShutdownStatePins::capture(root.path())
            .unwrap()
            .try_lock()
            .is_err());
        drop(guard);
        ShutdownStatePins::capture(root.path())
            .unwrap()
            .try_lock()
            .unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn state_pin_rejects_rebound_names_and_linked_state() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        for name in ["journal.sqlite", "worker.lock"] {
            fs::write(root.path().join(name), b"").unwrap();
        }
        for name in ["journal.sqlite", "worker.lock"] {
            let pins = ShutdownStatePins::capture(root.path()).unwrap();
            let original = root.path().join(name);
            let moved = root.path().join(format!("{name}.original"));
            fs::rename(&original, &moved).unwrap();
            fs::write(&original, b"").unwrap();
            assert!(pins.revalidate().is_err());
            assert!(pins.try_lock().is_err());
            fs::remove_file(&original).unwrap();
            fs::rename(moved, original).unwrap();
        }
        let alias = root.path().with_extension("alias");
        symlink(root.path(), &alias).unwrap();
        assert!(ShutdownStatePins::capture(&alias).is_err());
        fs::remove_file(alias).unwrap();
        fs::hard_link(
            root.path().join("worker.lock"),
            root.path().join("alias.lock"),
        )
        .unwrap();
        assert!(ShutdownStatePins::capture(root.path()).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn state_pin_blocks_windows_rename_until_all_observation_handles_are_dropped() {
        let root = tempfile::tempdir().unwrap();
        for name in ["journal.sqlite", "worker.lock"] {
            fs::write(root.path().join(name), b"").unwrap();
        }
        let pins = ShutdownStatePins::capture(root.path()).unwrap();
        for name in ["journal.sqlite", "worker.lock"] {
            assert!(fs::rename(
                root.path().join(name),
                root.path().join(format!("{name}.moved"))
            )
            .is_err());
        }
        pins.revalidate().unwrap();
        drop(pins);
        fs::rename(
            root.path().join("worker.lock"),
            root.path().join("worker.lock.moved"),
        )
        .unwrap();
    }
}
