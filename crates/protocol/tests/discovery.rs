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
