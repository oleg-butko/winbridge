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
