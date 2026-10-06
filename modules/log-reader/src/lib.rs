//! Bounded reads of a workflow's registered UTF-8 log; never mutates application files.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use winbridge_protocol::{LogCursor, LogPage};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn read(
    root: &Path,
    relative: &str,
    application_id: &str,
    cursor: Option<&LogCursor>,
    max_bytes: u32,
) -> Result<LogPage, String> {
    winbridge_core::workflow::validate_log_path(relative)?;
    if !(1..=65536).contains(&max_bytes) {
        return Err("Log maxBytes must be 1-65536".into());
    }
    if cursor.is_some_and(|c| {
        c.prefix_bytes > 128
            || c.file_identity.len() > 128
            || c.offset > i64::MAX as u64
            || c.prefix_sha256.len() != 64
            || !c.prefix_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return Err("Invalid log cursor".into());
    }
    let path = root.join(relative);
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    let mut file = File::open(&path).map_err(|e| format!("Log {}: {e}", path.display()))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Log must be a regular file".into());
    }
    let identity = winbridge_platform::file_identity(&file).map_err(|e| e.to_string())?;
    let mut prefix = vec![0; meta.len().min(128) as usize];
    file.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let reset_reason = cursor
        .and_then(|c| {
            if c.file_identity != identity {
                Some("rotated")
            } else if c.offset > meta.len() || usize::from(c.prefix_bytes) > prefix.len() {
                Some("truncated")
            } else if hash(&prefix[..usize::from(c.prefix_bytes)]) != c.prefix_sha256 {
                Some("rewritten")
            } else {
                None
            }
        })
        .map(str::to_owned);
    let offset = if reset_reason.is_some() {
        0
    } else {
        cursor.map_or(0, |c| c.offset)
    };
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(u64::from(max_bytes))
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let next_offset = offset + bytes.len() as u64;
    Ok(LogPage {
        application_id: application_id.into(),
        path: path.to_string_lossy().into(),
        encoding: "utf-8".into(),
        bytes,
        cursor: LogCursor {
            file_identity: identity,
            offset: next_offset,
            prefix_bytes: prefix.len() as u16,
            prefix_sha256: hash(&prefix),
        },
        reset_reason,
        end_of_file: next_offset >= meta.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_pages_preserve_unicode_and_detect_append_truncate_rewrite_rotation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let path = root.join("app.log");
        std::fs::write(&path, "Привет ✓\n").unwrap();
        let first = read(root, "app.log", "app", None, 3).unwrap();
        let next = read(root, "app.log", "app", Some(&first.cursor), 65536).unwrap();
        let all = [first.bytes, next.bytes].concat();
        assert_eq!(String::from_utf8(all).unwrap(), "Привет ✓\n");
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"append\n").unwrap();
        drop(file);
        let appended = read(root, "app.log", "app", Some(&next.cursor), 65536).unwrap();
        assert_eq!(appended.reset_reason, None);
        assert_eq!(appended.bytes, b"append\n");
        std::fs::write(&path, "tiny").unwrap();
        let truncated = read(root, "app.log", "app", Some(&appended.cursor), 65536).unwrap();
        assert_eq!(truncated.reset_reason.as_deref(), Some("truncated"));
        std::fs::write(&path, "same-size-or-larger").unwrap();
        let rewritten = read(root, "app.log", "app", Some(&truncated.cursor), 65536).unwrap();
        assert_eq!(rewritten.reset_reason.as_deref(), Some("rewritten"));
        std::fs::rename(&path, root.join("old.log")).unwrap();
        std::fs::write(&path, "new log").unwrap();
        assert_eq!(
            read(root, "app.log", "app", Some(&rewritten.cursor), 65536)
                .unwrap()
                .reset_reason
                .as_deref(),
            Some("rotated")
        );
        assert!(read(root, "../outside", "app", None, 1).is_err());
        assert!(read(root, "app.log", "app", None, 65537).is_err());
        std::fs::hard_link(&path, root.join("linked.log")).unwrap();
        assert!(read(root, "linked.log", "app", None, 8)
            .unwrap_err()
            .contains("Hard-linked"));
    }
}
