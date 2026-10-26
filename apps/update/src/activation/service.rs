//! Data binding only. Does not open a Store, start IPC, or construct a permit.
use super::{
    writer, ActivationRecord, ActivationState, ActivationWriterRecord, ActivationWriterState,
    Result,
};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::{update::*, ArtifactRole};
mod record;
pub use record::{ActivationServiceRecord, ActivationServiceState};

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .ok()
        .is_some_and(|id| id.to_string() == value)
}
fn digest(domain: &[u8], value: &impl serde::Serialize) -> Result<String> {
    let mut raw = domain.to_vec();
    raw.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(crate::sha256(&raw))
}

pub fn validate_command_envelope(command: &WorkerServiceActivationCommand) -> Result<()> {
    let b = &command.binding;
    let w = &b.writer_command;
    writer::validate_receipt(&b.writer_receipt, w)?;
    let identity = &b.expected_worker;
    let image = &identity.image;
    let d = &identity.descriptor;
    let build = &d.build;
    let text = |v: &str, limit: usize| {
        !v.is_empty() && v.len() <= limit && !v.chars().any(char::is_control)
    };
    let path = std::path::Path::new(&image.path);
    if command.schema_version != 1
        || !uuid(&command.command_id)
        || [
            &w.command_id,
            &w.binding.update_id,
            &w.binding.attempt_id,
            &w.binding.launch_id,
            &w.binding.updater_owner_id,
            &w.binding.physical.generation,
        ]
        .contains(&&command.command_id)
        || !valid_hash(&b.writer_record_sha256, &[64])
        || identity.schema_version != 1
        || identity.kind != "loaded_worker_identity"
        || identity.process != w.binding.physical.process
        || !text(&identity.runtime_identity, 128)
        || !uuid(&identity.set_id)
        || !valid_hash(&identity.manifest_sha256, &[64])
        || !path.is_absolute()
        || !text(&image.path, 4096)
        || path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
        || image.bytes == 0
        || image.bytes > 256 * 1024 * 1024
        || !text(&image.file_identity, 128)
        || !valid_hash(&image.sha256, &[64])
        || !image.query_supported
        || d.schema_version != 1
        || d.kind != "winbridge_artifact"
        || d.query_contract != 1
        || d.role != ArtifactRole::Worker
        || d.target_os != std::env::consts::OS
        || d.target_arch != std::env::consts::ARCH
        || build.source_dirty
        || !text(&build.version, 128)
        || !text(&build.build_id, 128)
        || build
            .source_commit
            .as_ref()
            .is_none_or(|v| !valid_hash(v, &[40, 64]))
        || build
            .source_manifest_sha256
            .as_ref()
            .is_none_or(|v| !valid_hash(v, &[64]))
        || b.worker_config_schema != 1
        || b.protocol_version != winbridge_protocol::PROTOCOL_VERSION
        || command.issued_at_ms < b.writer_receipt.committed_at_ms
        || command.issued_at_ms >= command.deadline_at_ms
        || command.deadline_at_ms != w.deadline_at_ms
        || command.deadline_at_ms > MAX_UPDATE_COUNTER
    {
        return Err("Service command writer/process/build/config/window binding invalid".into());
    }
    winbridge_platform::require_local_path_prefix(path).map_err(|e| e.to_string())
}

/// A pure proposal from already verified data. The future journal factory must
/// join its current owner/root, immutable physical prefix and original guard.
pub fn prepare_command(
    physical: &ActivationRecord,
    committed: &ActivationWriterRecord,
    key: &str,
    issued_at_ms: u64,
) -> Result<WorkerServiceActivationCommand> {
    committed.validate()?;
    let ActivationWriterState::Confirmed { receipt } = &committed.state else {
        return Err(
            "Service needs a confirmed journal writer, not an issued or uncertain delivery".into(),
        );
    };
    writer::validate_command(
        &committed.command,
        physical,
        &committed.command.binding.expected_authority,
    )?;
    let ready = physical
        .ready
        .as_ref()
        .ok_or("Service requires the original Ready identity")?;
    let ctx = &physical.intent.context;
    if physical.revision != 6
        || physical.state != (ActivationState::StateOwned {})
        || physical.local.is_some()
        || issued_at_ms < committed.updated_at_ms
        || issued_at_ms < physical.updated_at_ms
        || ready.identity.runtime_identity != physical.intent.source.runtime_identity
        || ready.identity.set_id != physical.intent.source.set_id
        || ready.identity.manifest_sha256 != physical.intent.source.manifest.sha256
        || ready.identity.image != physical.intent.source.worker
        || ready.identity.descriptor != physical.intent.source.worker_descriptor
        || ready.worker_config_sha256 != ctx.worker_config.sha256
    {
        return Err("Service proposal differs from current physical/writer/loaded identity".into());
    }
    let command = WorkerServiceActivationCommand {
        schema_version: 1,
        command_id: key.into(),
        issued_at_ms,
        deadline_at_ms: committed.command.deadline_at_ms,
        binding: WorkerServiceActivationBinding {
            writer_command: committed.command.clone(),
            writer_receipt: receipt.clone(),
            writer_record_sha256: digest(b"winbridge.activation.service-writer.v1\n", committed)?,
            expected_worker: ready.identity.clone(),
            worker_config_schema: ctx.worker_config_schema,
            protocol_version: ctx.protocol_version,
        },
    };
    validate_command_envelope(&command)?;
    Ok(command)
}

pub fn validate_command(
    command: &WorkerServiceActivationCommand,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
) -> Result<()> {
    if *command != prepare_command(physical, writer, &command.command_id, command.issued_at_ms)? {
        return Err(
            "Service command differs from immutable physical/confirmed writer binding".into(),
        );
    }
    Ok(())
}
pub fn command_sha256(command: &WorkerServiceActivationCommand) -> Result<String> {
    digest(b"winbridge.activation.service-command.v1\n", command)
}
pub fn validate_receipt(
    receipt: &WorkerServiceActivationReceipt,
    command: &WorkerServiceActivationCommand,
) -> Result<()> {
    validate_command_envelope(command)?;
    let b = &command.binding;
    let w = &b.writer_command.binding;
    let c = &receipt.compatibility;
    if receipt.schema_version != 1
        || receipt.command_id != command.command_id
        || receipt.update_id != w.update_id
        || receipt.attempt_id != w.attempt_id
        || receipt.command_sha256 != command_sha256(command)?
        || receipt.authority != b.writer_receipt.authority
        || c.schema_version != 1
        || c.kind != "worker_runtime_compatibility"
        || c.identity != b.expected_worker
        || c.context != w.physical.context
        || c.generation != w.physical.generation
        || c.worker_config_sha256 != w.worker_config_sha256
        || c.worker_config_schema != b.worker_config_schema
        || c.worker_journal_schema != w.worker_journal_schema
        || c.updater_journal_schema != w.updater_journal_schema
        || c.protocol_version != b.protocol_version
        || c.observed_at_ms < command.issued_at_ms
        || c.observed_at_ms > MAX_UPDATE_COUNTER
    {
        return Err(
            "Service receipt changed writer authority/epoch or actual local compatibility scope"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
