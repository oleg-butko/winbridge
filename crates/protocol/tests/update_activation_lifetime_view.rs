use winbridge_protocol::update::*;
#[test]
fn lifetime_actions_accept_no_caller_paths_urls_receipts_or_grants() {
    for action in [
        SupervisorAction::EnterActivationLifetime,
        SupervisorAction::ObserveActivationLifetime,
    ] {
        let raw = serde_json::to_value(SupervisorRequest {
            contract_version: 27,
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
fn lifetime_view_retains_unknown_residency_and_rejects_fabricated_execution_permission() {
    let view = ActivationLifetimeView {
        phase: ActivationLifetimePhase::NotResident,
        attempt_id: None,
        command_id: None,
        revision: Some(1),
        durable_phase: Some(ActivationLifetimeDurablePhase::Confirmed),
        saved_receipt: None,
        resident_process: None,
        process_alive: None,
        original_custody_held: None,
        observed_receipt: None,
        observed_at_ms: None,
        reason: None,
    };
    let raw = serde_json::to_value(view).unwrap();
    assert_eq!(raw["processAlive"], serde_json::Value::Null);
    let roundtrip: ActivationLifetimeView = serde_json::from_value(raw.clone()).unwrap();
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
        assert!(serde_json::from_value::<ActivationLifetimeView>(bad).is_err());
    }
}
