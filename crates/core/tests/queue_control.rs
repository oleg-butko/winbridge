use winbridge_core::{config::WorkerConfig, queue_control::admission_blocker};
use winbridge_protocol::*;
#[test]
fn root_policy_and_each_queue_mode_have_distinct_admission_effects() {
    let policy = QueuePolicy {
        max_pending_roots: 3,
        max_pending_roots_per_project: Some(2),
    };
    for mode in [QueueMode::Running, QueueMode::DispatchPaused] {
        assert_eq!(admission_blocker(mode, Some(&policy), 2, 1, "a"), None);
        assert_eq!(
            admission_blocker(mode, Some(&policy), 3, 1, "a"),
            Some(QueueBlocker::GlobalLimit { limit: 3 })
        );
        assert_eq!(
            admission_blocker(mode, Some(&policy), 2, 2, "a"),
            Some(QueueBlocker::ProjectLimit {
                project_id: "a".into(),
                limit: 2
            })
        );
    }
    for mode in [QueueMode::Paused, QueueMode::AdmissionPaused] {
        assert_eq!(
            admission_blocker(mode, None, 0, 0, "a"),
            Some(QueueBlocker::AdmissionPaused)
        );
    }
    assert!(admission_blocker(QueueMode::Running, None, 10001, 10001, "a").is_none());
}
#[test]
fn absent_queue_policy_keeps_legacy_config_and_fingerprint() {
    let root = tempfile::tempdir().unwrap();
    let value = serde_json::json!({"schemaVersion":1,"instanceId":"queue","nodeId":"worker","stateDirectory":root.path().join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"a","workspaceRoot":root.path(),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]});
    let config: WorkerConfig = serde_json::from_value(value.clone()).unwrap();
    assert!(config.queue_policy.is_none());
    config.validate().unwrap();
    let hash = |c: &WorkerConfig| {
        winbridge_core::readiness::project_policy_catalog(c, build_identity())
            .unwrap()
            .projects[0]
            .sha256
            .clone()
    };
    let baseline = hash(&config);
    let mut enabled = config.clone();
    enabled.queue_policy = Some(QueuePolicy {
        max_pending_roots: 2,
        max_pending_roots_per_project: Some(1),
    });
    enabled.validate().unwrap();
    assert_ne!(hash(&enabled), baseline);
    enabled.queue_policy = None;
    assert_eq!(hash(&enabled), baseline);
    assert!(serde_json::to_value(&config)
        .unwrap()
        .get("queuePolicy")
        .is_none());
    for policy in [
        serde_json::json!({"maxPendingRoots":0}),
        serde_json::json!({"maxPendingRoots":10001}),
        serde_json::json!({"maxPendingRoots":2,"maxPendingRootsPerProject":3}),
    ] {
        let mut v = value.clone();
        v["queuePolicy"] = policy;
        let c: WorkerConfig = serde_json::from_value(v).unwrap();
        assert!(c.validate().unwrap_err().contains("queuePolicy"));
    }
    let mut v = value;
    v["queuePolicy"] = serde_json::json!({"maxPendingRoots":1.5});
    assert!(serde_json::from_value::<WorkerConfig>(v).is_err());
}
