use winbridge_core::readiness::*;
use winbridge_protocol::*;

fn receipt() -> PairVerification {
    serde_json::from_str(include_str!("../../../tests/fixtures/readiness/pair.json")).unwrap()
}
fn definition() -> ContractDefinition {
    ContractDefinition {
        contract: ContractRef {
            id: WORKER_RPC_CONTRACT.into(),
            version: 2,
        },
        schema_sha256: "b".repeat(64),
        test_profile: "fenced-task-v1".into(),
        required_checks: vec!["fenced-reply".into(), "dedup".into(), "stale-scope".into()],
    }
}
#[test]
fn matched_evidence_needs_live_route_and_permission_separately() {
    let r = receipt();
    let d = definition();
    let ready = evaluate_readiness(&d, true, Some(&r.identity), Some(&r), true, true);
    assert!(ready.enabled && ready.build_verified && ready.pair_tested);
    for (supported, fresh, allowed, reason) in [
        (false, true, true, ReadinessReason::UnsupportedContract),
        (true, false, true, ReadinessReason::RouteNotFresh),
        (true, true, false, ReadinessReason::PolicyDenied),
    ] {
        let observed =
            evaluate_readiness(&d, supported, Some(&r.identity), Some(&r), fresh, allowed);
        assert!(observed.build_verified && observed.pair_tested && !observed.enabled);
        assert!(observed.reasons.contains(&reason));
        assert!(require_enabled(&observed).is_err());
    }
}
#[test]
fn every_identity_change_invalidates_old_pair_evidence() {
    let r = receipt();
    let base = serde_json::to_value(&r.identity).unwrap();
    for (pointer, value) in [
        ("/provider/buildId", "new-provider"),
        ("/consumer/buildId", "new-consumer"),
        ("/context/journalIncarnation", "new-incarnation"),
        ("/context/instanceId", "new-instance"),
        ("/projectPolicy/projectId", "other-project"),
        ("/projectPolicy/sha256", &"0".repeat(64)),
        ("/route/1/build/buildId", "new-controller"),
        ("/route/1/nodeId", "new-route"),
        ("/route/1/endpoint", "/local/other.sock"),
    ] {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = value.into();
        // Consumer/provider are also named in the corresponding hop.
        if pointer == "/provider/buildId" {
            changed["route"][2]["build"]["buildId"] = value.into();
        }
        if pointer == "/consumer/buildId" {
            changed["route"][0]["build"]["buildId"] = value.into();
        }
        let changed: PairIdentity = serde_json::from_value(changed).unwrap();
        let result = evaluate_readiness(&definition(), true, Some(&changed), Some(&r), true, true);
        assert!(!result.enabled && !result.pair_tested, "{pointer}");
        assert!(
            result.reasons.contains(&ReadinessReason::IdentityChanged),
            "{result:?}"
        );
    }
}
#[test]
fn unknown_schema_partial_checks_and_unverified_build_never_enable() {
    let mut r = receipt();
    r.checks.pop();
    let result = evaluate_readiness(&definition(), true, Some(&r.identity), Some(&r), true, true);
    assert!(result.build_verified && !result.pair_tested && !result.enabled);
    r = receipt();
    r.provider_verification.build.build_id = "unrelated".into();
    assert!(
        evaluate_readiness(&definition(), true, Some(&r.identity), Some(&r), true, true)
            .reasons
            .contains(&ReadinessReason::InvalidEvidence)
    );
    r = receipt();
    r.identity.schema_version = 2;
    assert!(
        evaluate_readiness(&definition(), true, Some(&r.identity), Some(&r), true, true)
            .reasons
            .contains(&ReadinessReason::InvalidIdentity)
    );
    r = receipt();
    r.identity.contract_schema_sha256 = "0".repeat(64);
    assert!(
        evaluate_readiness(&definition(), true, Some(&r.identity), Some(&r), true, true)
            .reasons
            .contains(&ReadinessReason::SchemaChanged)
    );
    r = receipt();
    r.identity.provider.source_dirty = true;
    assert!(validate_identity(&r.identity).is_err());
    assert!(!evaluate_readiness(&definition(), true, None, Some(&receipt()), true, true).enabled);
    assert!(
        !evaluate_readiness(
            &definition(),
            true,
            Some(&receipt().identity),
            None,
            true,
            true
        )
        .enabled
    );
}
#[test]
fn malformed_and_duplicate_evidence_fails_closed() {
    let mut r = receipt();
    r.checks.push(r.checks[0].clone());
    assert!(validate_verification(&r).is_err());
    r = receipt();
    r.identity.route.swap(0, 1);
    assert!(validate_verification(&r).is_err());
    r = receipt();
    r.report_sha256 = "../fake".into();
    assert!(validate_verification(&r).is_err());
    r = receipt();
    r.consumer_verification.platform = "windows".into();
    assert!(validate_verification(&r).is_err());
}
#[test]
fn effective_policy_hash_covers_profiles_limits_and_delivery_but_not_other_projects() {
    use winbridge_core::config::WorkerConfig;
    let root = tempfile::tempdir().unwrap();
    let value = serde_json::json!({"schemaVersion":1,"instanceId":"test","nodeId":"worker","stateDirectory":root.path(),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root.path(),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[{"id":"build","executable":"cargo","arguments":["build"],"workingDirectory":".","timeoutMs":10000}]}]});
    let hash = |v: serde_json::Value| {
        let config: WorkerConfig = serde_json::from_value(v).unwrap();
        project_policy_catalog(&config, receipt().identity.provider)
            .unwrap()
            .projects[0]
            .sha256
            .clone()
    };
    let original = hash(value.clone());
    for (pointer, replacement) in [
        ("/projects/0/allowDirectCommands", serde_json::json!(false)),
        (
            "/projects/0/recipes/0/arguments",
            serde_json::json!(["build", "--release"]),
        ),
        ("/projects/0/recipes/0/timeoutMs", serde_json::json!(12000)),
        (
            "/projects/0/workspaceRoot",
            serde_json::json!(root.path().join("changed")),
        ),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert_ne!(hash(changed), original, "{pointer}");
    }
    let mut changed = value.clone();
    changed["maxOutputBytesPerTask"] = serde_json::json!(2048);
    assert_ne!(hash(changed), original);
    let mut changed = value.clone();
    changed["delivery"] = serde_json::json!({"adapter":"shared-folder","root":root.path().join("shared"),"namespace":"WinBridge"});
    assert_ne!(hash(changed), original);
    let mut changed = value.clone();
    let mut other = changed["projects"][0].clone();
    other["id"] = "other".into();
    changed["projects"].as_array_mut().unwrap().push(other);
    assert_eq!(hash(changed), original);
    let mut profiled = value.clone();
    profiled["recipeProfiles"] = serde_json::json!([{"id":"profile","label":"Profile","description":"Test","recipes":[{"id":"extra","executable":"cargo","arguments":["check"],"workingDirectory":".","timeoutMs":10000}]}]);
    profiled["projects"][0]["recipeProfile"] = "profile".into();
    let first = hash(profiled.clone());
    profiled["recipeProfiles"][0]["recipes"][0]["arguments"] = serde_json::json!(["test"]);
    assert_ne!(hash(profiled), first);
}
