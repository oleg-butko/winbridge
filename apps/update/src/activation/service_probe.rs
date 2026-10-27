//! Independent parent readback through the authenticated NORMAL worker endpoint.
use super::{service, Result};
use std::path::Path;
use winbridge_protocol::{update::*, LocalRequest, LocalResponse};

pub(crate) async fn probe(
    command: &WorkerServiceActivationCommand,
    signal: &WorkerServiceActivationReceipt,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<WorkerServiceActivationReceipt> {
    service::validate_receipt(signal, command)?;
    checkpoint()?;
    let physical = &command.binding.writer_command.binding.physical;
    let endpoint = winbridge_transport::endpoint(
        Path::new(&physical.state.state_directory),
        &physical.context.instance_id,
    )
    .map_err(|e| e.to_string())?;
    let pid = physical.process.process_id;
    let observed: ActivationServiceObservation = winbridge_transport::supervisor::call(
        &endpoint,
        pid,
        &ActivationServiceQuery {
            service_observation_version: 1,
            command: command.clone(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    if observed.service_observation_version != 1 {
        return Err("Normal IPC service observation contract differs".into());
    }
    let ActivationServiceObservationResult::Observed { receipt } = observed.result else {
        return Err("Normal IPC did not confirm the staged local service".into());
    };
    service::validate_receipt(&receipt, command)?;
    if receipt.compatibility.observed_at_ms < signal.compatibility.observed_at_ms {
        return Err("Normal IPC observation predates private Ready".into());
    }
    let info: LocalResponse =
        winbridge_transport::supervisor::call(&endpoint, pid, &LocalRequest::Info)
            .await
            .map_err(|e| e.to_string())?;
    let LocalResponse::Info { info } = info else {
        return Err("Normal IPC Info missing".into());
    };
    validate_info(command, &info)?;
    // Also exercise real journal handlers. The full pipeline fixture compares
    // expected historical content; an empty installation may have no tasks.
    let tasks: LocalResponse =
        winbridge_transport::supervisor::call(&endpoint, pid, &LocalRequest::ListTasks)
            .await
            .map_err(|e| e.to_string())?;
    let LocalResponse::Tasks { tasks } = tasks else {
        return Err("Normal IPC task history missing".into());
    };
    if let Some(task) = tasks.first() {
        let record: LocalResponse = winbridge_transport::supervisor::call(
            &endpoint,
            pid,
            &LocalRequest::GetTask {
                task_id: task.task_id.clone(),
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        if !matches!(record, LocalResponse::Task { task:ref r } if r.task_id==task.task_id && r.submission.project_id==task.project_id && r.status==task.status)
        {
            return Err("Normal IPC history detail differs".into());
        }
        let output: LocalResponse = winbridge_transport::supervisor::call(
            &endpoint,
            pid,
            &LocalRequest::ReadOutput {
                task_id: task.task_id.clone(),
                after_sequence: 0,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        if !matches!(output, LocalResponse::Output { .. }) {
            return Err("Normal IPC saved output unavailable".into());
        }
    }
    checkpoint()?;
    Ok(*receipt)
}

pub(crate) fn validate_info(
    command: &WorkerServiceActivationCommand,
    info: &winbridge_protocol::WorkerInfo,
) -> Result<()> {
    let b = &command.binding;
    let physical = &b.writer_command.binding.physical;
    let connection_valid = info.controller_connection.as_ref().is_none_or(|c| {
        matches!(c.state, winbridge_protocol::ConnectionState::Disconnected)
            && c.session_id.is_none()
            && c.last_confirmed_at_ms.is_none()
            && c.controller_node_id.is_none()
            && c.controller_generation.is_none()
    });
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
        || !connection_valid
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
