//! Main25. Run immediately after writer confirmation on the original child.
//! All observations below use actual authenticated IPC/OS handles, not JSON custody.
use super::*;
use winbridge_protocol::{
    LocalRequest, LocalResponse, QueueControlCommand, QueueControlRequest, QueueControlResponse,
    QueueControlResult, QueueMode, WorkerInfo,
};
use winbridge_update::activation::{
    service::{self, ActivationServiceRecord, ActivationServiceState},
    ActivationState,
};

fn service_history(directory: &Path, id: &str) -> Vec<ActivationServiceRecord> {
    let c = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q = c
        .prepare("SELECT record FROM activation_service_history WHERE id=?1 ORDER BY revision")
        .unwrap();
    q.query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}
pub(super) fn normal<T: serde::de::DeserializeOwned>(
    rt: &tokio::runtime::Runtime,
    command: &WorkerServiceActivationCommand,
    request: &impl serde::Serialize,
) -> T {
    let physical = &command.binding.writer_command.binding.physical;
    let endpoint = winbridge_transport::endpoint(
        Path::new(&physical.state.state_directory),
        &physical.context.instance_id,
    )
    .unwrap();
    rt.block_on(winbridge_transport::supervisor::call(
        &endpoint,
        physical.process.process_id,
        request,
    ))
    .unwrap()
}
fn info(
    rt: &tokio::runtime::Runtime,
    command: &WorkerServiceActivationCommand,
    url: &str,
) -> WorkerInfo {
    let LocalResponse::Info { info } = normal(rt, command, &LocalRequest::Info) else {
        panic!("Normal IPC Info required")
    };
    let physical = &command.binding.writer_command.binding.physical;
    assert_eq!(info.generation, physical.generation);
    assert_eq!(info.instance_id, physical.context.instance_id);
    assert_eq!(
        info.build.as_ref(),
        Some(&command.binding.expected_worker.descriptor.build)
    );
    assert_eq!(info.protocol_version, command.binding.protocol_version);
    assert!(info.journal_inspection.is_none() && info.journal_recovery_required.is_none());
    assert!(info.maintenance_error.is_none() && info.resource_recovery.is_none());
    assert_eq!(info.active_task_count, Some(0));
    assert!(info.recovery_coverage.as_ref().is_some_and(|c| c.complete));
    let connection = info
        .controller_connection
        .as_ref()
        .expect("Preserved configured URL must remain visible");
    assert_eq!(connection.url, url);
    assert!(matches!(
        connection.state,
        winbridge_protocol::ConnectionState::Disconnected
    ));
    assert!(
        connection.session_id.is_none()
            && connection.last_confirmed_at_ms.is_none()
            && connection.controller_node_id.is_none()
            && connection.controller_generation.is_none()
    );
    assert!(
        !info.maintenance_available
            && !info.workflow_available
            && !info.snapshot_import_available
            && !info.snapshot_delta_available
            && !info.snapshot_resume_available
    );
    let fenced = info.fenced_requests.as_ref().unwrap();
    assert_eq!(fenced.context, physical.context);
    assert!(!fenced.legacy_mutations_allowed);
    let lease = info.update_coordination.as_ref().unwrap();
    assert_eq!(lease["readOnly"], true);
    assert_eq!(lease["lease"]["phase"], "quiesced");
    assert_eq!(
        lease["lease"]["spec"]["updateId"],
        command.binding.writer_command.binding.update_id
    );
    *info
}

