use winbridge_protocol::update::*;
#[test]
fn service_actions_carry_no_caller_command_paths_or_authority_grant() {
    for action in [
        SupervisorAction::ActivateLocalService,
        SupervisorAction::ObserveLocalService,
    ] {
        let raw = serde_json::to_value(SupervisorRequest {
            contract_version: 25,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        })
        .unwrap();
        serde_json::from_value::<SupervisorRequest>(raw.clone()).unwrap();
        for field in [
            "command",
            "commandId",
            "receipt",
            "epoch",
            "nonce",
            "force",
            "replay",
            "writerOwned",
            "permit",
            "config",
        ] {
            let mut bad = raw.clone();
            bad[field] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
}
#[test]
fn service_view_keeps_unknown_liveness_separate_and_rejects_permission_fields() {
    let view = ActivationServiceView {
        phase: ActivationServicePhase::NotResident,
        attempt_id: Some("attempt".into()),
        command_id: Some("command".into()),
        revision: Some(1),
        durable_phase: Some(ActivationServiceDurablePhase::Confirmed),
        saved_receipt: None,
        resident_process: None,
        process_alive: None,
        reason: None,
    };
    let raw = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::from_value::<ActivationServiceView>(raw.clone()).unwrap(),
        view
    );
    for field in [
        "token",
        "nonce",
        "canWriteState",
        "startupPermit",
        "canConnectController",
        "canTransferLifetime",
        "canHandoff",
    ] {
        let mut bad = raw.clone();
        bad[field] = true.into();
        assert!(serde_json::from_value::<ActivationServiceView>(bad).is_err());
    }
    for value in [serde_json::json!(1), serde_json::json!("true")] {
        let mut bad = raw.clone();
        bad["processAlive"] = value;
        assert!(serde_json::from_value::<ActivationServiceView>(bad).is_err());
    }
    assert_eq!(raw["processAlive"], serde_json::Value::Null);
}
