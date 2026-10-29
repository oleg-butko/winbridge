use super::*;
use winbridge_protocol::{ControllerConnection, NetworkRpcObservation};

pub(crate) fn fixture() -> (
    ActivationResidentQuery,
    WorkerControllerActivationTopology,
    WorkerLifetimeActivationReceipt,
    ActivationResidentObservation,
    controller::ActivationControllerSnapshot,
) {
    let (graph, command, entry) = lifetime::test_fixture();
    let topology = graph.controller.command.binding.topology.clone();
    let at = command.deadline_at_ms + 10_000;
    let session = uuid::Uuid::new_v4().to_string();
    let generation = uuid::Uuid::new_v4().to_string();
    let projection = ControllerReadinessProjection {
        schema_version: 1,
        controller_build: controller::test_fixture().projection.controller_build,
        controller_generation: generation.clone(),
        session_id: session.clone(),
        provider_build: command
            .binding
            .compatibility
            .identity
            .descriptor
            .build
            .clone(),
        provider_generation: command.binding.compatibility.generation.clone(),
        context: command.binding.compatibility.context.clone(),
        sequence: 2,
        views: vec![],
        verifications: vec![],
        truncated: false,
    };
    let endpoint = topology
        .controllers
        .iter()
        .find(|c| c.id == topology.confirmation_endpoint_id)
        .unwrap();
    let link = ControllerLinkObservation {
        id: endpoint.id.clone(),
        attempts: "2".into(),
        connection: ControllerConnection {
            url: endpoint.url.clone(),
            state: ConnectionState::Connected,
            session_id: Some(session),
            controller_node_id: Some("reconnected-controller".into()),
            controller_generation: Some(generation),
            last_confirmed_at_ms: Some(at - 1),
            error: None,
        },
        last_connected_at_ms: Some(at - 10),
        last_disconnected_at_ms: Some(at - 20),
        last_error: Some("Historical disconnect".into()),
        rpc: NetworkRpcObservation {
            in_flight: 0,
            limit: winbridge_protocol::CONTROLLER_RPC_LIMIT,
            admitted: "0".into(),
            completed: "0".into(),
            rejected: "0".into(),
        },
    };
    let query = ActivationResidentQuery {
        resident_observation_version: 7,
        request_id: uuid::Uuid::new_v4().to_string(),
        command,
    };
    let observed = ActivationResidentObservation {
        resident_observation_version: 7,
        request_id: Some(query.request_id.clone()),
        result: ActivationResidentObservationResult::Observed {
            entry: Box::new(entry.clone()),
            frame: Box::new(summarize_projection(&projection).unwrap()),
            link: Box::new(link),
            observed_at_ms: at,
        },
    };
    (query, topology, entry, observed, graph)
}

#[test]
fn current_reconnected_session_outlives_entry_window_without_rewriting_the_receipt() {
    let (query, topology, entry, observed, _) = fixture();
    validate_observation(&query, &entry, &topology, &observed).unwrap();
    assert_eq!(
        resident_observation_marker_pattern(),
        ACTIVATION_RESIDENT_OBSERVATION_MARKER
    );
    assert_ne!(
        ACTIVATION_RESIDENT_OBSERVATION_MARKER,
        ACTIVATION_LIFETIME_MARKER
    );
    let ActivationResidentObservationResult::Observed { observed_at_ms, .. } = observed.result
    else {
        unreachable!()
    };
    assert!(observed_at_ms > query.command.deadline_at_ms);
    lifetime::validate_receipt(&entry, &query.command).unwrap();
}

#[test]
fn responses_cannot_rebind_scope_time_provider_or_selected_current_session() {
    let (query, topology, entry, observed, _) = fixture();
    for mutation in 0..26 {
        let mut changed = observed.clone();
        match mutation {
            0 => changed.resident_observation_version = 6,
            1 => changed.request_id = Some(uuid::Uuid::new_v4().to_string()),
            2 => changed.request_id = None,
            _ => {
                let ActivationResidentObservationResult::Observed {
                    entry: e,
                    frame: p,
                    link: l,
                    observed_at_ms: at,
                } = &mut changed.result
                else {
                    unreachable!()
                };
                match mutation {
                    3 => e.compatibility.observed_at_ms += 1,
                    4 => p.context.instance_id.push('x'),
                    5 => p.provider_generation = uuid::Uuid::new_v4().to_string(),
                    6 => p.provider_build.build_id.push('x'),
                    7 => p.sequence = 0,
                    8 => p.controller_generation = "bad".into(),
                    9 => p.session_id = "bad".into(),
                    10 => p.controller_build.version = "".into(),
                    11 => l.id.push('x'),
                    12 => l.connection.url.push('x'),
                    13 => l.connection.controller_node_id = None,
                    14 => l.attempts = "02".into(),
                    15 => l.connection.state = ConnectionState::Disconnected,
                    16 => l.connection.error = Some("Disconnected".into()),
                    17 => l.connection.session_id = Some(uuid::Uuid::new_v4().to_string()),
                    18 => l.connection.controller_generation = None,
                    19 => l.last_connected_at_ms = None,
                    20 => l.connection.last_confirmed_at_ms = Some(*at + 1),
                    21 => l.last_disconnected_at_ms = Some(*at),
                    22 => *at = query.command.binding.compatibility.observed_at_ms - 1,
                    23 => p.projection_sha256 = "bad".into(),
                    24 => p.schema_version = 2,
                    _ => *at = MAX_UPDATE_COUNTER + 1,
                }
            }
        }
        assert!(
            validate_observation(&query, &entry, &topology, &changed).is_err(),
            "mutation {mutation}"
        );
    }
    for result in [
        ActivationResidentObservationResult::Unavailable {
            message: "No fresh projection".into(),
        },
        ActivationResidentObservationResult::Rejected {
            message: "Wrong keeper".into(),
        },
    ] {
        let mut changed = observed.clone();
        changed.result = result;
        assert!(validate_observation(&query, &entry, &topology, &changed).is_err());
    }
}

#[test]
fn query_is_strict_readonly_and_has_separate_correlation() {
    let (query, _, _, observed, _) = fixture();
    for mutation in 0..4 {
        let mut q = query.clone();
        match mutation {
            0 => q.resident_observation_version = 6,
            1 => q.request_id = q.command.command_id.clone(),
            2 => q.request_id = "untrusted".into(),
            _ => q.command.binding.job_name.clear(),
        }
        assert!(validate_query(&q).is_err());
    }
    for key in ["processId", "release", "canCommit", "keeperHandle"] {
        let mut raw = serde_json::to_value(&query).unwrap();
        raw[key] = true.into();
        assert!(serde_json::from_value::<ActivationResidentQuery>(raw).is_err());
        let mut raw = serde_json::to_value(&observed).unwrap();
        raw["result"][key] = true.into();
        assert!(serde_json::from_value::<ActivationResidentObservation>(raw).is_err());
    }
}
