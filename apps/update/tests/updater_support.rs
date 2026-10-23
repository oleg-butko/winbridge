//! Executable support discovery is readonly data, not approval to install.
use std::{fs, process::Command};
use winbridge_protocol::{update::*, ArtifactRole, ARTIFACT_QUERY_MARKER};
use winbridge_update::{artifact, updater_support};

#[test]
fn support_cli_ignores_broken_config_and_declares_only_implemented_contracts() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join(if cfg!(windows) {
        "updater.exe"
    } else {
        "updater"
    });
    fs::copy(env!("CARGO_BIN_EXE_winbridge-update"), &image).unwrap();
    fs::write(
        temp.path().join("winbridge-update.local.json"),
        b"invalid configuration: do not load",
    )
    .unwrap();
    let output = Command::new(&image)
        .arg("--runtime-support")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: UpdaterRuntimeSupport = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.query_contract, 1);
    assert_eq!(report.kind, "winbridge_updater_runtime_support");
    assert_eq!(report.descriptor.role, ArtifactRole::Updater);
    assert_eq!(
        report.updater_journal_schema,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    );
    assert_eq!(
        report.runtime_replacement_contract,
        cfg!(windows).then_some(17)
    );
    assert_eq!(
        report.runtime_preparation_contract,
        cfg!(windows).then_some(16)
    );
    assert_eq!(
        report.final_state_backup_contract,
        cfg!(windows).then_some(15)
    );
    assert_eq!(report.activation_contract, None);
    assert_eq!(report.controller_confirmation_contract, None);
    assert_eq!(report.desktop_update_contract, None);
    assert_eq!(
        report.build_flavor,
        if cfg!(feature = "update-validation") {
            UpdaterBuildFlavor::Validation
        } else {
            UpdaterBuildFlavor::Standard
        }
    );
    let identity = artifact::query_image(&image, ArtifactRole::Updater).unwrap();
    let inspection = updater_support::inspect(&identity.image, &identity.descriptor)
        .unwrap()
        .unwrap();
    assert_eq!(inspection.image, identity.image);
    assert_eq!(inspection.support.descriptor, identity.descriptor);
    assert!(inspection.cleanup_verified && inspection.read_only);
    assert!(!inspection.journal_bound && !inspection.candidate_verified);
    assert_ne!(inspection.support.process, report.process);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    for field in ["permit", "canInstall", "sourceOverride", "custody"] {
        let mut bad = serde_json::to_value(&report).unwrap();
        bad[field] = serde_json::json!(true);
        assert!(serde_json::from_value::<UpdaterRuntimeSupport>(bad).is_err());
    }
}

#[test]
fn support_marker_absence_or_mismatched_identity_never_launches_a_probe() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("not-an-executable.exe");
    fs::write(
        &image,
        [
            ARTIFACT_QUERY_MARKER,
            b"no new support marker; never execute",
        ]
        .concat(),
    )
    .unwrap();
    let evidence = artifact::inspect_image(&image).unwrap();
    let descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Updater);
    assert!(updater_support::inspect(&evidence, &descriptor)
        .unwrap()
        .is_none());
    let mut bad = evidence.clone();
    bad.file_identity = "different file".into();
    assert!(updater_support::inspect(&bad, &descriptor)
        .unwrap_err()
        .contains("no process started"));
    bad = evidence;
    bad.sha256 = "0".repeat(64);
    assert!(updater_support::inspect(&bad, &descriptor)
        .unwrap_err()
        .contains("no process started"));
    let wrong_role = winbridge_protocol::artifact_descriptor(ArtifactRole::Worker);
    assert!(updater_support::inspect(&bad, &wrong_role)
        .unwrap_err()
        .contains("pinned updater role"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn malformed_support_arguments_do_not_fall_through_to_work_or_config() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_winbridge-update"))
        .args(["--runtime-support", "install"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no configuration or work executed"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
