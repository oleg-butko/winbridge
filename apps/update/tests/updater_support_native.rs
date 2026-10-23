//! Real Windows probes of both build flavors and a retained pre-query image.
#![cfg(windows)]
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use winbridge_protocol::{update::*, ArtifactDescriptor, ArtifactRole};
use winbridge_update::{artifact, updater_support};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema_version: u16,
    standard: PathBuf,
    standard_sha256: String,
    validation: PathBuf,
    validation_sha256: String,
    old_image: artifact::ImageEvidence,
    old_descriptor: ArtifactDescriptor,
    proof: PathBuf,
}

fn inspect(
    path: &std::path::Path,
    expected_sha: &str,
    flavor: UpdaterBuildFlavor,
) -> serde_json::Value {
    let config = path.with_file_name("winbridge-update.local.json");
    let before = artifact::inspect_image(&config).unwrap();
    let identity = artifact::query_image(path, ArtifactRole::Updater).unwrap();
    assert_eq!(identity.image.sha256, expected_sha);
    let build = winbridge_protocol::build_identity();
    assert_eq!(identity.descriptor.build.source_commit, build.source_commit);
    assert_eq!(
        identity.descriptor.build.source_manifest_sha256,
        build.source_manifest_sha256
    );
    assert!(!identity.descriptor.build.source_dirty);
    let report = updater_support::inspect(&identity.image, &identity.descriptor)
        .unwrap()
        .unwrap();
    assert_eq!(report.support.build_flavor, flavor);
    assert!(report.read_only && report.cleanup_verified);
    assert!(!report.journal_bound && !report.candidate_verified);
    assert_eq!(report.support.runtime_replacement_contract, Some(17));
    assert_eq!(report.support.activation_contract, None);
    assert_eq!(report.support.controller_confirmation_contract, None);
    assert_eq!(report.support.desktop_update_contract, None);
    assert_ne!(report.support.process, identity.process);
    assert_eq!(artifact::inspect_image(path).unwrap(), identity.image);
    assert_eq!(artifact::inspect_image(&config).unwrap(), before);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
    serde_json::json!({"artifact":identity,"inspection":report,
        "configBefore":before,"configAfter":artifact::inspect_image(&config).unwrap(),
        "directoryEntries":["winbridge-update.exe","winbridge-update.local.json"]})
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Proof<'a> {
    schema_version: u16,
    kind: &'static str,
    platform: &'static str,
    passed: bool,
    build: winbridge_protocol::BuildIdentity,
    standard: serde_json::Value,
    validation: serde_json::Value,
    old_image: &'a artifact::ImageEvidence,
    old_descriptor: &'a ArtifactDescriptor,
    old_image_after: artifact::ImageEvidence,
    old_support: Option<serde_json::Value>,
    runtime_applied: bool,
    installed_runtime_applied: bool,
    windows_opened: bool,
}

#[test]
#[ignore = "requires pinned retained Windows updater and both build flavors"]
fn actual_support_queries_are_image_bound_and_old_marker_is_not_executed() {
    let path = std::env::var_os("WINBRIDGE_SUPPORT_GATE_INPUT").unwrap();
    let raw = fs::read(path).unwrap();
    assert!(raw.len() <= 65536);
    let input: Input = serde_json::from_slice(&raw).unwrap();
    assert_eq!(input.schema_version, 1);
    assert_ne!(input.standard, input.validation);
    assert_ne!(input.standard_sha256, input.validation_sha256);
    assert_eq!(input.old_descriptor.role, ArtifactRole::Updater);
    assert_eq!(input.old_descriptor.target_os, "windows");
    assert!(!input.old_descriptor.build.source_dirty);
    assert_eq!(
        artifact::inspect_image(std::path::Path::new(&input.old_image.path)).unwrap(),
        input.old_image
    );
    // No query/unknown-flag fallback on the retained old executable.
    assert!(
        updater_support::inspect(&input.old_image, &input.old_descriptor)
            .unwrap()
            .is_none()
    );
    let standard = inspect(
        &input.standard,
        &input.standard_sha256,
        UpdaterBuildFlavor::Standard,
    );
    let validation = inspect(
        &input.validation,
        &input.validation_sha256,
        UpdaterBuildFlavor::Validation,
    );
    let proof = Proof {
        schema_version: 1,
        kind: "updater_runtime_support_native_proof",
        platform: "windows",
        passed: true,
        build: winbridge_protocol::build_identity(),
        standard,
        validation,
        old_image: &input.old_image,
        old_descriptor: &input.old_descriptor,
        old_image_after: artifact::inspect_image(std::path::Path::new(&input.old_image.path))
            .unwrap(),
        old_support: None,
        runtime_applied: false,
        installed_runtime_applied: false,
        windows_opened: false,
    };
    assert_eq!(proof.old_image_after, input.old_image);
    let bytes = serde_json::to_vec_pretty(&proof).unwrap();
    use std::io::Write;
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(input.proof)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
}
