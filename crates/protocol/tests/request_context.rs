use winbridge_protocol::*;

fn context() -> RequestContext {
    RequestContext {
        instance_id: "pair".into(),
        journal_incarnation: "journal-a".into(),
    }
}

#[test]
fn closed_v1_parsers_cannot_accept_fenced_requests_or_replies() {
    let request = FencedRequest {
        request_contract_version: 2,
        context: context(),
        request: LocalRequest::Cancel {
            task_id: "task".into(),
        },
    };
    let value = serde_json::to_value(&request).unwrap();
    assert!(serde_json::from_value::<LocalRequest>(value.clone()).is_err());
    assert!(matches!(
        serde_json::from_value::<WireRequest>(value).unwrap(),
        WireRequest::Fenced(_)
    ));
    let response = FencedResponse::unconfirmed("lost response".into());
    let value = serde_json::to_value(&response).unwrap();
    assert!(serde_json::from_value::<LocalResponse>(value.clone()).is_err());
    assert!(matches!(
        serde_json::from_value::<WireResponse>(value).unwrap(),
        WireResponse::Fenced(_)
    ));
}

#[test]
fn legacy_frames_are_byte_identical_and_context_fields_are_required() {
    let request = LocalRequest::Info;
    assert_eq!(
        serde_json::to_vec(&request).unwrap(),
        serde_json::to_vec(&WireRequest::Legacy(request.clone())).unwrap()
    );
    let message = SessionMessage::Request {
        id: "rpc-1".into(),
        request,
    };
    assert_eq!(
        serde_json::to_vec(&message).unwrap(),
        serde_json::to_vec(&SessionFrame::Legacy(message)).unwrap()
    );
    for value in [
        serde_json::json!({"requestContractVersion":2,"request":{"method":"info"}}),
        serde_json::json!({"requestContractVersion":2,"context":{"instanceId":"pair"},"request":{"method":"info"}}),
        serde_json::json!({"requestContractVersion":2,"context":{"instanceId":"pair","journalIncarnation":"a","generation":"b"},"request":{"method":"info"}}),
    ] {
        assert!(serde_json::from_value::<WireRequest>(value).is_err());
    }
}

#[test]
fn reply_cannot_claim_success_in_a_foreign_or_unconfirmed_scope() {
    let mut response = FencedResponse {
        request_contract_version: 2,
        context: Some(context()),
        outcome: FencedReplyOutcome::Responded,
        response: LocalResponse::Tasks { tasks: vec![] },
    };
    response.validate_for(&context()).unwrap();
    response.context.as_mut().unwrap().journal_incarnation = "foreign".into();
    assert!(response.validate_for(&context()).is_err());
    response.context = Some(context());
    response.outcome = FencedReplyOutcome::Unconfirmed;
    assert!(response.validate_for(&context()).is_err());
    FencedResponse::unconfirmed("unknown effect".into())
        .validate_for(&context())
        .unwrap();
}
