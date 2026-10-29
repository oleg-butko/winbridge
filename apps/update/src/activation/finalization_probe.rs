//! Read a late fact from the original normal IPC server. No mutation/release.
use super::{finalization_observation as model, Result};
use std::{path::Path, time::Instant};
use winbridge_protocol::update::*;

#[allow(dead_code)] // Consumed by the forthcoming physical V8 factory.
pub(crate) async fn observe(
    query: &ActivationFinalizationQuery,
    shutdown: &crate::shutdown::ShutdownRecord,
    service: &WorkerServiceActivationCommand,
    deadline: Instant,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<ActivationFinalizationObservation> {
    model::validate_query(query)?;
    let physical = &service.binding.writer_command.binding.physical;
    let expected = &query.command.binding.lifetime_command.binding.compatibility;
    if physical.context != expected.context
        || physical.generation != expected.generation
        || physical.process != expected.identity.process
    {
        return Err("Finalization normal endpoint differs from original child/service".into());
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
                "Finalization readonly observation timed out; no startup grant renewed".to_string()
            })
    };
    checkpoint()?;
    let observed = winbridge_transport::supervisor::call_with_timeout(
        &endpoint,
        physical.process.process_id,
        query,
        remaining()?,
    )
    .await
    .map_err(|e| e.to_string())?;
    model::validate_observation(query, shutdown, &observed)?;
    checkpoint()?;
    remaining()?;
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn saved_active_data_never_substitutes_for_live_ipc_or_renews_a_budget() {
        let f = crate::activation::finalization::test_fixture();
        let query = ActivationFinalizationQuery {
            finalization_observation_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: f.command,
        };
        let service = &f.snapshot.controller.service.command;
        let mut checks = 0;
        assert!(observe(
            &query,
            &f.shutdown,
            service,
            Instant::now() + std::time::Duration::from_secs(1),
            || {
                checks += 1;
                Ok(())
            }
        )
        .await
        .is_err());
        assert_eq!(checks, 1);
        assert!(
            observe(&query, &f.shutdown, service, Instant::now(), || Ok(()))
                .await
                .unwrap_err()
                .contains("no startup grant renewed")
        );
        let mut wrong = service.clone();
        wrong
            .binding
            .writer_command
            .binding
            .physical
            .process
            .process_id += 1;
        assert!(observe(
            &query,
            &f.shutdown,
            &wrong,
            Instant::now() + std::time::Duration::from_secs(1),
            || panic!("foreign child must fail before IO")
        )
        .await
        .unwrap_err()
        .contains("original child/service"));
    }
}
