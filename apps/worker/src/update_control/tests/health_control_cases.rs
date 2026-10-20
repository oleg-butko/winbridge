//! Real clean candidate pipeline under a copied, explicitly test-only worker.
//! The installed runtime is untouched. This is not an activation/install test.
#[cfg(windows)]
use super::*;
#[cfg(windows)]
use serde::{Deserialize, Serialize};

#[cfg(windows)]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema_version: u16,
    target: UpdateSourcePin,
    archive: PathBuf,
    powershell: PathBuf,
    validation_cache: PathBuf,
}
#[cfg(windows)]
fn build_adapter(root: &Path, input: &Input) -> PathBuf {
    // Local fixture only. Forward the frozen command to the real build script;
    // adding its existing validation cache argument changes no production policy.
    let path = root.join("health-build-adapter.exe");
    let source = root.join("health-build-adapter.rs");
    let plain = |p: &Path| {
        p.to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap_or(&p.to_string_lossy())
            .to_owned()
    };
    fs::write(&source,format!(r#"fn main(){{use std::os::windows::process::CommandExt;let status=std::process::Command::new({:?}).creation_flags(0x08000000).args(std::env::args_os().skip(1)).arg("-ValidationCacheDirectory").arg({:?}).status().expect("start private PowerShell adapter");std::process::exit(status.code().unwrap_or(1));}}"#,plain(&input.powershell),plain(&input.validation_cache))).unwrap();
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("rustc.exe")
        .creation_flags(0x08000000)
        .args(["--edition=2021"])
        .arg(&source)
        .arg("-o")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    path
}

#[cfg(windows)]
async fn wait_phase(
    h: &SupervisorHandle,
    phase: UpdatePhase,
    limit: Duration,
) -> UpdatePreparationView {
    let until = Instant::now() + limit;
    loop {
        let view = h.candidate_health(false).await.unwrap();
        assert!(view.driver_error.is_none(), "{:?}", view.driver_error);
        if !view.driver_active {
            assert_eq!(view.update.phase, phase);
            return view;
        }
        assert!(
            Instant::now() < until,
            "Pipeline did not quiesce at {phase:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
#[cfg(windows)]
async fn prepare_candidate(f: &mut Fixture) -> UpdatePreparationView {
    let h = f.supervisor.as_mut().unwrap();
    let until = Instant::now() + Duration::from_secs(45);
    h.runtime_baseline(true).await.unwrap();
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
    h.runtime_preimages(true).await.unwrap();
    let until = Instant::now() + Duration::from_secs(45);
    loop {
        let v = h.runtime_preimages(false).await.unwrap();
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if !v.driver_active {
            assert!(matches!(
                v.runtime_preimages,
                Some(RuntimePreimagesView::Verified { .. })
            ));
            break;
        }
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    h.preparation(true).await.unwrap();
    wait_phase(h, UpdatePhase::Building, Duration::from_secs(600)).await;
    h.build_candidate().await.unwrap();
    wait_phase(h, UpdatePhase::Building, Duration::from_secs(330)).await;
    h.verify_candidate_artifacts().await.unwrap();
    wait_phase(h, UpdatePhase::ValidatingCandidate, Duration::from_secs(60)).await
}
#[cfg(windows)]
async fn retired(f: &mut Fixture) {
    let p = f
        .supervisor
        .as_ref()
        .unwrap()
        .record
        .process
        .clone()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match infrastructure::matches(&p) {
            Ok(false) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(87) => {
                break
            }
            Ok(true) => {}
            Err(e) => panic!("Supervisor exit unknown: {e}"),
        }
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    f.supervisor.take();
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[ignore = "Owned clean Windows health process helper; only its parent may invoke it"]
async fn health_actor_process_helper() {
    use winbridge_update::health::{HealthOutcome, HealthRecord};
    let args: Vec<_> = std::env::args().collect();
    let value = |key: &str| {
        args.iter()
            .find_map(|s| s.strip_prefix(key))
            .unwrap()
            .to_owned()
    };
    let root = PathBuf::from(value("health-root="));
    let binaries = PathBuf::from(value("health-binaries="));
    let input: Input = serde_json::from_slice(&fs::read(value("health-input=")).unwrap()).unwrap();
    assert_eq!(input.schema_version, 1);
    assert_eq!(
        fs::canonicalize(std::env::current_exe().unwrap()).unwrap(),
        fs::canonicalize(root.join("runtime/winbridge-worker.exe")).unwrap()
    );
    let adapter = build_adapter(&root, &input);
    let mut f = Fixture::with_build_script_at(
        None,
        true,
        Some(adapter),
        BUILD_ACTOR_SCRIPT,
        Some((root.clone(), binaries)),
        Some(TargetPackage {
            archive: input.archive.clone(),
            pin: input.target.clone(),
        }),
    )
    .await;
    let reg = f.worker.control_registration().unwrap();
    let old_manifest = fs::read(root.join("runtime/winbridge-installed-set.json")).unwrap();
    let mut cases = Vec::new();
    for case in [
        "pid-receipt-rejected",
        "cancel-before-helper",
        "cancel-running-rehearsal",
        "verified",
    ] {
        f.request.idempotency_key = format!("health-full-{case}");
        if !cases.is_empty() {
            f.baseline = input.target.manifest_sha256.clone();
            f.request.expected.baseline_manifest_sha256 = f.baseline.clone();
        }
        f.launch().await;
        let before = prepare_candidate(&mut f).await;
        let h = f.supervisor.as_mut().unwrap();
        let id = h.record.spec.update_id.clone();
        assert_eq!(before.update.request.source, input.target);
        assert_eq!(
            before.candidate_health.as_ref().unwrap().phase,
            CandidateHealthPhase::NotAdmitted
        );
        // A real v3 response is held AFTER identity hashing, outside the worker
        // store mutex. The actor must ACK/poll/abort while its driver is blocked.
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.runtime_verified_gate.lock().unwrap() = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        let ack = tokio::time::timeout(Duration::from_secs(3), h.candidate_health(true))
            .await
            .unwrap()
            .unwrap();
        assert!(ack.driver_active);
        assert_eq!(ack.update.revision, before.update.revision + 1);
        assert_eq!(
            ack.candidate_health.as_ref().unwrap().phase,
            CandidateHealthPhase::Pending
        );
        let admitted = UpdateJournal::inspect_health(&reg.directory, &id)
            .unwrap()
            .unwrap();
        assert!(admitted.context.is_none());
        assert!(admitted.probes.is_empty());
        tokio::pin!(entered_rx);
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            tokio::select! {
                entered=&mut entered_rx=>{entered.expect("Live compatibility callback did not arrive");break;},
                _=tokio::time::sleep(Duration::from_millis(100))=>{}
            }
            let status = h.candidate_health(false).await.unwrap();
            if !status.driver_active || status.driver_error.is_some() || Instant::now() >= until {
                let saved = UpdateJournal::inspect_health(&reg.directory, &id).unwrap();
                let diagnostic = serde_json::json!({"schemaVersion":1,"kind":"health_callback_failure","status":status,"health":saved,"callbackObserved":false,"runtimeApplied":false});
                let report =
                    PathBuf::from(std::env::var_os("WINBRIDGE_HEALTH_GATE_PROOF").unwrap())
                        .with_extension("failure.json");
                fs::write(report, serde_json::to_vec_pretty(&diagnostic).unwrap()).unwrap();
                panic!("Health callback pending/stopped: {diagnostic}");
            }
        }
        let view = tokio::time::timeout(Duration::from_secs(2), h.candidate_health(false))
            .await
            .unwrap()
            .unwrap();
        assert!(view.driver_active);
        assert_eq!(h.candidate_health(true).await.unwrap().update, ack.update);
        assert_eq!(
            UpdateJournal::inspect_health(&reg.directory, &id)
                .unwrap()
                .unwrap(),
            admitted
        );
        let health: HealthRecord;
        let abort;
        if case == "cancel-before-helper" {
            let cancel = h.abort_before_cutover();
            tokio::pin!(cancel);
            let until = Instant::now() + Duration::from_secs(3);
            loop {
                tokio::select! {r=&mut cancel=>panic!("Abort completed with blocked health callback: {r:?}"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
                if let Ok(r) = UpdateJournal::inspect_abort(&reg.directory, &id) {
                    assert_eq!(r.phase, winbridge_update::abort::AbortPhase::Intent);
                    assert!(!r.cleanup_verified);
                    break;
                }
                assert!(Instant::now() < until);
            }
            assert!(f
                .worker
                .store()
                .unwrap()
                .active_worker_update()
                .unwrap()
                .is_some());
            release_tx.send(()).unwrap();
            abort = cancel.await.unwrap();
            health = UpdateJournal::inspect_health(&reg.directory, &id)
                .unwrap()
                .unwrap();
            assert_eq!(
                health.result.as_ref().unwrap().outcome,
                HealthOutcome::Cancelled
            );
            assert!(health.probes.is_empty());
        } else if case == "cancel-running-rehearsal" {
            release_tx.send(()).unwrap();
            let until = Instant::now() + Duration::from_secs(45);
            let running = loop {
                let saved = UpdateJournal::inspect_health(&reg.directory, &id)
                    .unwrap()
                    .unwrap();
                if let Some(p) = saved
                    .probes
                    .get(1)
                    .filter(|p| p.resume_acknowledged && p.cleanup.is_none())
                {
                    let process = p.process.as_ref().unwrap();
                    if infrastructure::matches(process).unwrap_or(false) {
                        break process.clone();
                    }
                }
                assert!(
                    saved.result.is_none(),
                    "Rehearsal completed before live observation; no cancellation proof fabricated"
                );
                assert!(
                    Instant::now() < until,
                    "No running rehearsal process observed"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            };
            abort = h.abort_before_cutover().await.unwrap();
            health = UpdateJournal::inspect_health(&reg.directory, &id)
                .unwrap()
                .unwrap();
            assert_eq!(
                health.result.as_ref().unwrap().outcome,
                HealthOutcome::Cancelled
            );
            let p = &health.probes[1];
            assert_eq!(p.process.as_ref(), Some(&running));
            let c = p.cleanup.as_ref().unwrap();
            assert!(
                c.resume_requested && c.creation_confirmed && c.tree_empty && c.capture_complete
            );
            // Root cancellation must win; the helper may exit naturally before
            // the poll callback. Its exact cleanup record preserves that race.
        } else {
            if case == "pid-receipt-rejected" {
                let c = rusqlite::Connection::open(reg.directory.join("updates.sqlite")).unwrap();
                c.execute_batch("CREATE TRIGGER deny_health_pid BEFORE UPDATE ON candidate_health WHEN json_extract(NEW.record,'$.probes[0].resumeGranted')=1 BEGIN SELECT RAISE(IGNORE); END;").unwrap();
            }
            release_tx.send(()).unwrap();
            let until = Instant::now() + Duration::from_secs(60);
            loop {
                let v = h.candidate_health(false).await.unwrap();
                if !v.driver_active {
                    break;
                }
                assert!(Instant::now() < until, "Health driver did not retire");
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            health = UpdateJournal::inspect_health(&reg.directory, &id)
                .unwrap()
                .unwrap();
            let expected = if case == "pid-receipt-rejected" {
                HealthOutcome::Rejected
            } else {
                HealthOutcome::Verified
            };
            if health.result.as_ref().map(|r| &r.outcome) != Some(&expected) {
                let status = h.candidate_health(false).await.unwrap();
                let diagnostic = serde_json::json!({"schemaVersion":1,"kind":"health_outcome_failure","case":case,"expected":expected,"status":status,"health":health,"callbackObserved":true,"runtimeApplied":false});
                let report =
                    PathBuf::from(std::env::var_os("WINBRIDGE_HEALTH_GATE_PROOF").unwrap())
                        .with_extension("failure.json");
                fs::write(report, serde_json::to_vec_pretty(&diagnostic).unwrap()).unwrap();
            }
            if case == "pid-receipt-rejected" {
                assert_eq!(
                    health.result.as_ref().unwrap().outcome,
                    HealthOutcome::Rejected
                );
                assert_eq!(health.probes.len(), 1);
                let p = &health.probes[0];
                let c = p.cleanup.as_ref().unwrap();
                assert!(p.process.is_some() && !p.resume_granted && !p.resume_acknowledged);
                assert!(
                    !c.resume_requested
                        && c.creation_confirmed
                        && c.tree_empty
                        && c.capture_complete
                );
                let c = rusqlite::Connection::open(reg.directory.join("updates.sqlite")).unwrap();
                c.execute_batch("DROP TRIGGER deny_health_pid").unwrap();
            } else {
                assert_eq!(
                    health.result.as_ref().unwrap().outcome,
                    HealthOutcome::Verified,
                    "{health:?}"
                );
                assert_eq!(health.probes.len(), 2);
                let r = health.result.as_ref().unwrap();
                assert!(
                    r.config.is_some() && r.rehearsal.is_some() && r.final_compatibility.is_some()
                );
                assert_ne!(
                    health
                        .context
                        .as_ref()
                        .unwrap()
                        .compatibility
                        .identity
                        .descriptor
                        .build
                        .source_manifest_sha256,
                    Some(input.target.manifest_sha256.clone())
                );
                let verified = h.candidate_health(false).await.unwrap();
                assert_eq!(verified.update.phase, UpdatePhase::CandidateVerified);
                assert_eq!(verified.update.revision, ack.update.revision + 1);
                assert_eq!(
                    h.candidate_health(true).await.unwrap().update,
                    verified.update
                );
                assert_eq!(
                    UpdateJournal::inspect_health(&reg.directory, &id)
                        .unwrap()
                        .unwrap(),
                    health
                );
            }
            abort = h.abort_before_cutover().await.unwrap();
        }
        assert_eq!(abort.phase, winbridge_update::abort::AbortPhase::Completed);
        assert!(abort.cleanup_verified);
        assert!(abort.worker_released.is_some());
        assert!(f
            .worker
            .store()
            .unwrap()
            .active_worker_update()
            .unwrap()
            .is_none());
        assert_eq!(
            fs::read(root.join("runtime/winbridge-installed-set.json")).unwrap(),
            old_manifest
        );
        cases.push(serde_json::json!({"case":case,"health":health,"abort":abort}));
        retired(&mut f).await;
    }
    f.worker.stop();
    (&mut f.server).await.unwrap().unwrap();
    let proof = serde_json::json!({"schemaVersion":1,"kind":"candidate_health_actor_proof","passed":true,"platform":"windows","build":build_identity(),"target":input.target,"cases":cases,"actualCandidateBuild":true,"actualCandidateHelpers":true,"workerEntryPointIsTestHarness":true,"syntheticOldOtherRoleDescriptors":true,"intentBeforeAck":true,"duplicateObserved":true,"pidBeforeResume":true,"abortWaitedForDriver":true,"newEmptyJournalOnly":true,"runtimeApplied":false,"windowsOpened":false});
    fs::write(
        value("health-proof="),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "Explicit clean Windows health integration; actual build/rehearsal under private Jobs"]
fn health_actor_full_candidate_roundtrip() {
    use std::io::Read;
    let input = PathBuf::from(
        std::env::var_os("WINBRIDGE_HEALTH_GATE_INPUT")
            .expect("Explicit bounded local native test input"),
    );
    let build = build_identity();
    assert!(!build.source_dirty && build.source_manifest_sha256.is_some());
    let exe = std::env::current_exe().unwrap();
    let binaries = exe.parent().unwrap().parent().unwrap();
    let root = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let runtime = root.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    let image = runtime.join("winbridge-worker.exe");
    fs::copy(&exe, &image).unwrap();
    let proof = root.path().join("health-proof.json");
    let cmd = ResolvedCommand {
        executable: image.to_string_lossy().into(),
        arguments: vec![
            "--exact".into(),
            "update_control::tests::health::health_actor_process_helper".into(),
            "--ignored".into(),
            "--nocapture".into(),
            "--skip".into(),
            format!("health-root={}", root.path().display()),
            "--skip".into(),
            format!("health-binaries={}", binaries.display()),
            "--skip".into(),
            format!("health-input={}", input.display()),
            "--skip".into(),
            format!("health-proof={}", proof.display()),
        ],
        working_directory: runtime.to_string_lossy().into(),
        timeout_ms: 1500000,
    };
    let name = format!("WinBridge-health-validation-{}", uuid::Uuid::new_v4());
    let (mut child, stdout, stderr) =
        winbridge_platform::ManagedChild::prepare_atomic_executable(&cmd, Some(&name), None)
            .unwrap();
    let capture = |mut r: winbridge_platform::OutputReader| {
        std::thread::spawn(move || {
            let mut bytes = vec![];
            let mut buffer = [0u8; 16384];
            loop {
                let n = r.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                let save = n.min((2 * 1024 * 1024usize).saturating_sub(bytes.len()));
                bytes.extend_from_slice(&buffer[..save]);
                // Keep draining after the diagnostic quota; do not block a compiler.
            }
            bytes
        })
    };
    let out = (capture(stdout), capture(stderr));
    child.resume().unwrap();
    let until = Instant::now() + Duration::from_secs(1500);
    let mut progress = Instant::now();
    let status = loop {
        if progress.elapsed() >= Duration::from_secs(5) {
            let directory = root.path().join("runtime/update-state");
            if let Ok((_, Some(r))) = UpdateJournal::read_snapshot(&directory, None) {
                if let Ok(v) =
                    UpdateJournal::preparation_snapshot(&directory, &r.update_id, false, None)
                {
                    if let Some(source) = v.source {
                        println!(
                            "Windows health validation: {:?}; source files {}/{}; operation {}",
                            r.phase, v.source_applied_files, source.total_files, r.update_id
                        );
                    } else {
                        let prepared = root
                            .path()
                            .join("runtime/update")
                            .join(&r.update_id)
                            .join("source-transaction/prepared");
                        let count = fs::read_dir(prepared)
                            .map(|entries| entries.filter_map(std::result::Result::ok).count())
                            .unwrap_or(0);
                        println!(
                            "Windows health validation: {:?}; prepared files {}; operation {}",
                            r.phase, count, r.update_id
                        );
                    }
                }
            }
            progress = Instant::now();
        }
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if Instant::now() >= until {
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    child.terminate_tree().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !child.tree_is_empty().unwrap() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(child.tree_is_empty().unwrap());
    let stdout = String::from_utf8_lossy(&out.0.join().unwrap()).into_owned();
    let stderr = String::from_utf8_lossy(&out.1.join().unwrap()).into_owned();
    assert!(
        status.is_some_and(|s| s.success()),
        "Health native helper failed/timed out:\n{stdout}\n{stderr}"
    );
    let bytes = fs::read(proof).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 4);
    fs::write(
        std::env::var_os("WINBRIDGE_HEALTH_GATE_PROOF").expect("Explicit native proof path"),
        bytes,
    )
    .unwrap();
}
