//! Data-only controller-session proposals. No connection, owner or commit grant.
use super::{service, ActivationRecord, ActivationState, ActivationWriterRecord, Result};
use service::{ActivationServiceRecord, ActivationServiceState};
use winbridge_core::{
    config::WorkerConfig,
    controller_sessions,
    update::{valid_hash, MAX_UPDATE_COUNTER},
};
use winbridge_protocol::{
    update::*, BuildIdentity, ConnectionState, ControllerLinkObservation,
    ControllerReadinessProjection,
};
mod record;
pub use record::{ActivationControllerRecord, ActivationControllerState};

/// Fresh admission only, never reconstructed from journal JSON or an Existing
/// result. It authorizes one consuming preparation, not a cold connection.
pub struct ActivationControllerPermit {
    pub(crate) record: ActivationControllerRecord,
}
impl ActivationControllerPermit {
    pub fn record(&self) -> &ActivationControllerRecord {
        &self.record
    }
}
pub enum ActivationControllerAdmission {
    Issued(Box<ActivationControllerPermit>),
    Existing(Box<ActivationControllerRecord>),
}

/// Validated data for the already authenticated resident child. It does not
/// recreate custody, permission, the startup window or a network manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationControllerSnapshot {
    pub physical: ActivationRecord,
    pub writer: ActivationWriterRecord,
    pub service: ActivationServiceRecord,
    pub local: ActivationRecord,
    pub controller: ActivationControllerRecord,
}

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
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

pub fn validate_topology(topology: &WorkerControllerActivationTopology) -> Result<()> {
    let configured = controller_sessions::Topology {
        schema_version: topology.schema_version,
        primary: topology.primary.clone(),
        controllers: topology
            .controllers
            .iter()
            .map(|p| controller_sessions::Endpoint {
                id: p.id.clone(),
                url: p.url.clone(),
            })
            .collect(),
    };
    configured.validate()?;
    if !configured
        .controllers
        .iter()
        .any(|p| p.id == topology.confirmation_endpoint_id)
    {
        return Err("Confirmation endpoint is not in the frozen configured topology".into());
    }
    Ok(())
}

/// Derive the proposal from checked config; explicit selection must still be a
/// configured endpoint. The future original guard must pin the config SHA too.
pub fn topology_from_config(
    config: &WorkerConfig,
    selection: Option<&str>,
) -> Result<WorkerControllerActivationTopology> {
    let configured = config
        .worker
        .topology()?
        .ok_or("Controller activation requires configured endpoints")?;
    let result = WorkerControllerActivationTopology {
        schema_version: 1,
        confirmation_endpoint_id: selection.unwrap_or(&configured.primary).into(),
        primary: configured.primary,
        controllers: configured
            .controllers
            .into_iter()
            .map(|p| WorkerControllerActivationEndpoint {
                id: p.id,
                url: p.url,
            })
            .collect(),
    };
    validate_topology(&result)?;
    Ok(result)
}

pub fn validate_command_envelope(command: &WorkerControllerActivationCommand) -> Result<()> {
    let b = &command.binding;
    service::validate_receipt(&b.service_receipt, &b.service_command)?;
    validate_topology(&b.topology)?;
    let w = &b.service_command.binding.writer_command;
    if command.schema_version != 1
        || !uuid(&command.command_id)
        || [
            &b.service_command.command_id,
            &w.command_id,
            &w.binding.update_id,
            &w.binding.attempt_id,
            &w.binding.launch_id,
            &w.binding.updater_owner_id,
            &w.binding.physical.generation,
        ]
        .contains(&&command.command_id)
        || !valid_hash(&b.service_record_sha256, &[64])
        || !valid_hash(&b.local_record_sha256, &[64])
        || command.issued_at_ms < b.service_receipt.compatibility.observed_at_ms
        || command.issued_at_ms >= command.deadline_at_ms
        || command.deadline_at_ms != b.service_command.deadline_at_ms
        || command.deadline_at_ms > MAX_UPDATE_COUNTER
    {
        return Err(
            "Controller command service/key/topology/original window binding invalid".into(),
        );
    }
    Ok(())
}

