use winbridge_protocol::*;

fn request() -> OperationPageRequest {
    OperationPageRequest {
        context: RequestContext {
            instance_id: "pages".into(),
            journal_incarnation: "journal".into(),
        },
        query: OperationQuery {
            project_id: None,
            kind: None,
            search: String::new(),
            limit: 2,
            cursor: None,
        },
    }
}
fn reply(request: &OperationPageRequest) -> ExtensionResponse {
    let record = OperationObservation {
        reference: OperationRef {
            context: request.context.clone(),
            kind: HistoryKind::Task,
            operation_id: "task".into(),
        },
        project_id: "fixture".into(),
        state: OperationState::Execution {
            status: TaskStatus::Succeeded,
        },
        revision: "9007199254740993".into(),
        label: "fixture".into(),
        reason: None,
        text_truncated: false,
        accepted_at_ms: 1,
        finished_at_ms: Some(2),
        last_event_sequence: Some("9223372036854775807".into()),
        related: vec![],
        resources: vec![],
        resources_truncated: false,
    };
    ExtensionResponse {
        extension_version: 1,
        contract: operation_contract(),
        context: Some(request.context.clone()),
        result: ExtensionResult::Data {
            payload: serde_json::to_value(OperationPage {
                schema_version: 1,
                context: request.context.clone(),
                observation_id: "observation".into(),
                observed_at_ms: 3,
                records: vec![record],
                next_cursor: None,
                has_more: false,
                read_only: false,
            })
            .unwrap(),
        },
    }
}
#[test]
fn scoped_page_decoder_preserves_large_counters_and_rejects_mixed_or_future_pages() {
    let request = request();
    let good = reply(&request);
    let page = decode_operation_page(good.clone(), &request).unwrap();
    assert_eq!(page.records[0].revision, "9007199254740993");
    for mutation in 0..7 {
        let mut bad = good.clone();
        let ExtensionResult::Data { payload } = &mut bad.result else {
            unreachable!()
        };
        match mutation {
            0 => payload["schemaVersion"] = 99.into(),
            1 => {
                payload["records"][0]["reference"]["context"]["journalIncarnation"] =
                    "foreign".into()
            }
            2 => payload["records"][0]["revision"] = "01".into(),
            3 => {
                payload["records"][0]["state"] =
                    serde_json::json!({"family":"application","status":"ready"})
            }
            4 => {
                payload["records"] = serde_json::json!([
                    payload["records"][0].clone(),
                    payload["records"][0].clone()
                ])
            }
            5 => bad.context = None,
            _ => bad.contract.version = 2,
        }
        assert!(
            decode_operation_page(bad, &request).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn pagination_reply_cannot_replace_the_original_watermarks() {
    let mut request = request();
    request.query.cursor = Some(OperationCursor {
        context: request.context.clone(),
        query_sha256: operation_query_sha256(&request.query).unwrap(),
        watermarks: AdmissionWatermarks {
            tasks: "20".into(),
            imports: "0".into(),
            workflows: "0".into(),
            applications: "0".into(),
        },
        before: HistoryCursor {
            created_at_ms: 5,
            kind: HistoryKind::Task,
            operation_id: "z".into(),
        },
    });
    let mut response = reply(&request);
    let ExtensionResult::Data { payload } = &mut response.result else {
        unreachable!()
    };
    let mut next = request.query.cursor.clone().unwrap();
    next.before.created_at_ms = 1;
    next.before.operation_id = "task".into();
    payload["hasMore"] = true.into();
    payload["nextCursor"] = serde_json::to_value(&next).unwrap();
    assert!(decode_operation_page(response.clone(), &request).is_ok());
    let ExtensionResult::Data { payload } = &mut response.result else {
        unreachable!()
    };
    payload["nextCursor"]["watermarks"]["tasks"] = "21".into();
    assert!(decode_operation_page(response, &request).is_err());
}
