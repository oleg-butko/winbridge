//! Writer delivery history, never a permission to repeat a private command.
use super::{writer, Result};
use serde::{Deserialize, Serialize};
use winbridge_core::update::MAX_UPDATE_COUNTER;
use winbridge_protocol::update::{WorkerJournalActivationCommand, WorkerJournalActivationReceipt};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivationWriterState {
    Issued {},
    Confirmed {
        receipt: WorkerJournalActivationReceipt,
    },
    Indeterminate {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationWriterRecord {
    pub schema_version: u16,
    pub revision: u64,
    pub command: WorkerJournalActivationCommand,
    pub state: ActivationWriterState,
    pub updated_at_ms: u64,
}
impl ActivationWriterRecord {
    pub fn issued(command: WorkerJournalActivationCommand) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            revision: 0,
            updated_at_ms: command.issued_at_ms,
            command,
            state: ActivationWriterState::Issued {},
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        writer::validate_command_envelope(&self.command)?;
        if self.schema_version != 1
            || self.updated_at_ms < self.command.issued_at_ms
            || self.updated_at_ms > MAX_UPDATE_COUNTER
        {
            return Err("Writer history schema/chronology invalid".into());
        }
        match &self.state {
            ActivationWriterState::Issued {}
                if self.revision == 0 && self.updated_at_ms == self.command.issued_at_ms =>
            {
                Ok(())
            }
            ActivationWriterState::Indeterminate { reason }
                if self.revision == 1
                    && !reason.is_empty()
                    && reason.len() <= 1000
                    && !reason.chars().any(char::is_control) =>
            {
                Ok(())
            }
            ActivationWriterState::Confirmed { receipt }
                if matches!(self.revision, 1 | 2)
                    && receipt.committed_at_ms <= self.updated_at_ms =>
            {
                writer::validate_receipt(receipt, &self.command)
            }
            _ => Err("Writer history phase/revision/result invalid".into()),
        }
    }
    pub fn indeterminate(&self, reason: String, at: u64) -> Result<Self> {
        self.validate()?;
        if !matches!(self.state, ActivationWriterState::Issued {}) || at < self.updated_at_ms {
            return Err("Writer uncertainty can only follow the original issue; no replay".into());
        }
        let next = Self {
            state: ActivationWriterState::Indeterminate { reason },
            revision: 1,
            updated_at_ms: at,
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    /// A late receipt may resolve uncertainty as a fact, without a new grant.
    pub fn confirmed(&self, receipt: WorkerJournalActivationReceipt, at: u64) -> Result<Self> {
        self.validate()?;
        if matches!(self.state, ActivationWriterState::Confirmed { .. }) || at < self.updated_at_ms
        {
            return Err("Writer confirmation is terminal; history cannot be rewritten".into());
        }
        let next = Self {
            state: ActivationWriterState::Confirmed { receipt },
            revision: self.revision + 1,
            updated_at_ms: at,
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn advance(&self, next: &Self) -> Result<()> {
        let expected = match &next.state {
            ActivationWriterState::Confirmed { receipt } => {
                self.confirmed(receipt.clone(), next.updated_at_ms)?
            }
            ActivationWriterState::Indeterminate { reason } => {
                self.indeterminate(reason.clone(), next.updated_at_ms)?
            }
            _ => return Err("Writer history cannot return to Issued".into()),
        };
        if expected != *next {
            return Err("Writer history must append the exact immutable command/result".into());
        }
        Ok(())
    }
}