/// A proposal over the four exact frozen records. No physical prefix is rebuilt
/// from a later record, and no owner or permission is reconstructed from data.
pub fn prepare_command(
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
    service: &ActivationServiceRecord,
    local: &ActivationRecord,
    topology: WorkerControllerActivationTopology,
    key: &str,
    issued_at_ms: u64,
) -> Result<WorkerControllerActivationCommand> {
    service.validate()?;
    service::validate_command(&service.command, physical, writer)?;
    let ActivationServiceState::Confirmed { receipt } = &service.state else {
        return Err(
            "Controller connection requires confirmed service, not an uncertain delivery".into(),
        );
    };
    let mut expected = physical.clone();
    expected.state = ActivationState::LocalVerified {};
    expected.revision = 7;
    expected.updated_at_ms = local.updated_at_ms;
    expected.local = Some(receipt.compatibility.clone());
    if *local != expected
        || local.updated_at_ms < physical.updated_at_ms
        || local.updated_at_ms < receipt.compatibility.observed_at_ms
        || issued_at_ms < local.updated_at_ms
        || issued_at_ms < service.updated_at_ms
    {
        return Err(
            "Controller proposal changed the exact LocalVerified/service/physical graph".into(),
        );
    }
    let command = WorkerControllerActivationCommand {
        schema_version: 1,
        command_id: key.into(),
        issued_at_ms,
        deadline_at_ms: service.command.deadline_at_ms,
        binding: WorkerControllerActivationBinding {
            service_command: service.command.clone(),
            service_receipt: receipt.as_ref().clone(),
            service_record_sha256: digest(
                b"winbridge.activation.controller-service.v1\n",
                service,
            )?,
            local_record_sha256: digest(b"winbridge.activation.controller-local.v1\n", local)?,
            topology,
        },
    };
    validate_command_envelope(&command)?;
    Ok(command)
}

pub fn validate_command(
    command: &WorkerControllerActivationCommand,
    physical: &ActivationRecord,
    writer: &ActivationWriterRecord,
    service: &ActivationServiceRecord,
    local: &ActivationRecord,
    expected_topology: &WorkerControllerActivationTopology,
) -> Result<()> {
    if *command
        != prepare_command(
            physical,
            writer,
            service,
            local,
            expected_topology.clone(),
            &command.command_id,
            command.issued_at_ms,
        )?
    {
        return Err("Controller command differs from frozen graph/config selection".into());
    }
    Ok(())
}
pub fn command_sha256(command: &WorkerControllerActivationCommand) -> Result<String> {
    digest(b"winbridge.activation.controller-command.v1\n", command)
}

fn valid_build(build: &BuildIdentity) -> bool {
    let text = |s: &str| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control);
    text(&build.version)
        && text(&build.build_id)
        && build
            .source_commit
            .as_ref()
            .is_none_or(|v| valid_hash(v, &[40, 64]))
        && build
            .source_manifest_sha256
            .as_ref()
            .is_none_or(|v| valid_hash(v, &[64]))
}

