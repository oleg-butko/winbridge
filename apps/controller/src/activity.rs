//! Bounded, best-effort activity recording, independent of WebSocket progress.
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use winbridge_protocol::ActivityLogStatus;

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const ARCHIVES: u32 = 5;
const QUEUE_SIZE: usize = 256;
const MAX_RECORD_BYTES: usize = 8192;

#[derive(Default)]
struct Health {
    dropped_records: u64,
    last_error: Option<String>,
}
pub struct ActivityLog {
    sender: Option<mpsc::SyncSender<Vec<u8>>>,
    health: Arc<Mutex<Health>>,
    done: Mutex<mpsc::Receiver<()>>,
    path: PathBuf,
}
impl ActivityLog {
    pub fn open(root: &Path) -> Result<Self, String> {
        private_directory(root)?;
        let writer = RollingFile::open(root, MAX_FILE_BYTES, ARCHIVES)
            .map_err(|e| format!("controller activity log: {e}"))?;
        let path = writer.path.clone();
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(QUEUE_SIZE);
        let (finished, done) = mpsc::channel();
        let health = Arc::new(Mutex::new(Health {
            dropped_records: u64::from(writer.repaired_tail),
            last_error: None,
        }));
        if writer.repaired_tail {
            eprintln!("WinBridge activity log recovered an incomplete last record; see droppedRecords in controller status");
        }
        let writer_health = Arc::clone(&health);
        std::thread::Builder::new()
            .name("winbridge-activity".into())
            .spawn(move || {
                let mut writer = writer;
                let mut reported_drops = 0;
                for bytes in receiver {
                    let result = writer.append(&bytes);
                    let mut health = writer_health.lock().unwrap_or_else(|e| e.into_inner());
                    let mut warning = None;
                    match result {
                        Ok(()) => health.last_error = None,
                        Err(error) => {
                            let message = error.to_string();
                            if health.last_error.as_ref() != Some(&message) {
                                warning = Some(message.clone());
                            }
                            health.last_error = Some(message);
                            health.dropped_records = health.dropped_records.saturating_add(1);
                        }
                    }
                    let dropped_records = health.dropped_records;
                    drop(health);
                    // Even a blocked stderr must not hold the status/producer lock.
                    if let Some(message) = warning {
                        eprintln!("WinBridge activity log failed: {message}; see controller status");
                    }
                    if dropped_records > reported_drops {
                        eprintln!("WinBridge activity log dropped {dropped_records} records; see controller status");
                        reported_drops = dropped_records;
                    }
                }
                drop(writer);
                let _ = finished.send(());
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            health,
            done: Mutex::new(done),
            path,
        })
    }
    pub fn record(&self, mut fields: serde_json::Value) {
        fields["timestampMs"] = serde_json::json!(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64);
        let Ok(mut bytes) = serde_json::to_vec(&fields) else {
            return;
        };
        bytes.push(b'\n');
        if bytes.len() <= MAX_RECORD_BYTES
            && self
                .sender
                .as_ref()
                .is_some_and(|sender| sender.try_send(bytes).is_ok())
        {
            return;
        }
        let mut health = self.health.lock().unwrap_or_else(|e| e.into_inner());
        health.dropped_records = health.dropped_records.saturating_add(1);
    }
    pub fn status(&self) -> ActivityLogStatus {
        let health = self.health.lock().unwrap_or_else(|e| e.into_inner());
        ActivityLogStatus {
            path: self.path.to_string_lossy().into_owned(),
            max_file_bytes: MAX_FILE_BYTES,
            retained_archives: ARCHIVES,
            dropped_records: health.dropped_records,
            last_error: health.last_error.clone(),
        }
    }
}
impl Drop for ActivityLog {
    fn drop(&mut self) {
        // Drain on normal shutdown, but never wait indefinitely for a failed disk.
        self.sender.take();
        if self
            .done
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(Duration::from_secs(2))
            .is_err()
        {
            eprintln!(
                "WinBridge activity log shutdown deadline exceeded; queued records may be lost"
            );
        }
    }
}

pub fn private_directory(root: &Path) -> Result<(), String> {
    if !root.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(root)
            .map_err(|e| format!("controller stateDirectory: {e}"))?;
    }
    let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("controller stateDirectory must be a real private local directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        if metadata.uid() != unsafe { getuid() } || metadata.permissions().mode() & 0o077 != 0 {
            return Err(
                "controller stateDirectory must be owned by this user with permissions 0700".into(),
            );
        }
    }
    Ok(())
}

