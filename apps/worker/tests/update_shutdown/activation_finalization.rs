//! Main30: actual original V8, lost response, independent Active and one task.
use super::*;
use winbridge_protocol::*;
use winbridge_update::activation::finalization::ActivationFinalizationState;

fn observe_until_confirmed(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &mut SupervisorRequest,
) -> UpdatePreparationView {
    let until = Instant::now() + Duration::from_secs(90);
    let mut last_diagnostic = String::new();
    loop {
        let reply: std::io::Result<SupervisorResponse> =
            rt.block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                sup.process.as_ref().unwrap().process_id,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ));
        let diagnostic = match reply {
            Ok(reply) => match activation_reply::validate(&sup.spec.update_id, q, reply).unwrap() {
                Some(view) => {
                    assert_eq!(view.schema_version, 21);
                    let f = view.activation_finalization.as_ref().unwrap();
                    let diagnostic = format!(
                    "phase={:?}; durable={:?}; custody={:?}; pinsRetired={:?}; attempted={:?}; error={:?}; reason={:?}",
                    f.phase, f.durable_phase, f.original_custody_held,
                    f.verification_pins_retired, f.delivery_attempted, view.driver_error, f.reason
                );
                    assert!(
                        view.driver_active
                            || view.driver_error.is_none()
                            || f.delivery_attempted != Some(false),
                        "Main30 preparation stopped before V8; no delivery may be retried: {}",
                        diagnostic.chars().take(2048).collect::<String>()
                    );
                    if !view.driver_active
                        && view.driver_error.is_none()
                        && f.phase == ActivationFinalizationPhase::Confirmed
                    {
                        eprintln!("Main30 original finalization: {diagnostic}");
                        return view;
                    }
                    diagnostic
                }
                None => "Original actor operation is busy; observe without resending V8".into(),
            },
            Err(error) => {
                format!("Original actor transport: {error}; observe without resending V8")
            }
        };
        let diagnostic: String = diagnostic.chars().take(2048).collect();
        if diagnostic != last_diagnostic {
            eprintln!("Main30 original finalization: {diagnostic}");
            last_diagnostic = diagnostic;
        }
        // Only the first iteration may deliver V8. Every later request reads
        // the actual original owner; timeout never resubmits an effect.
        q.action = SupervisorAction::ObserveActivationFinalization;
        assert!(
            Instant::now() < until,
            "Original finalization did not confirm; inspect same owner/command: {last_diagnostic}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn sql_counts(root: &FixtureLayout, key: &str) -> (i64, i64) {
    let c = Connection::open_with_flags(
        root.join("state/journal.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let releases = c
        .query_row(
            "SELECT count(*) FROM worker_update_commands WHERE key=?1",
            [key],
            |r| r.get(0),
        )
        .unwrap();
    let tasks = c
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    (releases, tasks)
}
pub(in super::super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    prefix: &serde_json::Value,
    config_path: &Path,
) -> serde_json::Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let directory = root.runtime().join("update-state");
    let id = &sup.spec.update_id;
    let before = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
    let controller = UpdateJournal::inspect_activation_controller(&directory, id)
        .unwrap()
        .unwrap();
    let service_command = &controller.command.binding.service_command;
    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 29;
    q.action = SupervisorAction::FinalizeActivation;
    let first = observe_until_confirmed(&rt, sup, &mut q);
    let status = first.activation_finalization.as_ref().unwrap();
    assert_eq!(status.original_custody_held, Some(true));
    assert_eq!(status.process_alive, Some(true));
    assert_eq!(status.verification_pins_retired, Some(true));
    assert_eq!(status.delivery_attempted, Some(true));
    assert_eq!(
        status.interrupted_reply_prefix.as_ref().expect("Validation received no V8 header; inspect delivery timing and Indeterminate history").len(),
        4,
        "Actual V8 response must be interrupted, not synthetically accepted"
    );
    let saved = UpdateJournal::inspect_activation_finalization(&directory, id)
        .unwrap()
        .unwrap();
    let ActivationFinalizationState::Confirmed { receipt } = &saved.state else {
        panic!("Independent confirmation missing")
    };
    assert_eq!(
        saved.revision, 2,
        "ReleasePending→unknown ACK→independent confirmation"
    );
    assert_eq!(
        receipt.compatibility.identity.process,
        status.resident_process.clone().unwrap()
    );
    assert_eq!(
        receipt.compatibility.identity.process,
        serde_json::from_value::<SupervisorProcess>(
            prefix["record"]["command"]["binding"]["compatibility"]["identity"]["process"].clone()
        )
        .unwrap()
    );
    assert_eq!(receipt.mode, WorkerFinalizationMode::Active);
    assert_eq!(receipt.released_lease.phase, WorkerUpdatePhase::Released);
    assert_eq!(
        receipt.released_lease.revision,
        saved.command.binding.expected_lease_revision + 1
    );
    assert!(
        receipt.compatibility.observed_at_ms
            > saved.command.binding.lifetime_command.deadline_at_ms
    );
    let shutdown = UpdateJournal::inspect_shutdown(&directory, id)
        .unwrap()
        .unwrap();
    let obs = status.observation.as_ref().unwrap();
    let query = ActivationFinalizationQuery {
        finalization_observation_version: 1,
        request_id: obs.request_id.clone().unwrap(),
        command: saved.command.clone(),
    };
    winbridge_update::activation::finalization_observation::validate_observation(
        &query, &shutdown, obs,
    )
    .unwrap();
    let actual =
        infrastructure::PinnedProcess::live(&receipt.compatibility.identity.process).unwrap();
    actual
        .require_job_membership(&saved.command.binding.lifetime_command.binding.job_name)
        .unwrap();
    assert_eq!(sql_counts(root, &saved.command.command_id).0, 1);
    let worker_db = Connection::open_with_flags(
        root.join("state/journal.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let release_payload: String = worker_db
        .query_row(
            "SELECT payload FROM worker_update_commands WHERE key=?1",
            [&saved.command.command_id],
            |r| r.get(0),
        )
        .unwrap();
    let release_receipt: String = worker_db
        .query_row(
            "SELECT receipt FROM worker_update_commands WHERE key=?1",
            [&saved.command.command_id],
            |r| r.get(0),
        )
        .unwrap();
    let raw: LocalResponse = service::normal(&rt, service_command, &LocalRequest::Info);
    let LocalResponse::Info { info: info_before } = raw else {
        panic!("Actual worker Info unavailable")
    };
    assert!(info_before.maintenance_available);
    assert_eq!(info_before.generation, receipt.compatibility.generation);
    let wrong_peer: ActivationFinalizationResponse = service::normal(
        &rt,
        service_command,
        &ActivationFinalizationRequest {
            finalization_version: 8,
            command: saved.command.clone(),
        },
    );
    assert!(matches!(
        wrong_peer.result,
        ActivationFinalizationResult::Rejected { .. }
    ));
    let wrong_query: ActivationFinalizationObservation = service::normal(
        &rt,
        service_command,
        &ActivationFinalizationQuery {
            request_id: uuid::Uuid::new_v4().to_string(),
            ..query.clone()
        },
    );
    assert!(matches!(
        wrong_query.result,
        ActivationFinalizationObservationResult::Rejected { .. }
    ));
    let mut spoof = serde_json::to_value(&query).unwrap();
    spoof["processId"] = saved
        .command
        .binding
        .lifetime_command
        .binding
        .keeper
        .process_id
        .into();
    let spoof_reply: ActivationFinalizationObservation =
        service::normal(&rt, service_command, &spoof);
    assert!(matches!(
        spoof_reply.result,
        ActivationFinalizationObservationResult::Rejected { .. }
    ));
    // The parent and child verification handles must ALL be retired. Exercise
    // write and atomic replacement, then observe with the old loaded config.
    let config_before = inspect_image(config_path).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
    value["nodeId"] = "main30-next-restart".into();
    fs::write(config_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let temporary = config_path.with_extension("main30-replacement.json");
    fs::write(&temporary, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    fs::rename(&temporary, config_path).unwrap();
    let config_after = inspect_image(config_path).unwrap();
    assert_ne!(config_before.file_identity, config_after.file_identity);
    assert_ne!(config_before.sha256, config_after.sha256);
    q.action = SupervisorAction::ObserveActivationFinalization;
    let second = observe_until_confirmed(&rt, sup, &mut q);
    assert_ne!(
        first
            .activation_finalization
            .as_ref()
            .unwrap()
            .observation
            .as_ref()
            .unwrap()
            .request_id,
        second
            .activation_finalization
            .as_ref()
            .unwrap()
            .observation
            .as_ref()
            .unwrap()
            .request_id
    );
    assert_eq!(
        UpdateJournal::inspect_activation_finalization(&directory, id)
            .unwrap()
            .unwrap(),
        saved,
        "Readonly duplicate must not append another success"
    );
    let context = info_before
        .fenced_requests
        .as_ref()
        .unwrap()
        .context
        .clone();
    let queue: QueueControlResponse = service::normal(
        &rt,
        service_command,
        &QueueControlRequest {
            queue_control_version: 1,
            context: context.clone(),
            command: QueueControlCommand::Observe { command_id: None },
        },
    );
    let QueueControlResult::Observed { observation, .. } = queue.result else {
        panic!("Actual queue unavailable")
    };
    assert!(!observation.read_only);
    assert_eq!(
        observation.mode,
        QueueMode::Paused,
        "User pause survives Active"
    );
    let resume: QueueControlResponse = service::normal(
        &rt,
        service_command,
        &QueueControlRequest {
            queue_control_version: 1,
            context: context.clone(),
            command: QueueControlCommand::Set {
                command_id: uuid::Uuid::new_v4().to_string(),
                expected_revision: observation.revision,
                mode: QueueMode::Running,
            },
        },
    );
    assert!(matches!(resume.result, QueueControlResult::Applied { .. }));
    let config: winbridge_core::config::WorkerConfig =
        serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
    let request = FencedRequest {
        request_contract_version: 2,
        context,
        request: LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                project_id: config.projects[0].id.clone(),
                idempotency_key: "main30-active-once".into(),
                timeout_ms: 10000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "cmd.exe".into(),
                    arguments: vec![
                        "/d".into(),
                        "/c".into(),
                        "echo ACTIVE>>main30-active-count.txt".into(),
                    ],
                    working_directory: ".".into(),
                },
            },
        },
    };
    let accepted: FencedResponse = service::normal(&rt, service_command, &request);
    let LocalResponse::Task {
        task: accepted_task,
    } = &accepted.response
    else {
        panic!("Active task rejected: {:?}", accepted.response)
    };
    let until = Instant::now() + Duration::from_secs(20);
    let done = loop {
        let response: LocalResponse = service::normal(
            &rt,
            service_command,
            &LocalRequest::GetTask {
                task_id: accepted_task.task_id.clone(),
            },
        );
        if let LocalResponse::Task { task } = response {
            if task.status == TaskStatus::Succeeded {
                break task;
            }
            assert!(
                !matches!(
                    task.status,
                    TaskStatus::Failed | TaskStatus::Indeterminate | TaskStatus::Cancelled
                ),
                "Actual task failed: {task:?}"
            );
        }
        assert!(Instant::now() < until, "Active task did not execute");
        std::thread::sleep(Duration::from_millis(50));
    };
    let duplicate: FencedResponse = service::normal(&rt, service_command, &request);
    let LocalResponse::Task {
        task: duplicate_task,
    } = &duplicate.response
    else {
        panic!("Duplicate task missing")
    };
    assert_eq!(duplicate_task.task_id, done.task_id);
    std::thread::sleep(Duration::from_millis(100));
    let effect = fs::read_to_string(
        config.projects[0]
            .workspace_root
            .join("main30-active-count.txt"),
    )
    .unwrap();
    assert_eq!(effect.lines().collect::<Vec<_>>(), vec!["ACTIVE"]);
    let after = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
    assert_eq!(after.1.as_ref().unwrap().phase, UpdatePhase::Committed);
    assert_eq!(
        after.1.as_ref().unwrap().revision,
        before.1.as_ref().unwrap().revision + 1
    );
    let c = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let intent: String = c
        .query_row(
            "SELECT intent FROM activation_finalization_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let mut statement = c
        .prepare("SELECT record FROM activation_finalization_history WHERE id=?1 ORDER BY revision")
        .unwrap();
    let history: Vec<serde_json::Value> = statement
        .query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect();
    assert_eq!(history.len(), 3);
    assert_eq!(sql_counts(root, &saved.command.command_id).0, 1);
    assert!(
        serde_json::to_vec(&first).unwrap().len() < 32768
            && serde_json::to_vec(&second).unwrap().len() < 32768
    );
    json!({"first":first,"second":second,"record":saved,"intent":serde_json::from_str::<serde_json::Value>(&intent).unwrap(),"history":history,
        "shutdownEncoded":serde_json::to_string(&shutdown).unwrap(),"releasePayload":serde_json::from_str::<serde_json::Value>(&release_payload).unwrap(),"releaseReceipt":serde_json::from_str::<serde_json::Value>(&release_receipt).unwrap(),
        "updaterBefore":before,"updaterAfter":after,"infoBefore":info_before,"wrongPeerMutation":wrong_peer,"wrongPeerObservation":wrong_query,"pidSpoof":spoof_reply,
        "configBefore":config_before,"configAfter":config_after,"configWriteSucceeded":true,"atomicConfigReplaceSucceeded":true,
        "accepted":accepted,"task":done,"duplicate":duplicate,"taskEffect":effect,"taskEffectLines":effect.lines().count(),"releaseCommandCount":sql_counts(root,&saved.command.command_id).0,
        "physicalV8Proved":true,"authenticatedActiveProved":true,"permanentKeeperProved":false})
}
pub(in super::super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    proof: &serde_json::Value,
    worker: &infrastructure::PinnedProcess,
) -> serde_json::Value {
    assert!(
        worker.exited().unwrap(),
        "Original child must exit with the owned test Job"
    );
    let directory = root.runtime().join("update-state");
    let record = UpdateJournal::inspect_activation_finalization(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap(), proof["record"]);
    let view =
        UpdateJournal::activation_finalization_snapshot(&directory, id, false, None).unwrap();
    let f = view.activation_finalization.as_ref().unwrap();
    assert_eq!(f.phase, ActivationFinalizationPhase::NoOriginalCustody);
    assert!(
        f.original_custody_held.is_none()
            && f.observation.is_none()
            && f.delivery_attempted.is_none()
    );
    json!({"view":view,"record":record,"originalWorkerExited":true,"automaticColdLaunchCount":0,"coldObservationOnly":true})
}
