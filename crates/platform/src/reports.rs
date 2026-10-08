//! Create-only reports. A successful result follows closed-file readback verification.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};
use winbridge_protocol::SavedDiagnostic;

pub fn publish(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<SavedDiagnostic> {
    if !directory.is_absolute()
        || directory
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || !name.starts_with("diagnostic-")
        || !name.ends_with(".json")
        || name.len() > 160
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.')
        || bytes.len() > 512 * 1024
    {
        return Err(io::Error::other(
            "Invalid diagnostic directory/name or report exceeds 512 KiB",
        ));
    }
    super::require_safe_path(directory)?;
    fs::create_dir_all(directory)?;
    super::require_safe_path(directory)?;
    let path = directory.join(name);
    super::require_safe_path(&path)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    super::require_safe_path(&path)?;
    let actual = fs::read(&path)?;
    if actual != bytes {
        return Err(io::Error::other(
            "Diagnostic readback mismatch; file retained, save not confirmed",
        ));
    }
    Ok(SavedDiagnostic {
        path: path.to_string_lossy().into(),
        bytes: actual.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&actual)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_only_unicode_report_has_exact_bytes_and_hash() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("shared reports");
        let bytes = "{\"text\":\"Привет ✓\"}".as_bytes();
        let result = publish(&dir, "diagnostic-owned.json", bytes).unwrap();
        assert_eq!(result.bytes, bytes.len() as u64);
        assert_eq!(result.sha256, format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(
            publish(&dir, "diagnostic-owned.json", bytes)
                .unwrap()
                .sha256,
            result.sha256
        );
        assert!(publish(&dir, "diagnostic-owned.json", b"changed").is_err());
        assert_eq!(fs::read(result.path).unwrap(), bytes);
    }
    #[test]
    fn invalid_names_and_unavailable_destination_preserve_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let block = temp.path().join("blocked");
        fs::write(&block, b"keep").unwrap();
        assert!(publish(&block, "diagnostic-new.json", b"{}").is_err());
        for name in [
            "../diagnostic-escape.json",
            "diagnostic-escape.json/child",
            "diagnostic-escape.json:ads",
        ] {
            assert!(publish(temp.path(), name, b"{}").is_err());
        }
        assert_eq!(fs::read(block).unwrap(), b"keep");
    }
    #[cfg(unix)]
    #[test]
    fn linked_directory_is_refused_without_writing_outside_the_selected_root() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(publish(&link, "diagnostic-new.json", b"{}").is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
    #[cfg(windows)]
    #[test]
    fn junction_directory_is_refused_without_writing_outside_the_selected_root() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = temp.path().join("link");
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        assert!(publish(&link, "diagnostic-new.json", b"{}").is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::remove_dir(link).unwrap();
    }
}
