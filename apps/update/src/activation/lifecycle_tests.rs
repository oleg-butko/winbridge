//! Synthetic evidence only: no process, writer or startup permit is created.
use super::*;
fn process(pid: u32) -> SupervisorProcess {
    SupervisorProcess {
        process_id: pid,
        birth: (100 + pid).to_string(),
        host_id: "a".repeat(64),
        session_id: 1,
    }
}
fn fixture() -> (RuntimeReplacementRecord, ActivationRecord) {
    let replacement = crate::runtime_replacement::tests::completed();
    let runtime = std::path::Path::new(&replacement.intent.preparation.plan.runtime_directory);
    let context = ActivationContext {
        supervisor: process(11),
        previous_worker: process(10),
        previous_generation: uuid::Uuid::new_v4().to_string(),
        worker_context: RequestContext {
            instance_id: "activation-test".into(),
            journal_incarnation: uuid::Uuid::new_v4().to_string(),
        },
        worker_state: WorkerShutdownState {
            state_directory: runtime.join("state").to_str().unwrap().into(),
            directory_identity: "state-directory".into(),
            journal_identity: "state-journal".into(),
            lock_identity: "state-lock".into(),
        },
        worker_config: ImageEvidence {
            path: runtime
                .join("winbridge-worker.local.json")
                .to_str()
                .unwrap()
                .into(),
            bytes: 128,
            sha256: "b".repeat(64),
            file_identity: "worker-config".into(),
            query_supported: false,
        },
        worker_config_schema: 1,
        worker_journal_schema: 16,
        updater_journal_schema: 17,
        protocol_version: 1,
        startup_timeout_ms: 120000,
        channel_nonce_sha256: "c".repeat(64),
    };
    let intent = ActivationIntent {
        schema_version: 1,
        attempt_id: uuid::Uuid::new_v4().to_string(),
        admitted_at_ms: 200,
        source: ActivationSource::from_files_moved(&replacement).unwrap(),
        context,
    };
    let record = ActivationRecord::prepared(intent, &replacement).unwrap();
    (replacement, record)
}
fn created(v: &ActivationRecord, at: u64) -> ActivationCreated {
    ActivationCreated {
        process: process(12),
        job_name: v.intent.job_name().unwrap(),
        observed_at_ms: at,
    }
}
fn stages(initial: &ActivationRecord, r: &RuntimeReplacementRecord) -> Vec<ActivationRecord> {
    let mut values = vec![initial.clone(), initial.issue_create(201, r).unwrap()];
    values.push(
        values
            .last()
            .unwrap()
            .created(created(initial, 202), 202, r)
            .unwrap(),
    );
    values.push(values.last().unwrap().grant_resume(203, r).unwrap());
    let source = &initial.intent.source;
    let ready = ActivationReady {
        attempt_id: initial.intent.attempt_id.clone(),
        identity: LoadedWorkerIdentity {
            schema_version: 1,
            kind: "loaded_worker_identity".into(),
            process: process(12),
            runtime_identity: source.runtime_identity.clone(),
            set_id: source.set_id.clone(),
            manifest_sha256: source.manifest.sha256.clone(),
            descriptor: source.worker_descriptor.clone(),
            image: source.worker.clone(),
        },
        worker_config_sha256: initial.intent.context.worker_config.sha256.clone(),
        observed_at_ms: 204,
    };
    values.push(values.last().unwrap().ready(ready.clone(), 204, r).unwrap());
    values.push(values.last().unwrap().issue_handoff(205, r).unwrap());
    let c = &initial.intent.context;
    let owned = ActivationStateOwned {
        attempt_id: initial.intent.attempt_id.clone(),
        process: process(12),
        context: c.worker_context.clone(),
        generation: uuid::Uuid::new_v4().to_string(),
        state: c.worker_state.clone(),
        observed_at_ms: 206,
    };
    values.push(
        values
            .last()
            .unwrap()
            .state_owned(owned.clone(), 206, r)
            .unwrap(),
    );
    let local = WorkerRuntimeCompatibility {
        schema_version: 1,
        kind: "worker_runtime_compatibility".into(),
        identity: ready.identity,
        context: c.worker_context.clone(),
        generation: owned.generation,
        worker_config_sha256: c.worker_config.sha256.clone(),
        worker_config_schema: c.worker_config_schema,
        worker_journal_schema: c.worker_journal_schema,
        updater_journal_schema: c.updater_journal_schema,
        protocol_version: c.protocol_version,
        observed_at_ms: 207,
    };
    values.push(
        values
            .last()
            .unwrap()
            .local_verified(local, 207, r)
            .unwrap(),
    );
    values
}
#[test]
fn startup_history_is_one_frozen_causal_prefix_through_local_verification() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    assert_eq!(all.len(), 8);
    for (index, value) in all.iter().enumerate() {
        value.validate(&r).unwrap();
        assert_eq!(value.revision, index as u64);
        assert_eq!(value.intent, initial.intent);
    }
    for pair in all.windows(2) {
        pair[0].advance(&pair[1], &r).unwrap();
    }
    assert!(matches!(all[7].state, ActivationState::LocalVerified {}));
    assert!(all[7].issue_create(208, &r).is_err());
    assert!(all[7]
        .indeterminate("Late observer error".into(), 208, &r)
        .is_err());
}
#[test]
fn pid_resume_ready_and_state_handoff_cannot_be_skipped_or_replayed() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    assert!(initial.created(created(&initial, 201), 201, &r).is_err());
    assert!(all[1].grant_resume(203, &r).is_err());
    assert!(all[2]
        .ready(all[4].ready.clone().unwrap(), 204, &r)
        .is_err());
    assert!(all[3].issue_handoff(205, &r).is_err());
    assert!(all[4]
        .state_owned(all[6].state_owned.clone().unwrap(), 206, &r)
        .is_err());
    assert!(all[5]
        .local_verified(all[7].local.clone().unwrap(), 207, &r)
        .is_err());
    for value in all {
        assert!(value.advance(&value, &r).is_err());
    }
}
#[test]
fn creation_rejects_other_host_job_parent_and_old_worker() {
    let (r, initial) = fixture();
    let issue = initial.issue_create(201, &r).unwrap();
    for n in 0..6 {
        let mut v = created(&initial, 202);
        match n {
            0 => v.process = initial.intent.context.previous_worker.clone(),
            1 => v.process = initial.intent.context.supervisor.clone(),
            2 => v.process.host_id = "d".repeat(64),
            3 => v.process.session_id += 1,
            4 => v.job_name = "foreign-job".into(),
            _ => {
                v.process.process_id = initial.intent.context.supervisor.process_id;
                v.process.birth = "999".into();
            }
        }
        assert!(issue.created(v, 202, &r).is_err());
    }
}
#[test]
fn ready_is_bound_to_created_process_manifest_set_and_config() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    for n in 0..6 {
        let mut ready = all[4].ready.clone().unwrap();
        match n {
            0 => ready.attempt_id = uuid::Uuid::new_v4().to_string(),
            1 => ready.identity.process = process(13),
            2 => ready.identity.set_id = uuid::Uuid::new_v4().to_string(),
            3 => ready.identity.image.file_identity = "foreign".into(),
            4 => ready.worker_config_sha256 = "0".repeat(64),
            _ => ready.observed_at_ms = 202,
        }
        assert!(all[3].ready(ready, 204, &r).is_err());
    }
}
#[test]
fn state_receipt_cannot_rebind_database_incarnation_lock_or_generation() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    for n in 0..6 {
        let mut v = all[6].state_owned.clone().unwrap();
        match n {
            0 => v.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
            1 => v.generation = initial.intent.context.previous_generation.clone(),
            2 => v.state.journal_identity = "foreign-journal".into(),
            3 => v.state.lock_identity = "foreign-lock".into(),
            4 => v.process = process(13),
            _ => v.observed_at_ms = 204,
        }
        assert!(all[5].state_owned(v, 206, &r).is_err());
    }
}
#[test]
fn local_health_compares_frozen_schemas_and_the_same_loaded_worker() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    for n in 0..6 {
        let mut v = all[7].local.clone().unwrap();
        match n {
            0 => v.worker_journal_schema += 1,
            1 => v.updater_journal_schema += 1,
            2 => v.protocol_version += 1,
            3 => v.generation = uuid::Uuid::new_v4().to_string(),
            4 => v.identity.image.sha256 = "0".repeat(64),
            _ => v.observed_at_ms = 205,
        }
        assert!(all[6].local_verified(v, 207, &r).is_err());
    }
    // Data can preserve a later protocol's frozen facts without enabling it now.
    let mut future = initial.clone();
    future.intent.context.protocol_version = 2;
    assert!(stages(&future, &r).last().unwrap().validate(&r).is_ok());
}
#[test]
fn unknown_creation_or_handoff_retains_evidence_without_a_new_grant() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    for n in [1, 2, 3, 5, 6] {
        let unknown = all[n]
            .indeterminate("Outcome not confirmed".into(), 208, &r)
            .unwrap();
        all[n].advance(&unknown, &r).unwrap();
        assert_eq!(unknown.created, all[n].created);
        assert!(unknown.issue_create(209, &r).is_err());
        assert!(unknown.grant_resume(209, &r).is_err());
        assert!(unknown.issue_handoff(209, &r).is_err());
        assert!(unknown.indeterminate("Again".into(), 209, &r).is_err());
    }
    let failed = all[1]
        .creation_failed("CreateProcess reported no process".into(), 202, &r)
        .unwrap();
    assert!(failed.created.is_none());
    assert!(failed.issue_create(203, &r).is_err());
    assert!(all[2]
        .creation_failed("Incorrect no-PID claim".into(), 203, &r)
        .is_err());
}
#[test]
fn late_observations_are_retained_but_deadline_forbids_resume_and_handoff() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    let late = initial.intent.admitted_at_ms + initial.intent.context.startup_timeout_ms + 1;
    assert!(initial.issue_create(late, &r).is_err());
    let observed = all[1].created(created(&initial, late), late, &r).unwrap();
    assert!(observed.grant_resume(late, &r).is_err());
    let unknown = observed
        .indeterminate("Startup timed out after creation".into(), late, &r)
        .unwrap();
    assert!(unknown.created.is_some());
    let mut ready = all[4].ready.clone().unwrap();
    ready.observed_at_ms = late;
    let observed = all[3].ready(ready, late, &r).unwrap();
    assert!(observed.issue_handoff(late, &r).is_err());
}
#[test]
fn transition_cannot_rewrite_nonce_or_previous_grants_and_receipts() {
    let (r, initial) = fixture();
    let all = stages(&initial, &r);
    let mut changed = all[4].clone();
    changed.intent.context.channel_nonce_sha256 = "0".repeat(64);
    changed.created.as_mut().unwrap().job_name = changed.intent.job_name().unwrap();
    changed.validate(&r).unwrap();
    assert!(all[3].advance(&changed, &r).is_err());
    let mut changed = all[6].clone();
    changed.ready.as_mut().unwrap().observed_at_ms = 205;
    changed.validate(&r).unwrap();
    assert!(all[5].advance(&changed, &r).is_err());
    let mut changed = all[5].clone();
    changed.created = None;
    assert!(changed.validate(&r).is_err());
}
#[test]
fn invalid_deadline_context_and_fabricated_authority_are_rejected() {
    let (r, initial) = fixture();
    for n in 0..6 {
        let mut v = initial.clone();
        match n {
            0 => v.intent.context.startup_timeout_ms = 0,
            1 => v.intent.context.worker_journal_schema = 0,
            2 => v.intent.context.channel_nonce_sha256.clear(),
            3 => v.intent.admitted_at_ms = MAX_UPDATE_COUNTER,
            4 => v.intent.context.worker_config.query_supported = true,
            _ => {
                v.intent.context.worker_config.file_identity =
                    v.intent.context.worker_state.lock_identity.clone()
            }
        }
        assert!(v.validate(&r).is_err());
    }
    let mut value = serde_json::to_value(&initial).unwrap();
    value["permit"] = true.into();
    assert!(serde_json::from_value::<ActivationRecord>(value).is_err());
}
