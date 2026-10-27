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
    info.generation = "current".into();
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
    let page = OperationCancellation {
        schema_version: 1,
        reference: OperationRef {
            context: context.clone(),
            kind: HistoryKind::Task,
            operation_id: "task".into(),
        },
        intent: Some(CancellationIntent {
            requested_at_ms: 1,
            actor: Some(OperationCreator::Local {
                worker_generation: "historical".into(),
            }),
            inherited_from: Some(OperationRef {
                context: context.clone(),
                kind: HistoryKind::Application,
                operation_id: "app".into(),
            }),
        }),
        provider: build_identity(),
        worker_generation: info.generation.clone(),
        observed_at_ms: 2,
    };
    (
        info,
        request,
        ExtensionResponse {
            extension_version: 1,
            contract: operation_cancellation_contract(),
            context: Some(context),
            result: ExtensionResult::Data {
                payload: serde_json::to_value(page).unwrap(),
            },
        },
    )
}
#[test]
fn cancellation_keeps_historical_actor_and_absent_metadata_is_not_filled() {
    let (info, request, mut response) = fixture();
    assert!(operation_cancellation_envelope(&info, &request).is_ok());
    assert!(
        matches!(decode_operation_cancellation(response.clone(),&info,&request).unwrap().intent.unwrap().actor,Some(OperationCreator::Local {worker_generation}) if worker_generation=="historical")
    );
    let ExtensionResult::Data { payload } = &mut response.result else {
        unreachable!()
    };
    payload["intent"] = serde_json::Value::Null;
    assert!(decode_operation_cancellation(response, &info, &request)
        .unwrap()
        .intent
        .is_none());
}
#[test]
fn malformed_actor_parent_time_scope_and_provider_are_refused() {
    let (info, request, response) = fixture();
    for case in 0..10 {
        let mut response = response.clone();
        let ExtensionResult::Data { payload: p } = &mut response.result else {
            unreachable!()
        };
        match case {
            0 => p["reference"]["context"]["journalIncarnation"] = "foreign".into(),
            1 => p["intent"]["actor"]["kind"] = "unknown".into(),
            2 => p["intent"]["inheritedFrom"]["context"]["journalIncarnation"] = "foreign".into(),
            3 => p["intent"]["inheritedFrom"]["kind"] = "task".into(),
            4 => p["intent"]["requestedAtMs"] = 9_007_199_254_740_992u64.into(),
            5 => p["provider"]["buildId"] = "other".into(),
            6 => p["workerGeneration"] = "previous".into(),
            7 => p["reference"]["operationId"] = "other".into(),
            8 => p["intent"]["actor"]["workerGeneration"] = "".into(),
            _ => response.contract.version = 2,
        }
        assert!(
            decode_operation_cancellation(response, &info, &request).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn unsupported_worker_or_rebound_scope_prevents_negotiation() {
    let (mut info, request, _) = fixture();
    info.fenced_requests
        .as_mut()
        .unwrap()
        .context
        .journal_incarnation = "new".into();
    assert!(operation_cancellation_envelope(&info, &request).is_err());
    info.contract_catalog = None;
    assert!(operation_cancellation_envelope(&info, &request).is_err());
}
