//! Metadata fixtures only; these tests never create a writer or live permit.
use super::*;
fn fixture() -> (
    ActivationRecord,
    StateBackupAuthority,
    WorkerJournalActivationCommand,
    WorkerJournalActivationReceipt,
) {
    let (replacement, initial) = crate::activation::activation_fixture();
    let r = crate::activation::activation_stages(&initial, &replacement)[6].clone();
    let expected = StateBackupAuthority {
        incarnation: r.intent.context.worker_context.journal_incarnation.clone(),
        epoch: 7,
        fingerprint_sha256: "a".repeat(64),
    };
    let command = prepare_command(
        &r,
        &expected,
        &uuid::Uuid::new_v4().to_string(),
        r.updated_at_ms + 1,
    )
    .unwrap();
    let receipt = WorkerJournalActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: command.binding.update_id.clone(),
        attempt_id: command.binding.attempt_id.clone(),
        command_sha256: command_sha256(&command).unwrap(),
        authority: StateBackupAuthority {
            incarnation: expected.incarnation.clone(),
            epoch: 8,
            fingerprint_sha256: "b".repeat(64),
        },
        committed_at_ms: command.issued_at_ms + 1,
    };
    (r, expected, command, receipt)
}
#[test]
fn writer_command_rejects_stale_headers_foreign_scope_and_unlisted_record_changes() {
    let (r, expected, command, _) = fixture();
    validate_command(&command, &r, &expected).unwrap();
    for index in 0..7 {
        let mut changed = r.clone();
        match index {
            0 => changed.intent.source.owner_epoch += 1,
            1 => changed.intent.source.backup_sha256 = "d".repeat(64),
            2 => changed.intent.context.worker_config.sha256 = "d".repeat(64),
            3 => changed.intent.context.channel_nonce_sha256 = "d".repeat(64),
            4 => changed.intent.context.worker_journal_schema += 1,
            5 => changed
                .state_owned
                .as_mut()
                .unwrap()
                .process
                .birth
                .push('0'),
            _ => changed
                .state_owned
                .as_mut()
                .unwrap()
                .state
                .lock_identity
                .push('x'),
        }
        assert!(validate_command(&command, &changed, &expected).is_err());
    }
    let mut newer = expected.clone();
    newer.epoch += 1;
    assert!(validate_command(&command, &r, &newer).is_err());
    newer = expected.clone();
    newer.fingerprint_sha256 = "c".repeat(64);
    assert!(validate_command(&command, &r, &newer).is_err());
}
#[test]
fn writer_proposal_requires_state_owned_and_a_nonexhausted_old_header() {
    let (r, mut expected, command, _) = fixture();
    let (replacement, initial) = crate::activation::activation_fixture();
    for early in crate::activation::activation_stages(&initial, &replacement)
        .iter()
        .take(6)
    {
        assert!(prepare_command(
            early,
            &expected,
            &command.command_id,
            early.updated_at_ms + 1
        )
        .is_err());
    }
    let mut inconsistent = r.clone();
    inconsistent.state = ActivationState::ReadyForState {};
    assert!(prepare_command(
        &inconsistent,
        &expected,
        &command.command_id,
        command.issued_at_ms
    )
    .is_err());
    for epoch in [0, MAX_UPDATE_COUNTER, u64::MAX] {
        expected.epoch = epoch;
        assert!(prepare_command(&r, &expected, &command.command_id, command.issued_at_ms).is_err());
    }
    expected.epoch = 7;
    expected.incarnation = uuid::Uuid::new_v4().to_string();
    assert!(prepare_command(&r, &expected, &command.command_id, command.issued_at_ms).is_err());
}
#[test]
fn another_command_key_never_renews_the_original_writer_window() {
    let (mut r, expected, command, _) = fixture();
    assert!(prepare_command(&r, &expected, &command.command_id, r.updated_at_ms - 1).is_err());
    assert!(prepare_command(
        &r,
        &expected,
        &uuid::Uuid::new_v4().to_string(),
        command.deadline_at_ms + 1
    )
    .is_err());
    r.intent.admitted_at_ms = MAX_UPDATE_COUNTER - 1;
    assert!(prepare_command(&r, &expected, &command.command_id, command.issued_at_ms).is_err());
}
#[test]
fn writer_receipt_needs_exact_next_epoch_and_retains_a_late_committed_fact() {
    let (_, _, command, receipt) = fixture();
    validate_receipt(&receipt, &command).unwrap();
    for index in 0..7 {
        let mut bad = receipt.clone();
        match index {
            0 => bad.authority.epoch = 7,
            1 => bad.authority.epoch = 9,
            2 => bad.authority.incarnation = uuid::Uuid::new_v4().to_string(),
            3 => {
                bad.authority.fingerprint_sha256 = command
                    .binding
                    .expected_authority
                    .fingerprint_sha256
                    .clone()
            }
            4 => bad.command_sha256 = "d".repeat(64),
            5 => bad.attempt_id = uuid::Uuid::new_v4().to_string(),
            _ => bad.committed_at_ms = command.issued_at_ms - 1,
        }
        assert!(validate_receipt(&bad, &command).is_err());
    }
    let mut late = receipt;
    late.committed_at_ms = command.deadline_at_ms + 1000;
    validate_receipt(&late, &command).unwrap();
    // This observes a possibly late transaction result, never issues a new grant.
}
#[test]
fn same_key_with_changed_payload_cannot_reuse_the_committed_receipt() {
    let (r, expected, command, receipt) = fixture();
    let later =
        prepare_command(&r, &expected, &command.command_id, command.issued_at_ms + 1).unwrap();
    assert_ne!(
        command_sha256(&command).unwrap(),
        command_sha256(&later).unwrap()
    );
    assert!(validate_receipt(&receipt, &later).is_err());
}
#[test]
fn writer_data_cannot_smuggle_an_authority_id_token_or_permission() {
    let (r, expected, command, receipt) = fixture();
    for key in ["authorityId", "permit", "writerOwned", "nonce", "token"] {
        let mut bad = serde_json::to_value(&command).unwrap();
        bad[key] = "private-never-echo".into();
        assert!(serde_json::from_value::<WorkerJournalActivationCommand>(bad).is_err());
        let mut bad = serde_json::to_value(&receipt).unwrap();
        bad[key] = true.into();
        assert!(serde_json::from_value::<WorkerJournalActivationReceipt>(bad).is_err());
    }
    let error =
        prepare_command(&r, &expected, "private-never-echo", command.issued_at_ms).unwrap_err();
    assert!(!error.contains("private-never-echo"));
}

#[test]
fn persisted_writer_receipt_never_certifies_a_self_consistent_invalid_command() {
    let (_, _, command, receipt) = fixture();
    for index in 0..6 {
        let mut bad = command.clone();
        match index {
            0 => {
                bad.binding.physical.context.journal_incarnation = uuid::Uuid::new_v4().to_string()
            }
            1 => bad.binding.physical.attempt_id = uuid::Uuid::new_v4().to_string(),
            2 => bad.binding.physical.process.birth = "0001".into(),
            3 => bad.binding.activation_record_sha256 = "not a hash".into(),
            4 => bad.deadline_at_ms = bad.issued_at_ms - 1,
            _ => {
                bad.binding.physical.state.lock_identity =
                    bad.binding.physical.state.journal_identity.clone()
            }
        }
        let mut forged = receipt.clone();
        forged.command_sha256 = command_sha256(&bad).unwrap();
        assert!(validate_receipt(&forged, &bad).is_err());
    }
}
