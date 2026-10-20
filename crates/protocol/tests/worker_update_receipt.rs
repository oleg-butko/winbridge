use serde_json::{json, Value};
use winbridge_protocol::{build_identity, update::*};

fn command() -> WorkerUpdateCommand {
    WorkerUpdateCommand {
        command_id: "saved-command".into(),
        expected_revision: 1,
        action: WorkerUpdateAction::BeginDrain,
        source_manifest_sha256: None,
        reason: "Drain accepted work".into(),
    }
}
fn packet() -> Value {
    json!({
        "controlVersion":4,"updateId":"update","launchId":"launch",
        "binding":{"context":{"instanceId":"instance","journalIncarnation":"journal"},
            "generation":"generation","build":build_identity(),
            "process":{"processId":42,"birth":"123","hostId":"a".repeat(64),"sessionId":1},
            "configSha256":"b".repeat(64),"endpoint":"endpoint"},
        "result":{"kind":"command_receipt","command":command(),"observedAtMs":3,
            "receipt":{"spec":{"updateId":"update","instanceId":"instance","journalIncarnation":"journal",
                "workerGeneration":"generation","updaterOwnerId":"owner","updaterEpoch":2,
                "sourceDirectory":"K:\\source","baselineManifestSha256":"c".repeat(64),
                "targetManifestSha256":"d".repeat(64),"projects":["project"]},
                "revision":2,"phase":"draining","acceptedAtMs":1,"updatedAtMs":2,"reason":"Drain accepted work"}}
    })
}
fn parse(value: Value) -> WorkerUpdateControlResponse {
    serde_json::from_value(value).unwrap()
}
#[test]
fn exact_receipt_and_missing_receipt_have_explicit_v4_semantics() {
    let r = parse(packet())
        .into_command_receipt(&command())
        .unwrap()
        .unwrap();
    assert_eq!(r.phase, WorkerUpdatePhase::Draining);
    let mut value = packet();
    value["result"]["receipt"] = Value::Null;
    assert!(parse(value)
        .into_command_receipt(&command())
        .unwrap()
        .is_none());
    for version in [1, 2, 3] {
        let mut value = packet();
        value["controlVersion"] = version.into();
        assert!(parse(value).into_command_receipt(&command()).is_err());
    }
    assert!(parse(packet()).into_lease().is_err());
    assert!(parse(packet()).into_runtime_identity().is_err());
    assert!(parse(packet()).into_runtime_compatibility().is_err());
}
#[test]
fn receipt_cannot_be_rebound_to_another_command_scope_revision_or_phase() {
    for (pointer, replacement) in [
        ("/updateId", json!("other")),
        ("/result/command/commandId", json!("other")),
        ("/result/command/reason", json!("other")),
        ("/result/receipt/spec/instanceId", json!("other")),
        ("/result/receipt/spec/journalIncarnation", json!("other")),
        ("/result/receipt/spec/workerGeneration", json!("other")),
        ("/result/receipt/revision", json!(3)),
        ("/result/receipt/phase", json!("quiesced")),
        ("/result/receipt/reason", json!("other command")),
        ("/result/observedAtMs", json!(0)),
    ] {
        let mut value = packet();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            parse(value).into_command_receipt(&command()).is_err(),
            "{pointer}"
        );
    }
    let mut value = packet();
    value["result"] = json!({"kind":"snapshot","lease":null});
    assert!(parse(value).into_command_receipt(&command()).is_err());
    let mut value = packet();
    value["result"]["authority"] = true.into();
    assert!(serde_json::from_value::<WorkerUpdateControlResponse>(value).is_err());
}
