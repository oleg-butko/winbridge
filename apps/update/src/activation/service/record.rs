//! Retained local-service delivery facts; never a permission to reopen runtime.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivationServiceState {
    Issued {},
    Confirmed {
        receipt: Box<WorkerServiceActivationReceipt>,
    },
    Indeterminate {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationServiceRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub command: WorkerServiceActivationCommand,
    pub state: ActivationServiceState,
    pub updated_at_ms: u64,
}
impl ActivationServiceRecord {
    pub fn issued(command: WorkerServiceActivationCommand) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: command.issued_at_ms,
            command,
            state: ActivationServiceState::Issued {},
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
            return Err("Service history schema/time differs".into());
        }
        match &self.state {
            ActivationServiceState::Issued {}
                if self.revision == 0 && self.updated_at_ms == self.command.issued_at_ms =>
            {
                Ok(())
            }
            ActivationServiceState::Indeterminate { reason }
                if self.revision == 1
                    && !reason.is_empty()
                    && reason.len() <= 1000
                    && !reason.chars().any(char::is_control) =>
            {
                Ok(())
            }
            ActivationServiceState::Confirmed { receipt }
                if matches!(self.revision, 1 | 2)
                    && receipt.compatibility.observed_at_ms <= self.updated_at_ms =>
            {
                validate_receipt(receipt, &self.command)
            }
            _ => Err("Service history skipped or rewrote a causal edge".into()),
        }
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        self.validate()?;
        if !matches!(self.state, ActivationServiceState::Issued {}) || at < self.updated_at_ms {
            return Err("Service delivery already attempted; observe without retry".into());
        }
        let next = Self {
            revision: 1,
            updated_at_ms: at,
            state: ActivationServiceState::Indeterminate { reason },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn confirmed(&self, receipt: WorkerServiceActivationReceipt, at: u64) -> Result<Self> {
        self.validate()?;
        if matches!(self.state, ActivationServiceState::Confirmed { .. }) || at < self.updated_at_ms
        {
            return Err("Service confirmation is terminal; no new grant".into());
        }
        let next = Self {
            revision: self.revision + 1,
            updated_at_ms: at,
            state: ActivationServiceState::Confirmed {
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
            ActivationServiceState::Indeterminate { reason } => {
                self.indeterminate(reason.clone(), next.updated_at_ms)?
            }
            ActivationServiceState::Confirmed { receipt } => {
                self.confirmed(receipt.as_ref().clone(), next.updated_at_ms)?
            }
            ActivationServiceState::Issued {} => {
                return Err("Cannot reissue service delivery".into())
            }
        };
        if expected != *next {
            return Err("Service history changed its frozen command/scope".into());
        }
        Ok(())
    }
}
