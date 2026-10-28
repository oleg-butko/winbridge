//! Data-only lifetime FSM. No dispatch, adoption, OS effect or live permit.
use super::{controller, ActivationState, Result};
use controller::{ActivationControllerSnapshot, ActivationControllerState};
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::{update::*, ArtifactRole};

/// Only fresh original-guard admission constructs this non-Serde, non-Clone
/// permit. Existing records and receiver snapshots below are data only.
pub struct ActivationLifetimePermit {
    pub(crate) record: ActivationLifetimeRecord,
    pub(crate) topology: WorkerControllerActivationTopology,
}
impl ActivationLifetimePermit {
    pub fn record(&self) -> &ActivationLifetimeRecord {
        &self.record
    }
    pub fn topology(&self) -> &WorkerControllerActivationTopology {
        &self.topology
    }
}
pub enum ActivationLifetimeAdmission {
    Issued(Box<ActivationLifetimePermit>),
    Existing(Box<ActivationLifetimeRecord>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationLifetimeSnapshot {
    pub controller: ActivationControllerSnapshot,
    pub lifetime: ActivationLifetimeRecord,
}

/// A retained independent observation must exactly match the confirmed prefix.
/// This does not promise that the network link is still live; fresh liveness
/// and original custody/config/window are separately enforced by the owner.
#[cfg(any(windows, test))]
pub(crate) fn validate_controller_prefix(
    s: &ActivationControllerSnapshot,
    topology: &WorkerControllerActivationTopology,
    observed: &ActivationControllerObservation,
) -> Result<()> {
    s.controller.validate()?;
    controller::validate_command(
        &s.controller.command,
        &s.physical,
        &s.writer,
        &s.service,
        &s.local,
        topology,
    )?;
    let ActivationControllerState::Confirmed { receipt } = &s.controller.state else {
        return Err("Lifetime requires original confirmed controller prefix".into());
    };
    let ActivationControllerObservationResult::Observed {
        receipt: actual,
        projection,
        link,
    } = &observed.result
    else {
        return Err("Lifetime requires original independent controller observation".into());
    };
    if observed.controller_observation_version != 1 || actual != receipt {
        return Err("Lifetime controller observation differs from the exact saved fact".into());
    }
    controller::validate_projection_evidence(receipt, &s.controller.command, projection)?;
    controller::validate_link_evidence(receipt, &s.controller.command, link)
}

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
}
fn stamp(value: &SupervisorProcess) -> bool {
    value.process_id > 0
        && value
            .birth
            .parse::<u64>()
            .is_ok_and(|v| v > 0 && v.to_string() == value.birth)
        && valid_hash(&value.host_id, &[64])
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
pub fn command_sha256(command: &WorkerLifetimeActivationCommand) -> Result<String> {
    digest(b"winbridge.activation.lifetime-command.v1\n", command)
}
pub fn validate_command_envelope(command: &WorkerLifetimeActivationCommand) -> Result<()> {
    let b = &command.binding;
    let c = &b.compatibility;
    let p = &c.identity.process;
    let ids = [
        &b.update_id,
        &b.attempt_id,
        &b.launch_id,
        &b.updater_owner_id,
        &b.controller_command_id,
        &c.generation,
        &c.context.journal_incarnation,
    ];
    if command.schema_version != 1
        || !uuid(&command.command_id)
        || ids.iter().any(|v| !uuid(v))
        || ids.contains(&&command.command_id)
        || !(1..=MAX_UPDATE_COUNTER).contains(&b.updater_epoch)
        || !(1..=MAX_UPDATE_COUNTER).contains(&b.root_revision)
        || !valid_hash(&b.controller_record_sha256, &[64])
        || !valid_hash(&b.local_record_sha256, &[64])
        || !b
            .job_name
            .strip_prefix("Global\\WinBridge-Activation-")
            .is_some_and(|v| valid_hash(v, &[64]))
        || !stamp(&b.keeper)
        || !stamp(p)
        || b.keeper.process_id == p.process_id
        || b.keeper.host_id != p.host_id
        || b.keeper.session_id != p.session_id
        || c.schema_version != 1
        || c.kind != "worker_runtime_compatibility"
        || c.identity.schema_version != 1
        || c.identity.kind != "loaded_worker_identity"
        || c.identity.descriptor.role != ArtifactRole::Worker
        || !valid_hash(&c.identity.manifest_sha256, &[64])
        || !valid_hash(&c.identity.image.sha256, &[64])
        || !valid_hash(&c.worker_config_sha256, &[64])
        || !winbridge_core::config::valid_id(&c.context.instance_id)
        || c.worker_config_schema == 0
        || c.worker_journal_schema == 0
        || c.updater_journal_schema == 0
        || c.protocol_version == 0
        || c.observed_at_ms == 0
        || c.observed_at_ms > command.issued_at_ms
        || b.authority.incarnation != c.context.journal_incarnation
        || !(1..=MAX_UPDATE_COUNTER).contains(&b.authority.epoch)
        || !valid_hash(&b.authority.fingerprint_sha256, &[64])
        || command.issued_at_ms >= command.deadline_at_ms
        || command.deadline_at_ms > MAX_UPDATE_COUNTER
    {
        return Err("Lifetime binding/key/process/Job/original window invalid".into());
    }
    Ok(())
}

/// Pure proposal joined to the full retained graph. The future fresh admission
/// must also authenticate original custody, current root/config and live window.
pub fn prepare_command(
    snapshot: &ActivationControllerSnapshot,
    key: &str,
    at: u64,
) -> Result<WorkerLifetimeActivationCommand> {
    let s = snapshot;
    s.controller.validate()?;
    controller::validate_command(
        &s.controller.command,
        &s.physical,
        &s.writer,
        &s.service,
        &s.local,
        &s.controller.command.binding.topology,
    )?;
    let ActivationControllerState::Confirmed { receipt } = &s.controller.state else {
        return Err("Lifetime requires confirmed controller/local7 history".into());
    };
    if s.local.state != (ActivationState::LocalVerified {})
        || s.local.revision != 7
        || at < s.controller.updated_at_ms
    {
        return Err("Lifetime proposal skipped its confirmed local/controller prefix".into());
    }
    let source = &s.local.intent.source;
    let command = WorkerLifetimeActivationCommand {
        schema_version: 1,
        command_id: key.into(),
        mode: WorkerLifetimeActivationMode::SupervisedQuarantine,
        issued_at_ms: at,
        deadline_at_ms: s.controller.command.deadline_at_ms,
        binding: WorkerLifetimeActivationBinding {
            update_id: source.update_id.clone(),
            attempt_id: s.local.intent.attempt_id.clone(),
            launch_id: source.launch_id.clone(),
            updater_owner_id: source.owner_id.clone(),
            updater_epoch: source.owner_epoch,
            root_revision: source
                .parent_revision
                .checked_add(1)
                .ok_or("Root revision overflow")?,
            controller_command_id: s.controller.command.command_id.clone(),
            controller_record_sha256: digest(
                b"winbridge.activation.lifetime-controller.v1\n",
                &s.controller,
            )?,
            local_record_sha256: digest(b"winbridge.activation.lifetime-local.v1\n", &s.local)?,
            keeper: s.local.intent.context.supervisor.clone(),
            job_name: s.local.intent.job_name()?,
            compatibility: receipt.compatibility.clone(),
            authority: receipt.authority.clone(),
        },
    };
    if [&s.service.command.command_id, &s.writer.command.command_id].contains(&&command.command_id)
    {
        return Err("Lifetime command key aliases an earlier private operation".into());
    }
    validate_command_envelope(&command)?;
    Ok(command)
}
pub fn validate_command(
    command: &WorkerLifetimeActivationCommand,
    snapshot: &ActivationControllerSnapshot,
) -> Result<()> {
    if *command != prepare_command(snapshot, &command.command_id, command.issued_at_ms)? {
        return Err("Lifetime command detached from retained graph/keeper/window".into());
    }
    Ok(())
}
pub fn validate_receipt(
    receipt: &WorkerLifetimeActivationReceipt,
    command: &WorkerLifetimeActivationCommand,
) -> Result<()> {
    validate_command_envelope(command)?;
    let b = &command.binding;
    let mut expected = b.compatibility.clone();
    expected.observed_at_ms = receipt.compatibility.observed_at_ms;
    if receipt.schema_version != 1
        || receipt.command_id != command.command_id
        || receipt.update_id != b.update_id
        || receipt.attempt_id != b.attempt_id
        || receipt.command_sha256 != command_sha256(command)?
        || receipt.keeper != b.keeper
        || receipt.job_name != b.job_name
        || receipt.authority != b.authority
        || receipt.compatibility != expected
        || receipt.compatibility.observed_at_ms < command.issued_at_ms
        || receipt.compatibility.observed_at_ms >= command.deadline_at_ms
        || receipt.mode != command.mode
    {
        return Err(
            "Lifetime receipt changed same-child keeper/authority/quarantine/window".into(),
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivationLifetimeState {
    Issued {},
    Confirmed {
        receipt: Box<WorkerLifetimeActivationReceipt>,
    },
    Indeterminate {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationLifetimeRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub command: WorkerLifetimeActivationCommand,
    pub state: ActivationLifetimeState,
    pub updated_at_ms: u64,
}
impl ActivationLifetimeRecord {
    pub fn issued(command: WorkerLifetimeActivationCommand) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: command.issued_at_ms,
            command,
            state: ActivationLifetimeState::Issued {},
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
            return Err("Lifetime record schema/time invalid".into());
        }
        match &self.state {
            ActivationLifetimeState::Issued {}
                if self.revision == 0 && self.updated_at_ms == self.command.issued_at_ms =>
            {
                Ok(())
            }
            ActivationLifetimeState::Indeterminate { reason }
                if self.revision == 1
                    && !reason.is_empty()
                    && reason.len() <= 1000
                    && !reason.chars().any(char::is_control) =>
            {
                Ok(())
            }
            ActivationLifetimeState::Confirmed { receipt }
                if matches!(self.revision, 1 | 2)
                    && receipt.compatibility.observed_at_ms <= self.updated_at_ms =>
            {
                validate_receipt(receipt, &self.command)
            }
            _ => Err("Lifetime record skipped or rewrote a causal edge".into()),
        }
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        self.validate()?;
        if !matches!(self.state, ActivationLifetimeState::Issued {}) || at < self.updated_at_ms {
            return Err("Lifetime already attempted; no reissue".into());
        }
        let value = Self {
            revision: 1,
            updated_at_ms: at,
            state: ActivationLifetimeState::Indeterminate { reason },
            ..self.clone()
        };
        value.validate()?;
        Ok(value)
    }
    pub fn confirmed(&self, receipt: WorkerLifetimeActivationReceipt, at: u64) -> Result<Self> {
        self.validate()?;
        if matches!(self.state, ActivationLifetimeState::Confirmed { .. })
            || at < self.updated_at_ms
        {
            return Err("Lifetime confirmation is terminal; no new grant".into());
        }
        let value = Self {
            revision: self.revision + 1,
            updated_at_ms: at,
            state: ActivationLifetimeState::Confirmed {
                receipt: Box::new(receipt),
            },
            ..self.clone()
        };
        value.validate()?;
        Ok(value)
    }
    pub fn advance(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        let expected = match &next.state {
            ActivationLifetimeState::Indeterminate { reason } => {
                self.indeterminate(reason.clone(), next.updated_at_ms)?
            }
            ActivationLifetimeState::Confirmed { receipt } => {
                self.confirmed(receipt.as_ref().clone(), next.updated_at_ms)?
            }
            ActivationLifetimeState::Issued {} => {
                return Err("Cannot reissue lifetime command".into())
            }
        };
        if expected != *next {
            return Err("Lifetime edge changed frozen command/scope".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::fixture as test_fixture;
