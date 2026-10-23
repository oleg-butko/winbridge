//! Local source moves never overwrite a destination or fall back to copy/delete.
use std::{io, path::Path};

/// Retain every ancestor while creating children. Windows handles deny rename/
/// delete of the chain; other platforms additionally detect rebinding on check.
pub struct PinnedDirectory {
    chain: Vec<(std::path::PathBuf, std::fs::File, String)>,
}
impl PinnedDirectory {
    pub fn acquire(path: &Path, expected_identity: Option<&str>) -> io::Result<Self> {
        crate::require_local_path_prefix(path)?;
        crate::require_safe_path(path)?;
        crate::require_local_disk(path)?;
        let path = path.canonicalize()?;
        let mut chain = Vec::new();
        for directory in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            crate::require_safe_path(directory)?;
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
                    FILE_SHARE_WRITE,
                };
                options
                    .access_mode(FILE_READ_ATTRIBUTES)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
            }
            let file = options.open(directory)?;
            if !file.metadata()?.is_dir() {
                return Err(io::Error::other("Expected local directory"));
            }
            let id = crate::file_identity(&file)?;
            chain.push((directory.to_path_buf(), file, id));
        }
        let pin = Self { chain };
        pin.revalidate()?;
        if expected_identity.is_some_and(|id| id != pin.identity()) {
            return Err(io::Error::other("Directory identity changed"));
        }
        Ok(pin)
    }
    pub fn identity(&self) -> &str {
        &self
            .chain
            .last()
            .expect("canonical directory includes root")
            .2
    }
    pub fn revalidate(&self) -> io::Result<()> {
        for (path, file, id) in &self.chain {
            if !file.metadata()?.is_dir()
                || crate::file_identity(file)? != *id
                || directory_identity(path)? != *id
            {
                return Err(io::Error::other("Pinned directory chain changed"));
            }
        }
        Ok(())
    }
}

pub fn directory_identity(path: &Path) -> io::Result<String> {
    crate::require_local_path_prefix(path)?;
    crate::require_safe_path(path)?;
    crate::require_local_disk(path)?;
    if !path.is_dir() {
        return Err(io::Error::other("Expected a local directory"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        };
        options
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .access_mode(FILE_READ_ATTRIBUTES);
    }
    crate::file_identity(&options.open(path)?)
}
pub fn require_same_volume(a: &Path, b: &Path) -> io::Result<()> {
    if directory_identity(a)?.split(':').next() != directory_identity(b)?.split(':').next() {
        return Err(io::Error::other(
            "Source and transaction storage must use the same local filesystem volume",
        ));
    }
    Ok(())
}

pub fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing parent"))?,
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn move_new(source: &Path, destination: &Path) -> io::Result<()> {
    for path in [source, destination] {
        crate::require_local_path_prefix(path)?;
        crate::require_safe_path(path)?;
        crate::require_local_disk(path)?;
    }
    if !source.is_file() {
        return Err(io::Error::other("Source move requires a regular file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let from = CString::new(source.as_os_str().as_bytes())?;
        let to = CString::new(destination.as_os_str().as_bytes())?;
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        sync_parent(source)?;
        sync_parent(destination)?;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    return Err(io::Error::other(
        "Atomic no-replace source move unsupported on this platform",
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn directory_pin_rejects_wrong_identity_and_denies_or_detects_rebinding() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("parent").join("child");
        std::fs::create_dir_all(&directory).unwrap();
        let id = super::directory_identity(&directory).unwrap();
        assert!(super::PinnedDirectory::acquire(&directory, Some("wrong-identity")).is_err());
        let pin = super::PinnedDirectory::acquire(&directory, Some(&id)).unwrap();
        pin.revalidate().unwrap();
        let moved = root.path().join("moved");
        let result = std::fs::rename(directory.parent().unwrap(), &moved);
        if cfg!(windows) {
            assert!(result.is_err());
            pin.revalidate().unwrap();
        } else {
            result.unwrap();
            assert!(pin.revalidate().is_err());
        }
    }
    #[test]
    fn existing_destination_is_never_replaced_and_new_move_keeps_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        let to = dir.path().join("to");
        std::fs::write(&from, b"original").unwrap();
        std::fs::write(&to, b"user edit").unwrap();
        assert!(super::move_new(&from, &to).is_err());
        assert_eq!(std::fs::read(&from).unwrap(), b"original");
        assert_eq!(std::fs::read(&to).unwrap(), b"user edit");
        let fresh = dir.path().join("fresh");
        super::move_new(&from, &fresh).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read(fresh).unwrap(), b"original");
    }
}
