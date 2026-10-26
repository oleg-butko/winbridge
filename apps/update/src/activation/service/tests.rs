use super::*;

fn fixture() -> (
    ActivationRecord,
    ActivationWriterRecord,
    WorkerServiceActivationCommand,
    WorkerServiceActivationReceipt,
) {
    let (physical, written) = super::super::writer_view::fixture();
    let command = prepare_command(
        &physical,
        &written,
        &uuid::Uuid::new_v4().to_string(),
        written.updated_at_ms + 1,
    )
    .unwrap();
    let b = &command.binding;
    let w = &b.writer_command.binding;
    let receipt = WorkerServiceActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: w.update_id.clone(),
        attempt_id: w.attempt_id.clone(),
        command_sha256: command_sha256(&command).unwrap(),
        authority: b.writer_receipt.authority.clone(),
        compatibility: winbridge_protocol::WorkerRuntimeCompatibility {
            schema_version: 1,
            kind: "worker_runtime_compatibility".into(),
            identity: b.expected_worker.clone(),
            context: w.physical.context.clone(),
            generation: w.physical.generation.clone(),
            worker_config_sha256: w.worker_config_sha256.clone(),
            worker_config_schema: b.worker_config_schema,
            worker_journal_schema: w.worker_journal_schema,
            updater_journal_schema: w.updater_journal_schema,
            protocol_version: b.protocol_version,
            observed_at_ms: command.issued_at_ms + 1,
        },
    };
    (physical, written, command, receipt)
}

