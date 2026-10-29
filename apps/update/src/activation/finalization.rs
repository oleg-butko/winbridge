//! Pure finalization FSM. These data do not mint live permits, commit the root,
//! release a lease, reopen a Store or turn a worker Active.
use super::{lifetime, resident_observation, Result};
use lifetime::{ActivationLifetimeSnapshot, ActivationLifetimeState};
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, validate_update_request, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::*;

/// Fresh capture at admission, enforced again by a future monotonic owner.
/// This is not a replacement startup window or a delivery retry timeout.
pub const MAX_OBSERVATION_AGE_MS: u64 = 10_000;
pub const RELEASE_REASON: &str =
    "Обновление WinBridge завершено; maintenance освобождён для этого обновления";

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
}
fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let mut bytes = domain.to_vec();
    bytes.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(crate::sha256(&bytes))
}
pub fn command_sha256(command: &WorkerFinalizationCommand) -> Result<String> {
    digest(b"winbridge.activation.finalization-command.v1\n", command)
}
pub fn validate_command_envelope(command: &WorkerFinalizationCommand) -> Result<()> {
    let b = &command.binding;
    let old = &b.lifetime_command;
    lifetime::validate_command_envelope(old)?;
    if command.schema_version != 1
        || !uuid(&command.command_id)
        || !uuid(&b.resident_request_id)
        || !uuid(&b.installation_id)
        || [
            &old.command_id,
            &old.binding.update_id,
            &old.binding.attempt_id,
            &old.binding.launch_id,
            &old.binding.updater_owner_id,
            &old.binding.controller_command_id,
            &old.binding.compatibility.generation,
            &b.installation_id,
            &b.resident_request_id,
        ]
        .contains(&&command.command_id)
        || b.resident_request_id == old.command_id
        || ![
            &b.lifetime_record_sha256,
            &b.shutdown_record_sha256,
            &b.scope_sha256,
            &b.source_manifest_sha256,
            &b.resident_observation_sha256,
        ]
        .into_iter()
        .all(|v| valid_hash(v, &[64]))
        || old.binding.root_revision >= MAX_UPDATE_COUNTER
        || b.committed_root_revision != old.binding.root_revision + 1
        || b.expected_lease_revision == 0
        || b.expected_lease_revision >= MAX_UPDATE_COUNTER
        || command.issued_at_ms < old.issued_at_ms
        || command.issued_at_ms > MAX_UPDATE_COUNTER
    {
        return Err("Finalization key/scope/causal revision/time invalid".into());
    }
    Ok(())
}

