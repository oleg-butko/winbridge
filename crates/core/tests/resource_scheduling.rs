use winbridge_core::resource_scheduling::*;
use winbridge_protocol::*;
fn policy() -> ResourceSchedulingPolicy {
    serde_json::from_value(serde_json::json!({"resources":[{"id":"build","kind":"build","capacity":2},{"id":"cache","kind":"cache","capacity":1},{"id":"ipc","kind":"ipc","capacity":1}],"projects":[{"projectId":"a","task":[{"resourceId":"build","units":1}],"build":[{"resourceId":"cache","units":1}],"start":[{"resourceId":"ipc","units":1}]}]})).unwrap()
}

#[test]
fn unrelated_bindings_do_not_invalidate_another_projects_evidence() {
    use winbridge_core::{config::WorkerConfig, readiness::project_policy_catalog};
    let dir = tempfile::tempdir().unwrap();
    let mut projects = vec![];
    for id in ["a", "b"] {
        let root = dir.path().join(id);
        std::fs::create_dir(&root).unwrap();
        projects.push(serde_json::json!({"id":id,"workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}));
    }
    let mut config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"test","nodeId":"worker","stateDirectory":dir.path().join("state"),"worker":{"keepRunningOnUiExit":true},"projects":projects,"resourceScheduling":policy()})).unwrap();
    config.validate().unwrap();
    let fingerprint = |c: &WorkerConfig| {
        project_policy_catalog(c, build_identity())
            .unwrap()
            .projects
            .into_iter()
            .find(|p| p.project_id == "a")
            .unwrap()
            .sha256
    };
    let original = fingerprint(&config);
    let p = config.resource_scheduling.as_mut().unwrap();
    p.resources.push(ResourceDefinition {
        id: "b-only".into(),
        kind: ResourceKind::Port,
        capacity: 1,
    });
    p.projects.push(ProjectResourceBindings {
        project_id: "b".into(),
        task: vec![ResourceRequirement {
            resource_id: "b-only".into(),
            units: 1,
        }],
        import: vec![],
        build: vec![],
        start: vec![],
    });
    config.validate().unwrap();
    assert_eq!(fingerprint(&config), original);
    config.resource_scheduling.as_mut().unwrap().projects[1]
        .task
        .push(ResourceRequirement {
            resource_id: "cache".into(),
            units: 1,
        });
    config.validate().unwrap();
    assert_eq!(
        fingerprint(&config),
        original,
        "another holder changes availability, not this project's permission"
    );

    config.resource_scheduling.as_mut().unwrap().resources[0].capacity = 3;
    assert_ne!(fingerprint(&config), original);
    let mut disabled = config.clone();
    disabled.resource_scheduling = None;
    assert_ne!(fingerprint(&disabled), original);
}
#[test]
fn declarations_validate_full_sets_and_freeze_children_without_rebinding() {
    let p = policy();
    validate(&p, &["a".into()]).unwrap();
    let frozen = freeze(Some(&p), "a").unwrap();
    assert_eq!(frozen.task.len(), 1);
    assert_eq!(frozen.build.len(), 2);
    assert_eq!(frozen.start.len(), 2);
    assert_eq!(frozen.policy_sha256.as_ref().unwrap().len(), 64);
    assert!(frozen.start[1].kind.process_lifetime());
    assert!(!frozen.task[0].kind.process_lifetime());
    let mut edited = p.clone();
    edited.projects[0].start.clear();
    assert_ne!(freeze(Some(&edited), "a").unwrap(), frozen);
    assert_eq!(freeze(None, "a").unwrap(), FrozenResourcePlan::default());
    let mut bad = p.clone();
    let repeated = bad.projects[0].task[0].clone();
    bad.projects[0].start.push(repeated);
    assert!(validate(&bad, &["a".into()]).is_err());
    let mut bad = p.clone();
    bad.resources[1].capacity = 2;
    assert!(validate(&bad, &["a".into()]).is_err());
    let mut bad = p.clone();
    bad.projects[0].task[0].units = 3;
    assert!(validate(&bad, &["a".into()]).is_err());
    let mut bad = p.clone();
    bad.resources.push(bad.resources[0].clone());
    assert!(validate(&bad, &["a".into()]).is_err());
    assert!(validate(&p, &["foreign".into()]).is_err());
    assert!(serde_json::from_value::<ResourceSchedulingPolicy>(
        serde_json::json!({"resources":[{"id":"x","kind":"unknown","capacity":1}],"projects":[]})
    )
    .is_err());
}