#[test]
fn service_binding_keeps_same_committed_authority_child_and_generation() {
    let (physical, written, command, receipt) = fixture();
    validate_command(&command, &physical, &written).unwrap();
    validate_receipt(&receipt, &command).unwrap();
    assert_eq!(receipt.authority, command.binding.writer_receipt.authority);
    assert_eq!(
        receipt.compatibility.generation,
        physical.state_owned.as_ref().unwrap().generation
    );
    let issued = ActivationServiceRecord::issued(command).unwrap();
    let confirmed = issued
        .confirmed(receipt.clone(), receipt.compatibility.observed_at_ms)
        .unwrap();
    issued.advance(&confirmed).unwrap();
    assert_eq!(confirmed.revision, 1);
    assert!(confirmed
        .confirmed(receipt.clone(), confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed
        .indeterminate("late error".into(), confirmed.updated_at_ms + 1)
        .is_err());
    assert!(confirmed.advance(&issued).is_err());
}

#[test]
fn service_proposal_cannot_renew_original_window_or_use_uncertain_writer() {
    let (physical, written, command, _) = fixture();
    let issued = ActivationWriterRecord::issued(written.command.clone()).unwrap();
    let unknown = issued
        .indeterminate("Lost ACK".into(), issued.updated_at_ms + 1)
        .unwrap();
    for record in [issued, unknown] {
        assert!(prepare_command(
            &physical,
            &record,
            &command.command_id,
            command.issued_at_ms
        )
        .is_err());
    }
    for at in [
        written.updated_at_ms - 1,
        command.deadline_at_ms,
        command.deadline_at_ms + 1,
    ] {
        assert!(prepare_command(&physical, &written, &command.command_id, at).is_err());
    }
    let mut renewed = command.clone();
    renewed.deadline_at_ms += 1;
    assert!(validate_command_envelope(&renewed).is_err());
    assert!(validate_command(&renewed, &physical, &written).is_err());
}

#[test]
fn service_envelope_rejects_wrong_key_loaded_build_process_schema_and_paths() {
    let (_, _, command, _) = fixture();
    for index in 0..11 {
        let mut bad = command.clone();
        match index {
            0 => bad.command_id = bad.binding.writer_command.command_id.clone(),
            1 => bad.binding.expected_worker.process.process_id += 1,
            2 => bad.binding.expected_worker.descriptor.build.source_dirty = true,
            3 => bad.binding.expected_worker.descriptor.role = ArtifactRole::Updater,
            4 => bad.binding.expected_worker.image.path = "relative/worker.exe".into(),
            5 => bad.binding.expected_worker.image.query_supported = false,
            6 => bad.binding.expected_worker.set_id = "not-a-uuid".into(),
            7 => bad.binding.protocol_version += 1,
            8 => bad.binding.worker_config_schema += 1,
            9 => bad.binding.writer_receipt.authority.epoch += 1,
            _ => bad
                .binding
                .expected_worker
                .image
                .path
                .push_str("/../worker.exe"),
        }
        assert!(validate_command_envelope(&bad).is_err(), "mutation {index}");
    }
}

#[test]
fn structurally_valid_proposal_still_needs_current_immutable_graph() {
    let (physical, written, command, _) = fixture();
    let mut bad = command.clone();
    bad.binding.writer_record_sha256 = "0".repeat(64);
    validate_command_envelope(&bad).unwrap();
    assert!(validate_command(&bad, &physical, &written).is_err());
    let mut changed = physical.clone();
    changed.ready.as_mut().unwrap().identity.image.sha256 = "0".repeat(64);
    assert!(prepare_command(
        &changed,
        &written,
        &command.command_id,
        command.issued_at_ms
    )
    .is_err());
    let mut changed_writer = written.clone();
    changed_writer.updated_at_ms += 1;
    changed_writer.validate().unwrap();
    assert!(validate_command(&command, &physical, &changed_writer).is_err());
}

#[test]
fn service_receipt_cannot_add_epoch_rebind_runtime_or_invent_config_readiness() {
    let (_, _, command, receipt) = fixture();
    for index in 0..11 {
        let mut bad = receipt.clone();
        match index {
            0 => bad.authority.epoch += 1,
            1 => bad.compatibility.identity.process.process_id += 1,
            2 => bad.compatibility.generation = uuid::Uuid::new_v4().to_string(),
            3 => bad.compatibility.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
            4 => bad.compatibility.worker_config_sha256 = "0".repeat(64),
            5 => bad.compatibility.worker_journal_schema += 1,
            6 => bad.compatibility.updater_journal_schema += 1,
            7 => bad.compatibility.protocol_version += 1,
            8 => bad.compatibility.observed_at_ms = command.issued_at_ms - 1,
            9 => bad.command_sha256 = "0".repeat(64),
            _ => bad.compatibility.identity.descriptor.build.source_commit = Some("0".repeat(40)),
        }
        assert!(
            validate_receipt(&bad, &command).is_err(),
            "mutation {index}"
        );
    }
}

#[test]
fn late_service_receipt_resolves_data_uncertainty_without_reissue() {
    let (_, _, command, mut receipt) = fixture();
    let issued = ActivationServiceRecord::issued(command.clone()).unwrap();
    let unknown = issued
        .indeterminate(
            "Private service outcome unknown".into(),
            command.deadline_at_ms + 1,
        )
        .unwrap();
    assert!(unknown
        .indeterminate("retry".into(), unknown.updated_at_ms + 1)
        .is_err());
    receipt.compatibility.observed_at_ms = command.deadline_at_ms + 2;
    let late = unknown
        .confirmed(receipt, command.deadline_at_ms + 3)
        .unwrap();
    unknown.advance(&late).unwrap();
    assert_eq!(late.revision, 2);
    assert_eq!(late.command, command);
    let mut changed = late.clone();
    changed.command.command_id = uuid::Uuid::new_v4().to_string();
    assert!(unknown.advance(&changed).is_err());
    assert!(issued
        .indeterminate("bad\nreason".into(), issued.updated_at_ms + 1)
        .is_err());
}

#[test]
fn strict_service_data_rejects_injected_permissions_and_untyped_counters() {
    let (_, _, command, receipt) = fixture();
    let raw = serde_json::to_value(&command).unwrap();
    serde_json::from_value::<WorkerServiceActivationCommand>(raw.clone()).unwrap();
    for field in [
        "nonce",
        "authorityId",
        "permit",
        "force",
        "canStartService",
        "controllerConfirmed",
    ] {
        let mut bad = raw.clone();
        bad[field] = true.into();
        assert!(serde_json::from_value::<WorkerServiceActivationCommand>(bad).is_err());
    }
    for value in [
        serde_json::json!(true),
        serde_json::json!(1.5),
        serde_json::json!("1"),
    ] {
        let mut bad = raw.clone();
        bad["issuedAtMs"] = value;
        assert!(serde_json::from_value::<WorkerServiceActivationCommand>(bad).is_err());
    }
    let mut bad = serde_json::to_value(receipt).unwrap();
    bad["canReleaseMaintenance"] = true.into();
    assert!(serde_json::from_value::<WorkerServiceActivationReceipt>(bad).is_err());
}
