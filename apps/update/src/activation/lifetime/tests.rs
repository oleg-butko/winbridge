use super::*;
use serde_json::json;

fn observed_fixture() -> (
    ActivationControllerSnapshot,
    ActivationControllerObservation,
) {
    let f = controller::test_fixture();
    let session = &f.receipt.session;
    let link = winbridge_protocol::ControllerLinkObservation {
        id: session.controller_id.clone(),
        attempts: session.connection_attempt.to_string(),
        connection: winbridge_protocol::ControllerConnection {
            url: session.controller_url.clone(),
            state: winbridge_protocol::ConnectionState::Connected,
            session_id: Some(session.session_id.clone()),
            controller_node_id: Some(session.controller_node_id.clone()),
            controller_generation: Some(session.controller_generation.clone()),
            last_confirmed_at_ms: Some(session.connected_at_ms),
            error: None,
        },
        last_connected_at_ms: Some(session.connected_at_ms),
        last_disconnected_at_ms: None,
        last_error: None,
        rpc: winbridge_protocol::NetworkRpcObservation {
            in_flight: 0,
            limit: winbridge_protocol::CONTROLLER_RPC_LIMIT,
            admitted: "0".into(),
            completed: "0".into(),
            rejected: "0".into(),
        },
    };
    let observed = ActivationControllerObservation {
        controller_observation_version: 1,
        result: ActivationControllerObservationResult::Observed {
            receipt: Box::new(f.receipt.clone()),
            projection: Box::new(f.projection),
            link: Box::new(link),
        },
    };
    let at = f.receipt.compatibility.observed_at_ms;
    let controller = controller::ActivationControllerRecord::issued(f.command)
        .unwrap()
        .confirmed(f.receipt, at)
        .unwrap();
    let snapshot = ActivationControllerSnapshot {
        physical: f.physical,
        writer: f.writer,
        service: f.service,
        local: f.local,
        controller,
    };
    (snapshot, observed)
}
pub(crate) fn fixture() -> (
    ActivationControllerSnapshot,
    WorkerLifetimeActivationCommand,
    WorkerLifetimeActivationReceipt,
) {
    let (snapshot, _) = observed_fixture();
    let at = snapshot.controller.updated_at_ms;
    let command = prepare_command(&snapshot, &uuid::Uuid::new_v4().to_string(), at + 1).unwrap();
    let mut compatibility = command.binding.compatibility.clone();
    compatibility.observed_at_ms = command.issued_at_ms + 1;
    let receipt = WorkerLifetimeActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: command.binding.update_id.clone(),
        attempt_id: command.binding.attempt_id.clone(),
        command_sha256: command_sha256(&command).unwrap(),
        keeper: command.binding.keeper.clone(),
        job_name: command.binding.job_name.clone(),
        compatibility,
        authority: command.binding.authority.clone(),
        mode: command.mode,
    };
    (snapshot, command, receipt)
}

