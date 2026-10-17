use serde_json::{json, Value};
use winbridge_protocol::*;

fn info() -> WorkerInfo {
    let wire: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/legacy-v1/worker-info.json"
    ))
    .unwrap();
    let mut info: WorkerInfo = serde_json::from_value(wire["info"].clone()).unwrap();
    info.build = Some(build_identity());
    info
}

#[test]
fn advertised_contracts_bind_to_build_without_claiming_verification_or_permission() {
    let mut info = info();
    assert!(matches!(
        interpret_catalog(&info),
        CatalogObservation::Missing
    ));
    info.contract_catalog = Some(
        serde_json::to_value(ContractCatalog::implemented(info.build.clone().unwrap())).unwrap(),
    );
    let CatalogObservation::Known(catalog) = interpret_catalog(&info) else {
        panic!("expected catalog")
    };
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 1));
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 2));
    assert!(!catalog.supports("winbridge.run", 1));
    for field in ["verified", "pairTested", "enabled"] {
        assert!(info.contract_catalog.as_ref().unwrap().get(field).is_none());
    }
    info.build
        .as_mut()
        .unwrap()
        .build_id
        .push_str("-another-build");
    assert!(matches!(
        interpret_catalog(&info),
        CatalogObservation::Unsupported(_)
    ));
}

#[test]
fn future_or_malformed_metadata_roundtrips_without_blocking_info() {
    for raw in [
        json!(null),
        json!("future form"),
        json!({"schemaVersion":99,"newStates":["waiting"]}),
        json!({"schemaVersion":1,"provider":null}),
    ] {
        let mut info = info();
        info.contract_catalog = Some(raw.clone());
        let wire = serde_json::to_vec(&LocalResponse::Info {
            info: Box::new(info),
        })
        .unwrap();
        let LocalResponse::Info { info } = serde_json::from_slice(&wire).unwrap() else {
            panic!("info")
        };
        assert_eq!(info.contract_catalog, Some(raw));
        assert!(matches!(
            interpret_catalog(&info),
            CatalogObservation::Unsupported(_)
        ));
        assert_eq!(info.instance_id, "legacy-fixture");
    }
}

#[test]
fn update_coordination_is_optional_opaque_and_forwarded_by_current_info_codec() {
    assert!(info().update_coordination.is_none());
    for raw in [
        json!(null),
        json!("future form"),
        json!({"schemaVersion":99,"newStates":["installing"]}),
    ] {
        let mut i = info();
        i.update_coordination = Some(raw.clone());
        let decoded: WorkerInfo = serde_json::from_value(serde_json::to_value(i).unwrap()).unwrap();
        assert_eq!(decoded.update_coordination, Some(raw));
    }
}

