use winbridge_protocol::{CommandSource, TaskSubmission, PROTOCOL_VERSION};

#[test]
fn direct_command_preserves_windows_path_and_literal_arguments() {
    let wire = r#"{
        "protocolVersion":1,"requestId":"request-1","projectId":"sample",
        "idempotencyKey":"task-1","timeoutMs":60000,
        "command":{"kind":"direct","executable":"C:\\Program Files\\Tool\\tool.exe",
            "arguments":["--label","a & b","$value","a path with spaces"],
            "workingDirectory":"src"}
    }"#;
    let submission: TaskSubmission = serde_json::from_str(wire).unwrap();
    assert_eq!(submission.protocol_version, PROTOCOL_VERSION);
    let CommandSource::Direct {
        executable,
        arguments,
        working_directory,
    } = &submission.command
    else {
        panic!("expected a direct command");
    };
    assert_eq!(executable, r"C:\Program Files\Tool\tool.exe");
    assert_eq!(
        arguments,
        &["--label", "a & b", "$value", "a path with spaces"]
    );
    assert_eq!(working_directory, "src");
    let round_trip: TaskSubmission =
        serde_json::from_str(&serde_json::to_string(&submission).unwrap()).unwrap();
    assert_eq!(submission, round_trip);
}

#[test]
fn rejects_unrecognized_execution_mode_and_misspelled_fields() {
    for command in [
        r#"{"kind":"shell","script":"echo hidden action"}"#,
        r#"{"kind":"direct","executable":"tool.exe","arguments":[],"workingDirectory":".","workingDirecotry":"elsewhere"}"#,
    ] {
        let wire = format!(
            r#"{{"protocolVersion":1,"requestId":"r","projectId":"p","idempotencyKey":"k","timeoutMs":1000,"command":{command}}}"#
        );
        assert!(serde_json::from_str::<TaskSubmission>(&wire).is_err());
    }
}

#[test]
fn documented_requests_follow_the_wire_contract() {
    for example in [
        include_str!("../../../examples/task.direct.json"),
        include_str!("../../../examples/task.recipe.json"),
    ] {
        let request: TaskSubmission = serde_json::from_str(example).unwrap();
        assert_eq!(request.protocol_version, PROTOCOL_VERSION);
    }
}

#[test]
fn attention_targets_are_additive_and_legacy_records_keep_their_wire_shape() {
    use winbridge_protocol::{
        AttentionEvent, AttentionRecord, AttentionTarget, LocalRequest, WorkerInfo,
    };
    let legacy = serde_json::json!({
        "attentionId":"a", "sequence":17, "projectId":"p", "taskId":"t",
        "severity":"warning", "message":"check", "origin":"agent_request",
        "createdAtMs":100, "acknowledgedAtMs":null
    });
    let old: AttentionRecord = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap(), legacy);
    let mut generic = serde_json::to_value(AttentionEvent::from(old)).unwrap();
    assert!(generic.get("taskId").is_none());
    for target in [
        serde_json::json!({"kind":"task", "taskId":"t"}),
        serde_json::json!({"kind":"workflow", "workflowId":"w"}),
        serde_json::json!({"kind":"application", "applicationId":"a", "workflowId":"w"}),
    ] {
        generic["target"] = target.clone();
        let event: AttentionEvent = serde_json::from_value(generic.clone()).unwrap();
        assert_eq!(serde_json::to_value(event.target).unwrap(), target);
    }
    assert!(
        serde_json::from_value::<AttentionTarget>(serde_json::json!({"kind":"workflow"})).is_err()
    );
    for wire in [
        serde_json::json!({"method":"list_attention_events", "afterSequence":17,"pendingOnly":true}),
        serde_json::json!({"method":"acknowledge_attention_event", "attentionId":"a"}),
    ] {
        let request: LocalRequest = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), wire);
    }
    let old_info: WorkerInfo = serde_json::from_value(serde_json::json!({
        "protocolVersion":1,"instanceId":"i","nodeId":"n","generation":"g",
        "platform":"windows","runtimeValidation":"windows-native","projects":[],"attentionAvailable":true
    })).unwrap();
    assert!(
        !old_info.attention_targets_available,
        "old worker must use the legacy feed"
    );
}