/// Data proposal against exact retained history plus one CURRENT observation.
/// Only a future consuming original owner may admit it as a durable decision.
#[allow(clippy::too_many_arguments)]
pub fn prepare_command(
    snapshot: &ActivationLifetimeSnapshot,
    root: &UpdateRecord,
    shutdown: &crate::shutdown::ShutdownRecord,
    query: &ActivationResidentQuery,
    observed: &ActivationResidentObservation,
    key: &str,
    at: u64,
) -> Result<WorkerFinalizationCommand> {
    snapshot.lifetime.validate()?;
    lifetime::validate_command(&snapshot.lifetime.command, &snapshot.controller)?;
    let ActivationLifetimeState::Confirmed { receipt } = &snapshot.lifetime.state else {
        return Err("Finalization requires independently confirmed lifetime history".into());
    };
    let topology = &snapshot.controller.controller.command.binding.topology;
    if query.command != snapshot.lifetime.command {
        return Err("Finalization query changed original lifetime command".into());
    }
    resident_observation::validate_observation(query, receipt, topology, observed)?;
    let ActivationResidentObservationResult::Observed { observed_at_ms, .. } = &observed.result
    else {
        return Err("No current resident connection for finalization".into());
    };
    let source = &snapshot.controller.local.intent.source;
    let binding = &snapshot.lifetime.command.binding;
    validate_update_request(&root.request)?;
    shutdown.validate()?;
    let crate::shutdown::ShutdownDeliveryState::Exited {
        receipt: shutdown_receipt,
        at_ms: exit_at,
        ..
    } = &shutdown.delivery
    else {
        return Err("Finalization requires the retained exact old-worker exit".into());
    };
    let shutdown_record_sha256 = crate::runtime_baseline::digest(shutdown)?;
    let old_build = &shutdown.intent.binding.worker.build;
    let target_build = &binding.compatibility.identity.descriptor.build;
    let shutdown = shutdown_receipt.as_ref();
    let spec = &shutdown.lease.spec;
    if root.phase != UpdatePhase::AwaitingController
        || root.update_id != binding.update_id
        || root.revision != binding.root_revision
        || root.request.expected.installation_id != source.installation_id
        || root.request.expected.scope_sha256 != source.scope_sha256
        || root.request.expected.worker_build_id != old_build.build_id
        || target_build.source_dirty
        || target_build.source_commit.as_ref() != Some(&root.request.source.source_commit)
        || target_build.source_manifest_sha256.as_ref()
            != Some(&root.request.source.manifest_sha256)
        || root.request.source.manifest_sha256 != spec.target_manifest_sha256
        || shutdown_record_sha256 != source.shutdown_sha256
        || *exit_at > at
        || shutdown.process != snapshot.controller.physical.intent.context.previous_worker
        || shutdown.state != snapshot.controller.physical.intent.context.worker_state
        || spec.worker_generation
            != snapshot
                .controller
                .physical
                .intent
                .context
                .previous_generation
        || spec.baseline_manifest_sha256 != root.request.expected.baseline_manifest_sha256
        || root.request.expected.worker_config_sha256 != binding.compatibility.worker_config_sha256
        || shutdown.schema_version != 1
        || shutdown.update_id != binding.update_id
        || shutdown.launch_id != binding.launch_id
        || shutdown.updater_owner_id != binding.updater_owner_id
        || shutdown.updater_epoch != binding.updater_epoch
        || shutdown.lease.phase != WorkerUpdatePhase::Quiesced
        || shutdown.lease.revision != shutdown.command.expected_lease_revision
        || spec.update_id != binding.update_id
        || spec.updater_owner_id != binding.updater_owner_id
        || spec.updater_epoch != binding.updater_epoch
        || spec.instance_id != binding.compatibility.context.instance_id
        || spec.journal_incarnation != binding.compatibility.context.journal_incarnation
        || shutdown.accepted_at_ms < shutdown.lease.updated_at_ms
        || shutdown.lease.updated_at_ms < shutdown.lease.accepted_at_ms
        || at < shutdown.accepted_at_ms
        || at < root.updated_at_ms
        || root.updated_at_ms < root.accepted_at_ms
        || at < snapshot.lifetime.updated_at_ms
        || at < *observed_at_ms
        || at - observed_at_ms > MAX_OBSERVATION_AGE_MS
        || [
            snapshot.controller.writer.command.command_id.as_str(),
            snapshot.controller.service.command.command_id.as_str(),
            shutdown.command.command_id.as_str(),
        ]
        .contains(&key)
    {
        return Err("Finalization requires current connection and exact AwaitingController/local7/shutdown scope".into());
    }
    let command = WorkerFinalizationCommand {
        schema_version: 1,
        command_id: key.into(),
        issued_at_ms: at,
        binding: ActivationFinalizationBinding {
            lifetime_command: snapshot.lifetime.command.clone(),
            lifetime_record_sha256: digest(
                b"winbridge.activation.finalization-lifetime.v1\n",
                &snapshot.lifetime,
            )?,
            shutdown_record_sha256,
            installation_id: source.installation_id.clone(),
            scope_sha256: source.scope_sha256.clone(),
            committed_root_revision: root
                .revision
                .checked_add(1)
                .ok_or("Finalization root revision exhausted")?,
            expected_lease_revision: shutdown.lease.revision,
            source_manifest_sha256: root.request.source.manifest_sha256.clone(),
            resident_request_id: query.request_id.clone(),
            resident_observation_sha256: digest(
                b"winbridge.activation.finalization-observation.v1\n",
                observed,
            )?,
        },
    };
    validate_command_envelope(&command)?;
    Ok(command)
}

/// Check age from the ORIGINAL current observation, not from a later proposal
/// timestamp. The physical factory must additionally retain a monotonic bound.
pub fn validate_admission_time(
    command: &WorkerFinalizationCommand,
    observed: &ActivationResidentObservation,
    now: u64,
) -> Result<()> {
    validate_command_envelope(command)?;
    let ActivationResidentObservationResult::Observed { observed_at_ms, .. } = &observed.result
    else {
        return Err("Finalization observation unavailable".into());
    };
    if now < command.issued_at_ms
        || now > MAX_UPDATE_COUNTER
        || *observed_at_ms > command.issued_at_ms
        || now < *observed_at_ms
        || now - observed_at_ms > MAX_OBSERVATION_AGE_MS
    {
        return Err("Finalization original observation expired or clock moved backwards".into());
    }
    Ok(())
}

