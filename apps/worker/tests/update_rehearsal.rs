//! Actual candidate process in a private managed domain, never a production journal.
use std::{fs, path::Path, process::Command};
use winbridge_update::worker_rehearsal::inspect;
// Unix fork/exec in a parallel case can briefly inherit a write descriptor
// used by fs::copy in another case, making that copied image ETXTBSY. Serialize
// these fixture lifetimes; do not retry product launch or weaken image pins.
fn case_scope() -> std::sync::MutexGuard<'static, ()> {
    static CASES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CASES.lock().unwrap_or_else(|e| e.into_inner())
}
fn root() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("wb-rehearsal-")
        .tempdir_in(if cfg!(unix) {
            std::path::PathBuf::from("/var/tmp")
        } else {
            std::env::temp_dir()
        })
        .unwrap()
}
fn copied_worker(root: &Path) -> std::path::PathBuf {
    let copy = root.join("candidate-worker.exe");
    fs::copy(env!("CARGO_BIN_EXE_winbridge-worker"), &copy).unwrap();
    copy
}
fn sha(worker: &Path) -> String {
    winbridge_update::artifact::inspect_image(worker)
        .unwrap()
        .sha256
}
#[test]
fn actual_candidate_initializes_only_new_state_serves_ipc_and_stops() {
    let _case = case_scope();
    let root = root();
    let worker = copied_worker(root.path());
    let directory = root.path().join("fresh");
    let original = root.path().join("production-journal-canary");
    fs::write(&original, b"never open or recover").unwrap();
    let proof = inspect(
        &worker,
        &sha(&worker),
        &directory,
        winbridge_worker::store::JOURNAL_SCHEMA_VERSION,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION,
    )
    .unwrap();
    assert!(proof.same_schema && proof.cleanup_verified);
    assert!(!proof.journal_bound && !proof.candidate_verified && !proof.runtime_applied);
    assert!(
        proof.rehearsal.ipc_ready
            && proof.rehearsal.server_stopped
            && proof.rehearsal.fresh_journal
    );
    assert!(
        !proof.rehearsal.production_state_opened
            && !proof.rehearsal.controller_enabled
            && !proof.rehearsal.direct_commands_enabled
    );
    assert_eq!(fs::read(&original).unwrap(), b"never open or recover");
    let mut forged = serde_json::to_value(&proof.rehearsal).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<winbridge_protocol::WorkerRehearsalReport>(forged).is_err());
    let db = rusqlite::Connection::open_with_flags(
        directory.join("state/journal.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tasks: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 0);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        winbridge_worker::store::JOURNAL_SCHEMA_VERSION
    );
    drop(db);
    let error = inspect(&worker, &sha(&worker), &directory, 15, 9).unwrap_err();
    assert!(error.contains("Existing rehearsal directory"));
}
#[test]
fn successful_entry_emits_one_typed_report_without_console_diagnostics() {
    let _case = case_scope();
    let root = root();
    let output = Command::new(copied_worker(root.path()))
        .arg("--rehearsal")
        .arg(root.path().join("fresh"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let report: winbridge_protocol::WorkerRehearsalReport =
        serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.ipc_ready && report.server_stopped && report.fresh_journal);
    assert_ne!(report.process.process_id, std::process::id());
    assert!(!report.production_state_opened && !report.controller_enabled);
}
#[test]
fn actual_entry_refuses_existing_directory_and_production_config_arguments() {
    let _case = case_scope();
    let root = root();
    let worker = copied_worker(root.path());
    let existing = root.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("journal.sqlite"), b"unrelated history").unwrap();
    let output = Command::new(&worker)
        .arg("--rehearsal")
        .arg(&existing)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Create-only rehearsal directory"));
    assert_eq!(
        fs::read(existing.join("journal.sqlite")).unwrap(),
        b"unrelated history"
    );
    assert!(!existing.join("state").exists());
    let new = root.path().join("new");
    let output = Command::new(&worker)
        .arg("--rehearsal")
        .arg(&new)
        .args(["--config", "production.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!new.exists());
}
#[test]
fn unsupported_or_wrong_sha_images_are_not_executed() {
    let _case = case_scope();
    let root = root();
    let worker = copied_worker(root.path());
    let image = root.path().join("unsupported.exe");
    fs::write(&image, b"older image without rehearsal contract").unwrap();
    let hash = winbridge_update::artifact::inspect_image(&image)
        .unwrap()
        .sha256;
    let directory = root.path().join("fresh");
    assert!(inspect(&image, &hash, &directory, 15, 9)
        .unwrap_err()
        .contains("UnsupportedWorkerRehearsal"));
    assert!(!directory.exists());
    assert!(inspect(&worker, &"0".repeat(64), &directory, 15, 9)
        .unwrap_err()
        .contains("image SHA mismatch"));
    assert!(!directory.exists());
}
#[test]
fn schema_mismatch_does_not_become_candidate_approval() {
    let _case = case_scope();
    let root = root();
    let worker = copied_worker(root.path());
    let directory = root.path().join("mismatch");
    assert!(inspect(
        &worker,
        &sha(&worker),
        &directory,
        winbridge_worker::store::JOURNAL_SCHEMA_VERSION + 1,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    )
    .unwrap_err()
    .contains("schema proof rejected"));
    assert!(directory.join("state/journal.sqlite").is_file());
}
