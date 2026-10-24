use winbridge_protocol::update::*;
#[test]
fn production_drain_v19_never_accepts_caller_evidence_or_unknown_success_states() {
    for action in [
        SupervisorAction::DrainCompatibleCandidate,
        SupervisorAction::ObserveCompatibleDrain,
    ] {
        let q = SupervisorRequest {
            contract_version: 19,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let json = serde_json::to_value(&q).unwrap();
        for field in [
            "support",
            "assessment",
            "permit",
            "canInstall",
            "force",
            "replay",
            "executable",
        ] {
            let mut bad = json.clone();
            bad[field] = true.into();
            assert!(serde_json::from_value::<SupervisorRequest>(bad).is_err());
        }
    }
    assert!(
        serde_json::from_value::<CompatibleDrainPhase>(serde_json::json!("installed")).is_err()
    );
    assert_eq!(
        serde_json::to_value(CompatibleDrainPhase::Blocked).unwrap(),
        "blocked"
    );
}
