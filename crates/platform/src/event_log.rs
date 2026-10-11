//! Best-effort desktop diagnostics: producers never wait for filesystem writes.
use fs2::FileExt;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

enum Message {
    Record(Vec<u8>),
    Flush(mpsc::Sender<()>),
}
pub struct EventLog {
    sender: mpsc::SyncSender<Message>,
    path: PathBuf,
    session: String,
    dropped: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
}
impl EventLog {
    pub fn open(root: &Path, limit: u64, archives: u32) -> io::Result<Self> {
        if limit < 8192 || !(1..=10).contains(&archives) {
            return Err(io::Error::other("Invalid event log limits"));
        }
        super::require_safe_path(root)?;
        let existing = root
            .ancestors()
            .find(|path| path.exists())
            .ok_or_else(|| io::Error::other("Event log volume unavailable"))?;
        super::require_local_disk(existing)?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(root)?;
        let writer = RollingFile::open(root, limit, archives)?;
        let path = writer.path.clone();
        let dropped = Arc::new(AtomicU64::new(0));
        let error = Arc::new(Mutex::new(None));
        let (sender, receiver) = mpsc::sync_channel::<Message>(256);
        let writer_dropped = dropped.clone();
        let writer_error = error.clone();
        std::thread::Builder::new()
            .name("desktop-log".into())
            .spawn(move || {
                let mut writer = writer;
                for message in receiver {
                    match message {
                        Message::Record(bytes) => {
                            let result = writer.append(&bytes);
                            if result.is_err() {
                                writer_dropped.fetch_add(1, Ordering::Relaxed);
                            }
                            *writer_error.lock().unwrap_or_else(|e| e.into_inner()) =
                                result.err().map(|e| e.to_string());
                        }
                        Message::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })?;
        Ok(Self {
            sender,
            path,
            session: format!("{}-{}", std::process::id(), millis()),
            dropped,
            error,
        })
    }
    /// Callers supply only diagnostic metadata, never form contents or RPC payloads.
    pub fn record(&self, event: &str, details: Value) {
        let mut bytes = serde_json::to_vec(&json!({"timestampMs":millis(),"session":self.session,"pid":std::process::id(),"event":event,"details":details})).unwrap_or_default();
        bytes.push(b'\n');
        if bytes.len() > 8192 || self.sender.try_send(Message::Record(bytes)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    /// Explicit deadline; use on a background task, never to block the GUI loop.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (sender, receiver) = mpsc::channel();
        self.sender.try_send(Message::Flush(sender)).is_ok()
            && receiver.recv_timeout(timeout).is_ok()
    }
    pub fn status(&self) -> Value {
        json!({"path":self.path,"droppedRecords":self.dropped.load(Ordering::Relaxed),"error":self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()})
    }
}
fn millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn archive(path: &Path, index: u32) -> PathBuf {
    if index == 0 {
        path.to_owned()
    } else {
        path.with_file_name(format!("desktop.{index}.jsonl"))
    }
}
fn open_file(path: &Path) -> io::Result<File> {
    super::require_safe_path(path)?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
struct RollingFile {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
    archives: u32,
    _lock: File,
}
impl RollingFile {
    fn open(root: &Path, limit: u64, archives: u32) -> io::Result<Self> {
        let lock = open_file(&root.join("desktop.lock"))?;
        lock.try_lock_exclusive()?;
        let path = root.join("desktop.jsonl");
        for i in 0..=archives {
            super::require_safe_path(&archive(&path, i))?;
        }
        let mut file = open_file(&path)?;
        let bytes = repair_tail(&mut file)?;
        Ok(Self {
            path,
            file: Some(file),
            bytes,
            limit,
            archives,
            _lock: lock,
        })
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.file.is_none() {
            let mut file = open_file(&self.path)?;
            self.bytes = repair_tail(&mut file)?;
            self.file = Some(file);
        }
        if self.bytes + bytes.len() as u64 > self.limit {
            for i in 0..=self.archives {
                super::require_safe_path(&archive(&self.path, i))?;
            }
            self.file.take();
            let oldest = archive(&self.path, self.archives);
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for i in (0..self.archives).rev() {
                let old = archive(&self.path, i);
                if old.exists() {
                    fs::rename(old, archive(&self.path, i + 1))?;
                }
            }
            self.file = Some(open_file(&self.path)?);
            self.bytes = 0;
        }
        if let Err(e) = self.file.as_mut().expect("open log").write_all(bytes) {
            let file = self.file.take().expect("failed log");
            file.set_len(self.bytes)?;
            return Err(e);
        }
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
fn repair_tail(file: &mut File) -> io::Result<u64> {
    let bytes = file.metadata()?.len();
    let mut end = bytes;
    let mut valid = 0;
    while end > 0 {
        let start = end.saturating_sub(8192);
        file.seek(SeekFrom::Start(start))?;
        let mut tail = vec![0; (end - start) as usize];
        file.read_exact(&mut tail)?;
        if let Some(index) = tail.iter().rposition(|b| *b == b'\n') {
            valid = start + index as u64 + 1;
            break;
        }
        end = start;
    }
    if valid != bytes {
        file.set_len(valid)?;
    }
    file.seek(SeekFrom::Start(valid))?;
    Ok(valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_and_partial_crash_recovery_preserve_complete_records() {
        let temp = tempfile::tempdir().unwrap();
        let mut file = RollingFile::open(temp.path(), 8192, 2).unwrap();
        for i in 0..45 {
            let mut bytes =
                serde_json::to_vec(&json!({"sequence":i,"padding":"x".repeat(800)})).unwrap();
            bytes.push(b'\n');
            file.append(&bytes).unwrap();
        }
        drop(file);
        let path = temp.path().join("desktop.jsonl");
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&vec![b'x'; 9000])
            .unwrap();
        let mut file = RollingFile::open(temp.path(), 8192, 2).unwrap();
        file.append(b"{\"restarted\":true}\n").unwrap();
        drop(file);
        assert!(!archive(&path, 3).exists());
        for i in 0..=2 {
            let text = fs::read_to_string(archive(&path, i)).unwrap();
            assert!(text.len() <= 8192);
            for line in text.lines() {
                serde_json::from_str::<Value>(line).unwrap();
            }
        }
        assert!(fs::read_to_string(path)
            .unwrap()
            .ends_with("{\"restarted\":true}\n"));
    }
    #[test]
    fn producers_drop_when_writer_queue_is_full_instead_of_blocking() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let log = EventLog {
            sender,
            path: PathBuf::from("unused"),
            session: "test".into(),
            dropped: Arc::new(AtomicU64::new(0)),
            error: Arc::new(Mutex::new(None)),
        };
        log.record("first", json!({}));
        log.record("second", json!({}));
        assert_eq!(log.dropped.load(Ordering::Relaxed), 1);
        assert!(!log.flush(Duration::from_millis(10)));
    }
    #[test]
    fn only_one_writer_can_own_the_directory() {
        let temp = tempfile::tempdir().unwrap();
        let _first = RollingFile::open(temp.path(), 8192, 2).unwrap();
        assert!(RollingFile::open(temp.path(), 8192, 2).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rotation_refuses_a_link_instead_of_touching_its_target() {
        let temp = tempfile::tempdir().unwrap();
        let external = temp.path().join("preserve");
        fs::write(&external, b"user data").unwrap();
        let mut file = RollingFile::open(temp.path(), 8192, 2).unwrap();
        file.append(&vec![b'x'; 8192]).unwrap();
        std::os::unix::fs::symlink(&external, temp.path().join("desktop.2.jsonl")).unwrap();
        assert!(file.append(b"new\n").is_err());
        assert_eq!(fs::read(external).unwrap(), b"user data");
    }
}
