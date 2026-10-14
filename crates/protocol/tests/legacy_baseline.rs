//! Frozen v1 inputs: changes require an explicit compatibility decision.
use serde_json::{json, Value};
use winbridge_protocol::{LocalResponse, SessionMessage, TaskRecord, TaskStatus, TaskSubmission};

const HELLO: &str = include_str!("../../../tests/fixtures/legacy-v1/session-hello.json");
const SUBMIT: &str = include_str!("../../../tests/fixtures/legacy-v1/session-submit.json");
const RECORD: &str = include_str!("../../../tests/fixtures/legacy-v1/task-record.json");
const FINGERPRINT: &str = include_str!("../../../tests/fixtures/legacy-v1/task-fingerprint.txt");
const STATUSES: &str = include_str!("../../../tests/fixtures/legacy-v1/task-statuses.json");
#[path = "../../../tests/fixtures/legacy-v1/worker-info-codec.rs"]
mod frozen_info;

const INFO: &str = include_str!("../../../tests/fixtures/legacy-v1/worker-info.json");

#[test]
fn frozen_session_messages_preserve_their_v1_wire_shape() {
    for fixture in [HELLO, SUBMIT] {
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let message: SessionMessage = serde_json::from_str(fixture).unwrap();
        assert_eq!(serde_json::to_value(message).unwrap(), expected);
    }
}

#[test]
fn task_fingerprint_keeps_legacy_bytes_and_omits_only_correlation() {
    let wire: Value = serde_json::from_str(SUBMIT).unwrap();
    let mut submission: TaskSubmission =
        serde_json::from_value(wire["request"]["request"].clone()).unwrap();
    submission.request_id.clear();
    assert_eq!(
        serde_json::to_string(&submission).unwrap(),
        FINGERPRINT.trim_end(),
        "a DTO field/default/order change must not silently redefine old dedup payloads"
    );
}

#[test]
fn historical_record_loads_without_rewriting_legacy_fields() {
    let legacy: Value = serde_json::from_str(RECORD).unwrap();
    let record: TaskRecord = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(record.status, TaskStatus::Running);
    assert_eq!(record.process_id, None, "running can be a launch intent");
    assert!(record.recipe_origin.is_none());
    assert!(record.snapshot.is_none());
    assert!(record.output_removed_at_ms.is_none());
    assert_eq!(record.removed_output_bytes, 0);
    let mut expected = legacy;
    expected["snapshot"] = Value::Null;
    expected["outputRemovedAtMs"] = Value::Null;
    expected["removedOutputBytes"] = json!(0);
    assert_eq!(serde_json::to_value(record).unwrap(), expected);
}

#[test]
fn task_status_set_and_terminal_projection_remain_v1() {
    let statuses: Vec<TaskStatus> = serde_json::from_str(STATUSES).unwrap();
    assert_eq!(
        statuses,
        vec![
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Succeeded,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::Interrupted,
            TaskStatus::Indeterminate,
        ]
    );
    for (index, status) in statuses.iter().enumerate() {
        assert_eq!(status.is_terminal(), index >= 2);
    }
    assert_eq!(
        serde_json::to_value(statuses).unwrap(),
        serde_json::from_str::<Value>(STATUSES).unwrap()
    );
    assert!(serde_json::from_value::<TaskStatus>(json!("starting")).is_err());
}

#[test]
fn unnegotiated_hello_and_execution_extensions_fail_closed() {
    let hello: Value = serde_json::from_str(HELLO).unwrap();
    let mut unknown_capability = hello.clone();
    unknown_capability["peer"]["capabilities"][0] = json!("executor_run");
    let mut extra_peer_field = hello.clone();
    extra_peer_field["peer"]["contractCatalog"] = json!([]);
    let mut extra_envelope_field = hello;
    extra_envelope_field["contractCatalog"] = json!([]);
    let submit: Value = serde_json::from_str(SUBMIT).unwrap();
    let mut extra_command_field = submit.clone();
    extra_command_field["request"]["request"]["contractVersion"] = json!(2);
    let mut unknown_method = submit;
    unknown_method["request"]["method"] = json!("run_executor");
    for wire in [
        unknown_capability,
        extra_peer_field,
        extra_envelope_field,
        extra_command_field,
        unknown_method,
    ] {
        assert!(
            serde_json::from_value::<SessionMessage>(wire.clone()).is_err(),
            "legacy commands cannot negotiate an extension by ignoring it: {wire}"
        );
    }
}

#[test]
fn optional_info_metadata_is_readable_but_not_forwarded_by_the_legacy_codec() {
    let mut wire: Value = serde_json::from_str(INFO).unwrap();
    wire["info"]["contractCatalog"] = json!({"supported": ["executor.run@1"]});
    let info: frozen_info::FrozenV1WorkerInfo =
        serde_json::from_value(wire["info"].clone()).unwrap();
    assert_eq!(info.instance_id, "legacy-fixture");
    assert!(!info.workflow_available);
    assert!(info.active_task_count.is_none());
    let forwarded = json!({"kind":"info","info":serde_json::to_value(info).unwrap()});
    assert!(forwarded["info"].get("contractCatalog").is_none());
    let LocalResponse::Info { info } = serde_json::from_value(forwarded).unwrap() else {
        panic!("expected forwarded worker info");
    };
    assert_eq!(info.node_id, "fixture-windows");
    // This tests the codec boundary, not a negotiated network route.
}