pub fn validate_command(
    command: &WorkerFinalizationCommand,
    snapshot: &ActivationLifetimeSnapshot,
    root: &UpdateRecord,
    shutdown: &crate::shutdown::ShutdownRecord,
    query: &ActivationResidentQuery,
    observed: &ActivationResidentObservation,
) -> Result<()> {
    if *command
        != prepare_command(
            snapshot,
            root,
            shutdown,
            query,
            observed,
            &command.command_id,
            command.issued_at_ms,
        )?
    {
        return Err(
            "Finalization command detached from original causal graph/current observation".into(),
        );
    }
    Ok(())
}

/// Derive the exact existing worker journal command. The generic post-shutdown
/// Release path stays closed; a future trusted receiver must own the same Store.
pub fn release_command(command: &WorkerFinalizationCommand) -> Result<WorkerUpdateCommand> {
    validate_command_envelope(command)?;
    Ok(WorkerUpdateCommand {
        command_id: command.command_id.clone(),
        expected_revision: command.binding.expected_lease_revision,
        action: WorkerUpdateAction::Release,
        source_manifest_sha256: Some(command.binding.source_manifest_sha256.clone()),
        reason: RELEASE_REASON.into(),
    })
}

pub fn validate_receipt(
    receipt: &WorkerFinalizationReceipt,
    command: &WorkerFinalizationCommand,
    shutdown: &crate::shutdown::ShutdownRecord,
) -> Result<()> {
    validate_command_envelope(command)?;
    shutdown.validate()?;
    if crate::runtime_baseline::digest(shutdown)? != command.binding.shutdown_record_sha256 {
        return Err("Finalization receipt uses another shutdown prefix".into());
    }
    let crate::shutdown::ShutdownDeliveryState::Exited {
        receipt: shutdown, ..
    } = &shutdown.delivery
    else {
        return Err("Finalization requires original old-worker exit".into());
    };
    validate_receipt_envelope(receipt, command)?;
    let mut lease = shutdown.lease.clone();
    lease.phase = WorkerUpdatePhase::Released;
    lease.revision = command.binding.expected_lease_revision + 1;
    lease.updated_at_ms = receipt.released_at_ms;
    lease.reason = RELEASE_REASON.into();
    if receipt.released_lease != lease {
        return Err("Finalization receipt changed exact original shutdown lease".into());
    }
    Ok(())
}

