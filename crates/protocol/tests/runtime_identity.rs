//! Freeze the existing readonly JSON as DTO ownership moves to protocol.
use winbridge_protocol::{
    artifact_descriptor, update::SupervisorProcess, ArtifactRole, LoadedWorkerIdentity,
    RuntimeImageEvidence,
};

#[test]
fn runtime_image_keeps_existing_bytes_and_rejects_grant_fields() {
    let hash = "a".repeat(64);
    let image = RuntimeImageEvidence {
        path: r"K:\runtime\winbridge-worker.exe".into(),
        bytes: 128,
        sha256: hash.clone(),
        file_identity: "volume:file".into(),
        query_supported: true,
    };
    let expected = format!(
        r#"{{"path":"K:\\runtime\\winbridge-worker.exe","bytes":128,"sha256":"{hash}","fileIdentity":"volume:file","querySupported":true}}"#
    );
    assert_eq!(serde_json::to_string(&image).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<RuntimeImageEvidence>(&expected).unwrap(),
        image
    );
    let mut forged = serde_json::to_value(&image).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<RuntimeImageEvidence>(forged).is_err());
}

#[test]
fn loaded_worker_keeps_existing_bytes_without_becoming_an_authority_token() {
    let image = RuntimeImageEvidence {
        path: "worker.exe".into(),
        bytes: 128,
        sha256: "a".repeat(64),
        file_identity: "file".into(),
        query_supported: true,
    };
    let descriptor = artifact_descriptor(ArtifactRole::Worker);
    let process = SupervisorProcess {
        process_id: 42,
        birth: "99".into(),
        host_id: "host".into(),
        session_id: 1,
    };
    let identity = LoadedWorkerIdentity {
        schema_version: 1,
        kind: "loaded_worker_identity".into(),
        process,
        runtime_identity: "directory".into(),
        set_id: "set".into(),
        manifest_sha256: "b".repeat(64),
        descriptor,
        image,
    };
    let expected = format!(
        r#"{{"schemaVersion":1,"kind":"loaded_worker_identity","process":{},"runtimeIdentity":"directory","setId":"set","manifestSha256":"{}","descriptor":{},"image":{}}}"#,
        serde_json::to_string(&identity.process).unwrap(),
        identity.manifest_sha256,
        serde_json::to_string(&identity.descriptor).unwrap(),
        serde_json::to_string(&identity.image).unwrap()
    );
    assert_eq!(serde_json::to_string(&identity).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<LoadedWorkerIdentity>(&expected).unwrap(),
        identity
    );
    let mut forged = serde_json::to_value(identity).unwrap();
    forged["shutdownGranted"] = true.into();
    assert!(serde_json::from_value::<LoadedWorkerIdentity>(forged).is_err());
    // Deserializing an observation does not authenticate a peer or validate file
    // paths/hashes. Only a pinned, process-authenticated driver can do that.
}
