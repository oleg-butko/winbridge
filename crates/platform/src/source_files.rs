//! Local source moves never overwrite a destination or fall back to copy/delete.
use std::{io, path::Path};

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
