//! Private parent/child exchange. Messages validate scope, never grant a writer.
//! The nonce travels only after OS peer verification, never in argv or files.
use super::{ActivationRecord, ActivationState, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use winbridge_protocol::update::SupervisorProcess;

pub use winbridge_protocol::update::{
    ActivationReceiverChallenge as Challenge,
    ActivationReceiverControllerCommand as ControllerCommand,
    ActivationReceiverControllerReady as ControllerReady, ActivationReceiverReady as Ready,
    ActivationReceiverServiceCommand as ServiceCommand,
    ActivationReceiverServiceReady as ServiceReady, ActivationReceiverStateHandoff as StateHandoff,
    ActivationReceiverStateOwned as StateOwned, ActivationReceiverWriterCommand as WriterCommand,
    ActivationReceiverWriterReceipt as WriterReceipt,
    ACTIVATION_CONTROLLER_CONTRACT as CONTROLLER_CONTRACT,
    ACTIVATION_CONTROLLER_MARKER as CONTROLLER_MARKER, ACTIVATION_RECEIVER_CONTRACT as CONTRACT,
    ACTIVATION_RECEIVER_MARKER as RECEIVER_MARKER, ACTIVATION_SERVICE_CONTRACT as SERVICE_CONTRACT,
    ACTIVATION_SERVICE_MARKER as SERVICE_MARKER,
    ACTIVATION_STATE_HANDOFF_CONTRACT as HANDOFF_CONTRACT,
    ACTIVATION_STATE_HANDOFF_MARKER as HANDOFF_MARKER,
    ACTIVATION_WRITER_CONTRACT as WRITER_CONTRACT, ACTIVATION_WRITER_MARKER as WRITER_MARKER,
};
const MAX_BYTES: usize = 32768;

pub fn validate_controller_command(
    message: &ControllerCommand,
    expected: &winbridge_protocol::update::WorkerControllerActivationCommand,
) -> Result<()> {
    if message.contract_version != CONTROLLER_CONTRACT || &message.command != expected {
        return Err("Private controller command differs from durable intent".into());
    }
    super::controller::validate_command_envelope(expected)
}
pub fn validate_controller_ready(
    message: &ControllerReady,
    command: &winbridge_protocol::update::WorkerControllerActivationCommand,
) -> Result<()> {
    if message.contract_version != CONTROLLER_CONTRACT {
        return Err("Private controller Ready contract differs".into());
    }
    super::controller::validate_receipt(&message.receipt, command)
}

pub fn require_writer_receiver(
    record: &ActivationRecord,
    actual: &SupervisorProcess,
    attempt: &str,
) -> Result<()> {
    if !matches!(record.state, ActivationState::StateOwned {})
        || record.revision != 6
        || record.intent.attempt_id != attempt
        || record.created.as_ref().map(|r| &r.process) != Some(actual)
        || record.state_owned.as_ref().map(|r| &r.process) != Some(actual)
    {
        return Err("Writer requires the same current StateOwned child".into());
    }
    Ok(())
}
pub fn validate_writer_command(
    message: &WriterCommand,
    expected: &winbridge_protocol::update::WorkerJournalActivationCommand,
) -> Result<()> {
    if message.contract_version != WRITER_CONTRACT || &message.command != expected {
        return Err("Private writer command differs from the durable intent".into());
    }
    super::writer::validate_command_envelope(&message.command)
}
pub fn validate_writer_receipt(
    message: &WriterReceipt,
    command: &winbridge_protocol::update::WorkerJournalActivationCommand,
) -> Result<()> {
    if message.contract_version != WRITER_CONTRACT {
        return Err("Private writer receipt contract differs".into());
    }
    super::writer::validate_receipt(&message.receipt, command)
}

pub fn validate_service_command(
    message: &ServiceCommand,
    expected: &winbridge_protocol::update::WorkerServiceActivationCommand,
) -> Result<()> {
    if message.contract_version != SERVICE_CONTRACT || &message.command != expected {
        return Err("Private service command differs from durable intent".into());
    }
    super::service::validate_command_envelope(expected)
}
pub fn validate_service_ready(
    message: &ServiceReady,
    command: &winbridge_protocol::update::WorkerServiceActivationCommand,
) -> Result<()> {
    if message.contract_version != SERVICE_CONTRACT {
        return Err("Private service Ready contract differs".into());
    }
    super::service::validate_receipt(&message.receipt, command)
}

/// The caller also authenticates the OS pipe peer before using any message.
pub fn validate_challenge(challenge: &Challenge, record: &ActivationRecord) -> Result<()> {
    if challenge.contract_version != CONTRACT
        || challenge.update_id != record.intent.source.update_id
        || challenge.attempt_id != record.intent.attempt_id
        || challenge.nonce.len() != 64
        || !challenge
            .nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || crate::sha256(challenge.nonce.as_bytes()) != record.intent.context.channel_nonce_sha256
    {
        return Err("Private activation challenge version/scope/authentication rejected".into());
    }
    Ok(())
}
pub fn validate_ready(message: &Ready, record: &ActivationRecord) -> Result<()> {
    let ready = &message.ready;
    let source = &record.intent.source;
    let identity = &ready.identity;
    if message.contract_version != CONTRACT
        || !matches!(record.state, ActivationState::ResumeGranted {})
        || ready.attempt_id != record.intent.attempt_id
        || ready.worker_config_sha256 != record.intent.context.worker_config.sha256
        || identity.schema_version != 1
        || identity.kind != "loaded_worker_identity"
        || Some(&identity.process) != record.created.as_ref().map(|c| &c.process)
        || identity.runtime_identity != source.runtime_identity
        || identity.set_id != source.set_id
        || identity.manifest_sha256 != source.manifest.sha256
        || identity.image != source.worker
        || identity.descriptor != source.worker_descriptor
        || ready.observed_at_ms
            < record
                .resume_granted_at_ms
                .ok_or("Resume receipt missing")?
    {
        return Err("Private Ready differs from the created process/loaded set/config".into());
    }
    Ok(())
}
pub fn require_receiver(
    record: &ActivationRecord,
    actual: &SupervisorProcess,
    attempt: &str,
) -> Result<()> {
    if !matches!(record.state, ActivationState::ResumeGranted {})
        || record.intent.attempt_id != attempt
        || record.created.as_ref().map(|c| &c.process) != Some(actual)
    {
        return Err("Private activation requires the exact resumed child; no writer opened".into());
    }
    Ok(())
}
pub fn require_state_receiver(
    record: &ActivationRecord,
    actual: &SupervisorProcess,
    attempt: &str,
) -> Result<()> {
    if !matches!(record.state, ActivationState::HandoffIssued {})
        || record.revision != 5
        || record.intent.attempt_id != attempt
        || record.created.as_ref().map(|c| &c.process) != Some(actual)
        || record.ready.as_ref().map(|r| &r.identity.process) != Some(actual)
    {
        return Err(
            "State handoff requires the exact Ready child and current issued prefix".into(),
        );
    }
    Ok(())
}
/// The digest binds the actual durable HandoffIssued prefix, never a permission
/// reconstructed from JSON. Both peers separately authenticate current custody.
pub fn state_handoff(record: &ActivationRecord) -> Result<StateHandoff> {
    require_state_receiver(
        record,
        &record
            .created
            .as_ref()
            .ok_or("Created child missing")?
            .process,
        &record.intent.attempt_id,
    )?;
    let mut bytes = b"winbridge.activation.state-handoff.v1\n".to_vec();
    bytes.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(record).map_err(|_| "State handoff encoding rejected")?,
        )
        .map_err(|_| "State handoff canonical encoding rejected")?,
    );
    Ok(StateHandoff {
        contract_version: HANDOFF_CONTRACT,
        update_id: record.intent.source.update_id.clone(),
        attempt_id: record.intent.attempt_id.clone(),
        revision: record.revision,
        record_sha256: crate::sha256(&bytes),
    })
}
pub fn validate_state_handoff(message: &StateHandoff, record: &ActivationRecord) -> Result<()> {
    if message != &state_handoff(record)? {
        return Err("Private state handoff differs from the current issued scope/record".into());
    }
    Ok(())
}
pub fn validate_state_owned(message: &StateOwned, record: &ActivationRecord) -> Result<()> {
    let owned = &message.state_owned;
    require_state_receiver(record, &owned.process, &owned.attempt_id)?;
    let c = &record.intent.context;
    if message.contract_version != HANDOFF_CONTRACT
        || owned.context != c.worker_context
        || owned.state != c.worker_state
        || uuid::Uuid::parse_str(&owned.generation)
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some(owned.generation.as_str())
        || owned.generation == c.previous_generation
        || owned.observed_at_ms < record.handoff_issued_at_ms.ok_or("Handoff issue missing")?
    {
        return Err("Private StateOwned changed physical process/state/generation scope".into());
    }
    Ok(())
}
pub fn endpoint(directory: &Path, attempt: &str) -> Result<String> {
    if uuid::Uuid::parse_str(attempt)
        .ok()
        .map(|id| id.to_string())
        .as_deref()
        != Some(attempt)
    {
        return Err("Private activation requires a canonical attempt UUID".into());
    }
    winbridge_transport::supervisor::endpoint(directory, attempt)
        .map(|address| format!("{address}-activation"))
        .map_err(|e| e.to_string())
}
pub async fn read<S: AsyncRead + Unpin, T: DeserializeOwned>(
    stream: &mut S,
    timeout: Duration,
) -> Result<T> {
    tokio::time::timeout(timeout, async {
        let length = stream
            .read_u32_le()
            .await
            .map_err(|_| "Private activation channel read failed")? as usize;
        if length == 0 || length > MAX_BYTES {
            return Err("Private activation frame must contain 1..32768 bytes".into());
        }
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|_| "Private activation channel read failed")?;
        serde_json::from_slice(&bytes).map_err(|_| "Private activation message rejected".into())
    })
    .await
    .map_err(|_| "Private activation read expired; no replay".to_string())?
}
pub async fn write<S: AsyncWrite + Unpin, T: Serialize>(
    stream: &mut S,
    value: &T,
    timeout: Duration,
) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| "Private activation encoding failed")?;
    if bytes.len() > MAX_BYTES {
        return Err("Private activation frame exceeds 32 KiB".into());
    }
    tokio::time::timeout(timeout, winbridge_transport::write_frame(stream, &bytes))
        .await
        .map_err(|_| "Private activation write expired; acceptance unknown".to_string())?
        .map_err(|_| "Private activation channel write failed; acceptance unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn writer_fixture() -> (ActivationRecord, WriterCommand, WriterReceipt) {
        let (r, initial) = crate::activation::activation_fixture();
        let owned = crate::activation::activation_stages(&initial, &r)[6].clone();
        let authority = winbridge_protocol::update::StateBackupAuthority {
            incarnation: owned
                .intent
                .context
                .worker_context
                .journal_incarnation
                .clone(),
            epoch: 7,
            fingerprint_sha256: "a".repeat(64),
        };
        let command = super::super::writer::prepare_command(
            &owned,
            &authority,
            &uuid::Uuid::new_v4().to_string(),
            owned.updated_at_ms + 1,
        )
        .unwrap();
        let receipt = WriterReceipt {
            contract_version: WRITER_CONTRACT,
            receipt: winbridge_protocol::update::WorkerJournalActivationReceipt {
                schema_version: 1,
                command_id: command.command_id.clone(),
                update_id: command.binding.update_id.clone(),
                attempt_id: command.binding.attempt_id.clone(),
                command_sha256: super::super::writer::command_sha256(&command).unwrap(),
                authority: winbridge_protocol::update::StateBackupAuthority {
                    incarnation: authority.incarnation,
                    epoch: 8,
                    fingerprint_sha256: "b".repeat(64),
                },
                committed_at_ms: command.issued_at_ms + 1,
            },
        };
        (
            owned,
            WriterCommand {
                contract_version: WRITER_CONTRACT,
                command,
            },
            receipt,
        )
    }
    #[test]
    fn writer_v3_rejects_physical_v2_and_any_changed_command_or_receipt_scope() {
        let (owned, request, receipt) = writer_fixture();
        require_writer_receiver(
            &owned,
            &request.command.binding.physical.process,
            &owned.intent.attempt_id,
        )
        .unwrap();
        validate_writer_command(&request, &request.command).unwrap();
        validate_writer_receipt(&receipt, &request.command).unwrap();
        let mut changed = request.clone();
        changed.contract_version = HANDOFF_CONTRACT;
        assert!(validate_writer_command(&changed, &request.command).is_err());
        let mut changed = request.clone();
        changed.command.binding.backup_record_sha256 = "c".repeat(64);
        assert!(validate_writer_command(&changed, &request.command).is_err());
        let mut changed = receipt.clone();
        changed.contract_version = HANDOFF_CONTRACT;
        assert!(validate_writer_receipt(&changed, &request.command).is_err());
        let mut changed = receipt.clone();
        changed.receipt.authority.epoch += 1;
        assert!(validate_writer_receipt(&changed, &request.command).is_err());
        assert!(require_writer_receiver(
            &owned,
            &request.command.binding.physical.process,
            &uuid::Uuid::new_v4().to_string()
        )
        .is_err());
    }
    #[test]
    fn writer_private_frames_deny_unknown_fields_and_a_ready_prefix() {
        let (mut owned, request, receipt) = writer_fixture();
        owned.revision = 4;
        owned.state = ActivationState::ReadyForState {};
        assert!(require_writer_receiver(
            &owned,
            &request.command.binding.physical.process,
            &owned.intent.attempt_id
        )
        .is_err());
        let mut value = serde_json::to_value(request).unwrap();
        value["force"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<WriterCommand>(value).is_err());
        let mut value = serde_json::to_value(receipt).unwrap();
        value["receipt"]["startupPermit"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<WriterReceipt>(value).is_err());
    }
    fn fixture() -> (ActivationRecord, Challenge, Ready) {
        let (r, initial) = crate::activation::activation_fixture();
        let mut record = crate::activation::activation_stages(&initial, &r)[3].clone();
        record.intent.context.channel_nonce_sha256 = crate::sha256("a".repeat(64).as_bytes());
        let challenge = Challenge {
            contract_version: CONTRACT,
            update_id: record.intent.source.update_id.clone(),
            attempt_id: record.intent.attempt_id.clone(),
            nonce: "a".repeat(64),
        };
        let ready = Ready {
            contract_version: CONTRACT,
            ready: crate::activation::activation_stages(&initial, &r)[4]
                .ready
                .clone()
                .unwrap(),
        };
        (record, challenge, ready)
    }
    #[test]
    fn challenge_rejects_foreign_attempt_and_secret_without_echoing_it() {
        let (record, mut challenge, _) = fixture();
        validate_challenge(&challenge, &record).unwrap();
        challenge.attempt_id = uuid::Uuid::new_v4().to_string();
        assert!(validate_challenge(&challenge, &record).is_err());
        challenge.attempt_id = record.intent.attempt_id.clone();
        challenge.nonce = "b".repeat(64);
        let error = validate_challenge(&challenge, &record).unwrap_err();
        assert!(!error.contains(&challenge.nonce));
        assert!(serde_json::from_str::<Challenge>(r#"{"contractVersion":1,"updateId":"x","attemptId":"x","nonce":"x","writerGranted":true}"#).is_err());
    }
    #[test]
    fn ready_cannot_bind_another_process_or_config_or_phase() {
        let (mut record, _, mut ready) = fixture();
        validate_ready(&ready, &record).unwrap();
        let process = ready.ready.identity.process.clone();
        require_receiver(&record, &process, &record.intent.attempt_id).unwrap();
        ready.ready.identity.process.birth.push('0');
        assert!(validate_ready(&ready, &record).is_err());
        ready.ready.identity.process = process;
        ready.ready.worker_config_sha256 = "f".repeat(64);
        assert!(validate_ready(&ready, &record).is_err());
        record.state = ActivationState::Prepared {};
        assert!(require_receiver(
            &record,
            &ready.ready.identity.process,
            &record.intent.attempt_id
        )
        .is_err());
    }
    #[test]
    fn state_handoff_binds_the_entire_issued_prefix_and_keeps_v1_ready_separate() {
        let (r, initial) = crate::activation::activation_fixture();
        let stages = crate::activation::activation_stages(&initial, &r);
        let record = &stages[5];
        let message = state_handoff(record).unwrap();
        validate_state_handoff(&message, record).unwrap();
        for index in 0..5 {
            let mut bad = message.clone();
            match index {
                0 => bad.contract_version = CONTRACT,
                1 => bad.update_id = uuid::Uuid::new_v4().to_string(),
                2 => bad.attempt_id = uuid::Uuid::new_v4().to_string(),
                3 => bad.revision = 4,
                _ => bad.record_sha256 = "0".repeat(64),
            }
            assert!(validate_state_handoff(&bad, record).is_err());
        }
        assert!(state_handoff(&stages[4]).is_err());
        let mut changed = record.clone();
        changed.intent.context.previous_generation = uuid::Uuid::new_v4().to_string();
        assert!(validate_state_handoff(&message, &changed).is_err());
        let mut raw = serde_json::to_value(message).unwrap();
        raw["writerGranted"] = serde_json::json!(true);
        assert!(serde_json::from_value::<StateHandoff>(raw).is_err());
    }
    #[test]
    fn physical_state_receipt_rejects_foreign_scope_without_claiming_writer_authority() {
        let (r, initial) = crate::activation::activation_fixture();
        let stages = crate::activation::activation_stages(&initial, &r);
        let record = &stages[5];
        let message = StateOwned {
            contract_version: HANDOFF_CONTRACT,
            state_owned: stages[6].state_owned.clone().unwrap(),
        };
        validate_state_owned(&message, record).unwrap();
        for index in 0..7 {
            let mut bad = message.clone();
            match index {
                0 => bad.contract_version = CONTRACT,
                1 => bad.state_owned.state.lock_identity.push('0'),
                2 => bad.state_owned.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
                3 => bad.state_owned.process.birth.push('0'),
                4 => bad.state_owned.generation = record.intent.context.previous_generation.clone(),
                5 => bad.state_owned.generation = "not-a-generation".into(),
                _ => bad.state_owned.observed_at_ms = 0,
            }
            assert!(validate_state_owned(&bad, record).is_err());
        }
        let mut raw = serde_json::to_value(message).unwrap();
        raw["stateOwned"]["writerOwned"] = serde_json::json!(true);
        assert!(serde_json::from_value::<StateOwned>(raw).is_err());
    }
    #[tokio::test]
    async fn malformed_secret_message_never_appears_in_error() {
        use tokio::io::AsyncWriteExt;
        let (mut tx, mut rx) = tokio::io::duplex(1024);
        let secret = "do-not-echo-private-input";
        tx.write_u32_le(secret.len() as u32).await.unwrap();
        tx.write_all(secret.as_bytes()).await.unwrap();
        let error = read::<_, Challenge>(&mut rx, Duration::from_secs(1))
            .await
            .err()
            .unwrap();
        assert!(!error.contains(secret));
    }
    #[tokio::test]
    async fn invalid_frame_length_is_rejected_before_body_allocation_or_read() {
        use tokio::io::AsyncWriteExt;
        for length in [0, 32769, u32::MAX] {
            let (mut tx, mut rx) = tokio::io::duplex(64);
            tx.write_u32_le(length).await.unwrap();
            let error = read::<_, Challenge>(&mut rx, Duration::from_secs(1))
                .await
                .err()
                .unwrap();
            assert!(error.contains("frame must contain"), "{error}");
        }
    }
}
