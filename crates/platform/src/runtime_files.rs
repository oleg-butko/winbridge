//! Windows file-handle moves. Policy, durable intents and custody belong to updater.
//! No overwrite, cross-volume fallback, process control or delete-on-close flags.
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    FileRenameInfo, SetFileInformationByHandle, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
    FILE_RENAME_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

/// Non-Serde ownership of one actual file; not an installation permission.
pub struct OwnedRuntimeFile {
    file: File,
    path: PathBuf,
    identity: String,
    bytes: u64,
    sha256: String,
    attempted: bool,
}
fn error(reason: &str) -> io::Error {
    io::Error::other(reason)
}
fn directory(path: &Path) -> io::Result<File> {
    crate::require_local_path_prefix(path)?;
    crate::require_safe_path(path)?;
    crate::require_local_disk(path)?;
    let f = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    if !f.metadata()?.is_dir() {
        return Err(error("Runtime move requires a directory"));
    }
    Ok(f)
}
fn hash(file: &mut File, expected_bytes: u64) -> io::Result<String> {
    file.rewind()?;
    let mut h = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > expected_bytes {
            return Err(error("Runtime file grew while pinned"));
        }
        h.update(&buffer[..n]);
    }
    if bytes != expected_bytes {
        return Err(error("Runtime file size differs"));
    }
    Ok(format!("{:x}", h.finalize()))
}
impl OwnedRuntimeFile {
    /// Acquires read+DELETE, denying other writers/deleters, then verifies that
    /// same handle. A previously held no-delete read pin must be handed off first.
    pub fn acquire(path: &Path, identity: &str, bytes: u64, sha256: &str) -> io::Result<Self> {
        crate::require_local_path_prefix(path)?;
        crate::require_safe_path(path)?;
        crate::require_local_disk(path)?;
        if bytes == 0
            || bytes > 256 * 1024 * 1024
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(error("Invalid runtime file evidence"));
        }
        let file = OpenOptions::new()
            // GENERIC_READ | DELETE; explicit access avoids requesting write.
            .access_mode(0x8000_0000 | 0x0001_0000)
            .share_mode(FILE_SHARE_READ)
            .open(path)?;
        let mut owned = Self {
            file,
            path: path.canonicalize()?,
            identity: identity.into(),
            bytes,
            sha256: sha256.into(),
            attempted: false,
        };
        owned.revalidate()?;
        Ok(owned)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn revalidate(&mut self) -> io::Result<()> {
        crate::require_safe_path(&self.path)?;
        let m = self.file.metadata()?;
        if !m.is_file()
            || m.len() != self.bytes
            || crate::file_identity(&self.file)? != self.identity
            || crate::file_identity(&File::open(&self.path)?)? != self.identity
            || hash(&mut self.file, self.bytes)? != self.sha256
        {
            return Err(error("Owned runtime file/path/id/hash differs"));
        }
        Ok(())
    }
    /// Rename the owned file, relative to the retained destination directory
    /// handle. Existing destination fails atomically, including a racing creation.
    /// An error AFTER the OS call is indeterminate; caller must never retry it.
    pub fn move_new(
        &mut self,
        destination: &Path,
        expected_parent_identity: &str,
    ) -> io::Result<()> {
        if self.attempted {
            return Err(error(
                "Owned runtime move already attempted; inspect only, never replay",
            ));
        }
        self.revalidate()?;
        crate::require_local_path_prefix(destination)?;
        crate::require_safe_path(destination)?;
        crate::require_local_disk(destination)?;
        let parent = destination
            .parent()
            .ok_or_else(|| error("Runtime destination parent missing"))?;
        let parent = parent.canonicalize()?;
        let parent_file = directory(&parent)?;
        let parent_id = crate::file_identity(&parent_file)?;
        if parent_id != expected_parent_identity
            || parent_id.split(':').next() != self.identity.split(':').next()
        {
            return Err(error("Runtime destination rebound or crosses volume"));
        }
        let name = destination
            .file_name()
            .ok_or_else(|| error("Runtime destination name missing"))?;
        let wide: Vec<u16> = name.encode_wide().collect();
        if wide.is_empty() || wide.len() > 32767 || wide.contains(&0) {
            return Err(error("Runtime destination filename rejected"));
        }
        let destination = parent.join(name);
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let length = offset + (wide.len() + 1) * 2;
        // usize storage gives the SDK structure's native pointer alignment.
        let mut buffer = vec![0usize; length.div_ceil(std::mem::size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: zeroed/aligned allocation covers the SDK header and full UTF-16
        // tail, both handles remain owned, buffer is valid throughout this call.
        self.attempted = true;
        let ok = unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = parent_file.as_raw_handle();
            (*info).FileNameLength = (wide.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(wide.as_ptr(), (*info).FileName.as_mut_ptr(), wide.len());
            SetFileInformationByHandle(
                self.file.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                length as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // Update even if readback fails: this object must never point back at an
        // already moved old name. Durable caller records uncertainty on failure.
        self.path = destination;
        self.revalidate()?;
        if crate::file_identity(&parent_file)? != parent_id {
            return Err(error("Runtime destination parent changed after move"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evidence(path: &Path) -> (String, u64, String) {
        let mut f = File::open(path).unwrap();
        let size = f.metadata().unwrap().len();
        (
            crate::file_identity(&f).unwrap(),
            size,
            hash(&mut f, size).unwrap(),
        )
    }
    #[test]
    fn owned_move_preserves_same_file_and_cannot_replay() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("old.exe");
        let to = root.path().join("retained.exe");
        std::fs::write(&from, b"owned original").unwrap();
        let (id, size, sha) = evidence(&from);
        let parent = crate::source_files::directory_identity(root.path()).unwrap();
        let mut owned = OwnedRuntimeFile::acquire(&from, &id, size, &sha).unwrap();
        assert!(OpenOptions::new().write(true).open(&from).is_err());
        owned.move_new(&to, &parent).unwrap();
        assert!(!from.exists());
        assert_eq!(evidence(&to), (id, size, sha));
        owned.revalidate().unwrap();
        assert!(owned
            .move_new(&root.path().join("second.exe"), &parent)
            .is_err());
    }
    #[test]
    fn occupied_destination_is_preserved_and_failed_os_attempt_cannot_replay() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("source.exe");
        let to = root.path().join("occupied.exe");
        std::fs::write(&from, b"source").unwrap();
        std::fs::write(&to, b"preserve other file").unwrap();
        let (id, size, sha) = evidence(&from);
        let parent = crate::source_files::directory_identity(root.path()).unwrap();
        let mut owned = OwnedRuntimeFile::acquire(&from, &id, size, &sha).unwrap();
        assert!(owned.move_new(&to, &parent).is_err());
        assert_eq!(std::fs::read(&to).unwrap(), b"preserve other file");
        assert_eq!(std::fs::read(&from).unwrap(), b"source");
        assert!(owned
            .move_new(&root.path().join("free.exe"), &parent)
            .is_err());
    }
    #[test]
    fn changed_source_parent_and_no_delete_read_pin_reject_before_move() {
        let root = tempfile::tempdir().unwrap();
        let from = root.path().join("source.exe");
        std::fs::write(&from, b"source").unwrap();
        let (id, size, sha) = evidence(&from);
        assert!(OwnedRuntimeFile::acquire(&from, "different identity", size, &sha).is_err());
        assert!(OwnedRuntimeFile::acquire(&from, &id, size + 1, &sha).is_err());
        assert!(OwnedRuntimeFile::acquire(&from, &id, size, &"a".repeat(64)).is_err());
        let read = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&from)
            .unwrap();
        assert!(OwnedRuntimeFile::acquire(&from, &id, size, &sha).is_err());
        drop(read);
        let mut owned = OwnedRuntimeFile::acquire(&from, &id, size, &sha).unwrap();
        assert!(owned
            .move_new(&root.path().join("new.exe"), "wrong parent identity")
            .is_err());
        assert!(from.exists());
        assert!(!root.path().join("new.exe").exists());
    }
}
