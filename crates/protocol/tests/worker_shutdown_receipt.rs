use serde_json::{json, Value};
use winbridge_protocol::{build_identity, update::*};
fn command() -> WorkerShutdownCommand {
    WorkerShutdownCommand {
        command_id: "shutdown-fixture".into(),
        expected_lease_revision: 4,
        intent_sha256: "a".repeat(64),
        reason: "Exact accepted stop".into(),
    }
}
fn packet() -> Value {
    json!({"controlVersion":6,"updateId":"update","launchId":"launch",
        "binding":{"context":{"instanceId":"instance","journalIncarnation":"journal"},"generation":"generation","build":build_identity(),
            "process":{"processId":42,"birth":"123","hostId":"a".repeat(64),"sessionId":1},"configSha256":"b".repeat(64),"endpoint":"endpoint"},
        "result":{"kind":"shutdown_receipt","command":command(),"observedAtMs":30,"receipt":{
            "schemaVersion":1,"updateId":"update","launchId":"launch","updaterOwnerId":"owner","updaterEpoch":2,"command":command(),
            "lease":{"spec":{"updateId":"update","instanceId":"instance","journalIncarnation":"journal","workerGeneration":"generation",
                "updaterOwnerId":"owner","updaterEpoch":2,"sourceDirectory":"K:\\source","baselineManifestSha256":"c".repeat(64),"targetManifestSha256":"d".repeat(64),"projects":["source"]},
                "revision":4,"phase":"quiesced","acceptedAtMs":1,"updatedAtMs":2,"reason":"Quiesced"},
            "process":{"processId":42,"birth":"123","hostId":"a".repeat(64),"sessionId":1},
            "state":{"stateDirectory":"K:\\state","directoryIdentity":"directory","journalIdentity":"journal","lockIdentity":"lock"},"acceptedAtMs":20}}})
}
fn parse(v: Value) -> WorkerUpdateControlResponse {
    serde_json::from_value(v).unwrap()
}
#[test]
fn shutdown_acceptance_is_v6_scoped_history_not_exit_or_lease_grant() {
    let receipt = parse(packet())
        .into_shutdown_receipt(&command())
        .unwrap()
        .unwrap();
    assert_eq!(receipt.lease.phase, WorkerUpdatePhase::Quiesced);
    assert!(parse(packet()).into_lease().is_err());
    assert!(parse(packet()).into_runtime_identity().is_err());
    assert!(parse(packet()).into_shutdown_state().is_err());
    for version in 1..=5 {
        let mut v = packet();
        v["controlVersion"] = version.into();
        assert!(parse(v).into_shutdown_receipt(&command()).is_err());
    }
    let mut absent = packet();
    absent["result"]["receipt"] = Value::Null;
    assert!(parse(absent)
        .into_shutdown_receipt(&command())
        .unwrap()
        .is_none());
    for (pointer, replacement) in [
        ("/result/command/commandId", json!("different")),
        (
            "/result/receipt/command/intentSha256",
            json!("b".repeat(64)),
        ),
        ("/result/receipt/process/birth", json!("124")),
        ("/result/receipt/launchId", json!("other")),
        (
            "/result/receipt/lease/spec/journalIncarnation",
            json!("other"),
        ),
        (
            "/result/receipt/lease/spec/workerGeneration",
            json!("other"),
        ),
        ("/result/receipt/lease/spec/updaterEpoch", json!(3)),
        ("/result/receipt/lease/phase", json!("released")),
        ("/result/receipt/lease/revision", json!(5)),
        ("/result/receipt/state/lockIdentity", json!("journal")),
        ("/result/observedAtMs", json!(0)),
        ("/result/receipt/acceptedAtMs", json!(u64::MAX)),
    ] {
        let mut v = packet();
        *v.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            parse(v).into_shutdown_receipt(&command()).is_err(),
            "{pointer}"
        );
    }
    let mut v = packet();
    v["result"]["receipt"]["processExited"] = true.into();
    assert!(serde_json::from_value::<WorkerUpdateControlResponse>(v).is_err());
}
#[test]
fn shutdown_state_is_worker_computed_observation_without_fresh_acceptance() {
    let mut v = packet();
    v["result"] = json!({"kind":"shutdown_state","state":v["result"]["receipt"]["state"]});
    parse(v.clone()).into_shutdown_state().unwrap();
    assert!(parse(v.clone()).into_shutdown_receipt(&command()).is_err());
    v["controlVersion"] = 1.into();
    assert!(parse(v.clone()).into_shutdown_state().is_err());
    v["controlVersion"] = 6.into();
    v["result"]["state"]["stateDirectory"] = "\u{0}".into();
    assert!(parse(v).into_shutdown_state().is_err());
}
