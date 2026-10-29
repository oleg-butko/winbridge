use super::*;

/// Shared readonly/runtime/maintenance invariants. A later controller phase
/// validates its selected link separately; local-only V4 retains its stricter
/// no-connection check above.
pub fn validate_readonly_info(
    command: &WorkerServiceActivationCommand,
    info: &winbridge_protocol::WorkerInfo,
) -> Result<()> {
    let b = &command.binding;
    let physical = &b.writer_command.binding.physical;
    if info.instance_id != physical.context.instance_id
        || info.generation != physical.generation
        || info.platform != b.expected_worker.descriptor.target_os
        || info.build.as_ref() != Some(&b.expected_worker.descriptor.build)
        || info.protocol_version != b.protocol_version
        || info.journal_inspection.is_some()
        || info.journal_recovery_required.is_some()
        || info.maintenance_error.is_some()
        || info.active_task_count != Some(0)
        || info.resource_recovery.is_some()
        || (info.platform == "windows"
            && !info.recovery_coverage.as_ref().is_some_and(|r| r.complete))
        || info.maintenance_available
        || info.snapshot_import_available
        || info.snapshot_delta_available
        || info.snapshot_resume_available
        || info.workflow_available
        || !info.fenced_requests.as_ref().is_some_and(|f| {
            f.context == physical.context
                && !f.legacy_mutations_allowed
                && f.request_contract_version == winbridge_protocol::FENCED_REQUEST_CONTRACT
        })
    {
        return Err("Normal IPC Info changed expected readonly runtime/context/generation/maintenance scope".into());
    }
    let lease = info
        .update_coordination
        .as_ref()
        .ok_or("Normal IPC maintenance lease missing")?;
    if lease.get("readOnly") != Some(&serde_json::json!(true))
        || lease["lease"]["spec"]["updateId"] != command.binding.writer_command.binding.update_id
        || lease["lease"]["phase"] != "quiesced"
    {
        return Err("Normal IPC maintenance no longer quiesced/readonly".into());
    }
    Ok(())
}
