//! Explicit native validation diagnostics; never part of a grant or wire view.
use std::time::Instant;

pub(crate) struct ActivationTiming {
    #[cfg(feature = "update-validation")]
    operation: &'static str,
    #[cfg(feature = "update-validation")]
    started: Instant,
    #[cfg(feature = "update-validation")]
    previous: Instant,
    #[cfg(feature = "update-validation")]
    deadline: Option<Instant>,
    #[cfg(feature = "update-validation")]
    phase: &'static str,
}
impl ActivationTiming {
    pub(crate) fn new(operation: &'static str, deadline: Option<Instant>) -> Self {
        let _ = (operation, deadline);
        Self {
            #[cfg(feature = "update-validation")]
            operation,
            #[cfg(feature = "update-validation")]
            started: Instant::now(),
            #[cfg(feature = "update-validation")]
            previous: Instant::now(),
            #[cfg(feature = "update-validation")]
            deadline,
            #[cfg(feature = "update-validation")]
            phase: "begin",
        }
    }
    /// Observe a copy for diagnostics only; never replace the preparation window.
    pub(crate) fn observe_deadline(&mut self, deadline: Instant) {
        let _ = deadline;
        #[cfg(feature = "update-validation")]
        {
            self.deadline = Some(deadline);
        }
    }
    pub(crate) fn mark(
        &mut self,
        phase: &'static str,
        file_index: Option<usize>,
        checkpoint: Option<usize>,
    ) {
        let _ = (phase, file_index, checkpoint);
        #[cfg(feature = "update-validation")]
        {
            let now = Instant::now();
            eprintln!(
                "WinBridge activation timing {}",
                serde_json::json!({
                    "schemaVersion": 1, "event": "checkpoint", "operation": self.operation,
                    "phase": phase, "previousPhase": self.phase,
                    "elapsedMs": now.duration_since(self.started).as_millis(),
                    "stepMs": now.duration_since(self.previous).as_millis(),
                    "remainingMs": self.deadline.map(|d| d.saturating_duration_since(now).as_millis()),
                    "fileIndex": file_index, "checkpoint": checkpoint
                })
            );
            self.previous = now;
            self.phase = phase;
        }
    }
}
#[cfg(feature = "update-validation")]
impl Drop for ActivationTiming {
    fn drop(&mut self) {
        // This is a timing endpoint on success OR failure, never a success receipt.
        self.mark("trace_end", None, None);
    }
}
