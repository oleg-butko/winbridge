use winbridge_protocol::update::*;
#[test]
fn state_actions_expose_no_caller_process_paths_receipt_or_writer_permission() {
    for action in [
        SupervisorAction::HandoffActivationState,
        SupervisorAction::ObserveActivationState,
    ] {
        let value = serde_json::to_value(SupervisorRequest {
            contract_version: 23,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        })
        .unwrap();
        let _: SupervisorRequest = serde_json::from_value(value.clone()).unwrap();
        for field in [
            "state",
            "receipt",
            "epoch",
            "nonce",
            "force",
            "replay",
            "writerOwned",
            "canHandoff",
            "config",
        ] {
            let mut bad = value.clone();
            bad[field] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
}
#[test]
fn state_view_keeps_saved_receipts_separate_from_unknown_or_cold_resources() {
    let view = ActivationStateView {
        phase: ActivationStatePhase::NotResident,
        attempt_id: Some("attempt".into()),
        revision: Some(6),
        durable_phase: Some(ActivationDurablePhase::StateOwned),
        saved_ownership: None,
        original_lock_held: Some(false),
        resident_process: None,
        process_alive: None,
        reason: None,
    };
    let raw = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::from_value::<ActivationStateView>(raw.clone()).unwrap(),
        view
    );
    for field in [
        "token",
        "nonce",
        "writerOwned",
        "canWriteState",
        "permit",
        "canHandoff",
    ] {
        let mut bad = raw.clone();
        bad[field] = true.into();
        assert!(serde_json::from_value::<ActivationStateView>(bad).is_err());
    }
    for value in [
        serde_json::json!(0),
        serde_json::json!(1),
        serde_json::json!("true"),
    ] {
        let mut bad = raw.clone();
        bad["originalLockHeld"] = value;
        assert!(serde_json::from_value::<ActivationStateView>(bad).is_err());
    }
    let mut unknown = raw;
    unknown["originalLockHeld"] = serde_json::Value::Null;
    assert_eq!(
        serde_json::from_value::<ActivationStateView>(unknown)
            .unwrap()
            .original_lock_held,
        None
    );
}
