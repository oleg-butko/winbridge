//! Readonly config checks against real product Main; malformed live-looking DB is never opened.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use winbridge_update::worker_compatibility::inspect_config;
struct Fixture {
    root: tempfile::TempDir,
    worker: PathBuf,
    config: PathBuf,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let worker = root.path().join("candidate-worker.exe");
        fs::copy(env!("CARGO_BIN_EXE_winbridge-worker"), &worker).unwrap();
        let state = root.path().join("state");
        fs::create_dir(&state).unwrap();
        fs::write(
            state.join("journal.sqlite"),
            b"canary: not a SQLite database",
        )
        .unwrap();
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        let config = root.path().join("worker.json");
        fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"instanceId":"config-check","nodeId":"test","stateDirectory":state,"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"test","workspaceRoot":project,"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
        Self {
            root,
            worker,
            config,
            state,
        }
    }
    fn sha(p: &Path) -> String {
        winbridge_update::sha256(&fs::read(p).unwrap())
    }
    fn untouched(&self) {
        assert_eq!(
            fs::read(self.state.join("journal.sqlite")).unwrap(),
            b"canary: not a SQLite database"
        );
        assert_eq!(fs::read_dir(&self.state).unwrap().count(), 1);
        assert!(!self.root.path().join("runtime").exists());
    }
}
#[test]
fn actual_candidate_checks_full_config_without_opening_state_or_copying_values() {
    let f = Fixture::new();
    let before = fs::read(&f.config).unwrap();
    let proof = inspect_config(
        &f.worker,
        &Fixture::sha(&f.worker),
        &f.config,
        &Fixture::sha(&f.config),
    )
    .unwrap();
    assert!(proof.read_only && proof.cleanup_verified);
    assert!(!proof.candidate_verified && !proof.journal_bound && !proof.runtime_applied);
    assert!(!proof.check.worker_journal_opened && !proof.check.scheduler_started);
    assert_eq!(proof.check.instance_id, "config-check");
    assert_eq!(proof.check.config.sha256, Fixture::sha(&f.config));
    assert_eq!(fs::read(&f.config).unwrap(), before);
    f.untouched();
    let mut forged = serde_json::to_value(&proof.check).unwrap();
    forged["canInstall"] = true.into();
    assert!(serde_json::from_value::<winbridge_protocol::WorkerConfigCheckReport>(forged).is_err());
}
#[test]
fn actual_config_entry_rejects_bad_sha_policy_and_extra_args_without_journal_effects() {
    let f = Fixture::new();
    let badsha = Command::new(&f.worker)
        .args([
            "--check-update-config",
            f.config.to_str().unwrap(),
            &"0".repeat(64),
        ])
        .output()
        .unwrap();
    assert!(!badsha.status.success());
    assert!(String::from_utf8_lossy(&badsha.stderr).contains("SHA mismatch"));
    f.untouched();
    let mut invalid: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config).unwrap()).unwrap();
    invalid["schemaVersion"] = 2.into();
    fs::write(&f.config, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let badpolicy = Command::new(&f.worker)
        .arg("--check-update-config")
        .arg(&f.config)
        .arg(Fixture::sha(&f.config))
        .output()
        .unwrap();
    assert!(!badpolicy.status.success());
    assert!(String::from_utf8_lossy(&badpolicy.stderr).contains("schema/project policy rejected"));
    f.untouched();
    let extra = Command::new(&f.worker)
        .args([
            "--check-update-config",
            f.config.to_str().unwrap(),
            &Fixture::sha(&f.config),
            "serve",
        ])
        .output()
        .unwrap();
    assert!(!extra.status.success());
    f.untouched();
}
#[test]
fn unsupported_image_and_changed_config_pin_stop_before_any_process() {
    let f = Fixture::new();
    let unsupported = f.root.path().join("old.exe");
    fs::write(&unsupported, b"older image").unwrap();
    assert!(inspect_config(
        &unsupported,
        &Fixture::sha(&unsupported),
        &f.config,
        &Fixture::sha(&f.config)
    )
    .unwrap_err()
    .contains("UnsupportedWorkerConfigCheck"));
    assert!(inspect_config(
        &f.worker,
        &Fixture::sha(&f.worker),
        &f.config,
        &"0".repeat(64)
    )
    .unwrap_err()
    .contains("config size/SHA mismatch"));
    f.untouched();
}
