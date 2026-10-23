//! Whole-runtime exchange data. Saved JSON grants no rename/custody permission.
use crate::{
    runtime_exchange::{FileExchangeBatch, FileExchangeRecord, FileExchangeState},
    runtime_preparation::{PreparationPhase, RuntimePreparationRecord},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReplacementIntent {
    pub schema_version: u16,
    pub attempt_id: String,
    pub admitted_at_ms: u64,
    pub preparation: RuntimePreparationRecord,
}
/// Returned only after fresh durable admission under original OS custody.
/// A saved intent or an Existing reply cannot recreate this non-Serde value.
pub struct RuntimeReplacementPermit {
    pub(crate) intent: RuntimeReplacementIntent,
}
impl RuntimeReplacementPermit {
    pub fn intent(&self) -> &RuntimeReplacementIntent {
        &self.intent
    }
}
pub enum RuntimeReplacementAdmission {
    Fresh(Box<RuntimeReplacementPermit>),
    Existing(Box<RuntimeReplacementRecord>),
}
impl RuntimeReplacementIntent {
    pub fn validate(&self) -> Result<()> {
        self.preparation.validate()?;
        let p = &self.preparation.plan;
        if self.schema_version != 1
            || self.preparation.phase != PreparationPhase::Prepared
            || uuid::Uuid::parse_str(&self.attempt_id)
                .ok()
                .map(|v| v.to_string())
                .as_deref()
                != Some(self.attempt_id.as_str())
            || [
                &p.update_id,
                &p.attempt_id,
                &p.owner_id,
                &p.launch_id,
                &p.manifest.set_id,
            ]
            .contains(&&self.attempt_id)
            || self.admitted_at_ms < p.admitted_at_ms
            || self.admitted_at_ms > winbridge_core::update::MAX_UPDATE_COUNTER
        {
            return Err(
                "Replacement needs exact completed preparation and distinct attempt/scope".into(),
            );
        }
        self.preparation.exchange_batch()?.validate()
    }
    pub fn retained_directory(&self) -> PathBuf {
        self.preparation
            .plan
            .directory()
            .with_file_name("displaced-runtime")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeReplacementState {
    Planned {},
    DirectoryIssued {},
    Replacing {
        directory_identity: String,
    },
    FilesMoved {
        directory_identity: String,
    },
    Indeterminate {
        previous: Box<RuntimeReplacementState>,
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReplacementRecord {
    pub schema_version: u16,
    pub intent: RuntimeReplacementIntent,
    pub revision: u64,
    pub state: RuntimeReplacementState,
    pub batch: FileExchangeBatch,
}
impl RuntimeReplacementRecord {
    pub fn planned(intent: RuntimeReplacementIntent) -> Result<Self> {
        intent.validate()?;
        let record = Self {
            schema_version: 1,
            batch: intent.preparation.exchange_batch()?,
            intent,
            revision: 0,
            state: RuntimeReplacementState::Planned {},
        };
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        self.batch.validate()?;
        let original = self.intent.preparation.exchange_batch()?;
        let mut count = 0;
        for (file, prepared) in self.batch.files.iter().zip(&original.files) {
            if file.spec != prepared.spec
                || matches!(file.state, FileExchangeState::Indeterminate { .. })
            {
                return Err(
                    "Replacement changed immutable prepared set or nested uncertainty".into(),
                );
            }
            count += file.revision;
        }
        let rank = |state: &RuntimeReplacementState| -> Result<u64> {
            match state {
                RuntimeReplacementState::Planned {}
                | RuntimeReplacementState::DirectoryIssued {} => {
                    if count != 0 {
                        return Err("File exchange preceded retained-directory receipt".into());
                    }
                    Ok(u64::from(matches!(
                        state,
                        RuntimeReplacementState::DirectoryIssued {}
                    )))
                }
                RuntimeReplacementState::Replacing { directory_identity }
                | RuntimeReplacementState::FilesMoved { directory_identity } => {
                    let p = &self.intent.preparation;
                    if directory_identity.is_empty()
                        || directory_identity.len() > 128
                        || directory_identity.chars().any(char::is_control)
                        || directory_identity == &p.plan.runtime_identity
                        || directory_identity == &p.plan.parent_identity
                        || Some(directory_identity) == p.directory_identity.as_ref()
                        || p.plan.files.iter().any(|f| {
                            &f.old.file_identity == directory_identity
                                || f.source
                                    .as_ref()
                                    .is_some_and(|s| &s.file_identity == directory_identity)
                        })
                        || self
                            .batch
                            .files
                            .iter()
                            .any(|f| &f.spec.prepared.file_identity == directory_identity)
                        || self.batch.next_file()?.is_none()
                            != matches!(state, RuntimeReplacementState::FilesMoved { .. })
                    {
                        return Err("Replacement directory identity/completed phase differs".into());
                    }
                    Ok(2 + count)
                }
                RuntimeReplacementState::Indeterminate { .. } => {
                    Err("Nested replacement uncertainty".into())
                }
            }
        };
        let expected = match &self.state {
            RuntimeReplacementState::Indeterminate { previous, reason } => {
                if matches!(
                    previous.as_ref(),
                    RuntimeReplacementState::FilesMoved { .. }
                ) || reason.is_empty()
                    || reason.chars().count() > 900
                    || reason.chars().any(char::is_control)
                {
                    return Err("Replacement terminal outcome/reason differs".into());
                }
                rank(previous)? + 1
            }
            state => rank(state)?,
        };
        if self.schema_version != 1 || self.revision != expected {
            return Err("Replacement revision differs from exact causal prefix".into());
        }
        Ok(())
    }
    fn next(&self, state: RuntimeReplacementState, batch: FileExchangeBatch) -> Result<Self> {
        self.validate()?;
        let next = Self {
            revision: self.revision + 1,
            state,
            batch,
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn issue_directory(&self) -> Result<Self> {
        if !matches!(self.state, RuntimeReplacementState::Planned {}) {
            return Err("Replacement directory already issued; no replay".into());
        }
        self.next(
            RuntimeReplacementState::DirectoryIssued {},
            self.batch.clone(),
        )
    }
    pub fn directory_created(&self, directory_identity: String) -> Result<Self> {
        if !matches!(self.state, RuntimeReplacementState::DirectoryIssued {}) {
            return Err("Replacement directory receipt requires original issue".into());
        }
        self.next(
            RuntimeReplacementState::Replacing { directory_identity },
            self.batch.clone(),
        )
    }
    pub fn advance_file(&self, index: usize, next: FileExchangeRecord) -> Result<Self> {
        let RuntimeReplacementState::Replacing { directory_identity } = &self.state else {
            return Err("Replacement file effect needs saved retained-directory receipt".into());
        };
        let batch = self.batch.advance(index, next)?;
        let directory_identity = directory_identity.clone();
        let state = if batch.next_file()?.is_none() {
            RuntimeReplacementState::FilesMoved { directory_identity }
        } else {
            RuntimeReplacementState::Replacing { directory_identity }
        };
        self.next(state, batch)
    }
    pub fn indeterminate(&self, reason: String) -> Result<Self> {
        if matches!(
            self.state,
            RuntimeReplacementState::FilesMoved { .. }
                | RuntimeReplacementState::Indeterminate { .. }
        ) {
            return Err("Replacement outcome terminal; observe only".into());
        }
        self.next(
            RuntimeReplacementState::Indeterminate {
                previous: Box::new(self.state.clone()),
                reason,
            },
            self.batch.clone(),
        )
    }
    /// Validate an exact single step for a future transactional CAS ledger.
    pub fn advance(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        if self.intent != next.intent || next.revision != self.revision + 1 {
            return Err("Replacement immutable intent/CAS differs".into());
        }
        let expected = match &next.state {
            RuntimeReplacementState::DirectoryIssued {} => self.issue_directory()?,
            RuntimeReplacementState::Replacing { directory_identity }
                if matches!(self.state, RuntimeReplacementState::DirectoryIssued {}) =>
            {
                self.directory_created(directory_identity.clone())?
            }
            RuntimeReplacementState::Replacing { .. }
            | RuntimeReplacementState::FilesMoved { .. } => {
                let i = self
                    .batch
                    .next_file()?
                    .ok_or("Replacement already complete")?;
                self.advance_file(i, next.batch.files[i].clone())?
            }
            RuntimeReplacementState::Indeterminate { reason, .. } => {
                self.indeterminate(reason.clone())?
            }
            _ => return Err("Replacement cannot return to Planned".into()),
        };
        if expected != *next {
            return Err("Replacement transition changed receipt/prefix".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
