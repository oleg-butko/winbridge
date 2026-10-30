//! Physical custody only: moving this owner never detaches or clones a Job.
//! No file/config pins, journal authority, deserialization or keeper permission.
use super::Result;
use std::{
    io::Read,
    sync::{Arc, Mutex},
    thread,
};
use winbridge_platform::{ManagedChild, OutputReader};
use winbridge_transport::supervisor as ipc;

#[derive(Default)]
pub(super) struct Capture {
    pub(super) bytes: Vec<u8>,
    pub(super) incomplete: bool,
}
pub(super) fn drain(
    reader: OutputReader,
    capture: Arc<Mutex<Capture>>,
) -> Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("activation-output".into())
        .spawn(move || {
            let mut reader = reader;
            let mut chunk = [0; 4096];
            loop {
                let result = reader.read(&mut chunk);
                let mut state = capture.lock().unwrap_or_else(|p| p.into_inner());
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        let keep = n.min(65536usize.saturating_sub(state.bytes.len()));
                        state.bytes.extend_from_slice(&chunk[..keep]);
                        state.incomplete |= keep != n;
                    }
                    Err(_) => {
                        state.incomplete = true;
                        break;
                    }
                }
            }
        })
        .map_err(|_| "Activation output reader could not start".into())
}
/// Declaration order in the outer guard drops this before verification pins.
/// Drop requests tree termination; it does not claim a positive empty Job.
pub(super) struct ProcessCustody {
    pub(super) child: Option<ManagedChild>,
    pub(super) channel: Option<ipc::Stream>,
    pub(super) captures: [Arc<Mutex<Capture>>; 2],
    pub(super) readers: Vec<thread::JoinHandle<()>>,
}
impl ProcessCustody {
    pub(super) fn output(&self) -> (Vec<u8>, Vec<u8>, bool) {
        let stdout = self.captures[0].lock().unwrap_or_else(|p| p.into_inner());
        let stderr = self.captures[1].lock().unwrap_or_else(|p| p.into_inner());
        (
            stdout.bytes.clone(),
            stderr.bytes.clone(),
            stdout.incomplete || stderr.incomplete,
        )
    }
    pub(super) fn diagnostics(&self) -> (String, String, bool, bool) {
        fn tail(bytes: &[u8]) -> (String, bool) {
            let text = String::from_utf8_lossy(bytes);
            let count = text.chars().count();
            (
                text.chars().skip(count.saturating_sub(512)).collect(),
                count > 512,
            )
        }
        let (stdout, stderr, incomplete) = self.output();
        let (stdout, cut_out) = tail(&stdout);
        let (stderr, cut_err) = tail(&stderr);
        (
            stdout,
            stderr,
            incomplete || cut_out || cut_err,
            self.readers.len() == 2 && self.readers.iter().all(|r| r.is_finished()),
        )
    }
    pub(super) fn new() -> Self {
        Self {
            child: None,
            channel: None,
            captures: [
                Arc::new(Mutex::new(Capture::default())),
                Arc::new(Mutex::new(Capture::default())),
            ],
            readers: Vec::new(),
        }
    }
}
impl Drop for ProcessCustody {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            child.close_unconfirmed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{self, Cursor},
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
    };

    struct Burst {
        gate: Option<mpsc::Receiver<()>>,
        bytes: Cursor<Vec<u8>>,
        consumed: Arc<AtomicUsize>,
    }
    impl Read for Burst {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if let Some(gate) = self.gate.take() {
                gate.recv().map_err(io::Error::other)?;
            }
            let n = self.bytes.read(out)?;
            self.consumed.fetch_add(n, Ordering::Relaxed);
            Ok(n)
        }
    }
    #[test]
    fn both_readers_survive_owner_move_and_keep_draining_after_capture_limit() {
        let mut owner = ProcessCustody::new();
        let mut gates = vec![];
        let counts = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
        for (i, count) in counts.iter().enumerate() {
            let (tx, rx) = mpsc::channel();
            gates.push(tx);
            owner.readers.push(
                drain(
                    Box::new(Burst {
                        gate: Some(rx),
                        bytes: Cursor::new(vec![b'a' + i as u8; 1024 * 1024]),
                        consumed: Arc::clone(count),
                    }),
                    Arc::clone(&owner.captures[i]),
                )
                .unwrap(),
            );
        }
        let mut moved = std::hint::black_box(owner);
        for gate in gates {
            gate.send(()).unwrap();
        }
        for reader in moved.readers.drain(..) {
            reader.join().unwrap();
        }
        for (i, count) in counts.iter().enumerate() {
            assert_eq!(
                count.load(Ordering::Relaxed),
                1024 * 1024,
                "A full capture must not backpressure either pipe"
            );
            let data = moved.captures[i].lock().unwrap();
            assert_eq!(data.bytes, vec![b'a' + i as u8; 65536]);
            assert!(data.incomplete);
        }
    }
    #[test]
    fn read_error_retains_previous_bytes_and_never_reports_complete_output() {
        struct Fails(bool);
        impl Read for Fails {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("fixture read failure"));
                }
                self.0 = true;
                out[..3].copy_from_slice(b"abc");
                Ok(3)
            }
        }
        let capture = Arc::new(Mutex::new(Capture::default()));
        drain(Box::new(Fails(false)), Arc::clone(&capture))
            .unwrap()
            .join()
            .unwrap();
        let data = capture.lock().unwrap();
        assert_eq!(data.bytes, b"abc");
        assert!(data.incomplete);
    }
    #[test]
    fn clean_eof_preserves_unicode_bytes_without_claiming_truncation() {
        let capture = Arc::new(Mutex::new(Capture::default()));
        let bytes = "stdout Привет ✓\n".as_bytes().to_vec();
        drain(Box::new(Cursor::new(bytes.clone())), Arc::clone(&capture))
            .unwrap()
            .join()
            .unwrap();
        let data = capture.lock().unwrap();
        assert_eq!(data.bytes, bytes);
        assert!(!data.incomplete);
    }
}
