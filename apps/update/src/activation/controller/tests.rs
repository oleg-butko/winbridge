use super::*;
use serde_json::json;
use winbridge_protocol::{ControllerReadinessProjection, WorkerRuntimeCompatibility};

pub(crate) struct Fixture {
    pub(crate) physical: ActivationRecord,
    pub(crate) writer: ActivationWriterRecord,
    pub(crate) service: ActivationServiceRecord,
    pub(crate) local: ActivationRecord,
    pub(crate) command: WorkerControllerActivationCommand,
    pub(crate) receipt: WorkerControllerActivationReceipt,
    pub(crate) projection: ControllerReadinessProjection,
}
fn topology(count: usize) -> WorkerControllerActivationTopology {
    WorkerControllerActivationTopology {
        schema_version: 1,
        primary: "peer0".into(),
        confirmation_endpoint_id: "peer0".into(),
        controllers: (0..count)
            .map(|i| WorkerControllerActivationEndpoint {
                id: format!("peer{i}"),
                url: format!("ws://127.0.0.1:{}", 1144 + i),
            })
            .collect(),
    }
}
pub(crate) fn fixture() -> Fixture {
    let (physical, writer, service_command, service_receipt) = service::test_fixture();
    let at = service_receipt.compatibility.observed_at_ms;
    let mut local = physical.clone();
    local.state = ActivationState::LocalVerified {};
    local.revision = 7;
    local.updated_at_ms = at;
    local.local = Some(service_receipt.compatibility.clone());
    let service = ActivationServiceRecord::issued(service_command)
        .unwrap()
        .confirmed(service_receipt, at)
        .unwrap();
    let command = prepare_command(
        &physical,
        &writer,
        &service,
        &local,
        topology(2),
        &uuid::Uuid::new_v4().to_string(),
        at + 1,
    )
    .unwrap();
    let compatibility = WorkerRuntimeCompatibility {
        observed_at_ms: command.issued_at_ms + 3,
        ..command.binding.service_receipt.compatibility.clone()
    };
    let controller_build = BuildIdentity {
        version: "0.2.0".into(),
        source_commit: Some("d".repeat(40)),
        source_dirty: false,
        source_manifest_sha256: None,
        build_id: "controller-build".into(),
    };
    let projection = ControllerReadinessProjection {
        schema_version: 1,
        controller_build: controller_build.clone(),
        controller_generation: uuid::Uuid::new_v4().to_string(),
        session_id: uuid::Uuid::new_v4().to_string(),
        provider_build: compatibility.identity.descriptor.build.clone(),
        provider_generation: compatibility.generation.clone(),
        context: compatibility.context.clone(),
        sequence: 1,
        views: vec![],
        verifications: vec![],
        truncated: false,
    };
    let receipt = WorkerControllerActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: command.binding.service_receipt.update_id.clone(),
        attempt_id: command.binding.service_receipt.attempt_id.clone(),
        command_sha256: command_sha256(&command).unwrap(),
        authority: command.binding.service_receipt.authority.clone(),
        session: WorkerControllerSessionEvidence {
            controller_id: "peer0".into(),
            controller_url: "ws://127.0.0.1:1144".into(),
            controller_node_id: "debian".into(),
            controller_generation: projection.controller_generation.clone(),
            session_id: projection.session_id.clone(),
            controller_build,
            worker_context: compatibility.context.clone(),
            worker_generation: compatibility.generation.clone(),
            worker_build: compatibility.identity.descriptor.build.clone(),
            connection_attempt: 1,
            projection_sequence: 1,
            projection_sha256: projection_sha256(&projection).unwrap(),
            connected_at_ms: command.issued_at_ms + 1,
        },
        compatibility,
    };
    Fixture {
        physical,
        writer,
        service,
        local,
        command,
        receipt,
        projection,
    }
}
#[test]
fn controller_confirmation_preserves_same_child_authority_config_and_selected_session() {
    let f = fixture();
    validate_command(
        &f.command,
        &f.physical,
        &f.writer,
        &f.service,
        &f.local,
        &topology(2),
    )
    .unwrap();
    validate_projection_evidence(&f.receipt, &f.command, &f.projection).unwrap();
    let issued = ActivationControllerRecord::issued(f.command).unwrap();
    let confirmed = issued
        .confirmed(f.receipt.clone(), f.receipt.compatibility.observed_at_ms)
        .unwrap();
    issued.advance(&confirmed).unwrap();
    assert_eq!(confirmed.revision, 1);
    assert!(confirmed
        .confirmed(f.receipt.clone(), confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed
        .indeterminate("repeat".into(), confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed.advance(&issued).is_err());
}

#[test]
fn private_controller_v5_is_strict_and_never_accepts_local_only_or_rebound_messages() {
    let f = fixture();
    let command = super::super::channel::ControllerCommand {
        contract_version: super::super::channel::CONTROLLER_CONTRACT,
        command: f.command.clone(),
    };
    let ready = super::super::channel::ControllerReady {
        contract_version: super::super::channel::CONTROLLER_CONTRACT,
        receipt: f.receipt,
    };
    super::super::channel::validate_controller_command(&command, &f.command).unwrap();
    super::super::channel::validate_controller_ready(&ready, &f.command).unwrap();
    for version in [0, 1, 2, 3, 4, 6] {
        let mut bad = command.clone();
        bad.contract_version = version;
        assert!(super::super::channel::validate_controller_command(&bad, &f.command).is_err());
        let mut bad = ready.clone();
        bad.contract_version = version;
        assert!(super::super::channel::validate_controller_ready(&bad, &f.command).is_err());
    }
    let mut bad = command.clone();
    bad.command.binding.topology.confirmation_endpoint_id = "peer1".into();
    assert!(super::super::channel::validate_controller_command(&bad, &f.command).is_err());
    let mut raw = serde_json::to_value(command).unwrap();
    raw["lifetimeTransferred"] = true.into();
    assert!(serde_json::from_value::<super::super::channel::ControllerCommand>(raw).is_err());
    let mut raw = serde_json::to_value(ready).unwrap();
    raw["canReleaseMaintenance"] = true.into();
    assert!(serde_json::from_value::<super::super::channel::ControllerReady>(raw).is_err());
}
#[test]
fn topology_uses_configured_one_to_four_peers_with_separate_confirmation_selection() {
    for count in 1..=4 {
        let t = topology(count);
        validate_topology(&t).unwrap();
        let cfg:WorkerConfig=serde_json::from_value(json!({"schemaVersion":1,"instanceId":"instance","nodeId":"worker","stateDirectory":"/state","projects":[],"worker":{"keepRunningOnUiExit":true,"controllerTopology":{"schemaVersion":1,"primary":t.primary,"controllers":t.controllers}}})).unwrap();
        assert_eq!(topology_from_config(&cfg, None).unwrap(), t);
        let selected = format!("peer{}", count - 1);
        let actual = topology_from_config(&cfg, Some(&selected)).unwrap();
        assert_eq!(actual.primary, "peer0");
        assert_eq!(actual.confirmation_endpoint_id, selected);
        assert_eq!(actual.controllers, t.controllers);
        assert!(topology_from_config(&cfg, Some("foreign")).is_err());
    }
    let cfg:WorkerConfig=serde_json::from_value(json!({"schemaVersion":1,"instanceId":"instance","nodeId":"worker","stateDirectory":"/state","projects":[],"worker":{"keepRunningOnUiExit":true,"controllerUrl":"ws://127.0.0.1:1144"}})).unwrap();
    assert_eq!(
        topology_from_config(&cfg, None)
            .unwrap()
            .confirmation_endpoint_id,
        "legacy"
    );
    let mut none = cfg;
    none.worker.controller_url = None;
    assert!(topology_from_config(&none, None).is_err());
    for count in [0, 5] {
        assert!(validate_topology(&topology(count)).is_err());
    }
    for index in 0..6 {
        let mut bad = topology(2);
        match index {
            0 => bad.schema_version = 2,
            1 => bad.primary = "foreign".into(),
            2 => bad.confirmation_endpoint_id = "foreign".into(),
            3 => bad.controllers[1].id = bad.controllers[0].id.clone(),
            4 => bad.controllers[1].url = format!("{}/", bad.controllers[0].url),
            _ => bad.controllers[0].url = "ws://public.example:1144".into(),
        }
        assert!(validate_topology(&bad).is_err(), "mutation {index}");
    }
}
#[test]
fn controller_command_requires_exact_confirmed_service_and_original_local_prefix() {
    let f = fixture();
    let issued = ActivationServiceRecord::issued(f.service.command.clone()).unwrap();
    let uncertain = issued
        .indeterminate("lost".into(), issued.updated_at_ms + 1)
        .unwrap();
    for service in [issued, uncertain] {
        assert!(prepare_command(
            &f.physical,
            &f.writer,
            &service,
            &f.local,
            topology(2),
            &f.command.command_id,
            f.command.issued_at_ms
        )
        .is_err());
    }
    for index in 0..5 {
        let mut bad = f.local.clone();
        match index {
            0 => bad.local = None,
            1 => bad.intent.attempt_id = uuid::Uuid::new_v4().to_string(),
            2 => bad.state = ActivationState::StateOwned {},
            3 => bad.updated_at_ms = f.physical.updated_at_ms - 1,
            _ => bad.state_owned.as_mut().unwrap().generation = uuid::Uuid::new_v4().to_string(),
        }
        assert!(prepare_command(
            &f.physical,
            &f.writer,
            &f.service,
            &bad,
            topology(2),
            &f.command.command_id,
            f.command.issued_at_ms
        )
        .is_err());
    }
    for at in [
        f.service.updated_at_ms - 1,
        f.command.deadline_at_ms,
        f.command.deadline_at_ms + 1,
    ] {
        assert!(prepare_command(
            &f.physical,
            &f.writer,
            &f.service,
            &f.local,
            topology(2),
            &f.command.command_id,
            at
        )
        .is_err());
    }
    let mut bad = f.command.clone();
    bad.deadline_at_ms += 1;
    assert!(validate_command_envelope(&bad).is_err());
    let mut alias = f.command.clone();
    alias.command_id = alias.binding.service_command.command_id.clone();
    assert!(validate_command_envelope(&alias).is_err());
}

#[test]
fn explicitly_selected_secondary_can_confirm_without_relabeling_the_primary() {
    let mut f = fixture();
    let mut selected = topology(2);
    selected.confirmation_endpoint_id = "peer1".into();
    f.command = prepare_command(
        &f.physical,
        &f.writer,
        &f.service,
        &f.local,
        selected.clone(),
        &uuid::Uuid::new_v4().to_string(),
        f.command.issued_at_ms,
    )
    .unwrap();
    f.receipt.command_id = f.command.command_id.clone();
    f.receipt.command_sha256 = command_sha256(&f.command).unwrap();
    f.receipt.session.controller_id = "peer1".into();
    f.receipt.session.controller_url = selected.controllers[1].url.clone();
    f.projection.controller_generation = uuid::Uuid::new_v4().to_string();
    f.projection.session_id = uuid::Uuid::new_v4().to_string();
    f.receipt.session.controller_generation = f.projection.controller_generation.clone();
    f.receipt.session.session_id = f.projection.session_id.clone();
    f.receipt.session.projection_sha256 = projection_sha256(&f.projection).unwrap();
    validate_projection_evidence(&f.receipt, &f.command, &f.projection).unwrap();
    validate_command(
        &f.command,
        &f.physical,
        &f.writer,
        &f.service,
        &f.local,
        &selected,
    )
    .unwrap();
    assert_eq!(f.command.binding.topology.primary, "peer0");
    assert!(validate_command(
        &f.command,
        &f.physical,
        &f.writer,
        &f.service,
        &f.local,
        &topology(2)
    )
    .is_err());
}
#[test]
fn valid_proposal_still_needs_current_graph_and_exact_config_selection() {
    let f = fixture();
    for index in 0..3 {
        let mut changed = f.command.clone();
        match index {
            0 => changed.binding.local_record_sha256 = "0".repeat(64),
            1 => changed.binding.service_record_sha256 = "0".repeat(64),
            _ => changed.binding.topology.confirmation_endpoint_id = "peer1".into(),
        }
        validate_command_envelope(&changed).unwrap();
        assert!(validate_command(
            &changed,
            &f.physical,
            &f.writer,
            &f.service,
            &f.local,
            &topology(2)
        )
        .is_err());
    }
}
#[test]
fn controller_receipt_rejects_wrong_peer_stale_time_and_any_worker_scope_drift() {
    let f = fixture();
    for index in 0..20 {
        let mut bad = f.receipt.clone();
        match index {
            0 => bad.authority.epoch += 1,
            1 => bad.compatibility.identity.process.process_id += 1,
            2 => bad.compatibility.generation = uuid::Uuid::new_v4().to_string(),
            3 => bad.compatibility.worker_config_sha256 = "0".repeat(64),
            4 => bad.compatibility.worker_journal_schema += 1,
            5 => bad.command_sha256 = "0".repeat(64),
            6 => bad.session.controller_id = "peer1".into(),
            7 => bad.session.controller_url = "ws://127.0.0.1:1145".into(),
            8 => bad.session.controller_node_id = "bad/node".into(),
            9 => bad.session.controller_generation = "not-uuid".into(),
            10 => bad.session.session_id = "not-uuid".into(),
            11 => bad.session.worker_context.instance_id = "foreign".into(),
            12 => bad.session.worker_generation = uuid::Uuid::new_v4().to_string(),
            13 => bad.session.worker_build.build_id.push('x'),
            14 => bad.session.connection_attempt = 0,
            15 => bad.session.projection_sequence = 0,
            16 => bad.session.connected_at_ms = f.command.issued_at_ms - 1,
            17 => bad.session.connected_at_ms = f.command.deadline_at_ms,
            18 => bad.session.controller_build.source_commit = Some("invalid".into()),
            _ => bad.compatibility.observed_at_ms = bad.session.connected_at_ms - 1,
        }
        assert!(
            validate_receipt(&bad, &f.command).is_err(),
            "mutation {index}"
        );
    }
}
#[test]
fn full_projection_evidence_must_match_session_provider_sequence_and_canonical_digest() {
    let f = fixture();
    for index in 0..8 {
        let mut bad = f.projection.clone();
        match index {
            0 => bad.controller_generation = uuid::Uuid::new_v4().to_string(),
            1 => bad.session_id = uuid::Uuid::new_v4().to_string(),
            2 => bad.provider_generation = uuid::Uuid::new_v4().to_string(),
            3 => bad.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
            4 => bad.sequence += 1,
            5 => bad.controller_build.build_id.push('x'),
            6 => bad.schema_version += 1,
            _ => bad.truncated = true,
        }
        assert!(
            validate_projection_evidence(&f.receipt, &f.command, &bad).is_err(),
            "mutation {index}"
        );
    }
    let mut bad = f.receipt.clone();
    bad.session.projection_sha256 = "0".repeat(64);
    validate_receipt(&bad, &f.command).unwrap();
    assert!(validate_projection_evidence(&bad, &f.command, &f.projection).is_err());
}
#[test]
fn normal_ipc_link_evidence_rejects_other_welcome_node_and_reconnected_session() {
    let f = fixture();
    let s = &f.receipt.session;
    let link = winbridge_protocol::ControllerLinkObservation {
        id: s.controller_id.clone(),
        attempts: s.connection_attempt.to_string(),
        last_connected_at_ms: Some(s.connected_at_ms),
        last_disconnected_at_ms: None,
        last_error: None,
        rpc: winbridge_protocol::NetworkRpcObservation {
            in_flight: 0,
            limit: 16,
            admitted: "1".into(),
            completed: "1".into(),
            rejected: "0".into(),
        },
        connection: winbridge_protocol::ControllerConnection {
            url: s.controller_url.clone(),
            state: ConnectionState::Connected,
            session_id: Some(s.session_id.clone()),
            controller_node_id: Some(s.controller_node_id.clone()),
            controller_generation: Some(s.controller_generation.clone()),
            last_confirmed_at_ms: Some(s.connected_at_ms),
            error: None,
        },
    };
    validate_link_evidence(&f.receipt, &f.command, &link).unwrap();
    for index in 0..7 {
        let mut changed = link.clone();
        match index {
            0 => changed.connection.controller_node_id = Some("another-node".into()),
            1 => changed.connection.session_id = Some(uuid::Uuid::new_v4().to_string()),
            2 => changed.attempts = "2".into(),
            3 => changed.connection.state = ConnectionState::Disconnected,
            4 => changed.connection.error = Some("error".into()),
            5 => changed.last_connected_at_ms = None,
            _ => changed.connection.last_confirmed_at_ms = None,
        }
        assert!(
            validate_link_evidence(&f.receipt, &f.command, &changed).is_err(),
            "mutation {index}"
        );
    }
}
#[test]
fn late_controller_fact_can_resolve_uncertainty_without_reissue_or_deadline_renewal() {
    let mut f = fixture();
    let issued = ActivationControllerRecord::issued(f.command.clone()).unwrap();
    let unknown = issued
        .indeterminate("No ACK; not retried".into(), f.command.deadline_at_ms + 1)
        .unwrap();
    assert!(unknown
        .indeterminate("again".into(), unknown.updated_at_ms + 1)
        .is_err());
    f.receipt.compatibility.observed_at_ms = f.command.deadline_at_ms + 2;
    let late = unknown
        .confirmed(f.receipt, f.command.deadline_at_ms + 3)
        .unwrap();
    unknown.advance(&late).unwrap();
    assert_eq!(late.revision, 2);
    assert_eq!(late.command, f.command);
    let mut changed = late.clone();
    changed.command.command_id = uuid::Uuid::new_v4().to_string();
    assert!(unknown.advance(&changed).is_err());
    assert!(issued
        .indeterminate("bad\nreason".into(), issued.updated_at_ms + 1)
        .is_err());
}
#[test]
fn controller_dto_and_state_reject_injected_grants_unknown_phases_and_untyped_counters() {
    let f = fixture();
    let raw = serde_json::to_value(&f.command).unwrap();
    for field in [
        "nonce",
        "permit",
        "authorityId",
        "force",
        "canConnect",
        "committed",
    ] {
        let mut bad = raw.clone();
        bad["binding"][field] = json!(true);
        assert!(serde_json::from_value::<WorkerControllerActivationCommand>(bad).is_err());
    }
    for field in ["issuedAtMs", "deadlineAtMs"] {
        let mut bad = raw.clone();
        bad[field] = json!(true);
        assert!(serde_json::from_value::<WorkerControllerActivationCommand>(bad).is_err());
    }
    let mut bad = serde_json::to_value(&f.receipt).unwrap();
    bad["session"]["connectionAttempt"] = json!(true);
    assert!(serde_json::from_value::<WorkerControllerActivationReceipt>(bad).is_err());
    assert!(
        serde_json::from_value::<ActivationControllerState>(json!({"phase":"committed"})).is_err()
    );
}
