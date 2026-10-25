use winbridge_protocol::update::*;
#[test]
fn receiver_actions_accept_no_caller_executable_receipt_or_execution_permission() {
    for action in [
        SupervisorAction::LaunchActivationReceiver,
        SupervisorAction::ObserveActivationProcess,
    ] {
        let request = SupervisorRequest {
            contract_version: 22,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<SupervisorRequest>(value.clone()).unwrap()
            )
            .unwrap(),
            value
        );
        for field in [
            "executable",
            "config",
            "process",
            "job",
            "nonce",
            "receipt",
            "force",
            "replay",
            "canSpawn",
            "stateGrant",
        ] {
            let mut bad = value.clone();
            bad[field] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
}
#[test]
fn process_view_distinguishes_saved_ready_unknown_liveness_and_cold_handles() {
    let view = ActivationProcessView {
        phase: ActivationProcessPhase::NotResident,
        attempt_id: Some("attempt".into()),
        revision: Some(4),
        durable_phase: Some(ActivationDurablePhase::ReadyForState),
        saved_process: None,
        job_name: None,
        saved_ready: None,
        resident_process: None,
        process_custody_held: Some(false),
        process_alive: None,
        original_state_held: false,
        stdout_tail: None,
        stderr_tail: None,
        output_incomplete: None,
        capture_complete: None,
        reason: None,
    };
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::from_value::<ActivationProcessView>(value.clone()).unwrap(),
        view
    );
    for field in ["nonce", "permit", "canInstall", "canHandoff", "writerOwned"] {
        let mut bad = value.clone();
        bad[field] = true.into();
        assert!(serde_json::from_value::<ActivationProcessView>(bad).is_err());
    }
    for phase in ["installed", "committed", "running"] {
        let mut bad = value.clone();
        bad["phase"] = phase.into();
        assert!(serde_json::from_value::<ActivationProcessView>(bad).is_err());
    }
    let mut unknown = value;
    unknown["processCustodyHeld"] = serde_json::Value::Null;
    assert_eq!(
        serde_json::from_value::<ActivationProcessView>(unknown)
            .unwrap()
            .process_custody_held,
        None
    );
}