pub(super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    rt: &tokio::runtime::Runtime,
    mut q: SupervisorRequest,
    baseline: Baseline,
    mut proof: serde_json::Value,
    writer_view: UpdatePreparationView,
) -> serde_json::Value {
    // FIRST operation after the writer's confirmed response. No DB/hash audit.
    q.contract_version = 25;
    q.action = SupervisorAction::ActivateLocalService;
    dispatch_once(rt, sup, &q);
    q.action = SupervisorAction::ObserveLocalService;
    let until = Instant::now() + Duration::from_secs(300);
    let mut busy = 0u64;
    let view = loop {
        if let Some(v) = call(rt, sup, &q) {
            assert!(
                !v.driver_active && v.driver_error.is_none(),
                "{:?}",
                v.driver_error
            );
            assert_eq!(
                v.activation_service.as_ref().unwrap().phase,
                ActivationServicePhase::Confirmed
            );
            break v;
        }
        busy += 1;
        assert!(
            Instant::now() < until,
            "Observe the existing service; never dispatch again"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let id = &sup.spec.update_id;
    let directory = root.runtime().join("update-state");
    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(record.revision, 7);
    assert_eq!(record.state, ActivationState::LocalVerified {});
    assert_eq!(view.update.phase, UpdatePhase::AwaitingController);
    assert_eq!(view.update.revision, writer_view.update.revision + 1);
    let physical = record.state_owned.as_ref().unwrap();
    let actual = &physical.process;
    assert_eq!(&infrastructure::stamp(actual.process_id).unwrap(), actual);
    assert_eq!(
        writer_view
            .activation_writer
            .as_ref()
            .unwrap()
            .resident_process
            .as_ref(),
        Some(actual)
    );
    let actual_job = super::super::activation_process::job(&record);
    assert!(matches!(
        process_identity::observe_process(&actual_job),
        ProcessObservation::Occupied(_)
    ));
    assert_eq!(infrastructure::job_observation(actual).unwrap(), Some(true));
    lock_occupied(&physical.state);
    let saved = UpdateJournal::inspect_activation_service(&directory, id)
        .unwrap()
        .unwrap();
    let ActivationServiceState::Confirmed { receipt } = &saved.state else {
        panic!("Durable service confirmation required")
    };
    service::validate_receipt(receipt, &saved.command).unwrap();
    let service_view = view.activation_service.as_ref().unwrap();
    assert_eq!(service_view.saved_receipt.as_ref(), Some(receipt.as_ref()));
    assert_eq!(service_view.resident_process.as_ref(), Some(actual));
    assert_eq!(service_view.process_alive, Some(true));
    let writer = UpdateJournal::inspect_activation_writer(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.command.binding.writer_command, writer.command);
    let (authority, actual_receipt) = worker_receipt(root, &writer);
    assert_eq!(authority, receipt.authority);
    assert_eq!(actual_receipt, saved.command.binding.writer_receipt);
    assert_eq!(authority.incarnation, baseline.authority.incarnation);
    assert_eq!(authority.epoch, baseline.authority.epoch + 1);
    assert!(baseline
        .stale
        .check_authority()
        .unwrap_err()
        .contains("stale journal incarnation/owner epoch"));
    let current = Store::open_read_only(&root.join("state/journal.sqlite")).unwrap();
    assert_eq!(current.worker_update_lease(id).unwrap(), baseline.lease);
    assert_eq!(
        current
            .worker_shutdown_receipt(id, &baseline.shutdown.command)
            .unwrap()
            .as_ref(),
        Some(&baseline.shutdown)
    );
    assert_eq!(
        preserved_service_rows(root, baseline.authority.epoch, authority.epoch),
        baseline.preserved
    );
    let settings: serde_json::Value = serde_json::from_slice(
        &fs::read(root.runtime().join("winbridge-worker.local.json")).unwrap(),
    )
    .unwrap();
    let url = settings["worker"]["controllerUrl"]
        .as_str()
        .expect("Live test controller URL required");
    let preflight: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("00-controller-preflight.json")).unwrap())
            .unwrap();
    assert_eq!(preflight["beforeSupervisorAdmission"], true);
    assert_eq!(preflight["connection"]["url"], url);
    assert_eq!(preflight["connection"]["state"], "connected");
    assert_eq!(preflight["instanceId"], physical.context.instance_id);
    let initial_info = info(rt, &saved.command, url);
    let pause_seed: QueueControlResponse =
        serde_json::from_slice(&fs::read(root.join("seed-user-pause.json")).unwrap()).unwrap();
    let QueueControlResult::Applied {
        receipt: pause_receipt,
        ..
    } = pause_seed.result
    else {
        panic!("Actual user pause was not applied")
    };
    let pause_query = QueueControlRequest {
        queue_control_version: 1,
        context: physical.context.clone(),
        command: QueueControlCommand::Observe {
            command_id: Some("main25-user-pause".into()),
        },
    };
    let pause: QueueControlResponse = normal(rt, &saved.command, &pause_query);
    let QueueControlResult::Observed {
        observation,
        receipt: observed_pause,
    } = &pause.result
    else {
        panic!("Saved user pause unavailable through normal IPC")
    };
    assert_eq!(observed_pause.as_ref(), Some(&pause_receipt));
    assert_eq!(observation.mode, QueueMode::Paused);
    assert!(observation.read_only);
    let resume: QueueControlResponse = normal(
        rt,
        &saved.command,
        &QueueControlRequest {
            command: QueueControlCommand::Set {
                command_id: "main25-forbidden-resume".into(),
                expected_revision: observation.revision.clone(),
                mode: QueueMode::Running,
            },
            ..pause_query
        },
    );
    assert!(matches!(resume.result, QueueControlResult::Rejected { .. }));
    let observation: ActivationServiceObservation = normal(
        rt,
        &saved.command,
        &ActivationServiceQuery {
            service_observation_version: 1,
            command: saved.command.clone(),
        },
    );
    let ActivationServiceObservationResult::Observed { receipt: observed } = observation.result
    else {
        panic!("Normal IPC service receipt missing")
    };
    assert_eq!(observation.service_observation_version, 1);
    service::validate_receipt(&observed, &saved.command).unwrap();
    assert_eq!(observed.authority, authority);
    let LocalResponse::Tasks { tasks } = normal(rt, &saved.command, &LocalRequest::ListTasks)
    else {
        panic!("Normal task history required")
    };
    assert_eq!(
        tasks.len() as u64,
        baseline.preserved["tasks"].as_u64().unwrap()
    );
    assert!(!tasks.is_empty());
    let mut history_proof = Vec::new();
    let mut output_count = 0usize;
    for task in tasks {
        let LocalResponse::Task { task: actual_task } = normal(
            rt,
            &saved.command,
            &LocalRequest::GetTask {
                task_id: task.task_id.clone(),
            },
        ) else {
            panic!("Saved task unavailable")
        };
        assert_eq!(
            serde_json::to_value(&actual_task).unwrap(),
            serde_json::to_value(current.task_snapshot(&task.task_id).unwrap().record).unwrap()
        );
        let LocalResponse::Output { chunks } = normal(
            rt,
            &saved.command,
            &LocalRequest::ReadOutput {
                task_id: task.task_id.clone(),
                after_sequence: 0,
            },
        ) else {
            panic!("Saved output unavailable")
        };
        assert_eq!(
            serde_json::to_value(&chunks).unwrap(),
            serde_json::to_value(current.output(&task.task_id, 0).unwrap()).unwrap()
        );
        output_count += chunks.len();
        let rejected: LocalResponse = normal(
            rt,
            &saved.command,
            &LocalRequest::Submit {
                request: actual_task.submission.clone(),
            },
        );
        assert!(
            matches!(rejected, LocalResponse::Error { .. }),
            "Readonly service must reject submissions, including duplicates"
        );
        history_proof.push(json!({"taskId":task.task_id,"task":actual_task,"output":chunks,"mutationRefused":true}));
    }
    assert!(output_count > 0);
    let source = &record.intent.source;
    let installed = winbridge_update::installed_set::pin_installed_set(
        root.runtime(),
        &source.installation_id,
        &source.scope_sha256,
    )
    .unwrap();
    installed.revalidate().unwrap();
    let actual_set = installed.observation();
    let mut sharing = Vec::new();
    for image in actual_set
        .images
        .iter()
        .chain(std::iter::once(&actual_set.manifest_file))
    {
        assert!(fs::OpenOptions::new()
            .write(true)
            .open(&image.path)
            .is_err());
        assert!(fs::remove_file(&image.path).is_err());
        sharing.push(json!({"image":image,"writeDenied":true,"deleteDenied":true}));
    }
    let rows = service_history(&directory, id);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        ActivationServiceRecord::issued(saved.command.clone()).unwrap()
    );
    rows[0].advance(&rows[1]).unwrap();
    assert_eq!(rows.last(), Some(&saved));
    let writer_rows = history(&directory, id);
    assert_eq!(writer_rows.len(), 2);
    writer_rows[0].advance(&writer_rows[1]).unwrap();
    assert_eq!(writer_rows.last(), Some(&writer));
    let physical_history = super::super::activation_preparation::history(&directory, id);
    assert_eq!(physical_history.len(), 8);
    assert_eq!(physical_history.last(), Some(&record));
    // A disabled/misplaced validation hook must fail the native case itself,
    // before reporting success and exporting the independent barrier evidence.
    let trace = activation_trace_root(root, id);
    for (phase, available) in [("before_release", false), ("after_release", true)] {
        let marker: serde_json::Value = serde_json::from_slice(
            &fs::read(trace.join(format!("activation-state-{phase}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            marker["record"],
            serde_json::to_value(&physical_history[5]).unwrap()
        );
        assert_eq!(marker["phase"], phase);
        assert_eq!(marker["lockAvailable"], available);
    }
    let marker: serde_json::Value = serde_json::from_slice(
        &fs::read(trace.join("activation-writer-before-send.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        marker["record"],
        serde_json::to_value(&physical_history[6]).unwrap()
    );
    assert_eq!(marker["commandId"], writer.command.command_id);
    assert_eq!(
        marker["commandSha256"],
        writer::command_sha256(&writer.command).unwrap()
    );
    writer::validate_command(&writer.command, &physical_history[6], &baseline.authority).unwrap();
    q.action = SupervisorAction::ActivateLocalService;
    let duplicate = call(rt, sup, &q).expect("Completed service returns its resident observation");
    assert_eq!(duplicate.activation_service, view.activation_service);
    assert_eq!(duplicate.update, view.update);
    assert_eq!(service_history(&directory, id), rows);
    assert_eq!(
        worker_receipt(root, &writer),
        (authority.clone(), actual_receipt.clone())
    );
    // Allow several heartbeat periods with a usable configured controller. A
    // normal connector must still never appear before its separate contract.
    std::thread::sleep(Duration::from_secs(6));
    let final_info = info(rt, &saved.command, url);
    assert_eq!(
        preserved_service_rows(root, baseline.authority.epoch, authority.epoch),
        baseline.preserved
    );
    lock_occupied(&physical.state);
    proof.as_object_mut().unwrap().extend(json!({
        "writerConfirmed":writer_view,"confirmed":view,"duplicate":duplicate,"record":record,"history":physical_history,
        "serviceRecord":saved,"serviceHistory":rows,"writerRecord":writer,"writerHistory":writer_rows,
        "workerAuthorityBefore":baseline.authority,"workerAuthorityAfter":authority,"actualWorkerReceipt":actual_receipt,
        "workerLeaseBefore":baseline.lease,"acceptedShutdownBefore":baseline.shutdown,
        "preservedRowsBefore":baseline.preserved,"preservedRowsAfter":preserved_service_rows(root, baseline.authority.epoch, authority.epoch),
        "oldCachedAuthorityRefused":true,"actualProcess":actual,"actualJob":actual_job,"actualJobOccupied":true,
        "actualProcessInJob":true,"actualSet":actual_set,"sharing":sharing,"childInstanceLockOccupied":true,"initialInfo":initial_info,"finalInfo":final_info,
        "controllerPreflight":preflight,"userPause":pause,"userResumeRefused":true,"normalHistory":history_proof,"normalObservation":observed,
        "workerBytesAfter":journal_bytes(root),"lostServiceAckSentOnce":true,"busyObservations":busy,
        "workerLaunchCount":1,"stateHandoffCount":1,"savedWorkerCommandCount":1,"serviceDispatchCount":1,
        "normalServiceStarted":true,"controllerStarted":false,"publicActivationEnabled":false
    }).as_object().unwrap().clone());
    proof["resourcePolicyReconciliation"] = json!({"schemaVersion":1,
        "normalizedColumn":"resource_policy.epoch", "beforeEpoch":baseline.authority.epoch,
        "afterEpoch":authority.epoch, "policyUnchanged":true});
    proof
}

pub(in super::super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    saved: &serde_json::Value,
    pin: &infrastructure::PinnedProcess,
) -> serde_json::Value {
    let until = Instant::now() + Duration::from_secs(10);
    while !pin.exited().unwrap() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(pin.exited().unwrap());
    let record: ActivationRecord = serde_json::from_value(saved["record"].clone()).unwrap();
    assert!(matches!(
        process_identity::observe_process(&super::super::activation_process::job(&record)),
        ProcessObservation::Empty(_)
    ));
    let directory = root.runtime().join("update-state");
    let view = UpdateJournal::activation_service_snapshot(&directory, id, false, None).unwrap();
    let s = view.activation_service.as_ref().unwrap();
    assert_eq!(s.phase, ActivationServicePhase::NotResident);
    assert!(s.resident_process.is_none() && s.process_alive.is_none());
    assert_eq!(view.update.phase, UpdatePhase::AwaitingController);
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        saved["record"]
    );
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation_service(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        saved["serviceRecord"]
    );
    assert_eq!(
        serde_json::to_value(service_history(&directory, id)).unwrap(),
        saved["serviceHistory"]
    );
    assert_eq!(
        serde_json::to_value(history(&directory, id)).unwrap(),
        saved["writerHistory"]
    );
    let writer: ActivationWriterRecord =
        serde_json::from_value(saved["writerRecord"].clone()).unwrap();
    let (authority, receipt) = worker_receipt(root, &writer);
    assert_eq!(
        serde_json::to_value(authority).unwrap(),
        saved["workerAuthorityAfter"]
    );
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        saved["actualWorkerReceipt"]
    );
    assert_eq!(
        preserved_service_rows(
            root,
            saved["workerAuthorityBefore"]["epoch"].as_u64().unwrap(),
            saved["workerAuthorityAfter"]["epoch"].as_u64().unwrap()
        ),
        saved["preservedRowsBefore"]
    );
    assert_eq!(journal_bytes(root), saved["workerBytesAfter"]);
    let pins = winbridge_update::shutdown::ShutdownStatePins::capture(Path::new(
        &record.intent.context.worker_state.state_directory,
    ))
    .unwrap();
    assert_eq!(pins.observation(), &record.intent.context.worker_state);
    let lock = pins.try_lock().unwrap();
    lock.revalidate().unwrap();
    drop(lock);
    json!({"view":view,"actualRetainedProcessExited":true,"activationJobEmpty":true,"sameInstanceLockAvailable":true,
        "savedHistoryUnchanged":true,"automaticColdLaunchCount":0,"workerBytesAfter":journal_bytes(root),"normalServiceResident":false})
}
