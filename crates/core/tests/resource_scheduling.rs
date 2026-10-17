use winbridge_core::resource_scheduling::*;
use winbridge_protocol::*;
fn policy() -> ResourceSchedulingPolicy {
    serde_json::from_value(serde_json::json!({"resources":[{"id":"build","kind":"build","capacity":2},{"id":"cache","kind":"cache","capacity":1},{"id":"ipc","kind":"ipc","capacity":1}],"projects":[{"projectId":"a","task":[{"resourceId":"build","units":1}],"build":[{"resourceId":"cache","units":1}],"start":[{"resourceId":"ipc","units":1}]}]})).unwrap()
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
