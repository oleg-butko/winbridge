//! Real Store/IPC/scheduler/config locks; synthetic keeper/runtime metadata.
//! These tests cannot assert an authenticated V8 delivery or keeper conversion.
use super::finalization_service::active::ActiveReadiness;
use super::*;
use crate::store::activation_finalization::{ActivatedReleaseAdmission, ActivatedReleaseCommit};
use winbridge_protocol::*;

async fn pending(root: &Path) -> (RunningLocalService, Arc<PendingLifetime>, Arc<AtomicBool>) {
    let service = super::tests::fixture(root);
    let (old, entry) = super::tests::proposal(&service);
    let mut running = service.start().await.unwrap();
    let live = Arc::new(AtomicBool::new(true));
    running
        .seal_supervised_lifetime(
            old.clone(),
            entry,
            LifetimeKeeper::Fixture(Arc::clone(&live)),
        )
        .unwrap();
    let resident = {
        let scope = running.service.worker.local_activation.as_ref().unwrap();
        let mode = scope.lifetime.lock().unwrap();
        let ServiceLifetime::Supervised(value) = &*mode else {
            panic!("Supervised");
        };
        Arc::clone(value)
    };
    let command = super::tests::finalization_proposal(&running, old);
    running
        .seal_finalization_pending(command, &resident)
        .unwrap();
    let scope = running.service.worker.local_activation.as_ref().unwrap();
    let owner = match &*scope.lifetime.lock().unwrap() {
        ServiceLifetime::ReleasePending(value) => Arc::clone(value),
        _ => panic!("Pending"),
    };
    (running, owner, live)
}
fn commit(running: &RunningLocalService, owner: &PendingLifetime) -> Box<ActivatedReleaseCommit> {
    let worker = &running.service.worker;
    let scope = worker.local_activation.as_ref().unwrap();
    match worker
        .store()
        .unwrap()
        .release_activated_service(&scope.command, &scope.shutdown, &owner.command, || Ok(()))
        .unwrap()
    {
        ActivatedReleaseAdmission::Committed(value) => value,
        _ => panic!("Fresh known commit required"),
    }
}
fn info(worker: &Worker) -> Box<WorkerInfo> {
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("Info");
    };
    info
}

#[tokio::test]
async fn active_known_commit_keeps_same_store_and_pause_then_runs_one_fenced_task() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, owner, _) = pending(root.path()).await;
    let worker = Arc::clone(&running.service.worker);
    let context = worker.request_context().unwrap();
    let authority = worker.store().unwrap().state_backup_authority().unwrap();
    let sql = commit(&running, &owner);
    assert!(worker.activation_read_only());
    let receipt = running
        .seal_active_commit(sql, ActiveReadiness::fixture(Arc::clone(&owner)), || Ok(()))
        .unwrap();
    assert_eq!(receipt.mode, WorkerFinalizationMode::Active);
    assert!(!worker.activation_read_only());
    assert!(running.service._config_pin.is_none());
    assert_eq!(worker.request_context().unwrap(), context);
    assert_eq!(
        worker.store().unwrap().state_backup_authority().unwrap(),
        authority
    );
    worker
        .check_local_service_scope_at(running.service.deadline + Duration::from_secs(1))
        .unwrap();
    let status = info(&worker);
    assert!(status.maintenance_available);
    let before = worker
        .store()
        .unwrap()
        .queue_observation(context.clone())
        .unwrap();
    assert_eq!(
        before.mode,
        QueueMode::Paused,
        "Activation must preserve user pause"
    );
    let changed = worker.handle_queue_control(QueueControlRequest {
        queue_control_version: 1,
        context: context.clone(),
        command: QueueControlCommand::Set {
            command_id: uuid::Uuid::new_v4().to_string(),
            expected_revision: before.revision,
            mode: QueueMode::Running,
        },
    });
    assert!(matches!(changed.result, QueueControlResult::Applied { .. }));
    let (executable, arguments) = if cfg!(windows) {
        ("cmd.exe", vec!["/d", "/c", "echo active-ready"])
    } else {
        ("/bin/sh", vec!["-c", "printf active-ready"])
    };
    let request = FencedRequest {
        request_contract_version: 2,
        context,
        request: LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "active-task".into(),
                project_id: "source".into(),
                idempotency_key: "active-task".into(),
                timeout_ms: 5000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: executable.into(),
                    arguments: arguments.into_iter().map(str::to_owned).collect(),
                    working_directory: ".".into(),
                },
            },
        },
    };
    let first = worker.handle_fenced(request.clone());
    let LocalResponse::Task { task } = first.response else {
        panic!("Task not admitted: {:?}", first.response);
    };
    let id = task.task_id;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask {
            task_id: id.clone(),
        }) else {
            panic!("Task missing");
        };
        if task.status == TaskStatus::Succeeded {
            assert_eq!(task.exit_code, Some(0));
            break;
        }
        assert!(
            Instant::now() < until,
            "Active scheduler must execute its admitted task: {:?}",
            task
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let LocalResponse::Task { task: same } = worker.handle_fenced(request).response else {
        panic!("Duplicate missing");
    };
    assert_eq!(same.task_id, id);
    assert!(matches!(
        worker
            .handle_lifetime_observation(ActivationLifetimeQuery {
                lifetime_observation_version: 1,
                command: owner.resident.command.clone()
            })
            .result,
        ActivationLifetimeObservationResult::Rejected { .. }
    ));
    assert!(!worker.activation_read_only());
    drop(worker);
    running.close().await.unwrap();
}

