//! Contract-only fixtures; no OS exit, database backup or execution permission.
use serde_json::{json, Value};
use winbridge_protocol::{artifact_descriptor, update::*, ArtifactRole};
fn packet() -> Value {
    let id = "00000000-0000-4000-8000-000000000001";
    let journal = "00000000-0000-4000-8000-000000000002";
    let process = json!({"processId":42,"birth":"123","hostId":"host","sessionId":1});
    let state = json!({"stateDirectory":"K:\\state","directoryIdentity":"directory","journalIdentity":"journal","lockIdentity":"lock"});
    let authority = json!({"incarnation":journal,"epoch":7,"fingerprintSha256":"a".repeat(64)});
    let mut descriptor = artifact_descriptor(ArtifactRole::Worker);
    descriptor.target_os = "windows".into();
    let command = json!({"commandId":"shutdown-owned","expectedLeaseRevision":4,"intentSha256":"b".repeat(64),"reason":"Accepted stop"});
    let lease = json!({"spec":{"updateId":id,"instanceId":"instance","journalIncarnation":journal,"workerGeneration":"generation","updaterOwnerId":"owner","updaterEpoch":2,"sourceDirectory":"K:\\source","baselineManifestSha256":"c".repeat(64),"targetManifestSha256":"d".repeat(64),"projects":["source"]},"revision":4,"phase":"quiesced","acceptedAtMs":1,"updatedAtMs":2,"reason":"Quiesced"});
    json!({"schemaVersion":1,"kind":"worker_state_backup","inputSha256":"e".repeat(64),"updateId":id,"attemptId":"00000000-0000-4000-8000-000000000003",
        "helperDescriptor":descriptor,"helperProcess":{"processId":43,"birth":"124","hostId":"host","sessionId":1},
        "helperImage":{"path":"K:\\runtime\\winbridge-worker.exe","bytes":1234,"sha256":"f".repeat(64),"fileIdentity":"image","querySupported":true},
        "workerConfig":{"path":"K:\\runtime\\worker.local.json","bytes":100,"sha256":"1".repeat(64),"fileIdentity":"config","querySupported":false},
        "state":state,"workerJournalSchema":16,"sourceAuthority":authority,"backupAuthority":authority,
        "shutdownReceipt":{"schemaVersion":1,"updateId":id,"launchId":"launch","updaterOwnerId":"owner","updaterEpoch":2,"command":command,"lease":lease,"process":process,"state":state,"acceptedAtMs":20},
        "backup":{"path":"K:\\runtime\\update\\fixture.sqlite","bytes":8192,"sha256":"2".repeat(64),"fileIdentity":"copy"},
        "pageSize":4096,"pageCount":2,"completedAtMs":30,"integrityVerified":true,"backupCreatedNew":true,"journalOpenedReadOnly":true,"workerConstructed":false,"schedulerStarted":false,"recoveryMarkerChanged":false,"runtimeApplied":false})
}
#[test]
fn final_backup_contract_preserves_authority_and_rejects_partial_or_effectful_reports() {
    let value = packet();
    let report: WorkerStateBackupReport = serde_json::from_value(value.clone()).unwrap();
    report.validate_envelope().unwrap();
    assert_eq!(report.source_authority.epoch, 7);
    assert_eq!(report.shutdown_receipt.updater_epoch, 2);
    for (pointer, replace) in [
        ("/schemaVersion", json!(2)),
        ("/attemptId", json!("../escape")),
        ("/updateId", json!("other")),
        ("/backupAuthority/epoch", json!(8)),
        ("/backupAuthority/fingerprintSha256", json!("3".repeat(64))),
        ("/sourceAuthority/incarnation", json!("other")),
        ("/shutdownReceipt/lease/phase", json!("released")),
        ("/shutdownReceipt/lease/revision", json!(0)),
        ("/shutdownReceipt/updaterEpoch", json!(3)),
        ("/helperProcess/sessionId", json!(2)),
        ("/helperDescriptor/role", json!("updater")),
        ("/backup/fileIdentity", json!("journal")),
        ("/backup/bytes", json!(4096)),
        ("/pageCount", json!(u64::MAX)),
        ("/pageSize", json!(4000)),
        ("/completedAtMs", json!(19)),
        ("/integrityVerified", json!(false)),
        ("/backupCreatedNew", json!(false)),
        ("/journalOpenedReadOnly", json!(false)),
        ("/workerConstructed", json!(true)),
        ("/schedulerStarted", json!(true)),
        ("/recoveryMarkerChanged", json!(true)),
        ("/runtimeApplied", json!(true)),
    ] {
        let mut broken = value.clone();
        *broken.pointer_mut(pointer).unwrap() = replace;
        let report: WorkerStateBackupReport = serde_json::from_value(broken).unwrap();
        assert!(report.validate_envelope().is_err(), "{pointer}");
    }
    let mut unknown = value;
    unknown["processExited"] = true.into();
    assert!(serde_json::from_value::<WorkerStateBackupReport>(unknown).is_err());
}
