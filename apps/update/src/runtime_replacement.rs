//! Whole-runtime exchange data. Saved JSON grants no rename/custody permission.
use crate::{
    runtime_exchange::{FileExchangeBatch, FileExchangeRecord, FileExchangeState},
    runtime_preparation::{PreparationPhase, RuntimePreparationRecord},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
type Result<T> = std::result::Result<T, String>;
mod driver;
// Execute the portable kernel-probe regressions from the same native-hook
// source on Unix. Only the Windows driver calls the remaining hook functions.
#[cfg(all(test, not(windows), feature = "update-validation"))]
#[path = "runtime_replacement/activation_process_validation.rs"]
#[allow(dead_code)]
mod activation_process_validation_tests;
#[cfg(all(windows, feature = "update-validation"))]
mod validation;
pub(crate) use driver::{execute, ReplacementOutcome};
/// Live-object observation only; never an execution permission or durable fact.
#[derive(Clone, Default)]
pub struct CustodySignal(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl CustodySignal {
    pub fn held(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }
}
/// Conservative observation for legacy readonly diagnostics. Replacing does not
/// reveal whether an issued native effect happened. Never report `false` there.
/// `true` describes verified file placement, not activation or update completion.
pub fn applied_observation(phase: Option<winbridge_protocol::update::UpdatePhase>) -> Option<bool> {
    use winbridge_protocol::update::UpdatePhase::*;
    match phase? {
        Replacing | ManualIntervention => None,
        VerifyingLocal | AwaitingController | Committed => Some(true),
        Requested | Preparing | SourceApplying | Building | ValidatingCandidate
        | CandidateVerified | Draining | Quiesced | Stopping | Aborting | Deferred
        | FailedBeforeCutover => Some(false),
    }
}

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
    pub(crate) shutdown: crate::shutdown::ShutdownIntent,
}
impl RuntimeReplacementPermit {
    pub fn intent(&self) -> &RuntimeReplacementIntent {
        &self.intent
    }
    pub fn shutdown_intent(&self) -> &crate::shutdown::ShutdownIntent {
        &self.shutdown
    }
}
#[cfg(windows)]
mod physical;
#[cfg(windows)]
pub use physical::{
    ActivationControllerPreparation, ActivationFilesFailure, ActivationFilesGuard,
    ActivationLaunchFailure, ActivationProcessGuard, ActivationServicePreparation,
    RuntimeReplacementFailure, RuntimeReplacementGuard,
};
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
pub(crate) fn view(
    saved: Option<&RuntimeReplacementRecord>,
    active: bool,
    failed: bool,
    held: bool,
) -> winbridge_protocol::update::RuntimeReplacementView {
    use winbridge_protocol::update::{
        RuntimeReplacementMove as Move, RuntimeReplacementPhase as Phase, RuntimeReplacementView,
    };
    let Some(r) = saved else {
        return RuntimeReplacementView {
            phase: if failed {
                Phase::Blocked
            } else {
                Phase::NotAdmitted
            },
            attempt_id: None,
            revision: None,
            directory: None,
            retired_files: 0,
            installed_files: 0,
            total_files: 5,
            issued_file: None,
            issued_move: None,
            physical_custody_held: false,
            reason: None,
        };
    };
    let terminal = matches!(
        r.state,
        RuntimeReplacementState::FilesMoved { .. } | RuntimeReplacementState::Indeterminate { .. }
    );
    let (issued_file, issued_move) = r
        .batch
        .files
        .iter()
        .find_map(|f| match f.state {
            FileExchangeState::RetireIssued {} => {
                Some((Some(f.spec.file.name().into()), Some(Move::Retire)))
            }
            FileExchangeState::PlaceIssued { .. } => {
                Some((Some(f.spec.file.name().into()), Some(Move::Place)))
            }
            _ => None,
        })
        .unwrap_or((None, None));
    RuntimeReplacementView {
        phase: if !active && !terminal {
            Phase::Indeterminate
        } else {
            match r.state {
                RuntimeReplacementState::Planned {} => Phase::Acquiring,
                RuntimeReplacementState::DirectoryIssued {} => Phase::CreatingDirectory,
                RuntimeReplacementState::Replacing { .. } => Phase::MovingFiles,
                RuntimeReplacementState::FilesMoved { .. } => Phase::FilesMoved,
                RuntimeReplacementState::Indeterminate { .. } => Phase::Indeterminate,
            }
        },
        attempt_id: Some(r.intent.attempt_id.clone()),
        revision: Some(r.revision),
        directory: Some(r.intent.retained_directory().to_string_lossy().into()),
        retired_files: r.batch.files.iter().filter(|f| matches!(f.state,
            FileExchangeState::Retired { .. } | FileExchangeState::PlaceIssued { .. }
                | FileExchangeState::Installed { .. })).count() as u16,
        installed_files: r.batch.files.iter().filter(|f| matches!(f.state,
            FileExchangeState::Installed { .. })).count() as u16,
        total_files: 5,
        issued_file,
        issued_move,
        physical_custody_held: held,
        reason: match &r.state {
            RuntimeReplacementState::Indeterminate { reason, .. } => Some(reason.clone()),
            _ if !active && !terminal => Some("Сохранена незавершённая замена; активного исполнителя нет. Автоматического повтора нет.".into()),
            _ => None,
        },
    }
}
#[cfg(test)]
pub(crate) mod tests;
