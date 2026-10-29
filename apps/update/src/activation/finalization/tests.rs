//! Pure synthetic data only: none of these constructors grants live custody.
use super::*;
use crate::activation::{controller, service, writer, ActivationWriterRecord};
use serde_json::json;

pub(crate) struct Fixture {
    pub snapshot: ActivationLifetimeSnapshot,
    pub root: UpdateRecord,
    pub shutdown: crate::shutdown::ShutdownRecord,
    pub query: ActivationResidentQuery,
    pub observed: ActivationResidentObservation,
    pub command: WorkerFinalizationCommand,
    pub receipt: WorkerFinalizationReceipt,
}
pub(crate) fn fixture() -> Fixture {
    let (mut query, topology, mut entry, mut observed, mut graph) =
        resident_observation::test_fixture();
    let (mut intent, mut shutdown, mut exit) = crate::shutdown::tests::fixture();
    let source = &graph.physical.intent.source;
    let c = &graph.physical.intent.context;
    shutdown.update_id = source.update_id.clone();
    shutdown.launch_id = source.launch_id.clone();
    shutdown.updater_owner_id = source.owner_id.clone();
    shutdown.updater_epoch = source.owner_epoch;
    shutdown.process = c.previous_worker.clone();
    shutdown.state = c.worker_state.clone();
    shutdown.lease.spec.update_id = source.update_id.clone();
    shutdown.lease.spec.updater_owner_id = source.owner_id.clone();
    shutdown.lease.spec.updater_epoch = source.owner_epoch;
    shutdown.lease.spec.instance_id = c.worker_context.instance_id.clone();
    shutdown.lease.spec.journal_incarnation = c.worker_context.journal_incarnation.clone();
    shutdown.lease.spec.worker_generation = c.previous_generation.clone();
    shutdown.lease.spec.target_manifest_sha256 = query
        .command
        .binding
        .compatibility
        .identity
        .descriptor
        .build
        .source_manifest_sha256
        .clone()
        .unwrap();
    intent.binding.update_id = source.update_id.clone();
    intent.binding.launch_id = source.launch_id.clone();
    intent.binding.owner_id = source.owner_id.clone();
    intent.binding.owner_epoch = source.owner_epoch;
    intent.binding.worker.process = c.previous_worker.clone();
    intent.binding.worker.context = c.worker_context.clone();
    intent.binding.worker.generation = c.previous_generation.clone();
    intent.binding.loaded_worker.process = c.previous_worker.clone();
    intent.binding.worker_state = c.worker_state.clone();
    intent.binding.lease = shutdown.lease.clone();
    intent =
        crate::shutdown::ShutdownIntent::from_quiesced(intent.binding, intent.attempt_id).unwrap();
    shutdown.command = intent.command.clone();
    shutdown.accepted_at_ms = 150;
    exit.process = shutdown.process.clone();
    exit.state = shutdown.state.clone();
    exit.observed_at_ms = 190;
    let original_lease = shutdown.lease.clone();
    let shutdown = crate::shutdown::ShutdownRecord::prepare(intent)
        .unwrap()
        .issue(101)
        .unwrap()
        .acknowledge(151, shutdown)
        .unwrap()
        .confirm_exit(190, exit)
        .unwrap();
    let shutdown_sha = crate::runtime_baseline::digest(&shutdown).unwrap();
    graph.physical.intent.source.shutdown_sha256 = shutdown_sha.clone();
    graph.local.intent.source.shutdown_sha256 = shutdown_sha;
    let job = graph.physical.intent.job_name().unwrap();
    graph.physical.created.as_mut().unwrap().job_name = job.clone();
    graph.local.created.as_mut().unwrap().job_name = job;
    // Rebuild every affected causal hash from the changed source, never patch
    // the finalization digest to make an incoherent graph look acceptable.
    let w = writer::prepare_command(
        &graph.physical,
        &graph.writer.command.binding.expected_authority,
        &graph.writer.command.command_id,
        graph.writer.command.issued_at_ms,
    )
    .unwrap();
    let crate::activation::ActivationWriterState::Confirmed { mut receipt } =
        graph.writer.state.clone()
    else {
        unreachable!()
    };
    receipt.command_sha256 = writer::command_sha256(&w).unwrap();
    graph.writer = ActivationWriterRecord::issued(w)
        .unwrap()
        .confirmed(receipt, graph.writer.updated_at_ms)
        .unwrap();
    let s = service::prepare_command(
        &graph.physical,
        &graph.writer,
        &graph.service.command.command_id,
        graph.service.command.issued_at_ms,
    )
    .unwrap();
    let service::ActivationServiceState::Confirmed { mut receipt } = graph.service.state.clone()
    else {
        unreachable!()
    };
    receipt.command_sha256 = service::command_sha256(&s).unwrap();
    graph.service = service::ActivationServiceRecord::issued(s)
        .unwrap()
        .confirmed(*receipt, graph.service.updated_at_ms)
        .unwrap();
    let c = controller::prepare_command(
        &graph.physical,
        &graph.writer,
        &graph.service,
        &graph.local,
        topology,
        &graph.controller.command.command_id,
        graph.controller.command.issued_at_ms,
    )
    .unwrap();
    let controller::ActivationControllerState::Confirmed { mut receipt } =
        graph.controller.state.clone()
    else {
        unreachable!()
    };
    receipt.command_sha256 = controller::command_sha256(&c).unwrap();
    graph.controller = controller::ActivationControllerRecord::issued(c)
        .unwrap()
        .confirmed(*receipt, graph.controller.updated_at_ms)
        .unwrap();
    let l = lifetime::prepare_command(
        &graph,
        &query.command.command_id,
        query.command.issued_at_ms,
    )
    .unwrap();
    entry.command_sha256 = lifetime::command_sha256(&l).unwrap();
    entry.job_name = l.binding.job_name.clone();
    query.command = l.clone();
    let ActivationResidentObservationResult::Observed {
        entry: observed_entry,
        observed_at_ms,
        ..
    } = &mut observed.result
    else {
        unreachable!()
    };
    **observed_entry = entry.clone();
    let at = *observed_at_ms + 1;
    let lifetime = lifetime::ActivationLifetimeRecord::issued(l)
        .unwrap()
        .confirmed(entry.clone(), entry.compatibility.observed_at_ms)
        .unwrap();
    let snapshot = ActivationLifetimeSnapshot {
        controller: graph,
        lifetime,
    };
    let source = &snapshot.controller.local.intent.source;
    let root = UpdateRecord {
        update_id: source.update_id.clone(),
        revision: query.command.binding.root_revision,
        phase: UpdatePhase::AwaitingController,
        request: UpdateRequest {
            contract_version: 1,
            idempotency_key: "finalization-metadata".into(),
            source: UpdateSourcePin {
                archive_name: "source.zip".into(),
                archive_sha256: "a".repeat(64),
                manifest_sha256: original_lease.spec.target_manifest_sha256.clone(),
                source_commit: query
                    .command
                    .binding
                    .compatibility
                    .identity
                    .descriptor
                    .build
                    .source_commit
                    .clone()
                    .unwrap(),
            },
            expected: UpdateInstallationPin {
                installation_id: source.installation_id.clone(),
                scope_sha256: source.scope_sha256.clone(),
                worker_build_id: shutdown.intent.binding.worker.build.build_id.clone(),
                worker_config_sha256: query
                    .command
                    .binding
                    .compatibility
                    .worker_config_sha256
                    .clone(),
                baseline_manifest_sha256: original_lease.spec.baseline_manifest_sha256.clone(),
            },
        },
        accepted_at_ms: 100,
        updated_at_ms: snapshot.controller.local.updated_at_ms,
        reason: None,
    };
    let command = prepare_command(
        &snapshot,
        &root,
        &shutdown,
        &query,
        &observed,
        &uuid::Uuid::new_v4().to_string(),
        at,
    )
    .unwrap();
    let mut compatibility = query.command.binding.compatibility.clone();
    compatibility.observed_at_ms = at + 2;
    let mut lease = original_lease;
    lease.phase = WorkerUpdatePhase::Released;
    lease.revision += 1;
    lease.updated_at_ms = at + 1;
    lease.reason = RELEASE_REASON.into();
    let receipt = WorkerFinalizationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        command_sha256: command_sha256(&command).unwrap(),
        compatibility,
        authority: query.command.binding.authority.clone(),
        released_lease: lease,
        mode: WorkerFinalizationMode::Active,
        released_at_ms: at + 1,
    };
    validate_receipt(&receipt, &command, &shutdown).unwrap();
    Fixture {
        snapshot,
        root,
        shutdown,
        query,
        observed,
        command,
        receipt,
    }
}

