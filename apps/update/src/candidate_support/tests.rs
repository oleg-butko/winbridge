//! Synthetic histories validate metadata only; they never prove a real probe.
use super::*;
pub(crate) fn fixture() -> CandidateSupportRecord {
    let mut descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Updater);
    descriptor.build.source_dirty = false;
    descriptor.build.source_commit = Some("a".repeat(40));
    descriptor.build.source_manifest_sha256 = Some("b".repeat(64));
    descriptor.build.build_id = "fixture".into();
    let image = RuntimeImageEvidence {
        path: std::env::temp_dir()
            .join("support-fixture/updater.exe")
            .to_string_lossy()
            .into(),
        bytes: 10,
        sha256: "c".repeat(64),
        file_identity: "image".into(),
        query_supported: true,
    };
    let config = RuntimeImageEvidence {
        path: std::env::temp_dir()
            .join("support-fixture/worker.json")
            .to_string_lossy()
            .into(),
        bytes: 4,
        sha256: "d".repeat(64),
        file_identity: "config".into(),
        query_supported: false,
    };
    CandidateSupportRecord::admitted(CandidateSupportIntent {
        schema_version: 1,
        update_id: uuid::Uuid::new_v4().to_string(),
        attempt_id: uuid::Uuid::new_v4().to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        scope_sha256: "e".repeat(64),
        launch_id: uuid::Uuid::new_v4().to_string(),
        owner_id: uuid::Uuid::new_v4().to_string(),
        owner_epoch: 1,
        parent_revision: 10,
        admitted_at_ms: 100,
        config_sha256: "f".repeat(64),
        supervisor_image_sha256: "1".repeat(64),
        executor: descriptor.clone(),
        drivers: UpdaterDriverContracts::implemented_windows(),
        source_sha256: "2".repeat(64),
        health_sha256: "3".repeat(64),
        artifacts_sha256: "4".repeat(64),
        worker_config: config,
        image,
        descriptor,
        expected_updater_journal_schema: 16,
    })
    .unwrap()
}
pub(crate) fn stages(initial: &CandidateSupportRecord) -> Vec<CandidateSupportRecord> {
    let mut r = initial.clone();
    let mut all = vec![];
    r.revision = 1;
    r.phase = CandidateSupportPhase::Creating;
    r.probe = Some(CandidateSupportProbe {
        job_name: r.intent.job_name().unwrap(),
        command: r.intent.command().unwrap(),
        process: None,
        resume_granted: false,
        resume_acknowledged: false,
        cleanup: None,
    });
    all.push(r.clone());
    let process = winbridge_platform::infrastructure::stamp(std::process::id()).unwrap();
    r.revision = 2;
    r.phase = CandidateSupportPhase::ResumeGranted;
    r.probe.as_mut().unwrap().process = Some(process.clone());
    r.probe.as_mut().unwrap().resume_granted = true;
    all.push(r.clone());
    r.revision = 3;
    r.phase = CandidateSupportPhase::Running;
    r.probe.as_mut().unwrap().resume_acknowledged = true;
    all.push(r.clone());
    r.revision = 4;
    r.phase = CandidateSupportPhase::Captured;
    r.probe.as_mut().unwrap().cleanup = Some(ProbeCleanup {
        job_name: r.intent.job_name().unwrap(),
        process: Some(process.clone()),
        creation_confirmed: true,
        no_process_created: false,
        resume_requested: true,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: Some(0),
        failure: None,
    });
    all.push(r.clone());
    let report = UpdaterRuntimeSupport {
        schema_version: 1,
        kind: "winbridge_updater_runtime_support".into(),
        query_contract: 1,
        descriptor: r.intent.descriptor.clone(),
        process,
        updater_journal_schema: 16,
        build_flavor: UpdaterBuildFlavor::Standard,
        final_state_backup_contract: Some(15),
        runtime_preparation_contract: Some(16),
        runtime_replacement_contract: Some(17),
        activation_contract: None,
        controller_confirmation_contract: None,
        desktop_update_contract: None,
    };
    let assessment = assess_updater_compatibility_with_drivers(&report, 16, &r.intent.drivers);
    let mut worker = crate::health::tests::verified()
        .context
        .unwrap()
        .compatibility;
    worker.observed_at_ms = 109;
    r.revision = 5;
    r.phase = CandidateSupportPhase::Finished;
    r.result = Some(CandidateSupportResult {
        outcome: CandidateSupportOutcome::Checked,
        completed_at_ms: 110,
        reason: "Synthetic metadata, no physical approval".into(),
        report: Some(report),
        assessment: Some(assessment),
        final_worker: Some(worker),
    });
    all.push(r);
    all
}
#[test]
fn support_history_retains_exact_pid_cleanup_and_terminal_assessment() {
    let r = fixture();
    let all = stages(&r);
    let mut old = r;
    for next in &all {
        old.advance(next).unwrap();
        old = next.clone();
    }
    assert!(!old
        .result
        .as_ref()
        .unwrap()
        .assessment
        .as_ref()
        .unwrap()
        .blockers
        .is_empty());
    assert!(old.advance(&old).is_err());
    assert!(
        physically_quiescent(&old).is_err(),
        "Fixture process is still alive; metadata is not OS proof"
    );
    for mutate in 0..5 {
        let mut next = all[2].clone();
        match mutate {
            0 => next
                .probe
                .as_mut()
                .unwrap()
                .process
                .as_mut()
                .unwrap()
                .birth
                .push('0'),
            1 => next
                .probe
                .as_mut()
                .unwrap()
                .command
                .arguments
                .push("install".into()),
            2 => next.intent.owner_epoch += 1,
            3 => next.probe.as_mut().unwrap().resume_granted = false,
            _ => next.revision += 1,
        }
        assert!(all[1].advance(&next).is_err());
    }
}
#[test]
fn checked_result_cannot_skip_process_or_forge_schema_pid_and_policy() {
    let r = fixture();
    let all = stages(&r);
    assert!(r.advance(all.last().unwrap()).is_err());
    for mutate in 0..5 {
        let mut next = all[4].clone();
        let result = next.result.as_mut().unwrap();
        match mutate {
            0 => result.report.as_mut().unwrap().process.birth.push('0'),
            1 => result.assessment.as_mut().unwrap().blockers.clear(),
            2 => {
                next.probe
                    .as_mut()
                    .unwrap()
                    .cleanup
                    .as_mut()
                    .unwrap()
                    .tree_empty = false
            }
            3 => result.final_worker.as_mut().unwrap().observed_at_ms = 99,
            _ => result.report.as_mut().unwrap().activation_contract = Some(0),
        }
        assert!(next.validate().is_err());
    }
}
#[test]
fn unsupported_is_no_process_and_unknown_creation_never_releases_barrier() {
    let r = fixture();
    let mut done = r.clone();
    done.revision = 1;
    done.phase = CandidateSupportPhase::Finished;
    done.result = Some(CandidateSupportResult {
        outcome: CandidateSupportOutcome::Unsupported,
        completed_at_ms: 101,
        reason: "No marker; no execution".into(),
        report: None,
        assessment: None,
        final_worker: None,
    });
    r.advance(&done).unwrap();
    physically_quiescent(&done).unwrap();
    let creating = stages(&r)[0].clone();
    let mut unknown = creating.clone();
    unknown.revision = 2;
    unknown.phase = CandidateSupportPhase::Finished;
    unknown.result = Some(CandidateSupportResult {
        outcome: CandidateSupportOutcome::Indeterminate,
        completed_at_ms: 102,
        reason: "Creation outcome unknown".into(),
        report: None,
        assessment: None,
        final_worker: None,
    });
    creating.advance(&unknown).unwrap();
    assert!(physically_quiescent(&unknown).is_err());
    unknown.result.as_mut().unwrap().outcome = CandidateSupportOutcome::Rejected;
    assert!(unknown.validate().is_err());
}
#[test]
fn historical_assessment_uses_its_frozen_driver_capabilities() {
    let old = stages(&fixture()).pop().unwrap();
    let bytes = serde_json::to_vec(&old).unwrap();
    let parsed: CandidateSupportRecord = serde_json::from_slice(&bytes).unwrap();
    parsed.validate().unwrap();
    let mut future = parsed.intent.drivers.clone();
    future.activation = Some(18);
    future.controller_confirmation = Some(19);
    future.desktop_update = Some(20);
    let report = parsed.result.as_ref().unwrap().report.as_ref().unwrap();
    let new = assess_updater_compatibility_with_drivers(report, 16, &future);
    assert_ne!(
        parsed.result.as_ref().unwrap().assessment.as_ref(),
        Some(&new)
    );
    parsed.validate().unwrap();
    let mut bad = parsed;
    bad.intent.drivers = future;
    assert!(bad.validate().is_err());
}

#[test]
fn missing_pid_after_creation_never_becomes_confirmed_cleanup() {
    let initial = fixture();
    let creating = stages(&initial)[0].clone();
    let mut captured = creating.clone();
    captured.revision = 2;
    captured.phase = CandidateSupportPhase::Captured;
    captured.probe.as_mut().unwrap().cleanup = Some(ProbeCleanup {
        job_name: initial.intent.job_name().unwrap(),
        process: None,
        creation_confirmed: true,
        no_process_created: false,
        resume_requested: false,
        tree_empty: false,
        capture_complete: false,
        output_exceeded: false,
        exit_code: None,
        failure: Some("PID stamp unavailable".into()),
    });
    creating.advance(&captured).unwrap();
    assert!(!certain(captured.probe.as_ref().unwrap()));
    let mut forged = captured;
    let cleanup = forged.probe.as_mut().unwrap().cleanup.as_mut().unwrap();
    cleanup.tree_empty = true;
    cleanup.capture_complete = true;
    assert!(forged.validate().is_err());
}
