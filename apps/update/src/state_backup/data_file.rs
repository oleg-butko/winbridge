//! Read pins for SQLite data; executable image size limits do not apply.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};
pub struct StateBackupFilePin {
    file: File,
    evidence: StateBackupFileEvidence,
}
impl StateBackupFilePin {
    pub fn evidence(&self) -> &StateBackupFileEvidence {
        &self.evidence
    }
    pub fn revalidate(&self) -> Result<()> {
        let current = pin_backup_file(Path::new(&self.evidence.path))?;
        if current.evidence != self.evidence
            || winbridge_platform::file_identity(&self.file).map_err(|e| e.to_string())?
                != self.evidence.file_identity
        {
            return Err("Backup file changed after pinning".into());
        }
        Ok(())
    }
}
pub fn pin_backup_file(path: &Path) -> Result<StateBackupFilePin> {
    let error = |e: std::io::Error| e.to_string();
    winbridge_platform::require_local_path_prefix(path).map_err(error)?;
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    let path = path.canonicalize().map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options.open(&path).map_err(error)?;
    let meta = file.metadata().map_err(error)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_UPDATE_COUNTER {
        return Err("Invalid backup file size/type".into());
    }
    let identity = winbridge_platform::file_identity(&file).map_err(error)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > meta.len() {
            return Err("Backup grew while hashing".into());
        }
        hash.update(&buffer[..n]);
    }
    winbridge_platform::require_safe_path(&path).map_err(error)?;
    if bytes != meta.len()
        || file.metadata().map_err(error)?.len() != bytes
        || winbridge_platform::file_identity(&File::open(&path).map_err(error)?).map_err(error)?
            != identity
    {
        return Err("Backup changed while hashing".into());
    }
    let evidence = StateBackupFileEvidence {
        path: path.to_str().ok_or("Backup path UTF-8")?.into(),
        bytes,
        sha256: format!("{:x}", hash.finalize()),
        file_identity: identity,
    };
    Ok(StateBackupFilePin { file, evidence })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_pin_hashes_full_stream_rejects_empty_and_detects_or_prevents_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("backup.sqlite");
        std::fs::write(&path, []).unwrap();
        assert!(pin_backup_file(&path).is_err());
        let mut bytes = vec![b'a'; 150000];
        *bytes.last_mut().unwrap() = b'b';
        std::fs::write(&path, &bytes).unwrap();
        let pin = pin_backup_file(&path).unwrap();
        assert_eq!(pin.evidence().sha256, crate::sha256(&bytes));
        pin.revalidate().unwrap();
        *bytes.last_mut().unwrap() = b'c';
        let changed = std::fs::write(&path, &bytes);
        if cfg!(windows) {
            assert!(changed.is_err());
            pin.revalidate().unwrap();
        } else {
            changed.unwrap();
            assert!(pin.revalidate().is_err());
        }
    }
}
