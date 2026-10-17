use winbridge_core::update::*;
use winbridge_protocol::update::*;

fn file(path: &str, hash: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        bytes: 4,
        sha256: hash.repeat(64),
    }
}
fn manifest(files: Vec<SourceFile>) -> SourceManifest {
    SourceManifest {
        schema_version: 1,
        source_commit: "a".repeat(40),
        dirty: false,
        created_at: "2026-10-17T00:00:00Z".into(),
        includes_built_ui: false,
        files,
    }
}
fn current(m: &SourceManifest) -> Vec<CurrentSourceEntry> {
    m.files
        .iter()
        .map(|f| CurrentSourceEntry {
            path: f.path.clone(),
            directory: false,
            bytes: f.bytes,
            sha256: Some(f.sha256.clone()),
        })
        .collect()
}

#[test]
fn source_plan_accounts_for_deletions_and_is_deterministic() {
    let old = manifest(vec![
        file("src/gone.rs", "a"),
        file("src/change.rs", "b"),
        file("Cargo.toml", "c"),
    ]);
    let new = manifest(vec![
        file("src/new.rs", "d"),
        file("Cargo.toml", "c"),
        file("src/change.rs", "e"),
    ]);
    let mut entries = current(&old);
    let first = plan_source_update(&old, &new, &entries).unwrap();
    entries.reverse();
    assert_eq!(first, plan_source_update(&old, &new, &entries).unwrap());
    assert!(first.can_apply);
    assert_eq!(
        first
            .changes
            .iter()
            .map(|c| (&*c.path, c.action))
            .collect::<Vec<_>>(),
        vec![
            ("Cargo.toml", SourceAction::Keep),
            ("src/change.rs", SourceAction::Replace),
            ("src/gone.rs", SourceAction::Remove),
            ("src/new.rs", SourceAction::Add)
        ]
    );
}

#[test]
fn local_modification_deletion_and_unmanaged_file_block_the_whole_plan() {
    let old = manifest(vec![file("src/main.rs", "a"), file("Cargo.toml", "b")]);
    let mut entries = current(&old);
    entries.remove(1);
    entries[0].sha256 = Some("c".repeat(64));
    entries.push(CurrentSourceEntry {
        path: "src/extra.rs".into(),
        directory: false,
        bytes: 9,
        sha256: None,
    });
    let plan = plan_source_update(&old, &old, &entries).unwrap();
    assert!(!plan.can_apply);
    for code in [
        SourceBlockerCode::LocalMissing,
        SourceBlockerCode::LocalModified,
        SourceBlockerCode::UnmanagedFile,
    ] {
        assert!(plan.blockers.iter().any(|b| b.code == code));
    }
}

#[test]
fn case_renames_and_file_directory_changes_require_reconciliation() {
    let old = manifest(vec![file("src/File.rs", "a"), file("flat", "b")]);
    let new = manifest(vec![file("src/file.rs", "a"), file("flat/child.rs", "b")]);
    let plan = plan_source_update(&old, &new, &current(&old)).unwrap();
    assert!(!plan.can_apply);
    assert!(plan
        .blockers
        .iter()
        .any(|b| b.code == SourceBlockerCode::CaseConflict));
    assert!(plan
        .blockers
        .iter()
        .any(|b| b.code == SourceBlockerCode::LayoutChange));
}

#[test]
fn path_size_hash_dirty_and_manifest_collisions_are_rejected() {
    for path in [
        "../escape",
        "C:/absolute",
        "src\\main.rs",
        "src/./main.rs",
        "NUL.txt",
        "src/a:stream",
        "target/file",
        ".git/config",
        "nested/.env",
        "worker.local.json",
        "private.pem",
        "codex-session-example.md",
        "apps/desktop/src-tauri/gen/schemas/acl-manifests.json",
        "scripts/__pycache__/helper.pyc",
    ] {
        assert!(
            validate_source_manifest(&manifest(vec![file(path, "a")])).is_err(),
            "{path}"
        );
    }
    for files in [
        vec![file("src/a", "a"), file("SRC/A", "b")],
        vec![file("a", "a"), file("a/child", "b")],
    ] {
        assert!(validate_source_manifest(&manifest(files)).is_err());
    }
    let mut m = manifest(vec![file("Cargo.toml", "a")]);
    m.dirty = true;
    assert!(validate_source_manifest(&m).is_err());
    m.dirty = false;
    m.files[0].sha256 = "A".repeat(64);
    assert!(validate_source_manifest(&m).is_err());
    m.files[0].sha256 = "a".repeat(64);
    m.files[0].bytes = MAX_SOURCE_FILE_BYTES + 1;
    assert!(validate_source_manifest(&m).is_err());
}

