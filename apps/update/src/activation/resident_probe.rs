//! Normal authenticated IPC only. Never read or resend the original V6 stream.
use super::{resident_observation as model, Result};
use std::{path::Path, time::Instant};
use winbridge_protocol::update::*;

pub(crate) async fn observe(
    query: &ActivationResidentQuery,
    entry: &WorkerLifetimeActivationReceipt,
    service: &WorkerServiceActivationCommand,
    topology: &WorkerControllerActivationTopology,
    deadline: Instant,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<ActivationResidentObservation> {
    model::validate_query(query)?;
    let physical = &service.binding.writer_command.binding.physical;
    let expected = &query.command.binding.compatibility;
    if physical.context != expected.context
        || physical.generation != expected.generation
        || physical.process != expected.identity.process
    {
        return Err("Resident normal endpoint differs from original child/service".into());
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
            .ok_or_else(|| {
                "Resident readonly observation timed out; no startup grant renewed".to_string()
            })
    };
    checkpoint()?;
    let observed: ActivationResidentObservation =
        winbridge_transport::supervisor::call_with_timeout(
            &endpoint,
            physical.process.process_id,
            query,
            remaining()?,
        )
        .await
        .map_err(|e| e.to_string())?;
    model::validate_observation(query, entry, topology, &observed)?;
    checkpoint()?;
    remaining()?;
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn metadata_does_not_replace_the_live_server_or_renew_a_budget() {
        let (graph, command, entry) = crate::activation::lifetime::test_fixture();
        let query = ActivationResidentQuery {
            resident_observation_version: 7,
            request_id: uuid::Uuid::new_v4().to_string(),
            command,
        };
        let topology = &graph.controller.command.binding.topology;
        let mut checks = 0;
        assert!(observe(
            &query,
            &entry,
            &graph.service.command,
            topology,
            Instant::now() + std::time::Duration::from_secs(1),
            || {
                checks += 1;
                Ok(())
            }
        )
        .await
        .is_err());
        assert_eq!(checks, 1);
        assert!(observe(
            &query,
            &entry,
            &graph.service.command,
            topology,
            Instant::now(),
            || Ok(())
        )
        .await
        .unwrap_err()
        .contains("no startup grant renewed"));
        let mut wrong = graph.service.command.clone();
        wrong
            .binding
            .writer_command
            .binding
            .physical
            .process
            .process_id += 1;
        assert!(observe(
            &query,
            &entry,
            &wrong,
            topology,
            Instant::now() + std::time::Duration::from_secs(1),
            || panic!("foreign child must fail before IO")
        )
        .await
        .unwrap_err()
        .contains("original child/service"));
    }
}