#[test]
fn fresh_lifetime_prefix_requires_exact_saved_observation_and_pinned_topology() {
    let (snapshot, observed) = observed_fixture();
    let topology = &snapshot.controller.command.binding.topology;
    validate_controller_prefix(&snapshot, topology, &observed).unwrap();
    for mutation in 0..9 {
        let mut changed = observed.clone();
        let mut t = topology.clone();
        let mut s = snapshot.clone();
        match mutation {
            0 => changed.controller_observation_version = 2,
            1 => {
                changed.result = ActivationControllerObservationResult::Waiting {
                    message: "No accepted frame".into(),
                }
            }
            2 => {
                changed.result = ActivationControllerObservationResult::Rejected {
                    message: "Stale scope".into(),
                }
            }
            3 => t.primary = "unconfigured".into(),
            4 => t.confirmation_endpoint_id = "peer1".into(),
            5 => {
                s.controller =
                    controller::ActivationControllerRecord::issued(s.controller.command.clone())
                        .unwrap()
            }
            _ => {
                let ActivationControllerObservationResult::Observed {
                    receipt,
                    projection,
                    link,
                } = &mut changed.result
                else {
                    unreachable!()
                };
                match mutation {
                    6 => receipt.compatibility.observed_at_ms += 1,
                    7 => projection.sequence += 1,
                    _ => link.attempts = "999".into(),
                }
            }
        }
        assert!(
            validate_controller_prefix(&s, &t, &changed).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn proposed_v6_frames_are_strict_and_never_accept_v5_or_rebound_lifetime() {
    use crate::activation::channel;
    let (_, command, receipt) = fixture();
    assert_eq!(
        activation_lifetime_marker_pattern(),
        ACTIVATION_LIFETIME_MARKER
    );
    assert_ne!(ACTIVATION_LIFETIME_MARKER, ACTIVATION_CONTROLLER_MARKER);
    let frame = channel::LifetimeCommand {
        contract_version: channel::LIFETIME_CONTRACT,
        command: command.clone(),
    };
    let ready = channel::LifetimeReady {
        contract_version: channel::LIFETIME_CONTRACT,
        receipt,
    };
    channel::validate_lifetime_command(&frame, &command).unwrap();
    channel::validate_lifetime_ready(&ready, &command).unwrap();
    for version in [0, 1, 2, 3, 4, 5, 7] {
        let mut wrong = frame.clone();
        wrong.contract_version = version;
        assert!(channel::validate_lifetime_command(&wrong, &command).is_err());
        let mut wrong = ready.clone();
        wrong.contract_version = version;
        assert!(channel::validate_lifetime_ready(&wrong, &command).is_err());
    }
    let mut wrong = frame.clone();
    wrong.command.binding.root_revision += 1;
    assert!(channel::validate_lifetime_command(&wrong, &command).is_err());
    let mut wrong = ready.clone();
    wrong.receipt.authority.epoch += 1;
    assert!(channel::validate_lifetime_ready(&wrong, &command).is_err());
    let mut raw = serde_json::to_value(frame).unwrap();
    raw["canCommit"] = true.into();
    assert!(serde_json::from_value::<channel::LifetimeCommand>(raw).is_err());
    let mut raw = serde_json::to_value(ready).unwrap();
    raw["keeperHandle"] = true.into();
    assert!(serde_json::from_value::<channel::LifetimeReady>(raw).is_err());
}

#[test]
fn flat_lifetime_proposal_preserves_original_child_keeper_and_window() {
    let (snapshot, command, receipt) = fixture();
    validate_command(&command, &snapshot).unwrap();
    validate_receipt(&receipt, &command).unwrap();
    assert_eq!(
        command.deadline_at_ms,
        snapshot.local.intent.admitted_at_ms + snapshot.local.intent.context.startup_timeout_ms
    );
    assert_eq!(
        command.binding.keeper,
        snapshot.local.intent.context.supervisor
    );
    assert_eq!(
        command.binding.job_name,
        snapshot.local.intent.job_name().unwrap()
    );
    assert_eq!(
        command.mode,
        WorkerLifetimeActivationMode::SupervisedQuarantine
    );
    let raw = serde_json::to_value(&command).unwrap();
    assert!(!raw["binding"]
        .as_object()
        .unwrap()
        .contains_key("serviceCommand"));
    assert!(!raw["binding"]
        .as_object()
        .unwrap()
        .contains_key("controllerCommand"));
    assert!(serde_json::to_vec(&command).unwrap().len() < 32768);
    let decoded: WorkerLifetimeActivationCommand = serde_json::from_value(raw).unwrap();
    assert_eq!(decoded, command);
}

#[test]
fn controller_uncertainty_or_expired_original_window_cannot_propose_lifetime() {
    let (mut snapshot, command, _) = fixture();
    assert!(prepare_command(
        &snapshot,
        &uuid::Uuid::new_v4().to_string(),
        command.deadline_at_ms
    )
    .is_err());
    let confirmed = snapshot.controller.clone();
    snapshot.controller =
        controller::ActivationControllerRecord::issued(confirmed.command.clone()).unwrap();
    assert!(prepare_command(&snapshot, &command.command_id, command.issued_at_ms).is_err());
    snapshot.controller = snapshot
        .controller
        .indeterminate("ACK unknown".into(), command.issued_at_ms)
        .unwrap();
    assert!(prepare_command(&snapshot, &command.command_id, command.issued_at_ms).is_err());
    snapshot.controller = confirmed;
    for key in [
        &snapshot.service.command.command_id,
        &snapshot.writer.command.command_id,
        &snapshot.controller.command.command_id,
    ] {
        assert!(prepare_command(&snapshot, key, command.issued_at_ms).is_err());
    }
}

#[test]
fn flat_scope_cannot_be_rebound_to_changed_graph_or_keeper() {
    let (snapshot, command, _) = fixture();
    for index in 0..7 {
        let mut bad = snapshot.clone();
        match index {
            0 => bad.physical.intent.context.supervisor.birth = "1".into(),
            1 => bad.writer.command.binding.updater_epoch += 1,
            2 => bad.service.command.deadline_at_ms += 1,
            3 => bad.local.intent.source.owner_epoch += 1,
            4 => bad.local.revision = 6,
            5 => bad.local.local = None,
            _ => bad.controller.updated_at_ms = command.issued_at_ms + 1,
        }
        assert!(
            validate_command(&command, &bad).is_err(),
            "Graph mutation {index}"
        );
    }
    let values = [
        ("/binding/updateId", json!(uuid::Uuid::new_v4().to_string())),
        (
            "/binding/attemptId",
            json!(uuid::Uuid::new_v4().to_string()),
        ),
        ("/binding/launchId", json!(uuid::Uuid::new_v4().to_string())),
        (
            "/binding/updaterOwnerId",
            json!(uuid::Uuid::new_v4().to_string()),
        ),
        ("/binding/updaterEpoch", json!(0)),
        ("/binding/rootRevision", json!(0)),
        ("/binding/controllerRecordSha256", json!("0".repeat(64))),
        ("/binding/localRecordSha256", json!("0".repeat(64))),
        ("/binding/keeper/processId", json!(999999)),
        ("/binding/keeper/birth", json!("1")),
        ("/binding/jobName", json!("Global\\WinBridge-Domain-forged")),
        (
            "/binding/compatibility/generation",
            json!(uuid::Uuid::new_v4().to_string()),
        ),
        (
            "/binding/compatibility/workerConfigSha256",
            json!("0".repeat(64)),
        ),
        ("/binding/compatibility/updaterJournalSchema", json!(999)),
        (
            "/binding/authority/fingerprintSha256",
            json!("0".repeat(64)),
        ),
        ("/deadlineAtMs", json!(command.deadline_at_ms + 1)),
    ];
    for (path, value) in values {
        let mut raw = serde_json::to_value(&command).unwrap();
        assert_ne!(*raw.pointer(path).unwrap(), value, "No-op mutation {path}");
        *raw.pointer_mut(path).unwrap() = value;
        let bad: WorkerLifetimeActivationCommand = serde_json::from_value(raw).unwrap();
        assert!(
            validate_command(&bad, &snapshot).is_err(),
            "Binding mutation {path}"
        );
    }
}

#[test]
fn receipt_requires_same_authority_process_job_and_entry_inside_original_window() {
    let (_, command, receipt) = fixture();
    for (path, value) in [
        ("/commandSha256", json!("0".repeat(64))),
        ("/keeper/processId", json!(999999)),
        ("/jobName", json!("forged")),
        ("/authority/epoch", json!(0)),
        ("/compatibility/context/instanceId", json!("another")),
        ("/compatibility/identity/process/birth", json!("1")),
        ("/compatibility/observedAtMs", json!(command.deadline_at_ms)),
    ] {
        let mut raw = serde_json::to_value(&receipt).unwrap();
        assert_ne!(*raw.pointer(path).unwrap(), value, "No-op mutation {path}");
        *raw.pointer_mut(path).unwrap() = value;
        let bad = serde_json::from_value(raw).unwrap();
        assert!(
            validate_receipt(&bad, &command).is_err(),
            "Receipt mutation {path}"
        );
    }
}

#[test]
fn late_saved_confirmation_after_unknown_ack_is_only_a_causal_fact() {
    let (_, command, receipt) = fixture();
    let at = receipt.compatibility.observed_at_ms;
    let issued = ActivationLifetimeRecord::issued(command.clone()).unwrap();
    let uncertain = issued.indeterminate("ACK unknown".into(), at).unwrap();
    issued.advance(&uncertain).unwrap();
    let confirmed = uncertain
        .confirmed(receipt.clone(), command.deadline_at_ms + 1)
        .unwrap();
    uncertain.advance(&confirmed).unwrap();
    assert_eq!(confirmed.revision, 2);
    assert_eq!(confirmed.command, command);
    assert!(confirmed
        .confirmed(receipt, confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed
        .indeterminate("retry".into(), confirmed.updated_at_ms + 1)
        .is_err());
    assert!(issued.advance(&confirmed).is_err());
    let mut forged = confirmed.clone();
    forged.command.deadline_at_ms += 1;
    assert!(uncertain.advance(&forged).is_err());
    let raw = serde_json::to_value(&confirmed).unwrap();
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<ActivationLifetimeRecord>(raw.clone()).unwrap()
        )
        .unwrap(),
        raw
    );
}

#[test]
fn lifetime_dtos_reject_unknown_modes_fields_and_boolean_counters() {
    let (_, command, receipt) = fixture();
    let raw = serde_json::to_value(&command).unwrap();
    for field in [
        "nonce",
        "canLaunch",
        "canCommit",
        "releaseMaintenance",
        "ownerHandle",
    ] {
        let mut bad = raw.clone();
        bad[field] = json!(true);
        assert!(serde_json::from_value::<WorkerLifetimeActivationCommand>(bad).is_err());
    }
    let mut bad = raw.clone();
    bad["mode"] = json!("normal");
    assert!(serde_json::from_value::<WorkerLifetimeActivationCommand>(bad).is_err());
    let mut bad = raw;
    bad["binding"]["updaterEpoch"] = json!(true);
    assert!(serde_json::from_value::<WorkerLifetimeActivationCommand>(bad).is_err());
    let mut bad = serde_json::to_value(receipt).unwrap();
    bad["mode"] = json!("unmanaged");
    assert!(serde_json::from_value::<WorkerLifetimeActivationReceipt>(bad).is_err());
}