#[test]
fn finalization_binds_current_connection_after_expiry_without_renewing_v6() {
    let f = fixture();
    validate_command(
        &f.command,
        &f.snapshot,
        &f.root,
        &f.shutdown,
        &f.query,
        &f.observed,
    )
    .unwrap();
    assert!(f.command.issued_at_ms > f.command.binding.lifetime_command.deadline_at_ms);
    assert_eq!(f.command.binding.lifetime_command, f.query.command);
    assert_eq!(
        f.command.binding.committed_root_revision,
        f.root.revision + 1
    );
    let release = release_command(&f.command).unwrap();
    assert_eq!(release.action, WorkerUpdateAction::Release);
    assert_eq!(
        release.expected_revision,
        f.shutdown.intent.binding.lease.revision
    );
    assert_eq!(
        release.source_manifest_sha256,
        Some(f.root.request.source.manifest_sha256)
    );
    assert!(serde_json::to_vec(&f.command).unwrap().len() < 8 * 1024);
    assert!(serde_json::to_vec(&f.receipt).unwrap().len() < 8 * 1024);
}

#[test]
fn finalization_cannot_use_stale_future_unconfirmed_or_unrelated_evidence() {
    let f = fixture();
    for mutation in 0..25 {
        let mut snapshot = f.snapshot.clone();
        let mut root = f.root.clone();
        let mut shutdown = f.shutdown.clone();
        let mut query = f.query.clone();
        let mut observed = f.observed.clone();
        let mut at = f.command.issued_at_ms;
        match mutation {
            0 => root.phase = UpdatePhase::Committed,
            1 => root.revision += 1,
            2 => root.update_id = uuid::Uuid::new_v4().to_string(),
            3 => root.request.expected.installation_id = uuid::Uuid::new_v4().to_string(),
            4 => root.request.expected.scope_sha256 = "0".repeat(64),
            5 => root.request.source.manifest_sha256 = "0".repeat(64),
            6 => root.request.expected.worker_config_sha256 = "0".repeat(64),
            7 => {
                snapshot.lifetime =
                    lifetime::ActivationLifetimeRecord::issued(snapshot.lifetime.command.clone())
                        .unwrap()
            }
            8 => query.command.command_id = uuid::Uuid::new_v4().to_string(),
            9 => observed.request_id = Some(uuid::Uuid::new_v4().to_string()),
            10 => at += MAX_OBSERVATION_AGE_MS,
            11 => at = 0,
            12 => shutdown.intent.binding.worker.process.birth.push('0'),
            13 => shutdown.intent.binding.lease.phase = WorkerUpdatePhase::Released,
            14 => shutdown.intent.binding.owner_epoch += 1,
            15 => {
                shutdown.intent.binding.lease.spec.worker_generation =
                    uuid::Uuid::new_v4().to_string()
            }
            16 => shutdown
                .intent
                .binding
                .worker_state
                .journal_identity
                .push('x'),
            17 => shutdown
                .intent
                .binding
                .lease
                .spec
                .projects
                .push("another-project".into()),
            18 => snapshot.controller.controller.updated_at_ms += 1,
            19 => {
                snapshot
                    .controller
                    .local
                    .intent
                    .context
                    .worker_config
                    .sha256 = "0".repeat(64)
            }
            20 => {
                let ActivationResidentObservationResult::Observed { observed_at_ms, .. } =
                    &mut observed.result
                else {
                    unreachable!()
                };
                *observed_at_ms = at + 1;
            }
            22 => root.request.expected.worker_build_id.push('x'),
            23 => root.request.source.source_commit = "0".repeat(40),
            24 => root.accepted_at_ms = root.updated_at_ms + 1,
            _ => {
                let ActivationResidentObservationResult::Observed { frame, .. } =
                    &mut observed.result
                else {
                    unreachable!()
                };
                frame.sequence += 1;
            }
        }
        // A changed frame may still form a NEW valid proposal, but cannot pass
        // validation as the original frozen decision with its previous digest.
        let mut command = f.command.clone();
        command.issued_at_ms = at;
        assert!(
            validate_command(&command, &snapshot, &root, &shutdown, &query, &observed).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn finalization_release_scope_and_active_fact_are_independently_checked() {
    let f = fixture();
    for mutation in 0..18 {
        let mut r = f.receipt.clone();
        match mutation {
            0 => r.schema_version += 1,
            1 => r.command_id = uuid::Uuid::new_v4().to_string(),
            2 => r.command_sha256 = "0".repeat(64),
            3 => r.compatibility.identity.process.birth.push('0'),
            4 => r.compatibility.generation = uuid::Uuid::new_v4().to_string(),
            5 => r.authority.epoch += 1,
            6 => r.released_lease.phase = WorkerUpdatePhase::Quiesced,
            7 => r.released_lease.revision += 1,
            8 => r.released_lease.spec.projects.push("foreign".into()),
            9 => r.released_lease.accepted_at_ms += 1,
            10 => r.released_lease.spec.source_directory.push('x'),
            11 => r.released_lease.spec.target_manifest_sha256 = "0".repeat(64),
            12 => r.released_at_ms = f.command.issued_at_ms - 1,
            13 => r.compatibility.observed_at_ms = r.released_at_ms - 1,
            14 => r.released_lease.reason = "Unrelated ordinary release".into(),
            15 => r.released_lease.updated_at_ms += 1,
            16 => r.compatibility.worker_config_sha256 = "0".repeat(64),
            _ => r.compatibility.observed_at_ms = MAX_UPDATE_COUNTER + 1,
        }
        assert!(
            validate_receipt(&r, &f.command, &f.shutdown).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn lost_finalization_ack_stays_blocked_until_exact_independent_active_fact() {
    let f = fixture();
    let pending = ActivationFinalizationRecord::pending(f.command).unwrap();
    let unknown = pending
        .indeterminate(
            "Release ACK lost; inspect same key".into(),
            pending.updated_at_ms + 1,
        )
        .unwrap();
    let confirmed = unknown
        .confirmed(
            f.receipt.clone(),
            &f.shutdown,
            f.receipt.compatibility.observed_at_ms,
        )
        .unwrap();
    assert!(pending.blocks_next_update(&f.shutdown).unwrap());
    assert!(unknown.blocks_next_update(&f.shutdown).unwrap());
    assert!(!confirmed.blocks_next_update(&f.shutdown).unwrap());
    pending.advance(&unknown, &f.shutdown).unwrap();
    unknown.advance(&confirmed, &f.shutdown).unwrap();
    assert_eq!(confirmed.revision, 2);
    assert!(confirmed.advance(&pending, &f.shutdown).is_err());
    assert!(unknown.advance(&pending, &f.shutdown).is_err());
    assert!(unknown
        .indeterminate("Retry".into(), unknown.updated_at_ms + 1)
        .is_err());
    assert!(confirmed
        .confirmed(f.receipt, &f.shutdown, confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed
        .indeterminate("Late error".into(), confirmed.updated_at_ms + 1)
        .is_err());
}

#[test]
fn finalization_keys_versions_and_unknown_fields_never_grant_fallback() {
    let f = fixture();
    for key in [
        &f.query.command.command_id,
        &f.query.request_id,
        &f.snapshot.controller.writer.command.command_id,
        &f.snapshot.controller.service.command.command_id,
        &f.shutdown.intent.command.command_id,
    ] {
        assert!(prepare_command(
            &f.snapshot,
            &f.root,
            &f.shutdown,
            &f.query,
            &f.observed,
            key,
            f.command.issued_at_ms
        )
        .is_err());
    }
    let mut raw = serde_json::to_value(&f.command).unwrap();
    raw["canRelease"] = json!(true);
    assert!(serde_json::from_value::<WorkerFinalizationCommand>(raw).is_err());
    let mut raw = serde_json::to_value(&f.receipt).unwrap();
    raw["mode"] = json!("supervised_quarantine");
    assert!(serde_json::from_value::<WorkerFinalizationReceipt>(raw).is_err());
    let mut raw = serde_json::to_value(&f.command).unwrap();
    raw["binding"]["keeperHandle"] = json!(123);
    assert!(serde_json::from_value::<WorkerFinalizationCommand>(raw).is_err());
    for version in [0, 2, 6, 7, 8] {
        let mut wrong = f.command.clone();
        wrong.schema_version = version;
        assert!(validate_command_envelope(&wrong).is_err());
    }
    assert_eq!(PROPOSED_ACTIVATION_FINALIZATION_CONTRACT, 8);
}

#[test]
fn finalization_admission_age_is_not_renewed_by_a_later_proposal_time() {
    let f = fixture();
    let ActivationResidentObservationResult::Observed { observed_at_ms, .. } = f.observed.result
    else {
        unreachable!()
    };
    validate_admission_time(&f.command, &f.observed, f.command.issued_at_ms).unwrap();
    validate_admission_time(
        &f.command,
        &f.observed,
        observed_at_ms + MAX_OBSERVATION_AGE_MS,
    )
    .unwrap();
    assert!(validate_admission_time(
        &f.command,
        &f.observed,
        f.command.issued_at_ms + MAX_OBSERVATION_AGE_MS
    )
    .is_err());
    assert!(validate_admission_time(&f.command, &f.observed, f.command.issued_at_ms - 1).is_err());
}