/// Data validation only. Original resident owners must separately prove current
/// ticket/freshness and liveness; a late fact never grants a retry or commit.
pub fn validate_receipt(
    receipt: &WorkerControllerActivationReceipt,
    command: &WorkerControllerActivationCommand,
) -> Result<()> {
    validate_command_envelope(command)?;
    let b = &command.binding;
    let s = &b.service_receipt;
    let w = &b.service_command.binding.writer_command.binding;
    let observed = &receipt.compatibility;
    let mut expected = s.compatibility.clone();
    expected.observed_at_ms = observed.observed_at_ms;
    let peer = &receipt.session;
    let endpoint = b
        .topology
        .controllers
        .iter()
        .find(|p| p.id == b.topology.confirmation_endpoint_id)
        .ok_or("Missing selected controller")?;
    if receipt.schema_version != 1
        || receipt.command_id != command.command_id
        || receipt.update_id != w.update_id
        || receipt.attempt_id != w.attempt_id
        || receipt.command_sha256 != command_sha256(command)?
        || receipt.authority != s.authority
        || *observed != expected
        || observed.observed_at_ms < command.issued_at_ms
        || observed.observed_at_ms > MAX_UPDATE_COUNTER
        || peer.controller_id != endpoint.id
        || peer.controller_url != endpoint.url
        || !winbridge_core::config::valid_id(&peer.controller_node_id)
        || !uuid(&peer.controller_generation)
        || !uuid(&peer.session_id)
        || !valid_build(&peer.controller_build)
        || peer.worker_context != observed.context
        || peer.worker_generation != observed.generation
        || peer.worker_build != observed.identity.descriptor.build
        || !(1..=MAX_UPDATE_COUNTER).contains(&peer.connection_attempt)
        || !(1..=MAX_UPDATE_COUNTER).contains(&peer.projection_sequence)
        || !valid_hash(&peer.projection_sha256, &[64])
        || peer.connected_at_ms < command.issued_at_ms
        || peer.connected_at_ms >= command.deadline_at_ms
        || peer.connected_at_ms > observed.observed_at_ms
    {
        return Err(
            "Controller receipt changed selected session/child/authority/config/window scope"
                .into(),
        );
    }
    Ok(())
}

pub fn projection_sha256(projection: &ControllerReadinessProjection) -> Result<String> {
    digest(
        b"winbridge.activation.controller-projection.v1\n",
        projection,
    )
}

/// Independently bind the captured full projection to the current normal-IPC
/// link observation, including the Welcome node ID and reconnect attempt.
pub fn validate_link_evidence(
    receipt: &WorkerControllerActivationReceipt,
    command: &WorkerControllerActivationCommand,
    link: &ControllerLinkObservation,
) -> Result<()> {
    validate_receipt(receipt, command)?;
    let s = &receipt.session;
    let c = &link.connection;
    if link.id != s.controller_id
        || link.attempts != s.connection_attempt.to_string()
        || !matches!(c.state, ConnectionState::Connected)
        || c.error.is_some()
        || c.url != s.controller_url
        || c.controller_node_id.as_ref() != Some(&s.controller_node_id)
        || c.controller_generation.as_ref() != Some(&s.controller_generation)
        || c.session_id.as_ref() != Some(&s.session_id)
        || link.last_connected_at_ms != Some(s.connected_at_ms)
        || !c
            .last_confirmed_at_ms
            .is_some_and(|at| s.connected_at_ms <= at && at <= MAX_UPDATE_COUNTER)
    {
        return Err(
            "Controller link differs from the current selected session/Welcome/attempt".into(),
        );
    }
    Ok(())
}

/// Compare the separately captured full validated frame, not just connected=true
/// or a historical readiness view. A runtime still needs its current Ticket.
pub fn validate_projection_evidence(
    receipt: &WorkerControllerActivationReceipt,
    command: &WorkerControllerActivationCommand,
    projection: &ControllerReadinessProjection,
) -> Result<()> {
    validate_receipt(receipt, command)?;
    let s = &receipt.session;
    if !projection.valid()
        || projection.controller_build != s.controller_build
        || projection.controller_generation != s.controller_generation
        || projection.session_id != s.session_id
        || projection.context != s.worker_context
        || projection.provider_generation != s.worker_generation
        || projection.provider_build != s.worker_build
        || projection.sequence != s.projection_sequence
        || projection_sha256(projection)? != s.projection_sha256
    {
        return Err(
            "Controller projection differs from the actual selected session/provider frame".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::{fixture as test_fixture, Fixture as TestFixture};