#[tokio::test]
async fn active_seal_failure_keeps_pending_config_pin_and_closed_admission() {
    for fault in 0..5 {
        let root = tempfile::tempdir().unwrap();
        let (mut running, owner, live) = pending(root.path()).await;
        let sql = commit(&running, &owner);
        let mut ready = Arc::clone(&owner);
        if fault == 1 {
            live.store(false, Ordering::Release);
        }
        if fault == 2 {
            ready = Arc::new(PendingLifetime {
                resident: Arc::clone(&owner.resident),
                command: owner.command.clone(),
            });
        }
        if fault == 3 {
            running.service.worker.stop();
        }
        if fault == 4 {
            running
                .service
                .worker
                .active_jobs
                .store(1, Ordering::Release);
        }
        let result = running.seal_active_commit(sql, ActiveReadiness::fixture(ready), || {
            if fault == 0 {
                Err("Injected final runtime checkpoint refusal".into())
            } else {
                Ok(())
            }
        });
        let failure = result.expect_err(&format!("fault {fault}"));
        assert!(running.service._config_pin.is_some());
        assert!(running.service.worker.activation_read_only());
        running
            .service
            .worker
            .active_jobs
            .store(0, Ordering::Release);
        if fault == 0 || fault == 4 {
            let (known, readiness) = failure.into_parts();
            running
                .seal_active_commit(known, readiness, || Ok(()))
                .unwrap();
            assert!(
                !running.service.worker.activation_read_only(),
                "Finish the original known commit without a second SQL release"
            );
        } else {
            drop(failure);
        }
        running.close().await.unwrap();
    }
}

#[tokio::test]
async fn active_retires_worker_config_pin_and_saved_config_does_not_revoke_mode() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, owner, _) = pending(root.path()).await;
    let path = running
        .service
        .worker
        .config_path
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    #[cfg(windows)]
    assert!(
        std::fs::OpenOptions::new().write(true).open(&path).is_err(),
        "Worker must hold its verification config pin"
    );
    let sql = commit(&running, &owner);
    running
        .seal_active_commit(sql, ActiveReadiness::fixture(owner), || Ok(()))
        .unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["nodeId"] = "saved-for-next-restart".into();
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    // Settings editors commonly replace a file atomically. Active must not
    // retain the verification file ID or a hidden deny-delete handle either.
    let replacement = path.with_extension("active-replacement.json");
    std::fs::write(&replacement, serde_json::to_vec(&config).unwrap()).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    running
        .service
        .worker
        .check_local_service_scope_at(running.service.deadline + Duration::from_secs(1))
        .unwrap();
    assert!(!running.service.worker.activation_read_only());
    running.close().await.unwrap();
}

#[tokio::test]
async fn saved_sql_release_stays_data_and_never_opens_active_without_known_commit() {
    let root = tempfile::tempdir().unwrap();
    let (running, owner, _) = pending(root.path()).await;
    drop(commit(&running, &owner));
    let worker = &running.service.worker;
    let scope = worker.local_activation.as_ref().unwrap();
    assert!(matches!(
        worker
            .store()
            .unwrap()
            .release_activated_service(&scope.command, &scope.shutdown, &owner.command, || Ok(()))
            .unwrap(),
        ActivatedReleaseAdmission::Observed(_)
    ));
    assert!(worker.activation_read_only());
    assert!(running.service._config_pin.is_some());
    assert!(!info(worker).maintenance_available);
    running.close().await.unwrap();
}
