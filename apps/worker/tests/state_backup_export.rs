//! Actual CLI negative boundaries; successful export requires a native owned
//! supervisor/attempt and is verified by the separate final-backup gate.
use std::{fs, process::Command};
fn run(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_winbridge-worker"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}
#[test]
fn export_entry_rejects_wrong_arguments_and_digest_without_normal_startup() {
    let root = tempfile::tempdir().unwrap();
    let canary = root.path().join("journal.sqlite");
    fs::write(&canary, b"unrelated history").unwrap();
    for args in [
        vec!["--export-update-state"],
        vec!["--export-update-state", "input.json"],
        vec![
            "--export-update-state",
            "input.json",
            "sha",
            "--config",
            "production.json",
        ],
    ] {
        let out = run(root.path(), &args);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("Use --export-update-state"));
    }
    let missing = root.path().join("missing.json");
    let out = run(
        root.path(),
        &["--export-update-state", missing.to_str().unwrap(), "bad"],
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Expected final backup input SHA-256"));
    assert_eq!(fs::read(&canary).unwrap(), b"unrelated history");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn export_entry_rejects_untyped_intent_before_opening_or_creating_any_journal() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input.json");
    let bytes = br#"{"schemaVersion":1,"destination":"arbitrary.sqlite","workerConstructed":true}"#;
    fs::write(&input, bytes).unwrap();
    let hash = winbridge_update::sha256(bytes);
    let out = run(
        root.path(),
        &["--export-update-state", input.to_str().unwrap(), &hash],
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Invalid export intent/fields"));
    assert_eq!(fs::read(&input).unwrap(), bytes);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
