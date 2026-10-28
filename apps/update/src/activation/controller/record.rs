//! Retained network-dispatch facts. No adoption, resend, release or commit grant.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivationControllerState {
    Issued {},
    Confirmed {
        receipt: Box<WorkerControllerActivationReceipt>,
    },
    Indeterminate {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationControllerRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub command: WorkerControllerActivationCommand,
    pub state: ActivationControllerState,
    pub updated_at_ms: u64,
}
impl ActivationControllerRecord {
    pub fn issued(command: WorkerControllerActivationCommand) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: command.issued_at_ms,
            command,
            state: ActivationControllerState::Issued {},
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
            return Err("Controller history schema/time differs".into());
        }
        match &self.state {
            ActivationControllerState::Issued {}
                if self.revision == 0 && self.updated_at_ms == self.command.issued_at_ms =>
            {
                Ok(())
            }
            ActivationControllerState::Indeterminate { reason }
                if self.revision == 1
                    && !reason.is_empty()
                    && reason.len() <= 1000
                    && !reason.chars().any(char::is_control) =>
            {
                Ok(())
            }
            ActivationControllerState::Confirmed { receipt }
                if matches!(self.revision, 1 | 2)
                    && receipt.compatibility.observed_at_ms <= self.updated_at_ms =>
            {
                validate_receipt(receipt, &self.command)
            }
            _ => Err("Controller history skipped or rewrote a causal edge".into()),
        }
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        self.validate()?;
        if !matches!(self.state, ActivationControllerState::Issued {}) || at < self.updated_at_ms {
            return Err("Controller dispatch already attempted; observe without retry".into());
        }
        let next = Self {
            revision: 1,
            updated_at_ms: at,
            state: ActivationControllerState::Indeterminate { reason },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn confirmed(&self, receipt: WorkerControllerActivationReceipt, at: u64) -> Result<Self> {
        self.validate()?;
        if matches!(self.state, ActivationControllerState::Confirmed { .. })
            || at < self.updated_at_ms
        {
            return Err("Controller confirmation is terminal; no new grant".into());
        }
        let next = Self {
            revision: self.revision + 1,
            updated_at_ms: at,
            state: ActivationControllerState::Confirmed {
                receipt: Box::new(receipt),
            },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn advance(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        let expected = match &next.state {
            ActivationControllerState::Indeterminate { reason } => {
                self.indeterminate(reason.clone(), next.updated_at_ms)?
            }
            ActivationControllerState::Confirmed { receipt } => {
                self.confirmed(receipt.as_ref().clone(), next.updated_at_ms)?
            }
            ActivationControllerState::Issued {} => {
                return Err("Cannot reissue controller dispatch".into())
            }
        };
        if expected != *next {
            return Err("Controller history changed its frozen command/scope".into());
        }
        Ok(())
    }
}
