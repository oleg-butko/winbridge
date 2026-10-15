use std::collections::BTreeMap;
use winbridge_core::{config::WorkerConfig, work_scheduling::*};
use winbridge_protocol::*;

#[test]
fn fair_projects_rotate_by_durable_grants_independent_of_input_order_and_clocks() {
    let projects = vec!["c".into(), "a".into(), "b".into()];
    let mut grants = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for ordinal in 1..=300 {
        let selected = fair_project_order(&projects, &grants)[0].clone();
        *counts.entry(selected.clone()).or_insert(0) += 1;
        grants.insert(selected, ordinal);
    }
    assert_eq!(
        counts.values().copied().collect::<Vec<_>>(),
        vec![100, 100, 100]
    );
    grants.insert("a".into(), 999);
    assert_ne!(fair_project_order(&projects, &grants)[0], "a");
}
#[test]
fn absent_policy_roundtrips_as_legacy_and_active_policy_changes_effective_permission_hash() {
    let temp = tempfile::tempdir().unwrap();
    let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"slots","nodeId":"worker","stateDirectory":temp.path(),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":temp.path(),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    assert!(config.work_scheduling.is_none());
    assert!(serde_json::to_value(&config)
        .unwrap()
        .get("workScheduling")
        .is_none());
    let hash = |c: &WorkerConfig| {
        winbridge_core::readiness::project_policy_catalog(c, build_identity())
            .unwrap()
            .projects[0]
            .sha256
            .clone()
    };
    let legacy = hash(&config);
    let mut per_project = config.clone();
    per_project.projects[0].max_concurrent_mutations = 2;
    assert!(per_project
        .validate()
        .unwrap_err()
        .contains("workScheduling.maxConcurrentWork"));
    let mut enabled = config.clone();
    for limit in [1, 2, 32] {
        let policy = WorkSchedulingPolicy {
            max_concurrent_work: limit,
        };
        validate(&policy).unwrap();
        enabled.work_scheduling = Some(policy);
        enabled.validate().unwrap();
        assert_ne!(hash(&enabled), legacy);
    }
    for limit in [0, 33, u16::MAX] {
        let policy = WorkSchedulingPolicy {
            max_concurrent_work: limit,
        };
        assert!(validate(&policy).is_err());
        enabled.work_scheduling = Some(policy);
        assert!(enabled.validate().is_err());
    }
}
