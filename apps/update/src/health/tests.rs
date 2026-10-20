use super::*;
fn record() -> HealthRecord {
    HealthRecord {
        schema_version: 1,
        revision: 0,
        intent: HealthIntent {
            schema_version: 1,
            update_id: uuid::Uuid::new_v4().to_string(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
            launch_id: uuid::Uuid::new_v4().to_string(),
            owner_id: "fixture".into(),
            owner_epoch: 2,
            parent_revision: 8,
            admitted_at_ms: 100,
            artifacts_sha256: "a".repeat(64),
            preimages_sha256: "b".repeat(64),
            directory: "/fixture/health".into(),
            worker_config_path: "/fixture/config.json".into(),
        },
        context: None,
        probes: vec![],
        result: None,
    }
}
fn probe(h: &HealthRecord) -> HealthProbe {
    HealthProbe {
        kind: HealthProbeKind::Config,
        job_name: job(&h.intent, HealthProbeKind::Config),
        command: winbridge_protocol::ResolvedCommand {
            executable: "fixture-worker".into(),
            arguments: vec![],
            working_directory: ".".into(),
            timeout_ms: 10,
        },
        process: None,
        resume_granted: false,
        resume_acknowledged: false,
        cleanup: None,
    }
}
#[test]
fn health_steps_preserve_identity_process_and_cleanup_and_never_replay() {
    let mut h = record();
    h.probes.push(probe(&h));
    let mut n = h.clone();
    n.probes[0].process =
        Some(winbridge_platform::infrastructure::stamp(std::process::id()).unwrap());
    n.probes[0].resume_granted = true;
    validate_step(&h, &n).unwrap();
    h = n;
    let mut n = h.clone();
    n.probes[0].process.as_mut().unwrap().birth.push('1');
    assert!(validate_step(&h, &n).is_err());
    let mut n = h.clone();
    n.probes.clear();
    assert!(validate_step(&h, &n).is_err());
    let mut n = h.clone();
    n.probes.push(probe(&h));
    assert!(validate_step(&h, &n).is_err());
    let mut n = h.clone();
    n.probes[0].resume_granted = false;
    assert!(validate_step(&h, &n).is_err());
    let mut n = h.clone();
    n.intent.owner_epoch += 1;
    assert!(validate_step(&h, &n).is_err());
    h.probes[0].cleanup = Some(ProbeCleanup {
        job_name: h.probes[0].job_name.clone(),
        process: h.probes[0].process.clone(),
        creation_confirmed: true,
        no_process_created: false,
        resume_requested: true,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: Some(0),
        failure: None,
    });
    let mut n = h.clone();
    n.probes[0].cleanup = None;
    assert!(validate_step(&h, &n).is_err());
    let mut n = h.clone();
    n.probes[0].resume_acknowledged = true;
    assert!(validate_step(&h, &n).is_err());
}
#[test]
fn health_pending_and_unknown_cleanup_keep_barrier_even_with_zero_exit() {
    let mut h = record();
    assert!(quiescent(&h).is_err());
    h.result = Some(HealthResult {
        outcome: HealthOutcome::Rejected,
        completed_at_ms: 101,
        reason: "No helper was started".into(),
        config: None,
        rehearsal: None,
        final_compatibility: None,
        parent_revision: None,
    });
    quiescent(&h).unwrap();
    h.probes.push(probe(&h));
    assert!(quiescent(&h).is_err());
    h.probes[0].cleanup = Some(ProbeCleanup {
        job_name: h.probes[0].job_name.clone(),
        process: None,
        creation_confirmed: false,
        no_process_created: true,
        resume_requested: false,
        tree_empty: true,
        capture_complete: true,
        output_exceeded: false,
        exit_code: None,
        failure: Some("Before-create rejected".into()),
    });
    quiescent(&h).unwrap();
    h.probes[0].cleanup.as_mut().unwrap().no_process_created = false;
    assert!(quiescent(&h).is_err());
    h.probes[0].cleanup.as_mut().unwrap().no_process_created = true;
    h.result.as_mut().unwrap().outcome = HealthOutcome::Indeterminate;
    assert!(quiescent(&h).is_err());
    assert!(validate_step(&h, &h).is_err());
}
#[test]
fn health_config_rehearsal_jobs_are_distinct_and_attempt_bound() {
    let h = record();
    let a = job(&h.intent, HealthProbeKind::Config);
    assert_ne!(a, job(&h.intent, HealthProbeKind::Rehearsal));
    let mut i = h.intent.clone();
    i.attempt_id = uuid::Uuid::new_v4().to_string();
    assert_ne!(a, job(&i, HealthProbeKind::Config));
    assert!(validate_success(&h).is_err());
}

// Synthetic DTO evidence tests only. They do not prove executable health or mint a permit.
fn verified() -> HealthRecord {
    use crate::candidate_artifacts::{CandidateArtifact, CandidateArtifactInspection};
    let mut h = record();
    let p = winbridge_platform::infrastructure::stamp(std::process::id()).unwrap();
    let descriptor =
        winbridge_protocol::artifact_descriptor(winbridge_protocol::ArtifactRole::Worker);
    let image = RuntimeImageEvidence {
        path: "/fixture/worker.exe".into(),
        bytes: 10,
        sha256: "c".repeat(64),
        file_identity: "worker-file".into(),
        query_supported: true,
    };
    let cfg = RuntimeImageEvidence {
        path: h.intent.worker_config_path.clone(),
        bytes: 4,
        sha256: "d".repeat(64),
        file_identity: "config-file".into(),
        query_supported: false,
    };
    let compat = WorkerRuntimeCompatibility {
        schema_version: 1,
        kind: "worker_runtime_compatibility".into(),
        identity: winbridge_protocol::LoadedWorkerIdentity {
            schema_version: 1,
            kind: "loaded_worker_identity".into(),
            process: p.clone(),
            runtime_identity: "runtime".into(),
            set_id: uuid::Uuid::new_v4().to_string(),
            manifest_sha256: "e".repeat(64),
            descriptor: descriptor.clone(),
            image: image.clone(),
        },
        context: winbridge_protocol::RequestContext {
            instance_id: "fixture".into(),
            journal_incarnation: uuid::Uuid::new_v4().to_string(),
        },
        generation: uuid::Uuid::new_v4().to_string(),
        worker_config_sha256: cfg.sha256.clone(),
        worker_config_schema: 1,
        worker_journal_schema: 15,
        updater_journal_schema: crate::journal::UPDATE_SCHEMA_VERSION,
        protocol_version: winbridge_protocol::PROTOCOL_VERSION,
        observed_at_ms: 102,
    };
    h.context = Some(HealthContext {
        compatibility: compat.clone(),
        worker_config: cfg.clone(),
        artifacts: ArtifactSetRecord {
            schema_version: 1,
            owner_id: h.intent.owner_id.clone(),
            owner_epoch: h.intent.owner_epoch,
            build_digest: "f".repeat(64),
            source_digest: "a".repeat(64),
            parent_revision: h.intent.parent_revision - 1,
            observed_at_ms: 100,
            inspection: CandidateArtifactInspection {
                schema_version: 1,
                kind: "candidate_artifact_inspection".into(),
                read_only: true,
                runtime_applied: false,
                candidate_verified: false,
                report: crate::build_report::ReportEvidence {
                    schema_version: 1,
                    update_id: h.intent.update_id.clone(),
                    attempt_id: uuid::Uuid::new_v4().to_string(),
                    build_spec_sha256: "b".repeat(64),
                    candidate_directory_identity: "candidate".into(),
                    file_identity: "report".into(),
                    bytes: 100,
                    sha256: "c".repeat(64),
                },
                payload_directory_identity: "payload".into(),
                artifacts: vec![CandidateArtifact {
                    name: "winbridge-worker.exe".into(),
                    image,
                    descriptor: descriptor.clone(),
                    query_process: p.clone(),
                }],
                set_sha256: "d".repeat(64),
            },
        },
    });
    for kind in [HealthProbeKind::Config, HealthProbeKind::Rehearsal] {
        let mut probe = probe(&h);
        probe.kind = kind;
        probe.job_name = job(&h.intent, kind);
        probe.command = command(&h.intent, h.context.as_ref().unwrap(), kind);
        probe.process = Some(p.clone());
        probe.resume_granted = true;
        probe.resume_acknowledged = true;
        probe.cleanup = Some(ProbeCleanup {
            job_name: probe.job_name.clone(),
            process: Some(p.clone()),
            creation_confirmed: true,
            no_process_created: false,
            resume_requested: true,
            tree_empty: true,
            capture_complete: true,
            output_exceeded: false,
            exit_code: Some(0),
            failure: None,
        });
        h.probes.push(probe);
    }
    h.result = Some(HealthResult {
        outcome: HealthOutcome::Verified,
        completed_at_ms: 103,
        reason: "Synthetic typed evidence".into(),
        config: Some(WorkerConfigCheckReport {
            schema_version: 1,
            kind: "worker_config_check".into(),
            descriptor: descriptor.clone(),
            process: p.clone(),
            config: cfg,
            worker_config_schema: 1,
            instance_id: "fixture".into(),
            worker_journal_opened: false,
            scheduler_started: false,
            candidate_verified: false,
            runtime_applied: false,
        }),
        rehearsal: Some(WorkerRehearsalReport {
            schema_version: 1,
            kind: "worker_rehearsal".into(),
            contract_version: 1,
            descriptor,
            process: p,
            directory: std::path::Path::new(&h.intent.directory)
                .join("worker")
                .to_string_lossy()
                .into(),
            directory_identity: "fresh-worker".into(),
            instance_id: format!("rehearsal-{}", uuid::Uuid::new_v4()),
            generation: uuid::Uuid::new_v4().to_string(),
            journal_incarnation: uuid::Uuid::new_v4().to_string(),
            worker_journal_schema: 15,
            updater_journal_schema: crate::journal::UPDATE_SCHEMA_VERSION,
            worker_config_schema: 1,
            protocol_version: winbridge_protocol::PROTOCOL_VERSION,
            fresh_journal: true,
            ipc_ready: true,
            server_stopped: true,
            controller_enabled: false,
            direct_commands_enabled: false,
            production_state_opened: false,
            candidate_verified: false,
            runtime_applied: false,
        }),
        final_compatibility: Some(compat),
        parent_revision: Some(h.intent.parent_revision + 1),
    });
    h
}
#[test]
fn health_success_rejects_schema_process_isolation_staleness_and_incomplete_cleanup() {
    let h = verified();
    validate_success(&h).unwrap();
    quiescent(&h).unwrap();
    for field in [
        "schema",
        "process",
        "live-schema",
        "stale",
        "config",
        "production",
        "journal",
        "server",
        "generation",
        "capture",
        "tree",
        "resume",
        "quota",
    ] {
        let mut n = h.clone();
        let r = n.result.as_mut().unwrap();
        match field {
            "schema" => r.rehearsal.as_mut().unwrap().worker_journal_schema += 1,
            "process" => r.config.as_mut().unwrap().process.birth.push('9'),
            "live-schema" => {
                r.final_compatibility
                    .as_mut()
                    .unwrap()
                    .updater_journal_schema += 1
            }
            "stale" => r.final_compatibility.as_mut().unwrap().observed_at_ms -= 1,
            "config" => r.config.as_mut().unwrap().config.file_identity.push('x'),
            "production" => r.rehearsal.as_mut().unwrap().production_state_opened = true,
            "journal" => r.rehearsal.as_mut().unwrap().fresh_journal = false,
            "server" => r.rehearsal.as_mut().unwrap().server_stopped = false,
            "generation" => {
                r.rehearsal.as_mut().unwrap().generation =
                    n.context.as_ref().unwrap().compatibility.generation.clone()
            }
            "capture" => n.probes[1].cleanup.as_mut().unwrap().capture_complete = false,
            "tree" => n.probes[1].cleanup.as_mut().unwrap().tree_empty = false,
            "resume" => n.probes[0].resume_acknowledged = false,
            "quota" => n.probes[0].cleanup.as_mut().unwrap().output_exceeded = true,
            _ => unreachable!(),
        }
        assert!(validate_success(&n).is_err(), "{field}");
    }
}
#[test]
fn health_view_keeps_pending_separate_from_finished_and_exposes_both_processes() {
    assert_eq!(view(None).phase, CandidateHealthPhase::NotAdmitted);
    let mut h = verified();
    assert_eq!(view(Some(&h)).phase, CandidateHealthPhase::Verified);
    assert!(view(Some(&h)).cleanup_verified);
    h.result = None;
    assert_eq!(view(Some(&h)).phase, CandidateHealthPhase::Pending);
    assert!(!view(Some(&h)).cleanup_verified);
    assert!(view(Some(&h)).config_process.is_some());
    assert!(view(Some(&h)).rehearsal_process.is_some());
}
