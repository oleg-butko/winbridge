//! Data validators for current resident observation, never a finalization grant.
use super::{controller, lifetime, Result};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::{
    update::*, ConnectionState, ControllerLinkObservation, ControllerReadinessProjection,
};

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
}
pub fn validate_query(query: &ActivationResidentQuery) -> Result<()> {
    lifetime::validate_command_envelope(&query.command)?;
    if query.resident_observation_version != ACTIVATION_RESIDENT_OBSERVATION_CONTRACT
        || !uuid(&query.request_id)
        || query.request_id == query.command.command_id
    {
        return Err("Resident observation version/correlation differs".into());
    }
    Ok(())
}

/// Called only after the producer captured the accepted current Ticket/frame.
pub fn summarize_projection(
    projection: &ControllerReadinessProjection,
) -> Result<ActivationResidentFrame> {
    if !projection.valid() {
        return Err("Invalid resident source projection".into());
    }
    Ok(ActivationResidentFrame {
        schema_version: 1,
        controller_build: projection.controller_build.clone(),
        controller_generation: projection.controller_generation.clone(),
        session_id: projection.session_id.clone(),
        provider_build: projection.provider_build.clone(),
        provider_generation: projection.provider_generation.clone(),
        context: projection.context.clone(),
        sequence: projection.sequence,
        projection_sha256: controller::projection_sha256(projection)?,
    })
}

/// The producer must separately capture an accepted fresh Ticket under its links
/// lock. This validates the resulting data without manufacturing a V5 receipt.
pub fn validate_current_connection(
    command: &WorkerLifetimeActivationCommand,
    topology: &WorkerControllerActivationTopology,
    projection: &ActivationResidentFrame,
    link: &ControllerLinkObservation,
    observed_at_ms: u64,
) -> Result<()> {
    lifetime::validate_command_envelope(command)?;
    controller::validate_topology(topology)?;
    let endpoint = topology
        .controllers
        .iter()
        .find(|e| e.id == topology.confirmation_endpoint_id)
        .ok_or("Resident selected controller missing")?;
    let c = &link.connection;
    let b = &command.binding.compatibility;
    let attempts = link
        .attempts
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && n.to_string() == link.attempts);
    let node = c
        .controller_node_id
        .as_deref()
        .is_some_and(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control));
    let connected = link
        .last_connected_at_ms
        .ok_or("Resident connectedAt missing")?;
    let confirmed = c
        .last_confirmed_at_ms
        .ok_or("Resident heartbeat confirmation missing")?;
    if projection.schema_version != 1
        || projection.sequence == 0
        || projection.sequence > MAX_UPDATE_COUNTER
        || !valid_hash(&projection.projection_sha256, &[64])
        || !controller::valid_build(&projection.controller_build)
        || !uuid(&projection.controller_generation)
        || !uuid(&projection.session_id)
        || projection.context != b.context
        || projection.provider_generation != b.generation
        || projection.provider_build != b.identity.descriptor.build
        || link.id != endpoint.id
        || c.url != endpoint.url
        || !node
        || attempts.is_none()
        || !matches!(c.state, ConnectionState::Connected)
        || c.error.is_some()
        || c.controller_generation.as_ref() != Some(&projection.controller_generation)
        || c.session_id.as_ref() != Some(&projection.session_id)
        || observed_at_ms < b.observed_at_ms
        || observed_at_ms > MAX_UPDATE_COUNTER
        || connected > confirmed
        || confirmed > observed_at_ms
        || link
            .last_disconnected_at_ms
            .is_some_and(|at| at > connected)
    {
        return Err("Resident current frame/link/provider/selected controller differs".into());
    }
    Ok(())
}

pub fn validate_observation(
    query: &ActivationResidentQuery,
    expected_entry: &WorkerLifetimeActivationReceipt,
    topology: &WorkerControllerActivationTopology,
    observed: &ActivationResidentObservation,
) -> Result<()> {
    validate_query(query)?;
    lifetime::validate_receipt(expected_entry, &query.command)?;
    if observed.resident_observation_version != ACTIVATION_RESIDENT_OBSERVATION_CONTRACT
        || observed.request_id.as_ref() != Some(&query.request_id)
    {
        return Err("Resident response changed request/version; no fallback".into());
    }
    match &observed.result {
        ActivationResidentObservationResult::Observed {
            entry,
            frame,
            link,
            observed_at_ms,
        } => {
            if entry.as_ref() != expected_entry
                || *observed_at_ms < entry.compatibility.observed_at_ms
            {
                return Err("Resident response changed original supervised entry".into());
            }
            validate_current_connection(&query.command, topology, frame, link, *observed_at_ms)
        }
        ActivationResidentObservationResult::Unavailable { message }
        | ActivationResidentObservationResult::Rejected { message } => Err(message.clone()),
    }
}

#[cfg(test)]
mod tests;