fn validate_receipt_envelope(
    receipt: &WorkerFinalizationReceipt,
    command: &WorkerFinalizationCommand,
) -> Result<()> {
    validate_command_envelope(command)?;
    let b = &command.binding.lifetime_command.binding;
    let mut expected = b.compatibility.clone();
    expected.observed_at_ms = receipt.compatibility.observed_at_ms;
    let lease = &receipt.released_lease;
    let spec = &lease.spec;
    if receipt.schema_version != 1
        || receipt.command_id != command.command_id
        || receipt.command_sha256 != command_sha256(command)?
        || receipt.compatibility != expected
        || receipt.authority != b.authority
        || receipt.mode != WorkerFinalizationMode::Active
        || lease.phase != WorkerUpdatePhase::Released
        || lease.revision != command.binding.expected_lease_revision + 1
        || lease.updated_at_ms != receipt.released_at_ms
        || lease.reason != RELEASE_REASON
        || spec.update_id != b.update_id
        || spec.instance_id != b.compatibility.context.instance_id
        || spec.journal_incarnation != b.compatibility.context.journal_incarnation
        || spec.updater_owner_id != b.updater_owner_id
        || spec.updater_epoch != b.updater_epoch
        || spec.target_manifest_sha256 != command.binding.source_manifest_sha256
        || receipt.released_at_ms < command.issued_at_ms
        || receipt.compatibility.observed_at_ms < receipt.released_at_ms
        || receipt.compatibility.observed_at_ms > MAX_UPDATE_COUNTER
    {
        return Err(
            "Finalization receipt changed same worker/authority/exact release/Active fact".into(),
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivationFinalizationState {
    ReleasePending {},
    Indeterminate {
        reason: String,
    },
    Confirmed {
        receipt: Box<WorkerFinalizationReceipt>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationFinalizationRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub command: WorkerFinalizationCommand,
    pub state: ActivationFinalizationState,
    pub updated_at_ms: u64,
}
impl ActivationFinalizationRecord {
    /// Intended to be persisted atomically with root Committed. Data alone do
    /// not perform that transaction or authorize a release delivery.
    pub fn pending(command: WorkerFinalizationCommand) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: command.issued_at_ms,
            command,
            state: ActivationFinalizationState::ReleasePending {},
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        validate_command_envelope(&self.command)?;
        if self.schema_version != 1
            || self.updated_at_ms < self.command.issued_at_ms
            || self.updated_at_ms > MAX_UPDATE_COUNTER
        {
            return Err("Finalization record schema/time differs".into());
        }
        match &self.state {
            ActivationFinalizationState::ReleasePending {}
                if self.revision == 0 && self.updated_at_ms == self.command.issued_at_ms =>
            {
                Ok(())
            }
            ActivationFinalizationState::Indeterminate { reason }
                if self.revision == 1
                    && !reason.is_empty()
                    && reason.len() <= 1000
                    && !reason.chars().any(char::is_control) =>
            {
                Ok(())
            }
            // Exact released lease validation additionally joins the immutable
            // shutdown receipt in the ledger/receiver, not this envelope alone.
            ActivationFinalizationState::Confirmed { receipt }
                if matches!(self.revision, 1 | 2)
                    && receipt.compatibility.observed_at_ms <= self.updated_at_ms =>
            {
                validate_receipt_envelope(receipt, &self.command)
            }
            _ => Err("Finalization skipped/repeated a causal edge".into()),
        }
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        self.validate()?;
        if !matches!(self.state, ActivationFinalizationState::ReleasePending {})
            || at < self.updated_at_ms
        {
            return Err("Finalization already attempted; no new release intent".into());
        }
        let value = Self {
            revision: 1,
            state: ActivationFinalizationState::Indeterminate { reason },
            updated_at_ms: at,
            ..self.clone()
        };
        value.validate()?;
        Ok(value)
    }
    pub fn confirmed(
        &self,
        receipt: WorkerFinalizationReceipt,
        shutdown: &crate::shutdown::ShutdownRecord,
        at: u64,
    ) -> Result<Self> {
        self.validate()?;
        validate_receipt(&receipt, &self.command, shutdown)?;
        if matches!(self.state, ActivationFinalizationState::Confirmed { .. })
            || at < self.updated_at_ms
        {
            return Err("Finalization confirmation is terminal".into());
        }
        let value = Self {
            revision: self.revision + 1,
            state: ActivationFinalizationState::Confirmed {
                receipt: Box::new(receipt),
            },
            updated_at_ms: at,
            ..self.clone()
        };
        value.validate()?;
        Ok(value)
    }
    pub fn advance(&self, next: &Self, shutdown: &crate::shutdown::ShutdownRecord) -> Result<()> {
        self.validate()?;
        next.validate()?;
        let expected = match &next.state {
            ActivationFinalizationState::Indeterminate { reason } => {
                self.indeterminate(reason.clone(), next.updated_at_ms)?
            }
            ActivationFinalizationState::Confirmed { receipt } => {
                self.confirmed(receipt.as_ref().clone(), shutdown, next.updated_at_ms)?
            }
            ActivationFinalizationState::ReleasePending {} => {
                return Err("Cannot reissue finalization".into())
            }
        };
        if *next != expected {
            return Err("Finalization edge changed immutable command/scope".into());
        }
        Ok(())
    }
    /// An unresolved release closes the NEXT update even if root is Committed.
    /// Cold Confirmed data are historical, never live worker/keeper custody.
    pub fn blocks_next_update(&self, shutdown: &crate::shutdown::ShutdownRecord) -> Result<bool> {
        self.validate()?;
        if let ActivationFinalizationState::Confirmed { receipt } = &self.state {
            validate_receipt(receipt, &self.command, shutdown)?;
        }
        Ok(!matches!(
            self.state,
            ActivationFinalizationState::Confirmed { .. }
        ))
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
pub(crate) use tests::fixture as test_fixture;
