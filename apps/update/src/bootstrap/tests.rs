//! Synthetic data validation only. No process, writer or launch permit is created.
use super::*;
use winbridge_protocol::RequestContext;
fn id(n: u8) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
pub(crate) fn fixture() -> BootstrapIntent {
    let source = UpdateSourcePin {
        archive_name: "source.zip".into(),
        archive_sha256: "a".repeat(64),
        manifest_sha256: "b".repeat(64),
        source_commit: "c".repeat(40),
    };
    let process = |pid| SupervisorProcess {
        process_id: pid,
        birth: format!("{pid}00"),
        host_id: "d".repeat(64),
        session_id: 1,
    };
    let file = |i: &str| BootstrapFile {
        sha256: "e".repeat(64),
        bytes: 123,
        file_identity: i.into(),
    };
    BootstrapIntent {
        schema_version: 1,
        previous: KeeperBinding {
            update_id: id(1),
            launch_id: id(2),
            coordinator: process(10),
            worker: process(11),
            job_name: "Global\\WinBridge-fixture".into(),
            generation: id(3),
            context: RequestContext {
                instance_id: "bootstrap-model".into(),
                journal_incarnation: id(4),
            },
            finalization_command_id: id(5),
            finalization_command_sha256: "f".repeat(64),
            selection_sha256: Some("0".repeat(64)),
        },
        update: UpdateRecord {
            update_id: id(6),
            revision: 0,
            phase: UpdatePhase::Requested,
            request: UpdateRequest {
                contract_version: 1,
                idempotency_key: "bootstrap-once".into(),
                source: source.clone(),
                expected: UpdateInstallationPin {
                    installation_id: id(7),
                    scope_sha256: "1".repeat(64),
                    worker_build_id: "old-build".into(),
                    worker_config_sha256: "2".repeat(64),
                    baseline_manifest_sha256: "3".repeat(64),
                },
            },
            accepted_at_ms: 10,
            updated_at_ms: 10,
            reason: None,
        },
        launch_id: id(8),
        parent_owner_id: id(9),
        parent_epoch: 3,
        updater_build: BuildIdentity {
            version: "0.2.0".into(),
            source_commit: Some(source.source_commit),
            source_dirty: false,
            source_manifest_sha256: Some(source.manifest_sha256),
            build_id: "new-build".into(),
        },
        image: file("image"),
        config: file("config"),
        stdout_identity: "stdout".into(),
        stderr_identity: "stderr".into(),
        token_sha256: "4".repeat(64),
        excluded_domain: DomainIdentity {
            job_name: format!("Global\\WinBridge-Domain-{}", "5".repeat(64)),
            host_id: "d".repeat(64),
            session_id: 1,
            owner_id: id(10),
        },
        prepared_at_ms: 11,
    }
}
#[test]
fn fixed_helper_names_and_canonical_hash_are_data_only() {
    let intent = fixture();
    intent.validate().unwrap();
    assert_eq!(
        intent.helper_components().unwrap(),
        [
            "update",
            intent.update.update_id.as_str(),
            "supervisor",
            "winbridge-update.exe"
        ]
    );
    let decoded: BootstrapIntent =
        serde_json::from_value(serde_json::to_value(&intent).unwrap()).unwrap();
    assert_eq!(intent.sha256().unwrap(), decoded.sha256().unwrap());
    assert_ne!(
        intent.config.sha256, intent.update.request.expected.worker_config_sha256,
        "Updater and loaded worker configs are distinct"
    );
}
#[test]
fn alias_stale_phase_source_rebind_clock_and_foreign_domain_are_refused() {
    for fault in 0..18 {
        let mut value = fixture();
        match fault {
            0 => value.launch_id = value.previous.launch_id.clone(),
            1 => value.update.update_id = value.previous.update_id.clone(),
            2 => value.previous.worker = value.previous.coordinator.clone(),
            3 => value.previous.selection_sha256 = None,
            4 => value.update.phase = UpdatePhase::Preparing,
            5 => value.update.revision = 1,
            6 => value.updater_build.source_dirty = true,
            7 => value.updater_build.source_commit = Some("0".repeat(40)),
            8 => value.prepared_at_ms = 9,
            9 => value.excluded_domain.session_id = 2,
            10 => value.excluded_domain.host_id = "0".repeat(64),
            11 => value.stdout_identity = value.stderr_identity.clone(),
            12 => value.update.request.source.archive_name = "../source.zip".into(),
            13 => value.image.file_identity = value.config.file_identity.clone(),
            14 => value.stdout_identity = value.image.file_identity.clone(),
            15 => value.stderr_identity = value.config.file_identity.clone(),
            16 => value.update.updated_at_ms += 1,
            _ => value.config.file_identity = value.stdout_identity.clone(),
        }
        assert!(value.validate().is_err(), "fault {fault}");
        assert!(
            value.sha256().is_err(),
            "Invalid data must not gain a canonical intent key"
        );
    }
}
#[test]
fn extra_executable_argv_or_plaintext_token_never_enters_the_journal_model() {
    for field in ["executable", "arguments", "token", "runtimeDirectory"] {
        let mut raw = serde_json::to_value(fixture()).unwrap();
        raw[field] = "forbidden".into();
        assert!(
            serde_json::from_value::<BootstrapIntent>(raw).is_err(),
            "{field}"
        );
    }
    let mut value = fixture();
    value.image.file_identity = "x".repeat(32768);
    assert!(value.validate().is_err());
}

#[test]
fn package_name_is_opaque_metadata_not_an_executable_path_or_extension_policy() {
    for name in ["snapshot.ZIP", "snapshot.package"] {
        let mut value = fixture();
        value.update.request.source.archive_name = name.into();
        value.validate().unwrap();
    }
}
