//! Synthetic records only; no SQLite export, OS guard or launch permission.
use super::*;
fn fixture() -> StateBackupRecord {
    let (original, mut receipt, physical) = crate::shutdown::tests::fixture();
    let mut binding = original.binding;
    binding.loaded_worker.descriptor.target_os = "windows".into();
    let intent =
        crate::shutdown::ShutdownIntent::from_quiesced(binding, original.attempt_id).unwrap();
    receipt.command = intent.command.clone();
    let shutdown = ShutdownRecord::prepare(intent)
        .unwrap()
        .issue(101)
        .unwrap()
        .acknowledge(201, receipt)
        .unwrap()
        .confirm_exit(301, physical)
        .unwrap();
    let cfg = crate::health::tests::verified()
        .context
        .unwrap()
        .worker_config;
    StateBackupRecord::prepare(
        StateBackupIntent::from_exited(
            StateBackupBinding {
                shutdown: Box::new(shutdown),
                worker_config: cfg,
                worker_journal_schema: 16,
                parent_directory: "/fixture/runtime/update/run".into(),
                parent_identity: "backup-parent".into(),
                admitted_at_ms: 400,
            },
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn input(r: &StateBackupRecord) -> StateBackupFileEvidence {
    StateBackupFileEvidence {
        path: r.intent.input_path().to_str().unwrap().into(),
        bytes: 100,
        sha256: "e".repeat(64),
        file_identity: "input-file".into(),
    }
}
fn process(r: &StateBackupRecord) -> SupervisorProcess {
    let mut p = r
        .intent
        .binding
        .shutdown
        .intent
        .binding
        .worker
        .process
        .clone();
    p.process_id += 1;
    p.birth = "124".into();
    p
}
fn running() -> StateBackupRecord {
    let r = fixture();
    let p = process(&r);
    r.issue(input(&r), 401)
        .unwrap()
        .grant_resume(p, 402)
        .unwrap()
        .resumed(403)
        .unwrap()
}
fn cleanup(r: &StateBackupRecord) -> ProbeCleanup {
    ProbeCleanup {
        job_name: r.intent.job_name(),
        process: r.process.clone(),
        creation_confirmed: true,
        no_process_created: false,
        resume_requested: true,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: Some(0),
        failure: None,
    }
}
fn report(r: &StateBackupRecord) -> WorkerStateBackupReport {
    let b = &r.intent.binding;
    let s = &b.shutdown.intent.binding;
    let a = StateBackupAuthority {
        incarnation: s.worker.context.journal_incarnation.clone(),
        epoch: 7,
        fingerprint_sha256: "a".repeat(64),
    };
    WorkerStateBackupReport {
        schema_version: 1,
        kind: "worker_state_backup".into(),
        input_sha256: r.input_file.as_ref().unwrap().sha256.clone(),
        update_id: r.intent.update_id().into(),
        attempt_id: r.intent.attempt_id.clone(),
        helper_descriptor: s.loaded_worker.descriptor.clone(),
        helper_process: r.process.clone().unwrap(),
        helper_image: s.loaded_worker.image.clone(),
        worker_config: b.worker_config.clone(),
        state: s.worker_state.clone(),
        worker_journal_schema: 16,
        source_authority: a.clone(),
        backup_authority: a,
        shutdown_receipt: r.intent.receipt().clone(),
        backup: StateBackupFileEvidence {
            path: r.intent.backup_path().to_str().unwrap().into(),
            bytes: 8192,
            sha256: "f".repeat(64),
            file_identity: "backup-file".into(),
        },
        page_size: 4096,
        page_count: 2,
        completed_at_ms: 405,
        integrity_verified: true,
        backup_created_new: true,
        journal_opened_read_only: true,
        worker_constructed: false,
        scheduler_started: false,
        recovery_marker_changed: false,
        runtime_applied: false,
    }
}
#[test]
fn backup_process_order_and_reopened_issue_never_mint_a_second_attempt() {
    let r = fixture();
    assert_eq!(r.intent.parent_revision().unwrap(), 16);
    assert!(r.grant_resume(process(&r), 401).is_err());
    let issued = r.issue(input(&r), 401).unwrap();
    let restored: StateBackupRecord =
        serde_json::from_slice(&serde_json::to_vec(&issued).unwrap()).unwrap();
    restored.validate().unwrap();
    assert!(restored.issue(input(&r), 402).is_err());
    assert!(restored.resumed(402).is_err());
    let run = running();
    assert!(run.verify(report(&run), 410).is_err());
    let done = run.finished(cleanup(&run), 406).unwrap();
    let result = report(&done);
    let verified = done.verify(result.clone(), 410).unwrap();
    assert_eq!(verified.phase, StateBackupPhase::Verified);
    assert_eq!(verified.revision, 5);
    assert_eq!(verified.verify(result, 411).unwrap(), verified);
    assert!(verified.indeterminate("lost".into(), 412).is_err());
}
#[test]
fn backup_uncertainty_is_terminal_and_preserves_original_scope_and_process() {
    for r in [fixture(), running()] {
        let unknown = r
            .indeterminate("Outcome unknown; inspect original attempt".into(), 410)
            .unwrap();
        let saved: StateBackupRecord =
            serde_json::from_slice(&serde_json::to_vec(&unknown).unwrap()).unwrap();
        saved.validate().unwrap();
        assert_eq!(saved.intent, r.intent);
        assert_eq!(saved.process, r.process);
        assert!(saved.issue(input(&r), 411).is_err());
        assert!(saved.resumed(411).is_err());
        assert!(saved.indeterminate("retry".into(), 411).is_err());
    }
}
#[test]
fn backup_zero_exit_with_missing_cleanup_or_wrong_result_is_never_verified() {
    let r = running();
    for change in 0..6 {
        let mut c = cleanup(&r);
        match change {
            0 => c.tree_empty = false,
            1 => c.capture_complete = false,
            2 => c.exit_code = Some(1),
            3 => c.output_exceeded = true,
            4 => c.failure = Some("unconfirmed cleanup".into()),
            _ => c.resume_requested = false,
        };
        let done = r.finished(c, 406).unwrap();
        assert!(done.verify(report(&done), 410).is_err());
    }
    let done = r.finished(cleanup(&r), 406).unwrap();
    for change in 0..8 {
        let mut v = report(&done);
        match change {
            0 => v.input_sha256 = "a".repeat(64),
            1 => v.helper_process.birth.push('1'),
            2 => v.helper_image.file_identity.push('1'),
            3 => v.worker_config.file_identity.push('1'),
            4 => v.worker_journal_schema = 17,
            5 => v.backup.path.push('x'),
            6 => v.backup.file_identity = "input-file".into(),
            _ => v.attempt_id = uuid::Uuid::new_v4().to_string(),
        };
        assert!(done.verify(v, 410).is_err());
    }
}
#[test]
fn backup_no_process_or_unresumed_process_cleanup_does_not_authorize_export() {
    let r = fixture();
    let c = ProbeCleanup {
        job_name: r.intent.job_name(),
        process: None,
        creation_confirmed: false,
        no_process_created: true,
        resume_requested: false,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: None,
        failure: Some("before create rejected".into()),
    };
    let done = r.finished(c, 401).unwrap();
    assert_eq!(done.phase, StateBackupPhase::Cleaned);
    assert!(done.issue(input(&r), 402).is_err());
    let issued = r.issue(input(&r), 401).unwrap();
    let p = process(&r);
    let c = ProbeCleanup {
        job_name: r.intent.job_name(),
        process: Some(p.clone()),
        creation_confirmed: true,
        no_process_created: false,
        resume_requested: false,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: Some(0),
        failure: None,
    };
    let done = issued.finished(c, 403).unwrap();
    assert_eq!(done.process, Some(p));
    assert!(done.verify(report(&done), 410).is_err());
}
#[test]
fn backup_binding_digest_and_input_or_report_aliases_are_rejected() {
    let r = fixture();
    let mut broken = r.clone();
    broken.intent.binding.parent_identity.push('x');
    assert!(broken.validate().is_err());
    let mut early = r.intent.binding.clone();
    early.admitted_at_ms = 299;
    assert!(StateBackupIntent::from_exited(early, uuid::Uuid::new_v4().to_string()).is_err());
    let mut f = input(&r);
    f.file_identity = r
        .intent
        .binding
        .shutdown
        .intent
        .binding
        .worker_state
        .journal_identity
        .clone();
    assert!(r.issue(f, 401).is_err());
    let run = running();
    let done = run.finished(cleanup(&run), 406).unwrap();
    let mut verified = done.verify(report(&done), 410).unwrap();
    verified.intent.binding_sha256 = "0".repeat(64);
    let report = verified.report.as_deref().unwrap().clone();
    assert!(verified.verify(report, 411).is_err());
}
