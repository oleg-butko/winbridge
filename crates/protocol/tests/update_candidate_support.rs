use winbridge_protocol::update::*;

#[test]
fn support_v18_request_accepts_no_caller_reports_paths_or_permissions() {
    for action in [
        SupervisorAction::CaptureCandidateSupport,
        SupervisorAction::ObserveCandidateSupport,
    ] {
        let q = SupervisorRequest {
            contract_version: 18,
            launch_id: "launch".into(),
            expected_revision: 2,
            action,
            token: "a".repeat(64),
            worker_command: None,
        };
        let value = serde_json::to_value(&q).unwrap();
        assert_eq!(value["contractVersion"], 18);
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<SupervisorRequest>(value.clone()).unwrap()
            )
            .unwrap(),
            value
        );
        for field in [
            "report",
            "assessment",
            "drivers",
            "permit",
            "executable",
            "arguments",
            "config",
            "replay",
            "canDrain",
            "canInstall",
        ] {
            let mut bad = value.clone();
            bad[field] = serde_json::json!({});
            assert!(
                serde_json::from_value::<SupervisorRequest>(bad).is_err(),
                "{field}"
            );
        }
    }
}
#[test]
fn support_view_retains_indeterminate_and_rejects_fabricated_authority() {
    let view = CandidateSupportView {
        phase: CandidateSupportViewPhase::Indeterminate,
        revision: Some(1),
        attempt_id: Some("attempt".into()),
        command: None,
        process: None,
        report: None,
        assessment: None,
        cleanup_verified: false,
        reason: Some("Creation is unknown; no replay".into()),
    };
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["phase"], "indeterminate");
    assert_eq!(
        serde_json::from_value::<CandidateSupportView>(value.clone()).unwrap(),
        view
    );
    for field in ["canInstall", "canDrain", "permit", "custody"] {
        let mut bad = value.clone();
        bad[field] = serde_json::json!(true);
        assert!(serde_json::from_value::<CandidateSupportView>(bad).is_err());
    }
    let mut bad = value.clone();
    bad["revision"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CandidateSupportView>(bad).is_err());
    for state in ["success", "installed", "activated", "can_install"] {
        let mut bad = value.clone();
        bad["phase"] = serde_json::json!(state);
        assert!(serde_json::from_value::<CandidateSupportView>(bad).is_err());
    }
}
