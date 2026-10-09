use serde_json::{json, Value};
use winbridge_controller::mcp;
use winbridge_core::controller_config::ControllerConfig;

fn config(temp: &tempfile::TempDir) -> ControllerConfig {
    serde_json::from_value(json!({"schemaVersion":1,"instanceId":"mcp-test","nodeId":"debian","stateDirectory":temp.path().join("state"),"controller":{"listenAddress":"127.0.0.1:1143","workerNodeId":"test-worker"}})).unwrap()
}
fn init(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
async fn exchange(bytes: &[u8]) -> (Vec<Value>, Result<(), String>) {
    let temp = tempfile::tempdir().unwrap();
    let mut output = Vec::new();
    let result = mcp::run(config(&temp), bytes, &mut output).await;
    let messages = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    (messages, result)
}
fn lines(messages: &[Value]) -> Vec<u8> {
    messages
        .iter()
        .flat_map(|m| {
            let mut bytes = serde_json::to_vec(m).unwrap();
            bytes.push(b'\n');
            bytes
        })
        .collect()
}

#[tokio::test]
async fn lifecycle_pins_version_and_enforces_initialization_with_clean_stdout() {
    let input = lines(&[
        json!({"jsonrpc":"2.0","id":"early","method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":"ping","method":"ping"}),
        init("future-unsupported"),
        json!({"jsonrpc":"2.0","id":"not-ready","method":"tools/list"}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":"catalog","method":"tools/list"}),
        init(mcp::VERSION),
        json!({"jsonrpc":"2.0","id":"unsupported","method":"resources/list"}),
    ]);
    let (replies, result) = exchange(&input).await;
    result.unwrap();
    assert_eq!(replies.len(), 7);
    assert_eq!(replies[0]["error"]["code"], -32000);
    assert_eq!(replies[1]["result"], json!({}));
    assert_eq!(replies[2]["result"]["protocolVersion"], mcp::VERSION);
    assert_eq!(
        replies[2]["result"]["capabilities"],
        json!({"tools":{"listChanged":false}})
    );
    assert_eq!(replies[3]["error"]["code"], -32000);
    assert!(replies[4]["result"]["tools"].as_array().unwrap().len() > 20);
    assert_eq!(replies[5]["error"]["code"], -32602);
    assert_eq!(replies[6]["error"]["code"], -32601);
}
#[tokio::test]
async fn malformed_input_partial_eof_batches_and_frame_limits_do_not_dispatch() {
    let mut input = b"not-json\n\xff\n[]\n".to_vec();
    input.extend(lines(&[
        json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping","params":[]}),
        json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"winbridge_submit","arguments":{}}}),
    ]));
    let (messages, result) = exchange(&input).await;
    result.unwrap();
    assert_eq!(messages.len(), 6);
    assert_eq!(messages[0]["error"]["code"], -32700);
    assert_eq!(messages[1]["error"]["code"], -32700);
    assert!(messages[2..].iter().all(|m| m["error"]["code"] == -32600));
    let (messages, result) = exchange(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}").await;
    assert!(result.unwrap_err().contains("not dispatched"));
    assert!(messages.is_empty());
    let (messages, result) = exchange(&vec![b'x'; mcp::MAX_LINE_BYTES + 1]).await;
    assert!(result.unwrap_err().contains("exceeds"));
    assert_eq!(messages.len(), 1);
}
#[tokio::test]
async fn tool_argument_errors_are_protocol_errors_but_offline_rpc_is_a_tool_error() {
    for version in [mcp::VERSION, "2025-06-18", "2025-03-26"] {
        let input = lines(&[
            init(version),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"absent"}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"winbridge_submit","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"winbridge_info","arguments":{"method":"submit"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"winbridge_info","arguments":{"extra":true}}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/list","params":{"cursor":"unknown"}}),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"winbridge_info"}}),
        ]);
        let (messages, result) = exchange(&input).await;
        result.unwrap();
        assert_eq!(messages[0]["result"]["protocolVersion"], version);
        assert!(messages[1..6].iter().all(|m| m["error"]["code"] == -32602));
        let result = &messages[6]["result"];
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Do not automatically replay"));
        assert_eq!(
            result.get("structuredContent").is_some(),
            version != "2025-03-26"
        );
    }
}
#[test]
fn catalog_is_generated_from_tagged_protocol_and_has_only_reachable_definitions() {
    let tools = mcp::tools().unwrap();
    let request =
        serde_json::to_value(schemars::schema_for!(winbridge_protocol::LocalRequest)).unwrap();
    assert_eq!(
        tools.len(),
        request["oneOf"].as_array().unwrap().len() + 2 - 4
    );
    assert!(!tools
        .iter()
        .any(|t| t["name"] == "winbridge_save_runtime_settings"
            || t["name"] == "winbridge_apply_output_cleanup"));
    for tool in &tools {
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"].get("method").is_none());
        assert!(!schema["required"]
            .as_array()
            .is_some_and(|r| r.contains(&json!("method"))));
    }
    for name in ["winbridge_import_delta_snapshot", "winbridge_publish_delta"] {
        let delta = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(delta["annotations"]["idempotentHint"], true);
        assert_eq!(delta["annotations"]["readOnlyHint"], false);
    }
    let info = tools
        .iter()
        .find(|t| t["name"] == "winbridge_info")
        .unwrap();
    assert_eq!(info["annotations"]["readOnlyHint"], true);
    assert!(info["inputSchema"].get("definitions").is_none());
    let submit = tools
        .iter()
        .find(|t| t["name"] == "winbridge_submit")
        .unwrap();
    assert_eq!(
        submit["inputSchema"]["properties"]["request"]["$ref"],
        "#/definitions/TaskSubmission"
    );
    assert!(submit["inputSchema"]["definitions"]
        .get("TaskSubmission")
        .is_some());
    assert!(submit["inputSchema"]["definitions"]
        .get("WorkflowSubmission")
        .is_none());
    assert_eq!(submit["annotations"]["openWorldHint"], true);
    assert!(serde_json::to_vec(&tools).unwrap().len() < 100_000);
}

#[tokio::test]
async fn local_maintenance_requests_are_refused_without_a_worker_session() {
    let temp = tempfile::tempdir().unwrap();
    let controller = winbridge_controller::Controller::new(config(&temp)).unwrap();
    for request in [
        winbridge_protocol::LocalRequest::GetRuntimeSettings,
        winbridge_protocol::LocalRequest::PreviewOutputCleanup {
            query: winbridge_protocol::OutputCleanupQuery {
                older_than_days: 30,
                project_id: None,
            },
        },
    ] {
        let winbridge_protocol::LocalResponse::Error { message } = controller.call(request).await
        else {
            panic!("Local maintenance must not be forwarded")
        };
        assert!(message.contains("not transmitted"));
        assert!(message.contains("local IPC"));
    }
}
