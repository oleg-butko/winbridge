use winbridge_protocol::update::*;

#[test]
fn compatibility_assessment_is_strict_passive_data_not_a_permission() {
    let value = serde_json::json!({"schemaVersion":1,"policy":"same_schema_install_v1",
        "expectedUpdaterJournalSchema":15,"blockers":[{"kind":"missing_contract","contract":"activation"}]});
    let assessment: UpdaterCompatibilityAssessment = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        assessment.policy,
        UpdaterCompatibilityPolicy::SameSchemaInstallV1
    );
    assert_eq!(
        assessment.blockers,
        vec![UpdaterCompatibilityBlocker::MissingContract {
            contract: UpdaterExecutionContract::Activation
        }]
    );
    assert_eq!(serde_json::to_value(assessment).unwrap(), value);
    for (key, extra) in [
        ("canInstall", serde_json::json!(true)),
        ("policy", serde_json::json!("unknown_policy")),
        ("expectedUpdaterJournalSchema", serde_json::json!(true)),
        ("blockers", serde_json::json!([{"kind":"unknown_blocker"}])),
        (
            "blockers",
            serde_json::json!([{"kind":"missing_contract","contract":"unknown_contract"}]),
        ),
        (
            "blockers",
            serde_json::json!([{"kind":"missing_contract","contract":"activation","permit":true}]),
        ),
    ] {
        let mut bad = value.clone();
        bad[key] = extra;
        assert!(serde_json::from_value::<UpdaterCompatibilityAssessment>(bad).is_err());
    }
}
