use serde_json::Value;
use winbridge_core::task::{self, TaskEffect, TaskEvent, TaskOutcome, TaskState};
use winbridge_protocol::{TaskRecord, TaskStatus};

fn queued() -> TaskRecord {
    let legacy: TaskRecord = serde_json::from_str(include_str!(
        "../../../tests/fixtures/legacy-v1/task-record.json"
    ))
    .unwrap();
    task::accepted_record(
        legacy.task_id,
        legacy.submission,
        legacy.resolved_command,
        None,
        None,
        1000,
    )
}
fn starting() -> TaskRecord {
    task::reduce(&queued(), TaskEvent::LaunchIntent, 1001)
        .unwrap()
        .record
}
fn running() -> TaskRecord {
    task::reduce(
        &starting(),
        TaskEvent::ProcessStarted { process_id: 4242 },
        1002,
    )
    .unwrap()
    .record
}
fn outcome(record: &TaskRecord, outcome: TaskOutcome) -> TaskRecord {
    task::reduce(
        record,
        TaskEvent::Completed {
            outcome,
            exit_code: Some(0),
            reason: "Confirmed result".into(),
        },
        1005,
    )
    .unwrap()
    .record
}
fn value(record: &TaskRecord) -> Value {
    serde_json::to_value(record).unwrap()
}

