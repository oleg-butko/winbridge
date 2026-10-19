//! Freeze the existing readonly JSON as DTO ownership moves to protocol.
use winbridge_protocol::{
    artifact_descriptor, update::SupervisorProcess, ArtifactRole, LoadedWorkerIdentity,
    RuntimeImageEvidence,
};

#[test]
fn runtime_baseline_states_are_typed_and_reject_mixed_authority_fields() {
    use winbridge_protocol::update::RuntimeBaselineView;
    assert_eq!(
        serde_json::to_string(&RuntimeBaselineView::NotAdmitted).unwrap(),
        r#"{"phase":"not_admitted"}"#
    );
    let pending = RuntimeBaselineView::Pending {
        intent_revision: 1,
        admitted_at_ms: 42,
    };
    assert_eq!(
        serde_json::to_string(&pending).unwrap(),
        r#"{"phase":"pending","intentRevision":1,"admittedAtMs":42}"#
    );
    let mut forged = serde_json::to_value(pending).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<RuntimeBaselineView>(forged).is_err());
    assert!(serde_json::from_str::<RuntimeBaselineView>(
        r#"{"phase":"verified","intentRevision":1}"#
    )
    .is_err());
}

#[test]
fn runtime_preimages_view_requires_typed_details_without_install_authority() {
    use winbridge_protocol::update::RuntimePreimagesView;
    let pending = RuntimePreimagesView::Pending {
        intent_revision: 3,
        admitted_at_ms: 42,
        directory: "previous-runtime".into(),
    };
    assert_eq!(
        serde_json::to_string(&pending).unwrap(),
        r#"{"phase":"pending","intentRevision":3,"admittedAtMs":42,"directory":"previous-runtime"}"#
    );
    let mut forged = serde_json::to_value(pending).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<RuntimePreimagesView>(forged).is_err());
    assert!(serde_json::from_str::<RuntimePreimagesView>(
        r#"{"phase":"verified","directory":"previous-runtime"}"#
    )
    .is_err());
}

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

#[test]
fn lease_and_runtime_responses_cannot_be_interpreted_as_each_other_or_downgraded() {
    use winbridge_protocol::{build_identity, update::*, RequestContext};
    let descriptor = artifact_descriptor(ArtifactRole::Worker);
    let process = SupervisorProcess {
        process_id: 42,
        birth: "birth".into(),
        host_id: "host".into(),
        session_id: 1,
    };
    let binding = SupervisorWorkerBinding {
        context: RequestContext {
            instance_id: "instance".into(),
            journal_incarnation: "journal".into(),
        },
        generation: "generation".into(),
        build: build_identity(),
        process: process.clone(),
        config_sha256: "a".repeat(64),
        endpoint: "local".into(),
    };
    let snapshot = WorkerUpdateControlResponse {
        control_version: 1,
        binding: binding.clone(),
        update_id: "update".into(),
        launch_id: "launch".into(),
        result: WorkerUpdateControlResult::Snapshot { lease: None },
    };
    assert_eq!(
        serde_json::to_string(&snapshot.result).unwrap(),
        r#"{"kind":"snapshot","lease":null}"#
    );
    assert!(snapshot.clone().into_lease().unwrap().is_none());
    assert!(snapshot.clone().into_runtime_identity().is_err());
    let mut forged = snapshot;
    forged.control_version = 2;
    assert!(forged.clone().into_lease().is_err());
    assert!(forged.into_runtime_identity().is_err());
    let identity = LoadedWorkerIdentity {
        schema_version: 1,
        kind: "loaded_worker_identity".into(),
        process,
        runtime_identity: "directory".into(),
        set_id: "set".into(),
        manifest_sha256: "b".repeat(64),
        descriptor,
        image: RuntimeImageEvidence {
            path: "worker.exe".into(),
            bytes: 1,
            sha256: "c".repeat(64),
            file_identity: "image".into(),
            query_supported: true,
        },
    };
    let mut response = WorkerUpdateControlResponse {
        control_version: 2,
        binding,
        update_id: "update".into(),
        launch_id: "launch".into(),
        result: WorkerUpdateControlResult::RuntimeIdentity {
            identity: Box::new(identity.clone()),
        },
    };
    assert_eq!(response.clone().into_runtime_identity().unwrap(), identity);
    assert!(response.clone().into_lease().is_err());
    response.control_version = 1;
    assert!(response.clone().into_runtime_identity().is_err());
    assert!(response.into_lease().is_err());
}
