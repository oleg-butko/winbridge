use winbridge_protocol::update::*;
#[test]
fn activation_files_v21_rejects_caller_selected_paths_and_permissions() {
    for action in [
        SupervisorAction::BindActivationFiles,
        SupervisorAction::ObserveActivationFiles,
    ] {
        let q = SupervisorRequest {
            contract_version: 21,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(q).unwrap();
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<SupervisorRequest>(value.clone()).unwrap()
            )
            .unwrap(),
            value
        );
        for field in [
            "path", "files", "nonce", "permit", "force", "canSpawn", "replay",
        ] {
            let mut bad = value.clone();
            bad[field] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
}
#[test]
fn live_file_view_is_typed_and_cannot_claim_worker_activation() {
    let v = ActivationFilesView {
        phase: ActivationFilesPhase::Indeterminate,
        attempt_id: Some("attempt".into()),
        set_id: Some("set".into()),
        held_files: 3,
        worker: None,
        manifest_sha256: None,
        original_state_held: true,
        reason: Some("Scope retired during read handoff".into()),
    };
    let value = serde_json::to_value(&v).unwrap();
    assert_eq!(
        serde_json::from_value::<ActivationFilesView>(value.clone()).unwrap(),
        v
    );
    for field in ["nonce", "permit", "canSpawn", "canInstall"] {
        let mut bad = value.clone();
        bad[field] = true.into();
        assert!(serde_json::from_value::<ActivationFilesView>(bad).is_err());
    }
    for phase in ["installed", "activated", "committed"] {
        let mut bad = value.clone();
        bad["phase"] = phase.into();
        assert!(serde_json::from_value::<ActivationFilesView>(bad).is_err());
    }
    let mut bad = value;
    bad["heldFiles"] = true.into();
    assert!(serde_json::from_value::<ActivationFilesView>(bad).is_err());
}
