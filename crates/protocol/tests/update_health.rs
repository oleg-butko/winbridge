use winbridge_protocol::update::*;
#[test]
fn health_v10_contract_is_explicit_and_rejects_caller_evidence() {
    let request = SupervisorRequest {
        contract_version: 10,
        launch_id: "launch".into(),
        expected_revision: 2,
        action: SupervisorAction::ValidateCandidateHealth,
        token: "a".repeat(64),
        worker_command: None,
    };
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["action"], "validate_candidate_health");
    assert_eq!(json["contractVersion"], 10);
    assert!(json.get("workerCommand").is_none());
    assert_eq!(
        serde_json::from_value::<SupervisorRequest>(json.clone())
            .unwrap()
            .action,
        request.action
    );
    for field in [
        "compatibility",
        "config",
        "report",
        "workerSchema",
        "executable",
        "replay",
    ] {
        let mut bad = json.clone();
        bad[field] = serde_json::json!({});
        assert!(
            serde_json::from_value::<SupervisorRequest>(bad).is_err(),
            "{field}"
        );
    }
    let phase = serde_json::to_value(CandidateHealthPhase::Indeterminate).unwrap();
    assert_eq!(phase, "indeterminate");
    assert!(serde_json::from_value::<CandidateHealthPhase>(serde_json::json!("success")).is_err());
}
#[test]
fn preparation_v16_is_explicit_and_accepts_no_caller_paths_or_receipts() {
    for action in [
        SupervisorAction::PrepareRuntimeFiles,
        SupervisorAction::ObserveRuntimeFiles,
    ] {
        let r = SupervisorRequest {
            contract_version: 16,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(&r).unwrap();
        assert_eq!(
            serde_json::from_value::<SupervisorRequest>(value.clone())
                .unwrap()
                .action,
            action
        );
        for field in [
            "plan",
            "prepared",
            "backup",
            "source",
            "destination",
            "fileIdentity",
            "resume",
        ] {
            let mut bad = value.clone();
            bad[field] = serde_json::json!({});
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
    assert_eq!(
        serde_json::to_value(SupervisorAction::ObserveStateBackup).unwrap(),
        "observe_state_backup"
    );
    assert_eq!(
        serde_json::to_value(RuntimePreparationPhase::Indeterminate).unwrap(),
        "indeterminate"
    );
}
#[test]
fn replacement_v17_accepts_no_caller_authority_or_physical_evidence() {
    for (action, name) in [
        (SupervisorAction::ReplaceRuntime, "replace_runtime"),
        (
            SupervisorAction::ObserveRuntimeReplacement,
            "observe_runtime_replacement",
        ),
    ] {
        let request = SupervisorRequest {
            contract_version: 17,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["action"], name);
        assert_eq!(value["contractVersion"], 17);
        let decoded = serde_json::from_value::<SupervisorRequest>(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        for field in [
            "permit",
            "custody",
            "plan",
            "directory",
            "fileIdentity",
            "receipt",
            "resume",
            "replay",
            "executable",
        ] {
            let mut bad = value.clone();
            bad[field] = serde_json::json!({});
            assert!(
                serde_json::from_value::<SupervisorRequest>(bad).is_err(),
                "{field}"
            );
        }
    }
    assert!(
        serde_json::from_value::<RuntimeReplacementPhase>(serde_json::json!("committed")).is_err()
    );
    assert!(
        serde_json::from_value::<RuntimeReplacementPhase>(serde_json::json!("activated")).is_err()
    );
}
