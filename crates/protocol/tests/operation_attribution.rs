#![cfg(feature = "jsonschema")]
use winbridge_protocol::*;
fn fixture() -> (WorkerInfo, OperationAttributionRequest, ExtensionResponse) {
    let mut info: WorkerInfo = serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../tests/fixtures/legacy-v1/worker-info.json"
        ))
        .unwrap()["info"]
            .clone(),
    )
    .unwrap();
    info.build = Some(build_identity());
    info.generation = "current-worker".into();
    let context = RequestContext {
        instance_id: info.instance_id.clone(),
        journal_incarnation: "journal".into(),
    };
    info.fenced_requests = Some(FencedRequestInfo {
        request_contract_version: 2,
        context: context.clone(),
        legacy_mutations_allowed: false,
    });
    info.contract_catalog =
        Some(serde_json::to_value(ContractCatalog::implemented(build_identity())).unwrap());
    info.contract_schemas =
        Some(serde_json::to_value(implemented_contract_schemas(build_identity())).unwrap());
    let request = OperationAttributionRequest {
        context: context.clone(),
        query: OperationAttributionQuery {
            kind: HistoryKind::Task,
            operation_id: "task".into(),
        },
    };
    let page = OperationAttribution {
        schema_version: 1,
        reference: OperationRef {
            context: context.clone(),
            kind: HistoryKind::Task,
            operation_id: "task".into(),
        },
        creator: Some(OperationCreator::Controller {
            worker_generation: "previous-worker".into(),
            controller_id: "a".into(),
            controller_node_id: "controller".into(),
            controller_generation: "previous-controller".into(),
            session_id: "old-session".into(),
        }),
        inherited_from: None,
        provider: build_identity(),
        worker_generation: info.generation.clone(),
        observed_at_ms: 1,
    };
    (
        info,
        request,
        ExtensionResponse {
            extension_version: 1,
            contract: operation_attribution_contract(),
            context: Some(context),
            result: ExtensionResult::Data {
                payload: serde_json::to_value(page).unwrap(),
            },
        },
    )
}
#[test]
fn historical_creator_is_not_rebound_to_current_worker_generation_and_unknown_stays_unknown() {
    let (info, request, mut response) = fixture();
    assert!(
        matches!(decode_operation_attribution(response.clone(),&info,&request).unwrap().creator,Some(OperationCreator::Controller {worker_generation,..}) if worker_generation=="previous-worker")
    );
    let ExtensionResult::Data { payload } = &mut response.result else {
        unreachable!()
    };
    payload["creator"] = serde_json::Value::Null;
    assert!(decode_operation_attribution(response, &info, &request)
        .unwrap()
        .creator
        .is_none());
}
#[test]
fn changed_scope_operation_or_unknown_creator_never_becomes_a_valid_author() {
    let (info, request, response) = fixture();
    for case in 0..7 {
        let mut response = response.clone();
        let ExtensionResult::Data { payload: p } = &mut response.result else {
            unreachable!()
        };
        match case {
            0 => p["reference"]["context"]["journalIncarnation"] = "foreign".into(),
            1 => p["reference"]["operationId"] = "other-task".into(),
            2 => p["creator"]["kind"] = "unknown".into(),
            3 => p["creator"]["controllerId"] = "".into(),
            4 => p["workerGeneration"] = "old-worker".into(),
            5 => p["creator"]["extraPermission"] = true.into(),
            _ => p["provider"]["buildId"] = "other-build".into(),
        }
        assert!(
            decode_operation_attribution(response, &info, &request).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn old_worker_and_foreign_context_are_refused_before_extension_dispatch() {
    let (mut info, request, _) = fixture();
    assert!(operation_attribution_envelope(&info, &request).is_ok());
    info.fenced_requests
        .as_mut()
        .unwrap()
        .context
        .journal_incarnation = "new".into();
    assert!(operation_attribution_envelope(&info, &request).is_err());
    info.contract_catalog = None;
    assert!(operation_attribution_envelope(&info, &request).is_err());
}