#[test]
fn strict_source_contract_rejects_unexpected_fields() {
    let m = manifest(vec![file("Cargo.toml", "a")]);
    let mut value = serde_json::to_value(m).unwrap();
    value["installPath"] = serde_json::json!("K:\\runtime");
    assert!(parse_source_manifest(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn quiescence_requires_closed_dispatch_and_zero_logical_physical_unknown_owners() {
    for (closed, accepted, physical, unknown) in [
        (false, 0, 0, 0),
        (true, 1, 0, 0),
        (true, 0, 1, 0),
        (true, 0, 0, 1),
    ] {
        assert!(advance_update(
            UpdatePhase::Draining,
            UpdateEvent::Quiesce {
                dispatch_closed: closed,
                accepted_operations: accepted,
                physical_owners: physical,
                uncertain_owners: unknown,
            }
        )
        .is_err());
    }
    assert_eq!(
        advance_update(
            UpdatePhase::Draining,
            UpdateEvent::Quiesce {
                dispatch_closed: true,
                accepted_operations: 0,
                physical_owners: 0,
                uncertain_owners: 0,
            }
        )
        .unwrap(),
        UpdatePhase::Quiesced
    );
    assert!(advance_update(UpdatePhase::Requested, UpdateEvent::BeginShutdown).is_err());
    assert!(advance_update(
        UpdatePhase::VerifyingLocal,
        UpdateEvent::ControllerConfirmed
    )
    .is_err());
}

#[test]
fn failure_after_shutdown_requires_manual_intervention_and_terminal_states_do_not_retry() {
    for p in [
        UpdatePhase::SourceApplying,
        UpdatePhase::Stopping,
        UpdatePhase::Replacing,
        UpdatePhase::VerifyingLocal,
        UpdatePhase::AwaitingController,
    ] {
        assert_eq!(
            advance_update(p, UpdateEvent::Fail).unwrap(),
            UpdatePhase::ManualIntervention
        );
    }
    assert_eq!(
        advance_update(UpdatePhase::Building, UpdateEvent::Fail).unwrap(),
        UpdatePhase::FailedBeforeCutover
    );
    for p in [
        UpdatePhase::Committed,
        UpdatePhase::Deferred,
        UpdatePhase::ManualIntervention,
        UpdatePhase::FailedBeforeCutover,
    ] {
        assert!(advance_update(p, UpdateEvent::BeginPreparation).is_err());
        assert!(advance_update(p, UpdateEvent::Fail).is_err());
    }
}

#[test]
fn update_request_rejects_unpinned_paths_and_unknown_contract_fields() {
    let mut request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "request".into(),
        source: UpdateSourcePin {
            archive_name: "winbridge.zip".into(),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            source_commit: "c".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: "installation".into(),
            scope_sha256: "d".repeat(64),
            worker_build_id: "build".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    assert!(validate_update_request(&request).is_ok());
    for path in [
        "../winbridge.zip",
        "C:/winbridge.zip",
        "dir/source.zip",
        "CON.zip",
        "source.ZIP",
        "source.zip:stream",
    ] {
        request.source.archive_name = path.into();
        assert!(validate_update_request(&request).is_err(), "{path}");
    }
    request.source.archive_name = "source.zip".into();
    request.source.archive_sha256 = "A".repeat(64);
    assert!(validate_update_request(&request).is_err());
    request.source.archive_sha256 = "a".repeat(64);
    request.contract_version = 2;
    assert!(validate_update_request(&request).is_err());
    let mut value = serde_json::to_value(&request).unwrap();
    value["runtimeDirectory"] = serde_json::json!("K:\\elsewhere");
    assert!(serde_json::from_value::<UpdateRequest>(value).is_err());
    assert!(serde_json::from_value::<UpdateEvent>(
        serde_json::json!({"kind":"fail","releaseMaintenance":true})
    )
    .is_err());
    assert!(update_blocks_installation(UpdatePhase::ManualIntervention));
    assert!(!update_blocks_installation(
        UpdatePhase::FailedBeforeCutover
    ));
}
