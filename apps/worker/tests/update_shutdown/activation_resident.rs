//! Main29: actual V7 peer, current connection and unchanged original quarantine.
use super::*;
use winbridge_update::activation::lifetime::{ActivationLifetimeRecord, ActivationLifetimeState};

fn observe(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> UpdatePreparationView {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(view) = call(rt, sup, q) {
            if !view.driver_active {
                assert_eq!(view.schema_version, 20);
                assert!(view.driver_error.is_none());
                assert_eq!(
                    view.activation_resident.as_ref().unwrap().phase,
                    ActivationResidentPhase::Observed,
                    "Actual current resident observation failed: {:?}",
                    view.activation_resident
                );
                return view;
            }
        }
        assert!(
            Instant::now() < until,
            "Readonly resident query did not complete"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(in super::super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    prefix: &serde_json::Value,
) -> serde_json::Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let directory = root.runtime().join("update-state");
    let id = &sup.spec.update_id;
    let saved: ActivationLifetimeRecord = serde_json::from_value(prefix["record"].clone()).unwrap();
    let ActivationLifetimeState::Confirmed { receipt } = &saved.state else {
        panic!("Original lifetime not confirmed");
    };
    let controller = UpdateJournal::inspect_activation_controller(&directory, id)
        .unwrap()
        .unwrap();
    let service_command = &controller.command.binding.service_command;
    let before = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
    let worker_before = journal_bytes(root);
    let history_before = lifetime::history(&directory, id);
    let requester = infrastructure::stamp(std::process::id()).unwrap();
    assert_ne!(
        requester.process_id,
        saved.command.binding.keeper.process_id
    );
    let query = ActivationResidentQuery {
        resident_observation_version: 7,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: saved.command.clone(),
    };
    let rejected: ActivationResidentObservation = service::normal(&rt, service_command, &query);
    assert_eq!(rejected.request_id.as_ref(), Some(&query.request_id));
    assert!(
        matches!(&rejected.result, ActivationResidentObservationResult::Rejected { message } if message.contains("actual original keeper peer"))
    );
    let mut spoof = serde_json::to_value(&query).unwrap();
    spoof["processId"] = saved.command.binding.keeper.process_id.into();
    let spoof_reply: ActivationResidentObservation = service::normal(&rt, service_command, &spoof);
    assert!(spoof_reply.request_id.is_none());
    assert!(matches!(
        spoof_reply.result,
        ActivationResidentObservationResult::Rejected { .. }
    ));
    let mut unsupported = query.clone();
    unsupported.resident_observation_version = 6;
    let unsupported_reply: ActivationResidentObservation =
        service::normal(&rt, service_command, &unsupported);
    assert!(matches!(
        unsupported_reply.result,
        ActivationResidentObservationResult::Rejected { .. }
    ));

    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 28;
    q.action = SupervisorAction::ObserveResidentConnection;
    let first = observe(&rt, sup, &q);
    let second = observe(&rt, sup, &q);
    let mut ids = Vec::new();
    for view in [&first, &second] {
        let resident = view.activation_resident.as_ref().unwrap();
        assert_eq!(
            resident.lifetime_command_id.as_ref(),
            Some(&saved.command.command_id)
        );
        assert_eq!(
            resident.resident_process.as_ref(),
            Some(&receipt.compatibility.identity.process)
        );
        assert_eq!(resident.process_alive, Some(true));
        assert_eq!(resident.original_custody_held, Some(true));
        let observed = resident.observation.as_ref().unwrap();
        let actual_query = ActivationResidentQuery {
            request_id: observed.request_id.clone().unwrap(),
            ..query.clone()
        };
        winbridge_update::activation::resident_observation::validate_observation(
            &actual_query,
            receipt,
            &controller.command.binding.topology,
            observed,
        )
        .unwrap();
        let ActivationResidentObservationResult::Observed { observed_at_ms, .. } = &observed.result
        else {
            panic!("current observation missing");
        };
        assert!(*observed_at_ms > saved.command.deadline_at_ms);
        assert_eq!(&view.update, before.1.as_ref().unwrap());
        assert_eq!(view.update.phase, UpdatePhase::AwaitingController);
        assert!(serde_json::to_vec(view).unwrap().len() < 32768);
        ids.push(actual_query.request_id);
    }
    assert_ne!(ids[0], ids[1]);
    let after = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
    let worker_after = journal_bytes(root);
    let history_after = lifetime::history(&directory, id);
    assert_eq!(after, before);
    assert_eq!(worker_after, worker_before);
    assert_eq!(history_after, history_before);
    json!({"first":first,"second":second,"wrongPeer":{"requester":requester,"query":query,"response":rejected},
        "pidSpoof":{"query":spoof,"response":spoof_reply},"unsupportedVersion":{"query":unsupported,"response":unsupported_reply},
        "updaterBefore":before,"updaterAfter":after,"workerBytesBefore":worker_before,"workerBytesAfter":worker_after,
        "lifetimeHistoryBefore":history_before,"lifetimeHistoryAfter":history_after,
        "sameLifetimeCommand":true,"rootCommitted":false,"maintenanceReleased":false,"privateChannelRead":false})
}

pub(in super::super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    proof: &serde_json::Value,
) -> serde_json::Value {
    let view = UpdateJournal::activation_resident_snapshot(
        &root.runtime().join("update-state"),
        id,
        false,
        None,
    )
    .unwrap();
    let resident = view.activation_resident.as_ref().unwrap();
    assert_eq!(resident.phase, ActivationResidentPhase::NotResident);
    assert!(
        resident.observation.is_none()
            && resident.resident_process.is_none()
            && resident.process_alive.is_none()
            && resident.original_custody_held.is_none()
    );
    let rows = lifetime::history(&root.runtime().join("update-state"), id);
    assert_eq!(
        serde_json::to_value(&rows).unwrap(),
        proof["lifetimeHistoryAfter"]
    );
    json!({"view":view,"historyUnchanged":true,"automaticColdLaunchCount":0,"noColdResidentAdoption":true})
}
