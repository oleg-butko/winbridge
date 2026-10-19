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
    let mut f =
        Fixture::with_build_script_at(None, true, None, BUILD_ACTOR_SCRIPT, Some((root, binaries)))
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
        // The ordinary scheduler may briefly own Store. It must become
        // available while the runtime query remains held at this checkpoint;
        // a lock held for the entire query fails before its 10 s gate expires.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match f.worker.store.try_lock() {
                Ok(store) => {
                    drop(store);
                    break;
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    panic!("Worker journal mutex poisoned")
                }
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
            assert!(
                Instant::now() < deadline,
                "Runtime observation retained the journal mutex"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
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
    // Capture uses the real supervisor PID and the same pinned worker, but now
    // writes durable metadata before ACK. No source/lease/runtime effect follows.
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    *reg.runtime_verified_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
        entered: std::sync::Mutex::new(Some(entered_tx)),
        release: std::sync::Mutex::new(release_rx),
    }));
    let h = f.supervisor.as_mut().unwrap();
    let accepted = h.runtime_baseline(true).await.unwrap();
    assert!(accepted.driver_active);
    assert!(matches!(
        accepted.runtime_baseline,
        Some(RuntimeBaselineView::Pending {
            intent_revision: 1,
            ..
        })
    ));
    let id = h.record.spec.update_id.clone();
    let admitted = UpdateJournal::inspect_runtime_baseline(&reg.directory, &id).unwrap();
    assert!(admitted.intent.is_some() && admitted.receipt.is_none());
    tokio::time::timeout(Duration::from_secs(35), entered_rx)
        .await
        .unwrap()
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(2), h.runtime_baseline(false))
        .await
        .unwrap()
        .unwrap();
    assert!(status.driver_active);
    assert!(matches!(
        status.runtime_baseline,
        Some(RuntimeBaselineView::Pending { .. })
    ));
    assert_eq!(h.runtime_baseline(true).await.unwrap().update.revision, 1);
    assert!(h
        .preparation(true)
        .await
        .unwrap_err()
        .contains("Old runtime preparation active"));
    assert_eq!(h.observe().await.unwrap().phase, SupervisorPhase::Active);
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(40);
    let completed = loop {
        let view = h.runtime_baseline(false).await.unwrap();
        assert!(view.driver_error.is_none(), "{:?}", view.driver_error);
        if !view.driver_active
            && matches!(
                view.runtime_baseline,
                Some(RuntimeBaselineView::Verified { .. })
            )
        {
            break view;
        }
        assert!(
            Instant::now() < deadline,
            "Baseline receipt was not completed"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let baseline = UpdateJournal::inspect_runtime_baseline(&reg.directory, &id).unwrap();
    assert_eq!(baseline.receipt.as_ref().unwrap().loaded_worker, identity);
    assert_eq!(completed.update.revision, 2);
    assert_eq!(completed.update.phase, UpdatePhase::Requested);
    assert!(completed.source.is_none());
    // Historical duplicates do not rehash or rebind after a config edit.
    let config = fs::read(&f.updater_config).unwrap();
    fs::write(&f.updater_config, [config.as_slice(), b" "].concat()).unwrap();
    assert_eq!(
        h.runtime_baseline(true).await.unwrap().update,
        completed.update
    );
    fs::write(&f.updater_config, config).unwrap();
    assert_eq!(
        UpdateJournal::inspect_runtime_baseline(&reg.directory, &id).unwrap(),
        baseline
    );
    let (preimages, first_abort, old_supervisor) = {
        let h = f.supervisor.as_mut().unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.runtime_verified_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        let ack = h.runtime_preimages(true).await.unwrap();
        assert!(ack.driver_active);
        assert!(matches!(
            ack.runtime_preimages,
            Some(RuntimePreimagesView::Pending {
                intent_revision: 3,
                ..
            })
        ));
        let saved = UpdateJournal::inspect_runtime_preimages(&reg.directory, &id).unwrap();
        assert!(saved.intent.is_some() && saved.receipt.is_none());
        tokio::time::timeout(Duration::from_secs(35), entered_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), h.runtime_preimages(false))
                .await
                .unwrap()
                .unwrap()
                .driver_active
        );
        assert_eq!(h.runtime_preimages(true).await.unwrap().update.revision, 3);
        assert!(h
            .preparation(true)
            .await
            .unwrap_err()
            .contains("Old runtime preparation active"));
        assert!(h.command(SupervisorAction::Stop).await.is_err());
        release_tx.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(50);
        loop {
            let v = h.runtime_preimages(false).await.unwrap();
            assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
            if !v.driver_active {
                assert!(matches!(
                    v.runtime_preimages,
                    Some(RuntimePreimagesView::Verified { file_count: 5, .. })
                ));
                break;
            }
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let saved = UpdateJournal::inspect_runtime_preimages(&reg.directory, &id).unwrap();
        let receipt = saved.receipt.as_ref().unwrap();
        assert_eq!(receipt.parent_revision, 4);
        for (copy, original) in receipt.files.iter().zip(
            std::iter::once(&baseline.receipt.as_ref().unwrap().installed.manifest_file)
                .chain(&baseline.receipt.as_ref().unwrap().installed.images),
        ) {
            assert_eq!(
                winbridge_update::artifact::inspect_image(Path::new(&copy.path)).unwrap(),
                *copy
            );
            assert_eq!(copy.sha256, original.sha256);
            assert_eq!(copy.bytes, original.bytes);
            assert_ne!(copy.file_identity, original.file_identity);
        }
        let config = fs::read(&f.updater_config).unwrap();
        fs::write(&f.updater_config, [config.as_slice(), b" "].concat()).unwrap();
        assert_eq!(h.runtime_preimages(true).await.unwrap().update.revision, 4);
        fs::write(&f.updater_config, config).unwrap();
        let aborted = h.abort_before_cutover().await.unwrap();
        assert_eq!(
            aborted.phase,
            winbridge_update::abort::AbortPhase::Completed
        );
        assert!(aborted.worker_before.is_none() && aborted.worker_released.is_none());
        assert!(aborted.reason.contains("отсутствие lease"));
        (saved, aborted, h.record.process.clone().unwrap())
    };
    // Same installed test worker, new request/owner epoch; no reuse of another
    // operation's preimages or supervisor. Wait for confirmed process exit.
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match winbridge_platform::infrastructure::matches(&old_supervisor) {
            Ok(false) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(87) => {
                break
            }
            Ok(true) => {}
            Err(e) => panic!("Old supervisor exit unconfirmed: {e}"),
        }
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    f.supervisor.take();
    f.request.idempotency_key = "second-preimages-cancellation".into();
    f.launch().await;
    let (cancelled_preimages, cancelled_abort) = {
        let h = f.supervisor.as_mut().unwrap();
        let new_id = h.record.spec.update_id.clone();
        assert_ne!(new_id, id);
        h.runtime_baseline(true).await.unwrap();
        let until = Instant::now() + Duration::from_secs(45);
        loop {
            let v = h.runtime_baseline(false).await.unwrap();
            assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
            if !v.driver_active {
                assert!(matches!(
                    v.runtime_baseline,
                    Some(RuntimeBaselineView::Verified { .. })
                ));
                break;
            }
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.runtime_verified_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        h.runtime_preimages(true).await.unwrap();
        tokio::time::timeout(Duration::from_secs(35), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let saved = UpdateJournal::inspect_runtime_preimages(&reg.directory, &new_id).unwrap();
        assert!(
            saved.intent.as_ref().unwrap().owner_epoch
                > preimages.intent.as_ref().unwrap().owner_epoch
        );
        let cancel = h.abort_before_cutover();
        tokio::pin!(cancel);
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            tokio::select! {
                result=&mut cancel=>panic!("Abort completed before held copy callback returned: {result:?}"),
                _=tokio::time::sleep(Duration::from_millis(20))=>{}
            }
            if let Ok(r) = UpdateJournal::inspect_abort(&reg.directory, &new_id) {
                assert_eq!(r.phase, winbridge_update::abort::AbortPhase::Intent);
                assert!(!r.cleanup_verified);
                break;
            }
            assert!(
                Instant::now() < until,
                "Abort intent was not durable while copier held"
            );
        }
        assert!(!Path::new(&saved.intent.as_ref().unwrap().directory).exists());
        release_tx.send(()).unwrap();
        let aborted = cancel.await.unwrap();
        assert_eq!(
            aborted.phase,
            winbridge_update::abort::AbortPhase::Completed
        );
        let saved = UpdateJournal::inspect_runtime_preimages(&reg.directory, &new_id).unwrap();
        assert!(saved.receipt.is_none());
        assert!(!Path::new(&saved.intent.as_ref().unwrap().directory).exists());
        assert!(aborted.worker_before.is_none() && aborted.worker_released.is_none());
        assert!(aborted.reason.contains("отсутствие lease"));
        (saved, aborted)
    };
    let cfg = winbridge_update::load_config(&f.updater_config).unwrap();
    winbridge_update::verify_source(&cfg.source_directory, &f.baseline).unwrap();
    assert_eq!(f.slots(), 0);
    assert!(f
        .worker
        .store()
        .unwrap()
        .active_worker_update()
        .unwrap()
        .is_none());
    f.stop().await;
    let proof = value("runtime-proof=");
    let report = serde_json::json!({"schemaVersion":1,"kind":"runtime_identity_control_proof","passed":true,"platform":"windows","build":build_identity(),"identity":identity,"actualCopiedProcess":true,"workerEntryPointIsTestHarness":true,"syntheticOtherRoleDescriptors":true,"supervisorAuthenticated":true,"configChangesAfterHashRejected":true,"sourceGenerationIndependent":true,"journalMutexReleased":true,"noLeaseOrSlot":true,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false});
    let mut report = report;
    report["baseline"] = serde_json::to_value(baseline).unwrap();
    report["preimages"] = serde_json::to_value(preimages).unwrap();
    report["preimagesAbort"] = serde_json::to_value(first_abort).unwrap();
    report["cancelledPreimages"] = serde_json::to_value(cancelled_preimages).unwrap();
    report["cancelledAbort"] = serde_json::to_value(cancelled_abort).unwrap();
    for check in [
        "baselineIntentBeforeAck",
        "baselineMailboxResponsive",
        "baselineSourceBlocked",
        "baselineDuplicateObserved",
        "preimagesIntentBeforeAck",
        "preimagesMailboxResponsive",
        "preimagesSourceBlocked",
        "preimagesDuplicateObserved",
        "preimagesFiveIndependentFiles",
        "preimagesAbortAfterQuiescence",
    ] {
        report[check] = true.into();
    }
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
