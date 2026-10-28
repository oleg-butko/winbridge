use super::*;
use winbridge_protocol::update::*;

// Typed metadata only, never original OS custody or a fresh admission grant.
fn fixture() -> (controller::TestFixture, UpdateRecord, UpdateTransition) {
    let f = controller::test_fixture();
    let source = &f.local.intent.source;
    let root = UpdateRecord {
        update_id: source.update_id.clone(),
        revision: source.parent_revision + 1,
        phase: UpdatePhase::AwaitingController,
        request: UpdateRequest {
            contract_version: 1,
            idempotency_key: "root".into(),
            source: UpdateSourcePin {
                archive_name: "source.zip".into(),
                archive_sha256: "a".repeat(64),
                manifest_sha256: "b".repeat(64),
                source_commit: "c".repeat(40),
            },
            expected: UpdateInstallationPin {
                installation_id: source.installation_id.clone(),
                scope_sha256: source.scope_sha256.clone(),
                worker_build_id: "build".into(),
                worker_config_sha256: "d".repeat(64),
                baseline_manifest_sha256: "e".repeat(64),
            },
        },
        accepted_at_ms: 1,
        updated_at_ms: f.local.updated_at_ms,
        reason: Some("Local proof; no final commit".into()),
    };
    let edge = UpdateTransition {
        sequence: root.revision + 1,
        update_id: source.update_id.clone(),
        revision: root.revision,
        previous_phase: Some(UpdatePhase::VerifyingLocal),
        phase: root.phase,
        occurred_at_ms: root.updated_at_ms,
        owner_epoch: source.owner_epoch,
        owner_id: source.owner_id.clone(),
        command: Some(UpdateCommand {
            idempotency_key: format!("local-service-verified-{}", f.service.command.command_id),
            expected_revision: source.parent_revision,
            event: UpdateEvent::LocalHealthVerified,
            reason: root.reason.clone().unwrap(),
        }),
    };
    (f, root, edge)
}

#[test]
fn controller_root_requires_the_exact_atomic_local_proof_edge_not_merely_awaiting_phase() {
    let (f, root, edge) = fixture();
    local_edge(&f.local, &f.service, &root, &edge).unwrap();
    for index in 0..18 {
        let (mut f, mut root, mut edge) = fixture();
        match index {
            0 => f.local.revision = 6,
            1 => f.local.state = ActivationState::StateOwned {},
            2 => root.phase = UpdatePhase::Committed,
            3 => root.revision += 1,
            4 => root.update_id = Uuid::new_v4().to_string(),
            5 => root.request.expected.installation_id = Uuid::new_v4().to_string(),
            6 => root.request.expected.scope_sha256 = "0".repeat(64),
            7 => root.updated_at_ms += 1,
            8 => edge.previous_phase = Some(UpdatePhase::Replacing),
            9 => edge.phase = UpdatePhase::VerifyingLocal,
            10 => edge.owner_epoch += 1,
            11 => edge.owner_id = Uuid::new_v4().to_string(),
            12 => edge.command.as_mut().unwrap().event = UpdateEvent::ControllerConfirmed,
            13 => edge.command.as_mut().unwrap().expected_revision += 1,
            14 => edge.command.as_mut().unwrap().idempotency_key = "another-key".into(),
            15 => edge.command = None,
            16 => root.reason = Some("Another root".into()),
            _ => edge.update_id = Uuid::new_v4().to_string(),
        }
        assert!(
            local_edge(&f.local, &f.service, &root, &edge).is_err(),
            "mutation {index}"
        );
    }
}

#[test]
fn coordinated_phase_rewrite_cannot_skip_the_original_root_revision_or_change_service_key() {
    let (f, mut root, mut edge) = fixture();
    root.revision += 1;
    edge.revision = root.revision;
    local_edge(&f.local, &f.service, &root, &edge).unwrap_err();
    let (f, root, mut edge) = fixture();
    edge.command.as_mut().unwrap().idempotency_key =
        format!("local-service-verified-{}", f.writer.command.command_id);
    local_edge(&f.local, &f.service, &root, &edge).unwrap_err();
}
