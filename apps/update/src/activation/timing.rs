//! Bounded activation diagnostics; never part of a grant or wire view.
use std::{io::Write, time::Instant};

pub struct ActivationTiming {
    operation: &'static str,
    started: Instant,
    previous: Instant,
    deadline: Option<Instant>,
    phase: &'static str,
    hashes: [u64; 3],
}
impl ActivationTiming {
    pub fn new(operation: &'static str, deadline: Option<Instant>) -> Self {
        Self {
            operation,
            started: Instant::now(),
            previous: Instant::now(),
            deadline,
            phase: "begin",
            hashes: crate::artifact::hash_counters(),
        }
    }
    /// Observe a copy for diagnostics only; never replace the preparation window.
    pub fn observe_deadline(&mut self, deadline: Instant) {
        self.deadline = Some(deadline);
    }
    pub fn mark(
        &mut self,
        phase: &'static str,
        file_index: Option<usize>,
        checkpoint: Option<usize>,
    ) {
        let now = Instant::now();
        let hashes = crate::artifact::hash_counters();
        let _ = writeln!(
            std::io::stderr().lock(),
            "WinBridge activation timing {}",
            serde_json::json!({
                "schemaVersion": 1, "event": "checkpoint", "operation": self.operation,
                "phase": phase, "previousPhase": self.phase,
                "elapsedMs": now.duration_since(self.started).as_millis(),
                "stepMs": now.duration_since(self.previous).as_millis(),
                "remainingMs": self.deadline.map(|d| d.saturating_duration_since(now).as_millis()),
                "processId": std::process::id(),
                "processArtifactHashCallsDelta": hashes[0].saturating_sub(self.hashes[0]),
                "processArtifactHashBytesDelta": hashes[1].saturating_sub(self.hashes[1]),
                "processArtifactHashMsDelta": hashes[2].saturating_sub(self.hashes[2]) / 1_000_000,
                "fileIndex": file_index, "checkpoint": checkpoint
            })
        );
        self.previous = now;
        self.phase = phase;
        self.hashes = hashes;
    }
}
impl Drop for ActivationTiming {
    fn drop(&mut self) {
        // This is a timing endpoint on success OR failure, never a success receipt.
        self.mark("trace_end", None, None);
    }
}
