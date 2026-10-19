//! Pinned-process control fixture. Other role descriptors are synthetic; no
//! WinBridge candidate health, install, source effect or activation is claimed.
use super::*;

pub(super) fn write_known_fixture(runtime: &Path, config: &Path) {
    use winbridge_update::installed_set::{InstalledArtifact, InstalledSetManifest, MANIFEST_NAME};
    assert_eq!(
        std::env::current_exe().unwrap().canonicalize().unwrap(),
        runtime.join("winbridge-worker.exe").canonicalize().unwrap()
    );
    let build = build_identity();
    assert!(
        !build.source_dirty && build.source_manifest_sha256.is_some(),
        "Explicit clean exported native source required"
    );
    let cfg = winbridge_update::load_config(config).unwrap();
    let scope = winbridge_update::installation_scope(&cfg).unwrap();
    let info = UpdateJournal::open(&runtime.join("update-state"), &scope)
        .unwrap()
        .info()
        .unwrap();
    let mut artifacts = vec![];
    for (name, role) in [
        ("winbridge-worker.exe", ArtifactRole::Worker),
        ("winbridge-fixture.exe", ArtifactRole::Fixture),
        ("winbridge-desktop.exe", ArtifactRole::Desktop),
        ("winbridge-update.exe", ArtifactRole::Updater),
    ] {
        let path = runtime.join(name);
        if matches!(role, ArtifactRole::Fixture | ArtifactRole::Desktop) {
            fs::write(&path, [ARTIFACT_QUERY_MARKER, name.as_bytes()].concat()).unwrap();
        }
        let bytes = fs::read(&path).unwrap();
        artifacts.push(InstalledArtifact {
            name: name.into(),
            bytes: bytes.len() as u64,
            sha256: winbridge_update::sha256(&bytes),
            descriptor: artifact_descriptor(role),
        });
    }
    let manifest = InstalledSetManifest {
        schema_version: 1,
        kind: "winbridge_installed_set".into(),
        set_id: uuid::Uuid::new_v4().to_string(),
        installation_id: info.installation_id,
        scope_sha256: scope,
        artifacts,
    };
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(runtime.join(MANIFEST_NAME))
        .unwrap();
    std::io::Write::write_all(&mut file, &serde_json::to_vec(&manifest).unwrap()).unwrap();
    file.sync_all().unwrap();
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[ignore = "Owned clean native copied-process fixture; invoked only by parent below"]
async fn runtime_identity_process_helper() {
    let args: Vec<_> = std::env::args().collect();
    let value = |key: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(key))
            .unwrap()
            .to_owned()
    };
    let root = PathBuf::from(value("runtime-root="));
    let binaries = PathBuf::from(value("runtime-binaries="));
    assert_eq!(
        std::env::current_exe().unwrap().canonicalize().unwrap(),
        root.join("runtime/winbridge-worker.exe")
            .canonicalize()
            .unwrap()
    );
    let mut f = Fixture::with_build_script_at(
        None,
        false,
        None,
        BUILD_ACTOR_SCRIPT,
        Some((root, binaries)),
    )
    .await;
    f.launch().await;
    let reg = f.worker.control_registration().unwrap();
    let before = UpdateJournal::read_snapshot(
        &reg.directory,
        Some(&f.supervisor.as_ref().unwrap().record.spec.update_id),
    )
    .unwrap();
    let identity = f
        .supervisor
        .as_ref()
        .unwrap()
        .runtime_identity()
        .await
        .unwrap();
    assert_eq!(identity.process, reg.binding.process);
    assert_eq!(identity.process.process_id, std::process::id());
    assert_eq!(identity.descriptor.build, build_identity());
    assert_eq!(
        identity.image,
        winbridge_update::artifact::inspect_image(&std::env::current_exe().unwrap()).unwrap()
    );
    assert_ne!(
        identity.descriptor.build.source_manifest_sha256.as_ref(),
        Some(&f.baseline)
    );
    for path in [&f.config, &f.updater_config] {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.runtime_verified_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        let query = f.supervisor.as_ref().unwrap().runtime_identity();
        tokio::pin!(query);
        tokio::select! {
            result = &mut query => panic!("Runtime verification checkpoint not reached: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(35),entered_rx) => {result.unwrap().unwrap();}
        }
        // File hashing/checkpoint does not retain the journal mutex.
        assert!(f.worker.store.try_lock().is_ok());
        assert_eq!(
            f.supervisor
                .as_ref()
                .unwrap()
                .observe()
                .await
                .unwrap()
                .phase,
            SupervisorPhase::Active
        );
        let bytes = fs::read(path).unwrap();
        fs::write(path, [bytes.as_slice(), b" "].concat()).unwrap();
        release_tx.send(()).unwrap();
        assert!(query.await.unwrap_err().contains("ConfigRestartPending"));
        fs::write(path, bytes).unwrap();
    }
    assert_eq!(f.slots(), 0);
    assert!(f
        .worker
        .store()
        .unwrap()
        .active_worker_update()
        .unwrap()
        .is_none());
    assert_eq!(
        UpdateJournal::read_snapshot(&reg.directory, Some(&before.1.as_ref().unwrap().update_id))
            .unwrap(),
        before
    );
    f.stop().await;
    let proof = value("runtime-proof=");
    let report = serde_json::json!({"schemaVersion":1,"kind":"runtime_identity_control_proof","passed":true,"platform":"windows","build":build_identity(),"identity":identity,"actualCopiedProcess":true,"workerEntryPointIsTestHarness":true,"syntheticOtherRoleDescriptors":true,"supervisorAuthenticated":true,"configChangesAfterHashRejected":true,"sourceGenerationIndependent":true,"journalMutexReleased":true,"noLeaseOrSlot":true,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false});
    fs::write(proof, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "Explicit clean Windows runtime identity gate; no installed runtime changes"]
fn runtime_identity_roundtrip_from_pinned_process() {
    use std::io::Read;
    let build = build_identity();
    assert!(!build.source_dirty && build.source_manifest_sha256.is_some());
    let exe = std::env::current_exe().unwrap();
    let binaries = exe.parent().unwrap().parent().unwrap();
    let root = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let runtime = root.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    let child_image = runtime.join("winbridge-worker.exe");
    fs::copy(&exe, &child_image).unwrap();
    let proof_path = root.path().join("proof.json");
    let command = ResolvedCommand {
        executable: child_image.to_string_lossy().into(),
        arguments: vec![
            "--exact".into(),
            "update_control::tests::runtime::runtime_identity_process_helper".into(),
            "--ignored".into(),
            "--nocapture".into(),
            "--skip".into(),
            format!("runtime-root={}", root.path().display()),
            "--skip".into(),
            format!("runtime-binaries={}", binaries.display()),
            "--skip".into(),
            format!("runtime-proof={}", proof_path.display()),
        ],
        working_directory: runtime.to_string_lossy().into(),
        timeout_ms: 180000,
    };
    let job = format!("WinBridge-runtime-validation-{}", uuid::Uuid::new_v4());
    let (mut child, stdout, stderr) =
        winbridge_platform::ManagedChild::prepare_atomic_executable(&command, Some(&job), None)
            .unwrap();
    let capture = |reader: winbridge_platform::OutputReader| {
        std::thread::spawn(move || {
            let mut bytes = vec![];
            reader.take(1024 * 1024).read_to_end(&mut bytes).unwrap();
            bytes
        })
    };
    let output = (capture(stdout), capture(stderr));
    child.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    child.terminate_tree().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !child.tree_is_empty().unwrap() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.tree_is_empty().unwrap());
    let stdout = String::from_utf8_lossy(&output.0.join().unwrap()).into_owned();
    let stderr = String::from_utf8_lossy(&output.1.join().unwrap()).into_owned();
    assert!(
        status.is_some_and(|s| s.success()),
        "Owned process failed/timed out:\n{stdout}\n{stderr}"
    );
    let bytes = fs::read(&proof_path).unwrap();
    let proof: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(proof["passed"], true);
    if let Some(path) = std::env::var_os("WINBRIDGE_RUNTIME_IDENTITY_PROOF") {
        fs::write(path, bytes).unwrap();
    }
}
