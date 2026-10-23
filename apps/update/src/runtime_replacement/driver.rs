//! One blocking owner. Regular failure returns retained kernel custody to actor.
use super::*;
use crate::{
    journal::{UpdateAuthority, UpdateJournal},
    shutdown::Custody,
};
use winbridge_protocol::update::SupervisorRecord;
pub(crate) struct ReplacementOutcome {
    #[cfg(windows)]
    pub guard: Option<Box<RuntimeReplacementGuard>>,
    pub error: Option<String>,
}
#[cfg(windows)]
fn uncertainty(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: &RuntimeReplacementPermit,
    reason: &str,
) {
    let dir = PathBuf::from(&permit.intent.preparation.plan.runtime_directory).join("update-state");
    if let Ok(Some(saved)) = UpdateJournal::inspect_runtime_replacement(&dir, &s.spec.update_id) {
        if !matches!(
            saved.state,
            RuntimeReplacementState::FilesMoved { .. }
                | RuntimeReplacementState::Indeterminate { .. }
        ) {
            let text: String = reason
                .chars()
                .filter(|c| !c.is_control())
                .take(900)
                .collect();
            let _ = j.mark_runtime_replacement_indeterminate(
                a,
                s,
                &saved,
                permit,
                if text.is_empty() {
                    "Replacement failed; inspect original attempt".into()
                } else {
                    text
                },
            );
        }
    }
}
pub(crate) fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: RuntimeReplacementPermit,
    custody: &Custody,
    signal: CustodySignal,
) -> ReplacementOutcome {
    #[cfg(not(windows))]
    {
        let _ = (j, a, s, permit, custody, signal);
        ReplacementOutcome {
            error: Some("Runtime replacement requires native Windows".into()),
        }
    }
    #[cfg(windows)]
    {
        let original = match custody.take_for_replacement(j, a, s, &permit) {
            Ok(original) => original,
            Err(reason) => {
                uncertainty(j, a, s, &permit, &reason);
                return ReplacementOutcome {
                    guard: None,
                    error: Some(reason),
                };
            }
        };
        let mut guard = match RuntimeReplacementGuard::acquire(original, permit, j, a, s, signal) {
            Ok(guard) => Box::new(guard),
            Err(failure) => {
                uncertainty(j, a, s, failure.custody.permit(), &failure.reason);
                return ReplacementOutcome {
                    guard: Some(failure.custody),
                    error: Some(failure.reason),
                };
            }
        };
        let result = (|| -> Result<()> {
            let mut r = RuntimeReplacementRecord::planned(guard.permit().intent().clone())?;
            loop {
                let next = match &r.state {
                    RuntimeReplacementState::Planned {} => r.issue_directory()?,
                    RuntimeReplacementState::DirectoryIssued {} => {
                        guard.create_retained_directory(j, a, s, &r)?
                    }
                    RuntimeReplacementState::Replacing { .. } => {
                        let i = r
                            .batch
                            .next_file()?
                            .ok_or("Replacement missing terminal phase")?;
                        match &r.batch.files[i].state {
                            FileExchangeState::Prepared {} => {
                                r.advance_file(i, r.batch.files[i].issue_retire()?)?
                            }
                            FileExchangeState::Retired { .. } => {
                                r.advance_file(i, r.batch.files[i].issue_place()?)?
                            }
                            FileExchangeState::RetireIssued {}
                            | FileExchangeState::PlaceIssued { .. } => {
                                guard.execute_next_move(j, a, s, &r)?
                            }
                            _ => return Err("Unknown replacement prefix; no replay".into()),
                        }
                    }
                    RuntimeReplacementState::FilesMoved { .. } => {
                        j.finish_runtime_replacement(a, s, &r, &mut guard)
                            .map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                    RuntimeReplacementState::Indeterminate { .. } => {
                        return Err("Replacement indeterminate; observe only".into())
                    }
                };
                r = j
                    .update_runtime_replacement(a, s, &r, next, &mut guard)
                    .map_err(|e| e.to_string())?;
            }
        })();
        if let Err(reason) = &result {
            guard.stop(reason.clone());
            uncertainty(j, a, s, guard.permit(), reason);
        }
        ReplacementOutcome {
            guard: Some(guard),
            error: result.err(),
        }
    }
}
