//! Bounded, low-frequency wall-time diagnostics, including time waiting for Store.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Maintenance,
    Recovery,
    Admission,
    Workflows,
    QueueRead,
    Dispatch,
}
const NAMES: [&str; 6] = [
    "maintenance",
    "recovery",
    "admission_wait",
    "workflows",
    "queue_read",
    "dispatch",
];

pub(crate) struct Diagnostics {
    since: Instant,
    ticks: u64,
    total: Duration,
    max: Duration,
    phases: [Duration; 6],
    pending_max: [usize; 3],
    verbose: bool,
}
impl Diagnostics {
    pub(crate) fn new() -> Self {
        Self {
            since: Instant::now(),
            ticks: 0,
            total: Duration::ZERO,
            max: Duration::ZERO,
            phases: [Duration::ZERO; 6],
            pending_max: [0; 3],
            verbose: std::env::var("WINBRIDGE_SCHEDULER_DIAGNOSTICS").as_deref() == Ok("1"),
        }
    }
    fn observe(&mut self, total: Duration, phases: &[Duration; 6], pending: &[usize; 3]) {
        self.ticks += 1;
        self.total += total;
        self.max = self.max.max(total);
        for (sum, value) in self.phases.iter_mut().zip(phases) {
            *sum += *value;
        }
        for (max, value) in self.pending_max.iter_mut().zip(pending) {
            *max = (*max).max(*value);
        }
        let window = self.since.elapsed();
        if window < Duration::from_secs(30) {
            return;
        }
        // Wall occupancy, not CPU usage: contention and slow storage are included.
        let occupancy = self.total.as_secs_f64() / window.as_secs_f64();
        if self.verbose || occupancy >= 0.05 || self.max >= Duration::from_millis(100) {
            let phases = NAMES
                .iter()
                .zip(self.phases)
                .map(|(name, time)| format!("{name}Ms={:.1}", time.as_secs_f64() * 1000.0))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("WinBridge scheduler: windowMs={:.0} ticks={} wallMs={:.1} maxTickMs={:.1} wallPercent={:.2} {} pendingMax(tasks/imports/workflows)={}/{}/{} (wall time includes lock waits; not process CPU)",
                window.as_secs_f64()*1000.0, self.ticks, self.total.as_secs_f64()*1000.0,
                self.max.as_secs_f64()*1000.0, occupancy*100.0, phases,
                self.pending_max[0], self.pending_max[1], self.pending_max[2]);
        }
        self.since = Instant::now();
        self.ticks = 0;
        self.total = Duration::ZERO;
        self.max = Duration::ZERO;
        self.phases = [Duration::ZERO; 6];
        self.pending_max = [0; 3];
    }
}

pub(crate) struct Tick<'a> {
    diagnostics: &'a Mutex<Diagnostics>,
    start: Instant,
    changed: Instant,
    phase: Phase,
    phases: [Duration; 6],
    pending: [usize; 3],
}
impl<'a> Tick<'a> {
    pub(crate) fn new(diagnostics: &'a Mutex<Diagnostics>) -> Self {
        let now = Instant::now();
        Self {
            diagnostics,
            start: now,
            changed: now,
            phase: Phase::Maintenance,
            phases: [Duration::ZERO; 6],
            pending: [0; 3],
        }
    }
    pub(crate) fn phase(&mut self, phase: Phase) {
        let now = Instant::now();
        self.phases[self.phase as usize] += now.duration_since(self.changed);
        self.changed = now;
        self.phase = phase;
    }
    pub(crate) fn pending(&mut self, tasks: usize, imports: usize, workflows: usize) {
        self.pending = [tasks, imports, workflows];
    }
}
impl Drop for Tick<'_> {
    fn drop(&mut self) {
        let now = Instant::now();
        self.phases[self.phase as usize] += now.duration_since(self.changed);
        if let Ok(mut diagnostics) = self.diagnostics.lock() {
            diagnostics.observe(now.duration_since(self.start), &self.phases, &self.pending);
        }
    }
}
