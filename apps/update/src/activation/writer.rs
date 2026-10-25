//! Metadata proposals only. No Store opening, migration, authority or OS grant.
use super::{ActivationRecord, ActivationState, Result};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::{
    StateBackupAuthority, WorkerJournalActivationBinding, WorkerJournalActivationCommand,
    WorkerJournalActivationReceipt,
};

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .ok()
        .is_some_and(|id| id.to_string() == value)
}
fn digest(domain: &[u8], value: &impl serde::Serialize) -> Result<String> {
    let mut bytes = domain.to_vec();
    bytes.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(crate::sha256(&bytes))
}
/// Intrinsic data checks for persisted commands/readback. The caller still
/// needs `validate_command` against the current graph and live physical scope.
pub fn validate_command_envelope(command: &WorkerJournalActivationCommand) -> Result<()> {
    let b = &command.binding;
    let p = &b.physical;
    let old = &b.expected_authority;
    let text = |value: &str, limit: usize| {
        !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
    };
    if command.schema_version != 1
        || ![
            &command.command_id,
            &b.update_id,
            &b.attempt_id,
            &b.launch_id,
            &b.updater_owner_id,
            &p.generation,
            &old.incarnation,
        ]
        .into_iter()
        .all(|id| uuid(id))
        || [
            &b.update_id,
            &b.attempt_id,
            &b.launch_id,
            &b.updater_owner_id,
            &p.generation,
        ]
        .contains(&&command.command_id)
        || b.updater_epoch == 0
        || b.updater_epoch > MAX_UPDATE_COUNTER
        || b.activation_revision != 6
        || b.worker_journal_schema == 0
        || b.updater_journal_schema == 0
        || ![
            &b.activation_record_sha256,
            &b.backup_record_sha256,
            &b.worker_config_sha256,
            &old.fingerprint_sha256,
        ]
        .into_iter()
        .all(|hash| valid_hash(hash, &[64]))
        || old.incarnation != p.context.journal_incarnation
        || old.epoch == 0
        || old.epoch >= MAX_UPDATE_COUNTER
        || !winbridge_core::config::valid_id(&p.context.instance_id)
        || p.attempt_id != b.attempt_id
        || p.process.process_id == 0
        || !valid_hash(&p.process.host_id, &[64])
        || p.process
            .birth
            .parse::<u64>()
            .ok()
            .filter(|at| *at > 0)
            .is_none_or(|at| at.to_string() != p.process.birth)
        || !text(&p.state.state_directory, 4096)
        || ![
            &p.state.directory_identity,
            &p.state.journal_identity,
            &p.state.lock_identity,
        ]
        .into_iter()
        .all(|id| text(id, 128))
        || p.state.directory_identity == p.state.journal_identity
        || p.state.directory_identity == p.state.lock_identity
        || p.state.journal_identity == p.state.lock_identity
        || p.observed_at_ms == 0
        || p.observed_at_ms > command.issued_at_ms
        || command.issued_at_ms == 0
        || command.issued_at_ms > command.deadline_at_ms
        || command.deadline_at_ms > MAX_UPDATE_COUNTER
        || command.deadline_at_ms - command.issued_at_ms > 300000
    {
        return Err("Writer command envelope scope/header/counters/window invalid".into());
    }
    Ok(())
}
/// The future trusted factory must fetch `expected` from the graph-bound final
/// backup in the same updater snapshot. This pure function cannot prove that
/// association, original custody or current owner liveness and gives no permit.
pub fn prepare_command(
    record: &ActivationRecord,
    expected: &StateBackupAuthority,
    command_id: &str,
    issued_at_ms: u64,
) -> Result<WorkerJournalActivationCommand> {
    let context = &record.intent.context;
    let source = &record.intent.source;
    let physical = record
        .state_owned
        .as_ref()
        .ok_or("Physical receipt missing")?;
    let ready = record.ready.as_ref().ok_or("Ready receipt missing")?;
    let created = record.created.as_ref().ok_or("Created receipt missing")?;
    let deadline = record
        .intent
        .admitted_at_ms
        .checked_add(context.startup_timeout_ms)
        .filter(|at| *at <= MAX_UPDATE_COUNTER)
        .ok_or("Writer proposal deadline exhausted")?;
    context.validate()?;
    if record.schema_version != 1
        || record.intent.schema_version != 1
        || record.state != (ActivationState::StateOwned {})
        || record.revision != 6
        || record.local.is_some()
        || record.handoff_issued_at_ms.is_none()
        || physical.attempt_id != record.intent.attempt_id
        || physical.process != created.process
        || physical.process != ready.identity.process
        || physical.context != context.worker_context
        || physical.state != context.worker_state
        || !uuid(&physical.generation)
        || physical.generation == context.previous_generation
        || physical.observed_at_ms > record.updated_at_ms
        || expected.incarnation != context.worker_context.journal_incarnation
        || !uuid(&expected.incarnation)
        || expected.epoch == 0
        || expected.epoch >= MAX_UPDATE_COUNTER
        || !valid_hash(&expected.fingerprint_sha256, &[64])
        || !uuid(command_id)
        || ![
            &source.update_id,
            &source.owner_id,
            &source.launch_id,
            &record.intent.attempt_id,
        ]
        .into_iter()
        .all(|id| uuid(id))
        || source.owner_epoch == 0
        || source.owner_epoch > MAX_UPDATE_COUNTER
        || !valid_hash(&source.backup_sha256, &[64])
        || [
            &source.update_id,
            &source.owner_id,
            &source.launch_id,
            &record.intent.attempt_id,
            &physical.generation,
        ]
        .into_iter()
        .any(|id| id == command_id)
        || issued_at_ms < record.updated_at_ms
        || issued_at_ms > deadline
    {
        return Err("Writer proposal physical scope/old authority/key/window differs".into());
    }
    let command = WorkerJournalActivationCommand {
        schema_version: 1,
        command_id: command_id.into(),
        binding: WorkerJournalActivationBinding {
            update_id: source.update_id.clone(),
            attempt_id: record.intent.attempt_id.clone(),
            launch_id: source.launch_id.clone(),
            updater_owner_id: source.owner_id.clone(),
            updater_epoch: source.owner_epoch,
            activation_revision: record.revision,
            activation_record_sha256: digest(b"winbridge.activation.writer-binding.v1\n", record)?,
            backup_record_sha256: source.backup_sha256.clone(),
            physical: physical.clone(),
            expected_authority: expected.clone(),
            worker_journal_schema: context.worker_journal_schema,
            updater_journal_schema: context.updater_journal_schema,
            worker_config_sha256: context.worker_config.sha256.clone(),
        },
        issued_at_ms,
        deadline_at_ms: deadline,
    };
    validate_command_envelope(&command)?;
    Ok(command)
}
pub fn validate_command(
    command: &WorkerJournalActivationCommand,
    current: &ActivationRecord,
    expected: &StateBackupAuthority,
) -> Result<()> {
    if command != &prepare_command(current, expected, &command.command_id, command.issued_at_ms)? {
        return Err("Writer command differs from the current state/backup/header binding".into());
    }
    Ok(())
}
pub fn command_sha256(command: &WorkerJournalActivationCommand) -> Result<String> {
    digest(b"winbridge.activation.writer-command.v1\n", command)
}
/// Structural/causal validation only. A durable receipt must additionally be
/// read back from the atomic worker transaction, not accepted as authority.
pub fn validate_receipt(
    receipt: &WorkerJournalActivationReceipt,
    command: &WorkerJournalActivationCommand,
) -> Result<()> {
    validate_command_envelope(command)?;
    let old = &command.binding.expected_authority;
    if command.schema_version != 1
        || !uuid(&command.command_id)
        || !uuid(&old.incarnation)
        || old.epoch == 0
        || old.epoch >= MAX_UPDATE_COUNTER
        || !valid_hash(&old.fingerprint_sha256, &[64])
        || receipt.schema_version != 1
        || receipt.command_id != command.command_id
        || receipt.update_id != command.binding.update_id
        || receipt.attempt_id != command.binding.attempt_id
        || receipt.command_sha256 != command_sha256(command)?
        || receipt.authority.incarnation != old.incarnation
        || old.epoch.checked_add(1) != Some(receipt.authority.epoch)
        || receipt.authority.epoch > MAX_UPDATE_COUNTER
        || !valid_hash(&receipt.authority.fingerprint_sha256, &[64])
        || receipt.authority.fingerprint_sha256 == old.fingerprint_sha256
        || receipt.committed_at_ms < command.issued_at_ms
        || receipt.committed_at_ms > MAX_UPDATE_COUNTER
    {
        return Err("Writer receipt command/new authority/epoch/chronology differs".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
