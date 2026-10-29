//! Exercise the actual CLI startup barrier; synthetic non-worker images are never run.
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use winbridge_protocol::ArtifactRole;
use winbridge_update::{
    installed_set::{InstalledArtifact, InstalledSetManifest, MANIFEST_NAME},
    journal::UpdateJournal,
    UpdateConfig,
};

struct Fixture {
    _root: tempfile::TempDir,
    runtime: PathBuf,
    worker: PathBuf,
    config: PathBuf,
    state: PathBuf,
    manifest: InstalledSetManifest,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        let source = root.path().join("src");
        fs::create_dir(&runtime).unwrap();
        fs::create_dir(&source).unwrap();
        let state = root.path().join("worker-state");
        let config = runtime.join("winbridge-worker.local.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({
                "schemaVersion":1,"instanceId":"runtime-startup-test","nodeId":"test",
                "stateDirectory":state,"maxOutputBytesPerTask":4096,
                "worker":{"keepRunningOnUiExit":true},
                "projects":[{"id":"fixture","workspaceRoot":source,"allowDirectCommands":true,
                    "maxConcurrentMutations":1,"recipes":[]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let cfg = UpdateConfig {
            schema_version: 1,
            source_directory: source,
            runtime_directory: runtime.clone(),
            worker_config_path: config.clone(),
            source_packages_directory: None,
            build: None,
        };
        fs::write(
            runtime.join("winbridge-update.local.json"),
            serde_json::to_vec(&json!({
                "schemaVersion":1,"sourceDirectory":cfg.source_directory,"runtimeDirectory":runtime,
                "workerConfigPath":config
            }))
            .unwrap(),
        )
        .unwrap();
        let scope = winbridge_update::installation_scope(&cfg).unwrap();
        let installation = {
            let mut j = UpdateJournal::open(&runtime.join("update-state"), &scope).unwrap();
            j.info().unwrap().installation_id
        };
        let mut artifacts = vec![];
        for (name, role) in [
            ("winbridge-worker.exe", ArtifactRole::Worker),
            ("winbridge-fixture.exe", ArtifactRole::Fixture),
            ("winbridge-desktop.exe", ArtifactRole::Desktop),
            ("winbridge-update.exe", ArtifactRole::Updater),
        ] {
            let path = runtime.join(name);
            if role == ArtifactRole::Worker {
                fs::copy(env!("CARGO_BIN_EXE_winbridge-worker"), &path).unwrap();
            } else {
                fs::write(&path, winbridge_protocol::ARTIFACT_QUERY_MARKER).unwrap();
            }
            let bytes = fs::read(&path).unwrap();
            let mut descriptor = winbridge_protocol::artifact_descriptor(role);
            // Deliberately different from the compiled worker. This must never
            // authenticate a process, even with correct file hashes/markers.
            descriptor.build.source_dirty = false;
            descriptor.build.source_commit = Some("a".repeat(40));
            descriptor.build.source_manifest_sha256 = Some("b".repeat(64));
            descriptor.build.build_id = "foreign-worker".into();
            artifacts.push(InstalledArtifact {
                name: name.into(),
                bytes: bytes.len() as u64,
                sha256: winbridge_update::sha256(&bytes),
                descriptor,
            });
        }
        let manifest = InstalledSetManifest {
            schema_version: 1,
            kind: "winbridge_installed_set".into(),
            set_id: uuid::Uuid::new_v4().to_string(),
            installation_id: installation,
            scope_sha256: scope,
            artifacts,
        };
        Self {
            _root: root,
            worker: runtime.join("winbridge-worker.exe"),
            runtime,
            config,
            state,
            manifest,
        }
    }
    fn write_manifest(&self) {
        fs::write(
            self.runtime.join(MANIFEST_NAME),
            serde_json::to_vec(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn reject(&self, operation: &str, reason: &str) {
        let journal = self.runtime.join("update-state/updates.sqlite");
        let before = fs::read(&journal).unwrap();
        let mut child = Command::new(&self.worker)
            .args(["--config", self.config.to_str().unwrap(), operation])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || {
            let mut b = vec![];
            stdout.read_to_end(&mut b).unwrap();
            b
        });
        let err = thread::spawn(move || {
            let mut b = vec![];
            stderr.read_to_end(&mut b).unwrap();
            b
        });
        // Debug worker images are large; real SHA/File-ID verification must
        // complete, without replacing it with mocked metadata or an unbounded wait.
        let deadline = Instant::now() + Duration::from_secs(120);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Startup guard failed to stop {operation}");
            }
            thread::sleep(Duration::from_millis(20));
        };
        let output = String::from_utf8(out.join().unwrap()).unwrap();
        let error = String::from_utf8(err.join().unwrap()).unwrap();
        assert!(!status.success(), "{output}");
        assert!(error.contains(reason), "{error}");
        assert!(
            !self.state.exists(),
            "Startup opened the worker journal before validation"
        );
        assert_eq!(
            fs::read(journal).unwrap(),
            before,
            "Readonly startup changed updater DB"
        );
    }
}
#[test]
fn verify_runtime_without_manifest_never_bootstraps_or_opens_a_journal() {
    let f = Fixture::new();
    f.reject("verify-runtime", "Bootstrap required");
    assert!(!f.runtime.join(MANIFEST_NAME).exists());
}
#[test]
fn known_runtime_rejects_corrupt_foreign_and_changed_set_before_worker_journal() {
    let mut f = Fixture::new();
    fs::write(f.runtime.join(MANIFEST_NAME), b"{}").unwrap();
    f.reject("serve", "Installed-set manifest JSON rejected");
    let original = f.manifest.installation_id.clone();
    f.manifest.installation_id = uuid::Uuid::new_v4().to_string();
    f.write_manifest();
    f.reject(
        "serve",
        "Installed-set manifest version/installation/scope rejected",
    );
    f.manifest.installation_id = original;
    f.write_manifest();
    fs::write(f.runtime.join("winbridge-fixture.exe"), b"changed fixture").unwrap();
    f.reject("serve", "Installed runtime differs from its known manifest");
}
#[test]
fn correct_file_hashes_cannot_bind_a_fabricated_loaded_worker_descriptor() {
    let f = Fixture::new();
    f.write_manifest();
    f.reject("verify-runtime", "Loaded worker path/build does not match");
    f.reject("serve", "Loaded worker path/build does not match");
    let manifest: Value =
        serde_json::from_slice(&fs::read(f.runtime.join(MANIFEST_NAME)).unwrap()).unwrap();
    assert_eq!(
        manifest["artifacts"][0]["descriptor"]["build"]["buildId"],
        "foreign-worker"
    );
}

#[test]
fn ordinary_known_runtime_cannot_open_state_while_updater_decision_is_unfinished() {
    use winbridge_protocol::update::*;
    let f = Fixture::new();
    f.write_manifest();
    let mut j =
        UpdateJournal::open(&f.runtime.join("update-state"), &f.manifest.scope_sha256).unwrap();
    let a = j.acquire_owner().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "normal-start-barrier".into(),
        source: UpdateSourcePin {
            archive_name: "source.zip".into(),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            source_commit: "c".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: f.manifest.installation_id.clone(),
            scope_sha256: f.manifest.scope_sha256.clone(),
            worker_build_id: "old-build".into(),
            worker_config_sha256: "d".repeat(64),
            baseline_manifest_sha256: "e".repeat(64),
        },
    };
    let root = j.accept(&a, &request, &request.expected).unwrap();
    drop(a);
    drop(j);
    // Real CLI must refuse BEFORE Store creation even though the worker-side
    // journal, lease and startup-marker file do not exist at all. A pending
    // finalization uses the same validated info.active_update_id barrier;
    // actual Committed/Released/Active fault boundaries belong to native V8.
    f.reject("serve", "включая подтверждение release/Active");
    let after =
        UpdateJournal::read_snapshot(&f.runtime.join("update-state"), Some(&root.update_id))
            .unwrap();
    assert_eq!(after.1, Some(root));
    // The effect-free verifier retains its former identity-only semantics.
    f.reject("verify-runtime", "Loaded worker path/build does not match");
}