struct RollingFile {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
    archives: u32,
    repaired_tail: bool,
    _lock: File,
}
impl RollingFile {
    fn open(root: &Path, limit: u64, archives: u32) -> io::Result<Self> {
        let lock = private_file(&root.join("activity.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|e| io::Error::other(format!("activity log is already owned: {e}")))?;
        let path = root.join("activity.jsonl");
        for index in 0..=archives {
            validate_file(&archive_path(&path, index))?;
        }
        let mut file = private_file(&path)?;
        let repaired_tail = repair_tail(&mut file)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            bytes,
            limit,
            archives,
            repaired_tail,
            _lock: lock,
        })
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() as u64 > self.limit {
            return Err(io::Error::other("activity record exceeds file size limit"));
        }
        if self.file.is_none() {
            let mut file = private_file(&self.path)?;
            repair_tail(&mut file)?;
            self.bytes = file.metadata()?.len();
            self.file = Some(file);
        }
        if self.bytes + bytes.len() as u64 > self.limit {
            for index in 0..=self.archives {
                validate_file(&archive_path(&self.path, index))?;
            }
            self.file.take();
            let oldest = archive_path(&self.path, self.archives);
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for index in (0..self.archives).rev() {
                let source = archive_path(&self.path, index);
                if source.exists() {
                    fs::rename(source, archive_path(&self.path, index + 1))?;
                }
            }
            self.file = Some(private_file(&self.path)?);
            self.bytes = 0;
        }
        if let Err(error) = self.file.as_mut().expect("open log").write_all(bytes) {
            let file = self.file.take().expect("failed log");
            file.set_len(self.bytes).map_err(|rollback| {
                io::Error::other(format!("{error}; cannot remove partial record: {rollback}"))
            })?;
            return Err(error);
        }
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
// A crash can leave half a JSON line. Preserve complete records before appending.
fn repair_tail(file: &mut File) -> io::Result<bool> {
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0];
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(false);
    }
    let mut end = length;
    let mut buffer = [0; MAX_RECORD_BYTES];
    while end > 0 {
        let count = end.min(buffer.len() as u64) as usize;
        let start = end - count as u64;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
            file.set_len(start + index as u64 + 1)?;
            return Ok(true);
        }
        end = start;
    }
    file.set_len(0)?;
    Ok(true)
}
fn archive_path(path: &Path, index: u32) -> PathBuf {
    if index == 0 {
        path.into()
    } else {
        path.with_file_name(format!("activity.jsonl.{index}"))
    }
}
fn validate_file(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(format!(
            "{} is not a regular log file; preserved",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        if metadata.uid() != unsafe { getuid() }
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(io::Error::other(format!(
                "{} must be a private file owned by this user; preserved",
                path.display()
            )));
        }
    }
    Ok(())
}
fn private_file(path: &Path) -> io::Result<File> {
    validate_file(path)?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_is_bounded_and_keeps_latest_records_after_reopen() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = RollingFile::open(root.path(), 48, 5).unwrap();
        assert!(RollingFile::open(root.path(), 48, 5).is_err());
        for index in 0..50 {
            writer
                .append(format!("{{\"event\":{index:02}}}\n").as_bytes())
                .unwrap();
        }
        drop(writer);
        let mut writer = RollingFile::open(root.path(), 48, 5).unwrap();
        writer.append(b"{\"event\":50}\n").unwrap();
        drop(writer);
        let mut events = Vec::new();
        for index in (0..=5).rev() {
            let path = archive_path(&root.path().join("activity.jsonl"), index);
            let bytes = fs::read(path).unwrap();
            assert!(bytes.len() <= 48);
            for line in String::from_utf8(bytes).unwrap().lines() {
                events.push(
                    serde_json::from_str::<serde_json::Value>(line).unwrap()["event"]
                        .as_u64()
                        .unwrap(),
                );
            }
        }
        assert_eq!(events.last(), Some(&50));
        assert!(events.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert!(!root.path().join("activity.jsonl.6").exists());
    }
    #[cfg(unix)]
    #[test]
    fn rotation_refuses_symlinks_and_preserves_their_targets() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("important");
        fs::write(&target, "preserve").unwrap();
        let mut writer = RollingFile::open(root.path(), 24, 5).unwrap();
        writer.append(b"{\"event\":1}\n").unwrap();
        std::os::unix::fs::symlink(&target, root.path().join("activity.jsonl.5")).unwrap();
        assert!(writer.append(b"{\"event\":23456}\n").is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "preserve");
        assert!(RollingFile::open(root.path(), 24, 5).is_err());
    }
    #[test]
    fn shutdown_drains_records_without_serializing_command_payload() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("state");
        let log = ActivityLog::open(&directory).unwrap();
        log.record(serde_json::json!({"event":"tcp_accepted","sourceAddress":"127.0.0.2:1234"}));
        assert_eq!(log.status().dropped_records, 0);
        drop(log);
        let record: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(directory.join("activity.jsonl")).unwrap())
                .unwrap();
        assert_eq!(record["sourceAddress"], "127.0.0.2:1234");
        assert!(record["timestampMs"].as_u64().unwrap() > 0);
    }
    #[cfg(unix)]
    #[test]
    fn write_failure_is_visible_and_recording_recovers_after_path_is_fixed() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("state");
        private_directory(&directory).unwrap();
        let mut full_record = vec![b' '; MAX_FILE_BYTES as usize];
        *full_record.last_mut().unwrap() = b'\n';
        private_file(&directory.join("activity.jsonl"))
            .unwrap()
            .write_all(&full_record)
            .unwrap();
        let log = ActivityLog::open(&directory).unwrap();
        let archive = directory.join("activity.jsonl.5");
        std::os::unix::fs::symlink(root.path().join("untouched"), &archive).unwrap();
        log.record(serde_json::json!({"event":"failed_record"}));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while log.status().dropped_records == 0 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(log.status().last_error.is_some());
        assert!(!root.path().join("untouched").exists());
        fs::remove_file(archive).unwrap();
        log.record(serde_json::json!({"event":"recovered_record"}));
        while log.status().last_error.is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(log.status().dropped_records, 1);
        drop(log);
        let record: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(directory.join("activity.jsonl")).unwrap())
                .unwrap();
        assert_eq!(record["event"], "recovered_record");
    }
    #[test]
    fn restart_preserves_complete_lines_and_discards_only_crash_fragment() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("state");
        private_directory(&directory).unwrap();
        private_file(&directory.join("activity.jsonl"))
            .unwrap()
            .write_all(b"{\"event\":\"complete\"}\n{\"event\":\"half")
            .unwrap();
        let log = ActivityLog::open(&directory).unwrap();
        assert_eq!(log.status().dropped_records, 1);
        log.record(serde_json::json!({"event":"after_restart"}));
        drop(log);
        let lines: Vec<_> = fs::read_to_string(directory.join("activity.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["event"], "complete");
        assert_eq!(lines[1]["event"], "after_restart");
    }
}