#[test]
fn discovery_cases_have_the_same_rust_and_typescript_interpretation() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/discovery/catalog-cases.json"
    ))
    .unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let mut info = info();
        info.build = serde_json::from_value(case["workerBuild"].clone()).unwrap();
        info.contract_catalog = case.get("catalog").cloned();
        let actual = match interpret_catalog(&info) {
            CatalogObservation::Missing => "missing",
            CatalogObservation::Known(_) => "known",
            CatalogObservation::Unsupported(_) => "unsupported",
        };
        assert_eq!(
            actual,
            case["expected"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn policy_metadata_is_bound_to_build_and_complete_project_scope() {
    let mut i = info();
    i.projects = vec![serde_json::from_value(json!({"id":"fixture","workspaceRoot":"K:\\fixture","allowDirectCommands":true,"recipes":[]})).unwrap()];
    let catalog = ProjectPolicyCatalog {
        schema_version: 1,
        provider: i.build.clone().unwrap(),
        projects: i
            .projects
            .iter()
            .map(|p| ProjectPolicyFingerprint {
                project_id: p.id.clone(),
                sha256: "a".repeat(64),
            })
            .collect(),
    };
    i.project_policies = Some(serde_json::to_value(&catalog).unwrap());
    assert!(interpret_project_policies(&i).is_some());
    for raw in [
        serde_json::json!(null),
        serde_json::json!({"schemaVersion":2}),
        serde_json::json!("future policy format"),
    ] {
        i.project_policies = Some(raw.clone());
        let decoded: WorkerInfo =
            serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
        assert_eq!(decoded.project_policies, Some(raw));
        assert!(interpret_project_policies(&decoded).is_none());
    }
    let mut wrong = catalog.clone();
    wrong.provider.build_id.push_str("-other");
    i.project_policies = Some(serde_json::to_value(wrong).unwrap());
    assert!(interpret_project_policies(&i).is_none());
    let mut wrong = catalog.clone();
    wrong.projects.clear();
    i.project_policies = Some(serde_json::to_value(wrong).unwrap());
    assert!(interpret_project_policies(&i).is_none());
    if !catalog.projects.is_empty() {
        let mut wrong = catalog.clone();
        wrong.projects[0].sha256 = "bad".into();
        i.project_policies = Some(serde_json::to_value(wrong).unwrap());
        assert!(interpret_project_policies(&i).is_none());
        let mut wrong = catalog.clone();
        wrong.projects.push(wrong.projects[0].clone());
        i.project_policies = Some(serde_json::to_value(wrong).unwrap());
        assert!(interpret_project_policies(&i).is_none());
    }
}

#[test]
fn rust_typescript_share_policy_metadata_acceptance() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/readiness/policy-cases.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let mut i = info();
        i.build = Some(serde_json::from_value(case["workerBuild"].clone()).unwrap());
        i.projects = serde_json::from_value(case["projects"].clone()).unwrap();
        i.project_policies = Some(case["policies"].clone());
        assert_eq!(
            interpret_project_policies(&i).is_some(),
            case["known"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[cfg(feature = "jsonschema")]
#[test]
fn schema_metadata_binds_the_actual_dto_digest_to_provider_and_supported_contract() {
    let mut i = info();
    i.contract_catalog =
        Some(serde_json::to_value(ContractCatalog::implemented(i.build.clone().unwrap())).unwrap());
    let schemas = implemented_contract_schemas(i.build.clone().unwrap());
    i.contract_schemas = Some(serde_json::to_value(&schemas).unwrap());
    assert_eq!(interpret_contract_schemas(&i), Some(schemas.clone()));
    assert!(is_sha256(&schemas.schemas[0].sha256));
    assert_eq!(
        schemas.schemas[0].sha256,
        include_str!("../../../tests/fixtures/readiness/fenced-rpc-2.sha256").trim(),
        "Schema change requires an explicit contract review"
    );
    for raw in [
        json!(null),
        json!({"schemaVersion":99,"future":"schema"}),
        json!({"schemaVersion":1,"enabled":true}),
    ] {
        i.contract_schemas = Some(raw.clone());
        let decoded: WorkerInfo =
            serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
        assert_eq!(decoded.contract_schemas, Some(raw));
        assert!(interpret_contract_schemas(&decoded).is_none());
    }
    let mut changed = schemas.clone();
    changed.provider.build_id.push_str("-other");
    i.contract_schemas = Some(serde_json::to_value(changed).unwrap());
    assert!(interpret_contract_schemas(&i).is_none());
    let mut changed = schemas.clone();
    changed.schemas[0].contract.version = 99;
    i.contract_schemas = Some(serde_json::to_value(changed).unwrap());
    assert!(interpret_contract_schemas(&i).is_none());
    let mut changed = schemas.clone();
    changed.schemas.push(changed.schemas[0].clone());
    i.contract_schemas = Some(serde_json::to_value(changed).unwrap());
    assert!(interpret_contract_schemas(&i).is_none());
    let mut changed = schemas;
    changed.schemas[0].sha256 = "bad".into();
    i.contract_schemas = Some(serde_json::to_value(changed).unwrap());
    assert!(interpret_contract_schemas(&i).is_none());
}

#[test]
fn evidence_json_has_stable_nested_key_order_and_preserves_array_order() {
    let value: Value = serde_json::from_str(r#"{"z":1,"a":{"y":true,"b":["x",null]}}"#).unwrap();
    assert_eq!(
        canonical_evidence_json(&value).unwrap(),
        br#"{"a":{"b":["x",null],"y":true},"z":1}"#
    );
    let reordered: Value =
        serde_json::from_str(r#"{"a":{"b":["x",null],"y":true},"z":1}"#).unwrap();
    assert_eq!(
        canonical_evidence_json(&value).unwrap(),
        canonical_evidence_json(&reordered).unwrap()
    );
}
