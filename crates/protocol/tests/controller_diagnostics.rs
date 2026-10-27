use winbridge_protocol::*;
#[test]
fn diagnostic_envelope_is_disjoint_from_worker_rpc_and_controller_api() {
    let value = serde_json::json!({"controllerDiagnosticsVersion":1,"request":{"method":"controller_connections","request":{"context":{"instanceId":"pair","journalIncarnation":"journal"}}}});
    let request: ControllerDiagnosticsRequest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(request.request.method(), "controller_connections");
    assert!(serde_json::from_value::<WireRequest>(value.clone()).is_err());
    assert!(serde_json::from_value::<ControllerApiRequest>(value.clone()).is_err());
    for method in [
        "submit",
        "cancel",
        "verify_pair",
        "operation_page",
        "future",
    ] {
        let mut v = value.clone();
        v["request"]["method"] = method.into();
        assert!(serde_json::from_value::<ControllerDiagnosticsRequest>(v).is_err());
    }
    for key in ["controllerContractVersion", "method", "payload", "adapter"] {
        let mut v = value.clone();
        v[key] = serde_json::json!("unexpected");
        assert!(serde_json::from_value::<ControllerDiagnosticsRequest>(v).is_err());
    }
}
#[test]
fn reply_version_kind_schema_and_scope_are_pinned() {
    let context = RequestContext {
        instance_id: "pair".into(),
        journal_incarnation: "journal".into(),
    };
    let request = ControllerDiagnosticsRequest {
        controller_diagnostics_version: 1,
        request: ControllerDiagnosticQuery::ControllerConnections {
            request: ControllerConnectionsRequest {
                context: context.clone(),
            },
        },
    };
    let data = ControllerConnections {
        schema_version: 1,
        context: context.clone(),
        provider: build_identity(),
        worker_generation: "worker".into(),
        observed_at_ms: 0,
        primary: None,
        connections: vec![],
        rpc: NetworkRpcObservation {
            in_flight: 0,
            limit: 32,
            admitted: "0".into(),
            completed: "0".into(),
            rejected: "0".into(),
        },
    };
    let response = ControllerDiagnosticsResponse {
        controller_diagnostics_version: 1,
        result: ControllerDiagnosticResult::ControllerConnections {
            observation: Box::new(data),
        },
    };
    response.validate_for(&request).unwrap();
    for fault in 0..4 {
        let mut r = response.clone();
        if fault == 0 {
            r.controller_diagnostics_version = 2;
        } else if let ControllerDiagnosticResult::ControllerConnections { observation } =
            &mut r.result
        {
            match fault {
                1 => observation.context.journal_incarnation = "foreign".into(),
                2 => observation.schema_version = 2,
                _ => observation.worker_generation.clear(),
            }
        }
        assert!(r.validate_for(&request).is_err());
    }
    let wrong = ControllerDiagnosticsRequest {
        controller_diagnostics_version: 1,
        request: ControllerDiagnosticQuery::OperationAttribution {
            request: OperationAttributionRequest {
                context,
                query: OperationAttributionQuery {
                    kind: HistoryKind::Task,
                    operation_id: "task".into(),
                },
            },
        },
    };
    assert!(response.validate_for(&wrong).is_err());
    ControllerDiagnosticsResponse::error("offline".into())
        .validate_for(&request)
        .unwrap();
}
