//! Independent current lifetime observation, never another private delivery.
use super::{lifetime, service_probe, Result};
use std::{path::Path, time::Instant};
use winbridge_protocol::{update::*, LocalRequest, LocalResponse};

pub(crate) async fn probe(
    command: &WorkerLifetimeActivationCommand,
    signal: &WorkerLifetimeActivationReceipt,
    service: &WorkerServiceActivationCommand,
    deadline: Instant,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<ActivationLifetimeObservation> {
    lifetime::validate_receipt(signal, command)?;
    let physical = &service.binding.writer_command.binding.physical;
    if physical.context != command.binding.compatibility.context
        || physical.generation != command.binding.compatibility.generation
    {
        return Err("Lifetime normal endpoint differs from original service".into());
    }
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
            .ok_or_else(|| "Original lifetime observation window expired".to_string())
    };
    checkpoint()?;
    let observed: ActivationLifetimeObservation =
        winbridge_transport::supervisor::call_with_timeout(
            &endpoint,
            physical.process.process_id,
            &ActivationLifetimeQuery {
                lifetime_observation_version: 1,
                command: command.clone(),
            },
            remaining()?,
        )
        .await
        .map_err(|e| e.to_string())?;
    validate_observation(&observed, command, signal)?;
    let info: LocalResponse = winbridge_transport::supervisor::call_with_timeout(
        &endpoint,
        physical.process.process_id,
        &LocalRequest::Info,
        remaining()?,
    )
    .await
    .map_err(|e| e.to_string())?;
    let LocalResponse::Info { info } = info else {
        return Err("Normal lifetime Info missing".into());
    };
    service_probe::validate_readonly_info(service, &info)?;
    checkpoint()?;
    remaining()?;
    Ok(observed)
}

pub(crate) fn validate_observation(
    observed: &ActivationLifetimeObservation,
    command: &WorkerLifetimeActivationCommand,
    signal: &WorkerLifetimeActivationReceipt,
) -> Result<()> {
    if observed.lifetime_observation_version != 1 {
        return Err("Normal lifetime observation version differs".into());
    }
    match &observed.result {
        ActivationLifetimeObservationResult::Observed {
            receipt,
            observed_at_ms,
        } => {
            lifetime::validate_receipt(receipt, command)?;
            if receipt.as_ref() != signal
                || *observed_at_ms < receipt.compatibility.observed_at_ms
                || *observed_at_ms > winbridge_core::update::MAX_UPDATE_COUNTER
            {
                return Err(
                    "Normal lifetime observation changed entry receipt or current time".into(),
                );
            }
            Ok(())
        }
        ActivationLifetimeObservationResult::Rejected { message } => Err(message.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn private_ready_cannot_replace_real_normal_ipc_or_renew_a_deadline() {
        let (graph, command, receipt) = lifetime::test_fixture();
        let mut calls = 0;
        assert!(probe(
            &command,
            &receipt,
            &graph.service.command,
            Instant::now() + std::time::Duration::from_secs(1),
            || {
                calls += 1;
                Ok(())
            }
        )
        .await
        .is_err());
        assert_eq!(calls, 1);
        assert!(probe(
            &command,
            &receipt,
            &graph.service.command,
            Instant::now(),
            || Ok(())
        )
        .await
        .unwrap_err()
        .contains("window expired"));
    }
    #[test]
    fn current_time_is_separate_from_the_immutable_entry_window_and_receipt() {
        let (_, command, receipt) = lifetime::test_fixture();
        let observed = ActivationLifetimeObservation {
            lifetime_observation_version: 1,
            result: ActivationLifetimeObservationResult::Observed {
                receipt: Box::new(receipt.clone()),
                observed_at_ms: command.deadline_at_ms + 1,
            },
        };
        validate_observation(&observed, &command, &receipt).unwrap();
        let mut changed = observed.clone();
        let ActivationLifetimeObservationResult::Observed {
            receipt: signal, ..
        } = &mut changed.result
        else {
            panic!("observed")
        };
        signal.compatibility.observed_at_ms += 1;
        assert!(validate_observation(&changed, &command, &receipt).is_err());
        let mut changed = observed;
        let ActivationLifetimeObservationResult::Observed { observed_at_ms, .. } =
            &mut changed.result
        else {
            panic!("observed")
        };
        *observed_at_ms = receipt.compatibility.observed_at_ms - 1;
        assert!(validate_observation(&changed, &command, &receipt).is_err());
    }
}
