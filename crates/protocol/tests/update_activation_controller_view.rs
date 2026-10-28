use winbridge_protocol::update::*;
#[test]
fn controller_actions_accept_no_caller_paths_urls_receipts_or_grants() {
    for action in [
        SupervisorAction::ConnectActivationController,
        SupervisorAction::ObserveActivationController,
    ] {
        let raw = serde_json::to_value(SupervisorRequest {
            contract_version: 26,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        })
        .unwrap();
        serde_json::from_value::<SupervisorRequest>(raw.clone()).unwrap();
        for key in [
            "command",
            "commandId",
            "receipt",
            "url",
            "topology",
            "selection",
            "epoch",
            "nonce",
            "force",
            "replay",
            "permit",
            "config",
        ] {
            let mut bad = raw.clone();
            bad[key] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
}
#[test]
fn controller_view_retains_unknown_residency_and_rejects_fabricated_execution_permission() {
    let view = ActivationControllerView {
        phase: ActivationControllerPhase::NotResident,
        attempt_id: None,
        command_id: None,
        revision: Some(1),
        durable_phase: Some(ActivationControllerDurablePhase::Confirmed),
        saved_receipt: None,
        resident_process: None,
        process_alive: None,
        observed_projection: None,
        observed_link: None,
        reason: None,
    };
    let raw = serde_json::to_value(view).unwrap();
    assert_eq!(raw["processAlive"], serde_json::Value::Null);
    let roundtrip: ActivationControllerView = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), raw);
    for key in [
        "nonce",
        "token",
        "canConnect",
        "startupPermit",
        "writeGrant",
        "canTransferLifetime",
        "committed",
    ] {
        let mut bad = raw.clone();
        bad[key] = true.into();
        assert!(serde_json::from_value::<ActivationControllerView>(bad).is_err());
    }
}
