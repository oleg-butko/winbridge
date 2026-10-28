//! Independent observation over the actual normal IPC/PID, never a resend.
use super::{controller, service_probe, Result};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use winbridge_protocol::{update::*, LocalRequest, LocalResponse};

pub(crate) async fn probe(
    command: &WorkerControllerActivationCommand,
    signal: &WorkerControllerActivationReceipt,
    deadline: Instant,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<ActivationControllerObservation> {
    controller::validate_receipt(signal, command)?;
    let physical = &command
        .binding
        .service_command
        .binding
        .writer_command
        .binding
        .physical;
    let endpoint = winbridge_transport::endpoint(
        Path::new(&physical.state.state_directory),
        &physical.context.instance_id,
    )
    .map_err(|e| e.to_string())?;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .map(|d| d.min(winbridge_transport::IPC_TIMEOUT))
            .ok_or_else(|| "Original controller observation window expired".to_string())
    };
    loop {
        checkpoint()?;
        let observed: ActivationControllerObservation =
            winbridge_transport::supervisor::call_with_timeout(
                &endpoint,
                physical.process.process_id,
                &ActivationControllerQuery {
                    controller_observation_version: 1,
                    command: command.clone(),
                },
                remaining()?,
            )
            .await
            .map_err(|e| e.to_string())?;
        if observed.controller_observation_version != 1 {
            return Err("Normal controller observation version differs".into());
        }
        match &observed.result {
            ActivationControllerObservationResult::Observed {
                receipt,
                projection,
                link,
            } => {
                controller::validate_projection_evidence(receipt, command, projection)?;
                controller::validate_link_evidence(receipt, command, link)?;
                if receipt.compatibility.observed_at_ms < signal.compatibility.observed_at_ms {
                    return Err("Normal controller observation predates private Ready".into());
                }
                let info: LocalResponse = winbridge_transport::supervisor::call_with_timeout(
                    &endpoint,
                    physical.process.process_id,
                    &LocalRequest::Info,
                    remaining()?,
                )
                .await
                .map_err(|e| e.to_string())?;
                let LocalResponse::Info { info } = info else {
                    return Err("Normal controller Info missing".into());
                };
                service_probe::validate_readonly_info(&command.binding.service_command, &info)?;
                checkpoint()?;
                return Ok(observed);
            }
            ActivationControllerObservationResult::Rejected { message } => {
                return Err(message.clone())
            }
            ActivationControllerObservationResult::Waiting { .. } => {
                tokio::time::sleep(remaining()?.min(Duration::from_millis(100))).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn private_ready_alone_cannot_confirm_controller_without_normal_ipc_or_after_deadline() {
        let f = controller::test_fixture();
        let mut checkpoints = 0;
        let result = probe(
            &f.command,
            &f.receipt,
            Instant::now() + Duration::from_secs(1),
            || {
                checkpoints += 1;
                Ok(())
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            checkpoints, 1,
            "No independent normal IPC observation was obtained"
        );
        let result = probe(&f.command, &f.receipt, Instant::now(), || Ok(())).await;
        assert!(result.unwrap_err().contains("window expired"));
    }
}
