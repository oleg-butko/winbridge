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
