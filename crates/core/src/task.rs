//! Pure task lifecycle decisions. The worker commits a decision before acting on it.
//! Internal states are projected into the existing v1 record; no new wire/schema fields.
use std::fmt;
use winbridge_protocol::{
    JournalEvent, RecipeOrigin, ResolvedCommand, TaskRecord, TaskStatus, TaskSubmission,
    VerifiedSnapshotRef,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    Indeterminate,
}

impl TaskOutcome {
    pub fn status(self) -> TaskStatus {
        match self {
            Self::Succeeded => TaskStatus::Succeeded,
            Self::Failed => TaskStatus::Failed,
            Self::Cancelled => TaskStatus::Cancelled,
            Self::Interrupted => TaskStatus::Interrupted,
            Self::Indeterminate => TaskStatus::Indeterminate,
        }
    }
}

impl TryFrom<TaskStatus> for TaskOutcome {
    type Error = TransitionError;

    fn try_from(status: TaskStatus) -> Result<Self, Self::Error> {
        match status {
            TaskStatus::Succeeded => Ok(Self::Succeeded),
            TaskStatus::Failed => Ok(Self::Failed),
            TaskStatus::Cancelled => Ok(Self::Cancelled),
            TaskStatus::Interrupted => Ok(Self::Interrupted),
            TaskStatus::Indeterminate => Ok(Self::Indeterminate),
            TaskStatus::Queued | TaskStatus::Running => {
                Err(TransitionError::NonTerminalCompletion(status))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Queued,
    Starting {
        cancellation_requested: bool,
    },
    Running {
        process_id: u32,
        cancellation_requested: bool,
    },
    Terminal(TaskOutcome),
}

impl TaskState {
    pub fn from_record(record: &TaskRecord) -> Self {
        match record.status {
            TaskStatus::Queued => Self::Queued,
            TaskStatus::Running => match record.process_id {
                None => Self::Starting {
                    cancellation_requested: record.cancel_requested,
                },
                Some(process_id) => Self::Running {
                    process_id,
                    cancellation_requested: record.cancel_requested,
                },
            },
            TaskStatus::Succeeded => Self::Terminal(TaskOutcome::Succeeded),
            TaskStatus::Failed => Self::Terminal(TaskOutcome::Failed),
            TaskStatus::Cancelled => Self::Terminal(TaskOutcome::Cancelled),
            TaskStatus::Interrupted => Self::Terminal(TaskOutcome::Interrupted),
            TaskStatus::Indeterminate => Self::Terminal(TaskOutcome::Indeterminate),
        }
    }

    pub fn status(self) -> TaskStatus {
        match self {
            Self::Queued => TaskStatus::Queued,
            Self::Starting { .. } | Self::Running { .. } => TaskStatus::Running,
            Self::Terminal(outcome) => outcome.status(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TaskEvent {
    LaunchIntent,
    ProcessStarted {
        process_id: u32,
    },
    CancellationRequested,
    Completed {
        outcome: TaskOutcome,
        exit_code: Option<i32>,
        reason: String,
    },
    Recovered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEventKind {
    LaunchIntent,
    ProcessStarted,
    CancellationRequested,
    Completed,
    Recovered,
}

impl TaskEvent {
    fn kind(&self) -> TaskEventKind {
        match self {
            Self::LaunchIntent => TaskEventKind::LaunchIntent,
            Self::ProcessStarted { .. } => TaskEventKind::ProcessStarted,
            Self::CancellationRequested => TaskEventKind::CancellationRequested,
            Self::Completed { .. } => TaskEventKind::Completed,
            Self::Recovered => TaskEventKind::Recovered,
        }
    }
}

/// A proposed effect, not permission to bypass a successful journal commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEffect {
    None,
    RecordObservation,
    ExecuteAcceptedCommand,
    RequestExecutionCancellation,
    RecordTerminalOutcome,
}

#[derive(Clone, Debug)]
pub struct TaskTransition {
    pub record: TaskRecord,
    pub effect: TaskEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition {
        state: TaskState,
        event: TaskEventKind,
    },
    NonTerminalCompletion(TaskStatus),
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { state, event } => {
                write!(formatter, "task lifecycle rejects {event:?} in {state:?}")
            }
            Self::NonTerminalCompletion(status) => {
                write!(
                    formatter,
                    "task completion requires a terminal outcome, got {status:?}"
                )
            }
        }
    }
}
impl std::error::Error for TransitionError {}

pub fn accepted_record(
    task_id: String,
    submission: TaskSubmission,
    resolved_command: ResolvedCommand,
    snapshot: Option<VerifiedSnapshotRef>,
    recipe_origin: Option<RecipeOrigin>,
    now_ms: u64,
) -> TaskRecord {
    TaskRecord {
        task_id,
        submission,
        resolved_command,
        recipe_origin,
        snapshot,
        status: TaskStatus::Queued,
        accepted_at_ms: now_ms,
        started_at_ms: None,
        finished_at_ms: None,
        process_id: None,
        exit_code: None,
        reason: None,
        cancel_requested: false,
        output_bytes: 0,
        output_truncated: false,
        output_removed_at_ms: None,
        removed_output_bytes: 0,
        events: vec![JournalEvent {
            sequence: 1,
            occurred_at_ms: now_ms,
            status: TaskStatus::Queued,
            summary: "Task accepted and committed to the local journal".into(),
        }],
    }
}

fn append_event(record: &mut TaskRecord, now_ms: u64, summary: impl Into<String>) {
    record.events.push(JournalEvent {
        sequence: record.events.len() as u64 + 1,
        occurred_at_ms: now_ms,
        status: record.status,
        summary: summary.into(),
    });
}

fn complete(
    record: &mut TaskRecord,
    now_ms: u64,
    outcome: TaskOutcome,
    exit_code: Option<i32>,
    reason: String,
) {
    record.status = outcome.status();
    record.exit_code = exit_code;
    record.finished_at_ms = Some(now_ms);
    record.reason = Some(reason.clone());
    append_event(record, now_ms, reason);
}

/// Deterministic and side-effect free. `now_ms` is supplied by the execution authority.
/// Legacy completion before a PID observation is retained (e.g. admission/launch failure).
pub fn reduce(
    current: &TaskRecord,
    event: TaskEvent,
    now_ms: u64,
) -> Result<TaskTransition, TransitionError> {
    let state = TaskState::from_record(current);
    let invalid = TransitionError::InvalidTransition {
        state,
        event: event.kind(),
    };
    let mut record = current.clone();
    let effect = match event {
        TaskEvent::LaunchIntent => {
            if state != TaskState::Queued {
                return Err(invalid);
            }
            record.status = TaskStatus::Running;
            append_event(
                &mut record,
                now_ms,
                "Launch intent committed; awaiting process creation",
            );
            TaskEffect::ExecuteAcceptedCommand
        }
        TaskEvent::ProcessStarted { process_id } => {
            match state {
                TaskState::Starting { .. } => {}
                TaskState::Running {
                    process_id: observed,
                    ..
                } if observed == process_id => {
                    return Ok(TaskTransition {
                        record,
                        effect: TaskEffect::None,
                    });
                }
                _ => return Err(invalid),
            }
            record.process_id = Some(process_id);
            record.started_at_ms = Some(now_ms);
            append_event(
                &mut record,
                now_ms,
                format!("Process {process_id} started; stdout/stderr are drained independently"),
            );
            TaskEffect::RecordObservation
        }
        TaskEvent::CancellationRequested => {
            if current.status.is_terminal() || current.cancel_requested {
                TaskEffect::None
            } else if state == TaskState::Queued {
                complete(
                    &mut record,
                    now_ms,
                    TaskOutcome::Cancelled,
                    None,
                    "Cancelled before process launch".into(),
                );
                TaskEffect::RecordTerminalOutcome
            } else {
                record.cancel_requested = true;
                append_event(
                    &mut record,
                    now_ms,
                    "Cancellation requested; waiting for managed process tree termination",
                );
                TaskEffect::RequestExecutionCancellation
            }
        }
        TaskEvent::Completed {
            outcome,
            exit_code,
            reason,
        } => {
            if current.status.is_terminal() {
                TaskEffect::None
            } else {
                complete(&mut record, now_ms, outcome, exit_code, reason);
                TaskEffect::RecordTerminalOutcome
            }
        }
        TaskEvent::Recovered => {
            if matches!(
                state,
                TaskState::Starting { .. } | TaskState::Running { .. }
            ) {
                complete(&mut record, now_ms, TaskOutcome::Indeterminate, None,
                    "Worker restarted after launch intent; external command outcome is unknown. Automatic retry is disabled".into());
                TaskEffect::RecordTerminalOutcome
            } else {
                TaskEffect::None
            }
        }
    };
    Ok(TaskTransition { record, effect })
}