#[test]
fn starting_and_running_project_into_v1_without_new_wire_fields() {
    let intent = starting();
    assert_eq!(
        TaskState::from_record(&intent),
        TaskState::Starting {
            cancellation_requested: false
        }
    );
    assert_eq!(intent.status, TaskStatus::Running);
    assert!(intent.process_id.is_none());
    assert!(intent.started_at_ms.is_none());
    let observed = running();
    assert_eq!(
        TaskState::from_record(&observed),
        TaskState::Running {
            process_id: 4242,
            cancellation_requested: false
        }
    );
    assert_eq!(observed.status, intent.status);
    assert_eq!(observed.started_at_ms, Some(1002));
    assert_eq!(
        value(&intent)
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        value(&queued())
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        intent
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(observed.events.last().unwrap().sequence, 3);
}

#[test]
fn launch_and_observation_reject_wrong_phases_without_mutating_input() {
    for record in [
        starting(),
        running(),
        outcome(&running(), TaskOutcome::Failed),
    ] {
        let original = value(&record);
        assert!(task::reduce(&record, TaskEvent::LaunchIntent, 1006).is_err());
        assert_eq!(value(&record), original);
    }
    for record in [
        queued(),
        running(),
        outcome(&running(), TaskOutcome::Succeeded),
    ] {
        let original = value(&record);
        assert!(task::reduce(
            &record,
            TaskEvent::ProcessStarted { process_id: 9999 },
            1006
        )
        .is_err());
        assert_eq!(value(&record), original);
    }
    let record = running();
    let duplicate = task::reduce(
        &record,
        TaskEvent::ProcessStarted { process_id: 4242 },
        1006,
    )
    .unwrap();
    assert_eq!(duplicate.effect, TaskEffect::None);
    assert_eq!(value(&duplicate.record), value(&record));
}

#[test]
fn cancellation_has_distinct_effects_before_and_after_launch() {
    let cancelled = task::reduce(&queued(), TaskEvent::CancellationRequested, 1003).unwrap();
    assert_eq!(cancelled.record.status, TaskStatus::Cancelled);
    assert_eq!(cancelled.effect, TaskEffect::RecordTerminalOutcome);
    assert_eq!(cancelled.record.finished_at_ms, Some(1003));
    for record in [starting(), running()] {
        let cancelled = task::reduce(&record, TaskEvent::CancellationRequested, 1003).unwrap();
        assert_eq!(cancelled.effect, TaskEffect::RequestExecutionCancellation);
        assert_eq!(cancelled.record.status, TaskStatus::Running);
        assert!(cancelled.record.cancel_requested);
        assert!(cancelled.record.finished_at_ms.is_none());
        let duplicate =
            task::reduce(&cancelled.record, TaskEvent::CancellationRequested, 1004).unwrap();
        assert_eq!(duplicate.effect, TaskEffect::None);
        assert_eq!(value(&duplicate.record), value(&cancelled.record));
    }
}

#[test]
fn pid_observation_does_not_erase_cancellation_between_launch_and_spawn() {
    let cancelled = task::reduce(&starting(), TaskEvent::CancellationRequested, 1003)
        .unwrap()
        .record;
    let observed = task::reduce(
        &cancelled,
        TaskEvent::ProcessStarted { process_id: 4242 },
        1004,
    )
    .unwrap();
    assert_eq!(observed.record.status, TaskStatus::Running);
    assert!(observed.record.cancel_requested);
    assert!(observed.record.finished_at_ms.is_none());
    assert_eq!(
        observed.record.events.last().unwrap().sequence,
        cancelled.events.len() as u64 + 1
    );
}

#[test]
fn confirmed_exit_can_win_the_cancel_race_and_terminal_results_remain_immutable() {
    let cancellation = task::reduce(&running(), TaskEvent::CancellationRequested, 1003)
        .unwrap()
        .record;
    // The existing execution loop observes try_wait before checking cancel/timeout.
    let success = outcome(&cancellation, TaskOutcome::Succeeded);
    assert_eq!(success.status, TaskStatus::Succeeded);
    assert!(success.cancel_requested);
    for record in [success, outcome(&cancellation, TaskOutcome::Cancelled)] {
        for event in [
            TaskEvent::CancellationRequested,
            TaskEvent::Completed {
                outcome: TaskOutcome::Failed,
                exit_code: Some(1),
                reason: "Late completion".into(),
            },
            TaskEvent::Recovered,
        ] {
            let duplicate = task::reduce(&record, event, 9000).unwrap();
            assert_eq!(duplicate.effect, TaskEffect::None);
            assert_eq!(value(&duplicate.record), value(&record));
        }
    }
}

#[test]
fn recovery_never_proposes_relaunch_and_does_not_rewrite_resolved_records() {
    for record in [starting(), running()] {
        let recovered = task::reduce(&record, TaskEvent::Recovered, 1006).unwrap();
        assert_eq!(recovered.effect, TaskEffect::RecordTerminalOutcome);
        assert_eq!(recovered.record.status, TaskStatus::Indeterminate);
        assert_eq!(recovered.record.process_id, record.process_id);
        assert_eq!(recovered.record.resolved_command, record.resolved_command);
        assert!(recovered
            .record
            .reason
            .unwrap()
            .contains("Automatic retry is disabled"));
    }
    let mut settled = vec![queued()];
    for outcome_kind in [
        TaskOutcome::Succeeded,
        TaskOutcome::Failed,
        TaskOutcome::Cancelled,
        TaskOutcome::Interrupted,
        TaskOutcome::Indeterminate,
    ] {
        settled.push(outcome(&running(), outcome_kind));
    }
    for record in settled {
        let recovered = task::reduce(&record, TaskEvent::Recovered, 1006).unwrap();
        assert_eq!(recovered.effect, TaskEffect::None);
        assert_eq!(value(&recovered.record), value(&record));
    }
}

#[test]
fn terminal_outcomes_exclude_queued_and_running_and_keep_legacy_prelaunch_finish() {
    assert!(TaskOutcome::try_from(TaskStatus::Queued).is_err());
    assert!(TaskOutcome::try_from(TaskStatus::Running).is_err());
    for status in [
        TaskStatus::Succeeded,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
        TaskStatus::Interrupted,
        TaskStatus::Indeterminate,
    ] {
        let outcome_kind = TaskOutcome::try_from(status).unwrap();
        assert_eq!(outcome_kind.status(), status);
        let completed = task::reduce(
            &queued(),
            TaskEvent::Completed {
                outcome: outcome_kind,
                exit_code: None,
                reason: "Legacy completion".into(),
            },
            1005,
        )
        .unwrap();
        assert_eq!(completed.record.status, status);
        assert_eq!(completed.effect, TaskEffect::RecordTerminalOutcome);
    }
}
