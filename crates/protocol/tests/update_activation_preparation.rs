use winbridge_protocol::update::*;

#[test]
fn preparation_v20_accepts_no_caller_launch_material_or_permissions() {
    for action in [
        SupervisorAction::PrepareActivation,
        SupervisorAction::ObserveActivationPreparation,
    ] {
        let request = SupervisorRequest {
            contract_version: 20,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<SupervisorRequest>(value.clone()).unwrap()
            )
            .unwrap(),
            value
        );
        for field in [
            "nonce",
            "context",
            "record",
            "executable",
            "config",
            "permit",
            "canSpawn",
            "force",
            "replay",
        ] {
            let mut forged = value.clone();
            forged[field] = serde_json::json!(true);
            assert!(serde_json::from_value::<SupervisorRequest>(forged).is_err());
        }
    }
}

#[test]
fn preparation_view_cannot_claim_installation_or_carry_a_nonce() {
    let view = ActivationPreparationView {
        phase: ActivationPreparationPhase::NotPrepared,
        attempt_id: None,
        revision: None,
        set_id: None,
        replacement_sha256: None,
        worker: None,
        worker_config_sha256: None,
        prepared_at_ms: None,
        physical_custody_held: false,
        reason: None,
    };
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::from_value::<ActivationPreparationView>(value.clone()).unwrap(),
        view
    );
    for field in ["nonce", "channelNonce", "permit", "canSpawn", "canInstall"] {
        let mut forged = value.clone();
        forged[field] = true.into();
        assert!(serde_json::from_value::<ActivationPreparationView>(forged).is_err());
    }
    for state in ["installed", "activated", "committed", "can_spawn"] {
        let mut forged = value.clone();
        forged["phase"] = state.into();
        assert!(serde_json::from_value::<ActivationPreparationView>(forged).is_err());
    }
}
