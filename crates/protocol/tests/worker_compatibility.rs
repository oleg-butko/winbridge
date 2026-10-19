use winbridge_protocol::{update::*, *};
fn response() -> WorkerUpdateControlResponse {
    let descriptor = artifact_descriptor(ArtifactRole::Worker);
    let binding = SupervisorWorkerBinding {
        context: RequestContext {
            instance_id: "instance".into(),
            journal_incarnation: "journal".into(),
        },
        generation: "generation".into(),
        build: descriptor.build.clone(),
        process: SupervisorProcess {
            process_id: 42,
            birth: "123".into(),
            host_id: "a".repeat(64),
            session_id: 1,
        },
        config_sha256: "b".repeat(64),
        endpoint: "endpoint".into(),
    };
    let compatibility = WorkerRuntimeCompatibility {
        schema_version: 1,
        kind: "worker_runtime_compatibility".into(),
        identity: LoadedWorkerIdentity {
            schema_version: 1,
            kind: "loaded_worker_identity".into(),
            process: binding.process.clone(),
            runtime_identity: "directory".into(),
            set_id: "set".into(),
            manifest_sha256: "c".repeat(64),
            descriptor,
            image: RuntimeImageEvidence {
                path: "worker.exe".into(),
                bytes: 1,
                sha256: "d".repeat(64),
                file_identity: "image".into(),
                query_supported: true,
            },
        },
        context: binding.context.clone(),
        generation: binding.generation.clone(),
        worker_config_sha256: binding.config_sha256.clone(),
        worker_config_schema: 1,
        worker_journal_schema: 15,
        updater_journal_schema: 9,
        protocol_version: 1,
        observed_at_ms: 42,
    };
    WorkerUpdateControlResponse {
        control_version: 3,
        binding,
        update_id: "update".into(),
        launch_id: "launch".into(),
        result: WorkerUpdateControlResult::RuntimeCompatibility {
            compatibility: Box::new(compatibility),
        },
    }
}
#[test]
fn compatibility_cannot_be_a_lease_identity_or_downgraded_reply() {
    let reply = response();
    assert!(reply.clone().into_lease().is_err());
    assert!(reply.clone().into_runtime_identity().is_err());
    assert_eq!(
        reply
            .clone()
            .into_runtime_compatibility()
            .unwrap()
            .worker_journal_schema,
        15
    );
    for version in [1, 2, 4] {
        let mut wrong = reply.clone();
        wrong.control_version = version;
        assert!(wrong.into_runtime_compatibility().is_err());
    }
    let mut wrong = reply.clone();
    wrong.result = WorkerUpdateControlResult::Snapshot { lease: None };
    assert!(wrong.into_runtime_compatibility().is_err());
    let mut wrong = reply.clone();
    wrong.result = WorkerUpdateControlResult::RuntimeIdentity {
        identity: Box::new(reply.clone().into_runtime_compatibility().unwrap().identity),
    };
    assert!(wrong.into_runtime_compatibility().is_err());
}
#[test]
fn compatibility_rejects_foreign_scopes_schemas_and_permission_fields() {
    for case in [
        "schema",
        "context",
        "generation",
        "config",
        "build",
        "pid",
        "worker-schema",
        "updater-schema",
        "config-schema",
        "protocol",
        "timestamp",
    ] {
        let mut reply = response();
        let WorkerUpdateControlResult::RuntimeCompatibility { compatibility: c } =
            &mut reply.result
        else {
            unreachable!()
        };
        match case {
            "schema" => c.schema_version = 2,
            "context" => c.context.journal_incarnation.push('x'),
            "generation" => c.generation.push('x'),
            "config" => c.worker_config_sha256.push('x'),
            "build" => c.identity.descriptor.build.build_id.push('x'),
            "pid" => c.identity.process.process_id += 1,
            "worker-schema" => c.worker_journal_schema = 0,
            "updater-schema" => c.updater_journal_schema = 0,
            "config-schema" => c.worker_config_schema = 2,
            "protocol" => c.protocol_version = 2,
            "timestamp" => c.observed_at_ms = 0,
            _ => unreachable!(),
        }
        assert!(reply.into_runtime_compatibility().is_err(), "{case}");
    }
    let mut forged =
        serde_json::to_value(response().into_runtime_compatibility().unwrap()).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<WorkerRuntimeCompatibility>(forged).is_err());
}
