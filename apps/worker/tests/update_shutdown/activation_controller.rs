//! Main26: actual original child, one network command, independent normal IPC.
use super::*;
use winbridge_protocol::*;
use winbridge_update::activation::controller::{
    self, ActivationControllerRecord, ActivationControllerState,
};

fn controller_history(directory: &Path, id: &str) -> Vec<ActivationControllerRecord> {
    let c = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q = c
        .prepare("SELECT record FROM activation_controller_history WHERE id=?1 ORDER BY revision")
        .unwrap();
    q.query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|v| serde_json::from_str(&v.unwrap()).unwrap())
        .collect()
}

pub(in super::super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    local: &serde_json::Value,
) -> serde_json::Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 26;
    q.action = SupervisorAction::ConnectActivationController;
    // Drop only volatile dispatch ACK. No DB/image audit before this one send.
    dispatch_once(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationController;
    let until = Instant::now() + Duration::from_secs(300);
    let mut busy = 0u64;
    let view = loop {
        if let Some(view) = call(&rt, sup, &q) {
            assert!(
                !view.driver_active && view.driver_error.is_none(),
                "Controller failed: {:?}",
                view.driver_error
            );
            assert_eq!(
                view.activation_controller.as_ref().unwrap().phase,
                ActivationControllerPhase::Observed
            );
            break view;
        }
        busy += 1;
        assert!(
            Instant::now() < until,
            "Observe same controller attempt; never reconnect by command replay"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let id = &sup.spec.update_id;
    let directory = root.runtime().join("update-state");
    let saved = UpdateJournal::inspect_activation_controller(&directory, id)
        .unwrap()
        .unwrap();
    let ActivationControllerState::Confirmed { receipt } = &saved.state else {
        panic!("Durable controller fact missing")
    };
    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap(), local["record"]);
    assert_eq!(
        serde_json::to_value(&view.update).unwrap(),
        local["confirmed"]["update"]
    );
    assert_eq!(view.update.phase, UpdatePhase::AwaitingController);
    assert_eq!(record.revision, 7);
    assert_eq!(
        record.state,
        winbridge_update::activation::ActivationState::LocalVerified {}
    );
    assert_eq!(
        record.state_owned.as_ref().unwrap().process,
        receipt.compatibility.identity.process
    );
    let child =
        infrastructure::PinnedProcess::live(&receipt.compatibility.identity.process).unwrap();
    child.require_live().unwrap();
    lock_occupied(&record.intent.context.worker_state);
    let controller_view = view.activation_controller.as_ref().unwrap();
    assert_eq!(
        controller_view.saved_receipt.as_ref(),
        Some(receipt.as_ref())
    );
    assert_eq!(controller_view.process_alive, Some(true));
    controller::validate_projection_evidence(
        receipt,
        &saved.command,
        controller_view.observed_projection.as_ref().unwrap(),
    )
    .unwrap();
    controller::validate_link_evidence(
        receipt,
        &saved.command,
        controller_view.observed_link.as_ref().unwrap(),
    )
    .unwrap();
    assert_eq!(receipt.session.connection_attempt, 1);
    assert_eq!(
        receipt.authority,
        saved.command.binding.service_receipt.authority
    );
    let observed: ActivationControllerObservation = service::normal(
        &rt,
        &saved.command.binding.service_command,
        &ActivationControllerQuery {
            controller_observation_version: 1,
            command: saved.command.clone(),
        },
    );
    let ActivationControllerObservationResult::Observed {
        receipt: actual,
        projection,
        link,
    } = &observed.result
    else {
        panic!("Normal IPC selected session missing")
    };
    assert_eq!(observed.controller_observation_version, 1);
    controller::validate_projection_evidence(actual, &saved.command, projection).unwrap();
    controller::validate_link_evidence(actual, &saved.command, link).unwrap();
    assert_eq!(actual.session.session_id, receipt.session.session_id);
    assert_eq!(
        actual.session.controller_generation,
        receipt.session.controller_generation
    );
    assert!(actual.session.projection_sequence >= receipt.session.projection_sequence);
    let LocalResponse::Info { info } = service::normal(
        &rt,
        &saved.command.binding.service_command,
        &LocalRequest::Info,
    ) else {
        panic!("Info")
    };
    assert_eq!(info.generation, receipt.compatibility.generation);
    assert_eq!(
        info.build.as_ref(),
        Some(&receipt.compatibility.identity.descriptor.build)
    );
    assert_eq!(info.active_task_count, Some(0));
    assert!(info.journal_recovery_required.is_none() && info.maintenance_error.is_none());
    assert!(
        !info.maintenance_available
            && !info.workflow_available
            && !info.snapshot_import_available
            && !info.snapshot_resume_available
            && !info.snapshot_delta_available
    );
    let fenced = info.fenced_requests.as_ref().unwrap();
    assert_eq!(fenced.context, receipt.compatibility.context);
    assert!(!fenced.legacy_mutations_allowed);
    let lease = info.update_coordination.as_ref().unwrap();
    assert_eq!(lease["readOnly"], true);
    assert_eq!(lease["lease"]["phase"], "quiesced");
    let request = ControllerConnectionsRequest {
        context: receipt.compatibility.context.clone(),
    };
    let envelope = controller_connections_envelope(&info, &request).unwrap();
    let connections = decode_controller_connections(
        service::normal(&rt, &saved.command.binding.service_command, &envelope),
        &info,
        &request,
    )
    .unwrap();
    assert_eq!(
        connections.connections.len(),
        saved.command.binding.topology.controllers.len()
    );
    let selected = connections
        .connections
        .iter()
        .find(|c| c.id == receipt.session.controller_id)
        .unwrap();
    controller::validate_link_evidence(receipt, &saved.command, selected).unwrap();
    let mut history = Vec::new();
    for expected in local["normalHistory"].as_array().unwrap() {
        let task: TaskRecord = serde_json::from_value(expected["task"].clone()).unwrap();
        let detail: LocalResponse = service::normal(
            &rt,
            &saved.command.binding.service_command,
            &LocalRequest::GetTask {
                task_id: task.task_id.clone(),
            },
        );
        let LocalResponse::Task { task: detail } = detail else {
            panic!("Old task")
        };
        assert_eq!(serde_json::to_value(detail).unwrap(), expected["task"]);
        let output: LocalResponse = service::normal(
            &rt,
            &saved.command.binding.service_command,
            &LocalRequest::ReadOutput {
                task_id: task.task_id,
                after_sequence: 0,
            },
        );
        let LocalResponse::Output { chunks } = output else {
            panic!("Old output")
        };
        assert_eq!(serde_json::to_value(chunks).unwrap(), expected["output"]);
        for fresh in [false, true] {
            let mut submission = task.submission.clone();
            if fresh {
                submission.request_id = uuid::Uuid::new_v4().to_string();
                submission.idempotency_key = format!("main26-forbidden-{}", uuid::Uuid::new_v4());
            }
            let result: LocalResponse = service::normal(
                &rt,
                &saved.command.binding.service_command,
                &LocalRequest::Submit {
                    request: submission,
                },
            );
            assert!(
                matches!(result, LocalResponse::Error { .. }),
                "Network startup must retain closed roots, including duplicate submission"
            );
        }
        history.push(expected.clone());
    }
    assert!(!history.is_empty());
    let rows = controller_history(&directory, id);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.last(), Some(&saved));
    q.action = SupervisorAction::ConnectActivationController;
    let duplicate = call(&rt, sup, &q).expect("Resident completed controller operation");
    assert_eq!(
        serde_json::to_value(&duplicate.activation_controller).unwrap(),
        serde_json::to_value(&view.activation_controller).unwrap()
    );
    assert_eq!(controller_history(&directory, id), rows);
    assert_eq!(journal_bytes(root), local["workerBytesAfter"]);
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation_service(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        local["serviceRecord"]
    );
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation_writer(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        local["writerRecord"]
    );
    json!({"record":saved,"history":rows,"confirmed":view,"duplicate":duplicate,"normalObservation":observed,"normalInfo":info,"connections":connections,"normalHistory":history,
        "physicalRecord":record,"workerBytesAfter":journal_bytes(root),"lostControllerAckSentOnce":true,"busyObservations":busy,
        "savedControllerCommandCount":1,"sameChildVerified":true,"sameAuthorityVerified":true,"sameLocalPrefixVerified":true,
        "rootCommitted":false,"maintenanceReleased":false,"publicActivationEnabled":false})
}

pub(in super::super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    saved: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.runtime().join("update-state");
    let view = UpdateJournal::activation_controller_snapshot(&directory, id, false, None).unwrap();
    let observation = view.activation_controller.as_ref().unwrap();
    assert_eq!(observation.phase, ActivationControllerPhase::NotResident);
    assert!(observation.resident_process.is_none() && observation.process_alive.is_none());
    assert!(observation.observed_projection.is_none() && observation.observed_link.is_none());
    assert_eq!(view.update.phase, UpdatePhase::AwaitingController);
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation_controller(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        saved["record"]
    );
    assert_eq!(
        serde_json::to_value(controller_history(&directory, id)).unwrap(),
        saved["history"]
    );
    assert_eq!(journal_bytes(root), saved["workerBytesAfter"]);
    json!({"view":view,"historyUnchanged":true,"automaticColdLaunchCount":0,"noColdNetworkReplay":true})
}
