//! Actual product Main plus the v18 actor, in an owned isolated Job.
use super::*;
use winbridge_update::candidate_support::{CandidateSupportOutcome, CandidateSupportRecord};
#[test]
#[ignore = "Explicit clean actual Main support gate; preserves all isolated files"]
fn actual_main_queries_candidate_support_without_draining_and_retains_lost_ack() {
    run(false);
}
#[test]
#[ignore = "Explicit clean actual17 production drain prerequisite gate"]
fn actual_main_blocks_incompatible_production_drain_before_worker_effects() {
    run(true);
}
fn run(production: bool) {
    let path = std::env::var_os("WINBRIDGE_SHUTDOWN_GATE_INPUT").expect("Explicit support gate");
    let input: GateInput = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(input.candidate_support);
    assert_eq!(input.compatible_drain, production);
    assert!(
        !input.final_backup
            && !input.runtime_replacement
            && !input.runtime_preparation
            && !input.supplemental
    );
    assert_ne!(
        winbridge_protocol::build_identity().source_commit.as_ref(),
        Some(&input.target.source_commit)
    );
    let stage = input.root.join("seed");
    winbridge_update::archive::stage_archive(&input.archive, &input.target, &stage).unwrap();
    fs::rename(stage.join("source"), input.root.join("source")).unwrap();
    let adapter = adapter(&input);
    let (root, config, manifest) = setup(
        &input,
        Fault::None,
        &adapter,
        BackupFault::None,
        PreparationFault::None,
        replacement::ReplacementFault::None,
    );
    let input_file = root.join("input.json");
    let vars = [
        (
            "WINBRIDGE_SHUTDOWN_MAIN_INPUT",
            input_file.as_os_str().to_owned(),
        ),
        (
            "WINBRIDGE_SHUTDOWN_MAIN_INPUT_SHA256",
            winbridge_update::sha256(&fs::read(&input_file).unwrap()).into(),
        ),
    ];
    let prior: Vec<_> = vars
        .iter()
        .map(|(k, _)| (*k, std::env::var_os(k)))
        .collect();
    for (k, v) in &vars {
        std::env::set_var(k, v);
    }
    let prepared = winbridge_platform::ManagedChild::prepare_atomic_executable(
        &ResolvedCommand {
            executable: root
                .join("runtime/winbridge-worker.exe")
                .to_string_lossy()
                .into(),
            arguments: vec![
                "--config".into(),
                config.to_string_lossy().into(),
                "serve".into(),
            ],
            working_directory: root.to_string_lossy().into(),
            timeout_ms: 900000,
        },
        Some(&format!(
            "WinBridge-support-main-validation-{}",
            uuid::Uuid::new_v4()
        )),
        None,
    );
    for (k, v) in prior {
        if let Some(v) = v {
            std::env::set_var(k, v)
        } else {
            std::env::remove_var(k)
        }
    }
    let (mut child, stdout, stderr) = prepared.unwrap();
    let capture = (capture(stdout), capture(stderr));
    child.resume().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let until = Instant::now() + Duration::from_secs(850);
        let mut progress = Instant::now();
        while !root.join("support-ready.json").is_file() {
            if progress.elapsed() >= Duration::from_secs(5) {
                let last = fs::read_dir(&root)
                    .unwrap()
                    .flatten()
                    .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                    .filter(|n| n.starts_with(|c: char| c.is_ascii_digit()))
                    .max();
                println!(
                    "Actual Main support: {}",
                    last.unwrap_or_else(|| "startup".into())
                );
                progress = Instant::now();
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "Main exited before support proof"
            );
            assert!(Instant::now() < until, "Main support pipeline deadline");
            std::thread::sleep(Duration::from_millis(100));
        }
        let before: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("07-support.json")).unwrap()).unwrap();
        let sup: SupervisorRecord = serde_json::from_value(before["supervisor"].clone()).unwrap();
        let binding: SupervisorWorkerBinding =
            serde_json::from_value(before["worker"].clone()).unwrap();
        assert_eq!(
            binding.process.process_id,
            child.identity().unwrap().process_id
        );
        let rt = tokio::runtime::Runtime::new().unwrap();
        // binding.endpoint speaks WorkerUpdateControlRequest, not LocalRequest.
        // The normal serve pipe is selected by the immutable instance context.
        let normal_endpoint =
            winbridge_transport::endpoint(&root.join("state"), &binding.context.instance_id)
                .unwrap();
        let winbridge_protocol::LocalResponse::Info { info } = rt
            .block_on(winbridge_transport::call(
                &normal_endpoint,
                &winbridge_protocol::LocalRequest::Info,
            ))
            .unwrap()
        else {
            panic!("Actual normal serve did not answer Info")
        };
        assert_eq!(info.generation, binding.generation);
        assert_eq!(info.build.as_ref(), Some(&binding.build));
        assert!(info.maintenance_error.is_none() && info.journal_recovery_required.is_none());
        let directory = root.join("runtime/update-state");
        let record = UpdateJournal::inspect_candidate_support(&directory, &sup.spec.update_id)
            .unwrap()
            .unwrap();
        record.validate().unwrap();
        assert_eq!(record.revision, 5);
        assert_eq!(
            record.result.as_ref().unwrap().outcome,
            CandidateSupportOutcome::Checked
        );
        assert!(!record
            .result
            .as_ref()
            .unwrap()
            .assessment
            .as_ref()
            .unwrap()
            .blockers
            .is_empty());
        let c = rusqlite::Connection::open_with_flags(
            directory.join("updates.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut q = c
            .prepare("SELECT record FROM candidate_support_history WHERE id=?1 ORDER BY revision")
            .unwrap();
        let history = q
            .query_map([&sup.spec.update_id], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|raw| serde_json::from_str::<CandidateSupportRecord>(&raw.unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(history.len(), 6);
        assert_eq!(history.last(), Some(&record));
        for pair in history.windows(2) {
            pair[0].advance(&pair[1]).unwrap();
        }
        drop(q);
        drop(c);
        let image_ids = manifest
            .artifacts
            .iter()
            .map(|a| inspect_image(&root.join("runtime").join(&a.name)).unwrap())
            .collect::<Vec<_>>();
        write(
            &root.join("support-cleanup.request"),
            &json!({"fixtureCleanup":true}),
        );
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until, "Fixture cleanup did not finish");
            std::thread::sleep(Duration::from_millis(25));
        }
        let cleanup: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("08-support-cleanup.json")).unwrap())
                .unwrap();
        let cold = UpdateJournal::inspect_candidate_support(&directory, &sup.spec.update_id)
            .unwrap()
            .unwrap();
        assert_eq!(cold, record);
        let view = if production {
            UpdateJournal::compatible_drain_snapshot(&directory, &sup.spec.update_id, false, None)
        } else {
            UpdateJournal::candidate_support_snapshot(&directory, &sup.spec.update_id, false, None)
        }
        .unwrap();
        assert_eq!(view.schema_version, if production { 11 } else { 10 });
        assert_eq!(view.update.phase, UpdatePhase::FailedBeforeCutover);
        assert_eq!(
            view.candidate_support.as_ref().unwrap().phase,
            CandidateSupportViewPhase::Checked
        );
        for (a, pin) in manifest.artifacts.iter().zip(&image_ids) {
            assert_eq!(
                inspect_image(&root.join("runtime").join(&a.name)).unwrap(),
                *pin
            );
        }
        if production {
            let gate = UpdateJournal::inspect_compatible_drain(&directory, &sup.spec.update_id)
                .unwrap()
                .unwrap();
            gate.validate(&record).unwrap();
            assert_eq!(
                serde_json::to_value(&gate).unwrap(),
                before["compatible"]["record"]
            );
            assert_eq!(
                view.compatible_drain.as_ref().unwrap().phase,
                CompatibleDrainPhase::Blocked
            );
        }
        json!({"schemaVersion":1,"kind":if production {"compatible_drain_main_native_proof"} else {"candidate_support_actor_main_native_proof"},"passed":true,
            "platform":"windows","build":winbridge_protocol::build_identity(),"target":input.target,
            "beforeCleanup":before,"cleanup":cleanup,"record":record,"coldRecord":cold,"coldView":view,"history":history,
            "normalIpcGeneration":info.generation,"normalIpcBuild":info.build,"oldRuntimeImages":image_ids,
            "actualNormalIpc":true,"oldRuntimeUnchanged":true,"runtimeApplied":false,
            "installedRuntimeApplied":false,"windowsOpened":false,"productionDrainEnforced":production})
    }));
    child.terminate_tree().unwrap();
    fs::write(root.join("stdout.log"), capture.0.join().unwrap()).unwrap();
    fs::write(root.join("stderr.log"), capture.1.join().unwrap()).unwrap();
    let proof = result.unwrap_or_else(|e| std::panic::resume_unwind(e));
    let record: CandidateSupportRecord = serde_json::from_value(proof["record"].clone()).unwrap();
    let process = record.probe.as_ref().unwrap().process.as_ref().unwrap();
    assert!(!winbridge_platform::infrastructure::matches(process).unwrap_or(false));
    write(&input.root.join("proof.json"), &proof);
}
