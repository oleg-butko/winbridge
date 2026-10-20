//! Full journal/RPC/queue cases under the isolated clean-candidate health fixture.
use super::*;

pub(super) async fn run(
    h: &mut SupervisorHandle,
    worker: &Arc<Worker>,
    directory: &Path,
    case: &str,
    health: &winbridge_update::health::HealthRecord,
) -> (winbridge_update::abort::AbortRecord, serde_json::Value) {
    // Read-only older-reader refusal on the actual joined schema11 journal.
    let previous =
        std::env::var_os("WINBRIDGE_PREVIOUS_UPDATER10").expect("Pinned schema10 reference");
    let before = UpdateJournal::read_snapshot(directory, Some(&h.record.spec.update_id)).unwrap();
    use std::os::windows::process::CommandExt;
    let refusal = std::process::Command::new(previous)
        .creation_flags(0x08000000)
        .args([
            "--config",
            &h.record.spec.config_path,
            "journal",
            "status",
            &h.record.spec.update_id,
        ])
        .output()
        .unwrap();
    assert!(!refusal.status.success());
    assert!(
        String::from_utf8_lossy(&refusal.stderr).contains("UnsupportedSchema"),
        "{}",
        String::from_utf8_lossy(&refusal.stderr)
    );
    assert_eq!(
        UpdateJournal::read_snapshot(directory, Some(&h.record.spec.update_id)).unwrap(),
        before
    );
    let context = worker.request_context().unwrap();
    let queue = worker
        .store()
        .unwrap()
        .queue_observation(context.clone())
        .unwrap();
    let revision = queue.revision.clone();
    let pause = worker
        .store()
        .unwrap()
        .queue_control(
            context.clone(),
            QueueControlCommand::Set {
                command_id: format!("user-pause-{case}"),
                expected_revision: revision,
                mode: QueueMode::DispatchPaused,
            },
        )
        .unwrap();
    let QueueControlResult::Applied {
        observation: paused,
        ..
    } = pause
    else {
        panic!("User pause not applied")
    };
    let mut tasks = Vec::new();
    for n in 0..2 {
        let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: format!("accepted-{case}-{n}"),
                idempotency_key: format!("accepted-{case}-{n}"),
                project_id: "other".into(),
                timeout_ms: 30000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: health
                        .context
                        .as_ref()
                        .unwrap()
                        .artifacts
                        .inspection
                        .artifacts[1]
                        .image
                        .path
                        .clone(),
                    arguments: vec!["success".into()],
                    working_directory: ".".into(),
                },
            },
        }) else {
            panic!("Expected accepted other-project task")
        };
        assert_eq!(task.status, TaskStatus::Queued);
        tasks.push(task.task_id.clone());
    }
    let update_id = h.record.spec.update_id.clone();
    let id = &update_id;
    let registration = worker.control_registration().unwrap();
    let begins_before = registration.begin_drain_requests.load(Ordering::Acquire);
    if case == "lost-drain-ack" {
        registration.drop_drain_ack.store(true, Ordering::Release);
    }
    let start = h.candidate_drain(true).await.unwrap();
    assert!(start.driver_active);
    assert_eq!(start.update.phase, UpdatePhase::Draining);
    assert_eq!(
        start.candidate_drain.as_ref().unwrap().confirmed_commands,
        0
    );
    let until = Instant::now() + Duration::from_secs(60);
    let waiting = loop {
        let v = h.candidate_drain(false).await.unwrap();
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if v.candidate_drain.as_ref().is_some_and(|d| {
            d.phase == CandidateDrainPhase::WaitingAccepted
                && d.worker_observation
                    .as_ref()
                    .is_some_and(|o| o.accepted_operations == 2)
        }) {
            break v;
        }
        assert!(
            v.driver_active && Instant::now() < until,
            "Drain did not reach live waiting: {v:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(
        waiting
            .candidate_drain
            .as_ref()
            .unwrap()
            .worker_observation
            .as_ref()
            .unwrap()
            .queue_mode,
        QueueMode::DispatchPaused
    );
    assert_eq!(
        worker
            .store()
            .unwrap()
            .queue_observation(context.clone())
            .unwrap()
            .mode,
        paused.mode
    );
    assert_eq!(
        worker
            .store()
            .unwrap()
            .queue_observation(context.clone())
            .unwrap()
            .revision,
        paused.revision
    );
    assert_eq!(
        h.candidate_drain(true).await.unwrap().update,
        waiting.update
    );
    let saved = UpdateJournal::inspect_drain(directory, id).unwrap();
    assert_eq!(saved.commands.len(), 3);
    assert!(matches!(
        saved.commands[0].delivery,
        winbridge_update::drain::DrainDeliveryState::Confirmed { .. }
    ));
    for r in &saved.commands[1..] {
        assert!(matches!(
            r.delivery,
            winbridge_update::drain::DrainDeliveryState::Prepared
        ));
    }
    // New admission must fail during drain, without disturbing accepted tasks.
    assert!(matches!(
        worker.handle(LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: format!("blocked-{case}"),
                idempotency_key: format!("blocked-{case}"),
                project_id: "other".into(),
                timeout_ms: 1000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "never-executed-new-root".into(),
                    arguments: vec![],
                    working_directory: ".".into()
                }
            }
        }),
        LocalResponse::Error { .. }
    ));
    let abort;
    let final_view;
    if case == "cancel-drain" {
        abort = h.abort_before_cutover().await.unwrap();
        assert_eq!(
            worker
                .store()
                .unwrap()
                .queue_observation(context.clone())
                .unwrap()
                .mode,
            paused.mode
        );
        assert_eq!(
            worker
                .store()
                .unwrap()
                .queue_observation(context.clone())
                .unwrap()
                .revision,
            paused.revision
        );
        for id in &tasks {
            assert_eq!(
                worker.store().unwrap().get(id).unwrap().status,
                TaskStatus::Queued
            );
        }
        final_view = UpdateJournal::drain_preparation_snapshot(directory, id, false, None).unwrap();
    } else {
        if case == "deny-drain-receipt" {
            let c = rusqlite::Connection::open(directory.join("updates.sqlite")).unwrap();
            c.execute_batch("CREATE TRIGGER deny_final_drain BEFORE INSERT ON drain_history WHEN NEW.ordinal=2 AND NEW.revision=2 BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        }
        let unpause = worker
            .store()
            .unwrap()
            .queue_control(
                context.clone(),
                QueueControlCommand::Set {
                    command_id: format!("user-resume-{case}"),
                    expected_revision: paused.revision.clone(),
                    mode: QueueMode::Running,
                },
            )
            .unwrap();
        assert!(matches!(unpause, QueueControlResult::Applied { .. }));
        let until = Instant::now() + Duration::from_secs(90);
        final_view = loop {
            let v = h.candidate_drain(false).await.unwrap();
            if !v.driver_active {
                break v;
            }
            assert!(Instant::now() < until, "Drain failed to retire: {v:?}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        for id in &tasks {
            let task = worker.store().unwrap().get(id).unwrap();
            assert_eq!(task.status, TaskStatus::Succeeded, "{task:?}");
            assert_eq!(task.exit_code, Some(0));
        }
        let saved = UpdateJournal::inspect_drain(directory, id).unwrap();
        if case == "deny-drain-receipt" {
            assert_eq!(final_view.update.phase, UpdatePhase::Draining);
            assert!(final_view.driver_error.is_some());
            assert!(saved.quiesced_revision.is_none());
            assert!(matches!(
                saved.commands[2].delivery,
                winbridge_update::drain::DrainDeliveryState::Uncertain { .. }
            ));
            let lease = worker
                .store()
                .unwrap()
                .active_worker_update()
                .unwrap()
                .unwrap();
            assert_eq!(lease.phase, WorkerUpdatePhase::Quiesced);
            let c = rusqlite::Connection::open(directory.join("updates.sqlite")).unwrap();
            c.execute_batch("DROP TRIGGER deny_final_drain;").unwrap();
            assert_eq!(
                h.candidate_drain(true).await.unwrap().update,
                final_view.update
            );
        } else {
            assert!(
                final_view.driver_error.is_none(),
                "{:?}",
                final_view.driver_error
            );
            assert_eq!(final_view.update.phase, UpdatePhase::Quiesced);
            assert_eq!(saved.quiesced_revision, Some(final_view.update.revision));
            assert_eq!(
                final_view
                    .candidate_drain
                    .as_ref()
                    .unwrap()
                    .confirmed_commands,
                3
            );
            for r in saved.commands {
                assert!(matches!(
                    r.delivery,
                    winbridge_update::drain::DrainDeliveryState::Confirmed { .. }
                ));
            }
        }
        abort = h.abort_before_cutover().await.unwrap();
    }
    let begin_requests = registration.begin_drain_requests.load(Ordering::Acquire) - begins_before;
    assert_eq!(
        begin_requests, 1,
        "Mutation must not be resent after a lost ACK"
    );
    assert!(!registration.drop_drain_ack.load(Ordering::Acquire));
    let proof = serde_json::json!({"case":case,"waiting":waiting,"final":final_view,"journal":UpdateJournal::inspect_drain(directory,id).unwrap(),"tasks":tasks.iter().map(|id|worker.store().unwrap().get(id).unwrap()).collect::<Vec<_>>(),"userPausePreserved":true,"newRootRejected":true,"previousSchemaRefused":true,"beginDrainRequests":begin_requests,"lostAckReconciled":case=="lost-drain-ack","runtimeApplied":false});
    // Explicitly retire fixture tasks after the abort assertions; updater itself
    // never cancels accepted work or overrides the user's pause.
    if case == "cancel-drain" {
        for id in &tasks {
            assert!(matches!(
                worker.handle(LocalRequest::Cancel {
                    task_id: id.clone()
                }),
                LocalResponse::Task { .. }
            ));
        }
        let result = worker
            .store()
            .unwrap()
            .queue_control(
                context.clone(),
                QueueControlCommand::Set {
                    command_id: "fixture-reset-queue".into(),
                    expected_revision: paused.revision,
                    mode: QueueMode::Running,
                },
            )
            .unwrap();
        assert!(matches!(result, QueueControlResult::Applied { .. }));
    }
    (abort, proof)
}
