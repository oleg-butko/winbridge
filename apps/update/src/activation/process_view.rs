//! Cold projection deliberately reports no resident process or state custody.
use super::{ActivationRecord, ActivationState};
use winbridge_protocol::update::{
    ActivationDurablePhase as Saved, ActivationProcessPhase as Phase, ActivationProcessView,
};
pub(crate) fn view(record: Option<&ActivationRecord>) -> ActivationProcessView {
    let durable_phase = record.map(|r| match &r.state {
        ActivationState::Prepared {} => Saved::Prepared,
        ActivationState::Creating {} => Saved::Creating,
        ActivationState::Created {} => Saved::Created,
        ActivationState::ResumeGranted {} => Saved::ResumeGranted,
        ActivationState::ReadyForState {} => Saved::ReadyForState,
        ActivationState::HandoffIssued {} => Saved::HandoffIssued,
        ActivationState::StateOwned {} => Saved::StateOwned,
        ActivationState::LocalVerified {} => Saved::LocalVerified,
        ActivationState::CreationFailed { .. } => Saved::CreationFailed,
        ActivationState::Indeterminate { .. } => Saved::Indeterminate,
    });
    ActivationProcessView {
        phase: if matches!(durable_phase, None | Some(Saved::Prepared)) {
            Phase::NotStarted
        } else {
            Phase::NotResident
        },
        attempt_id: record.map(|r| r.intent.attempt_id.clone()),
        revision: record.map(|r| r.revision),
        durable_phase,
        saved_process: record
            .and_then(|r| r.created.as_ref())
            .map(|v| v.process.clone()),
        job_name: record
            .and_then(|r| r.created.as_ref())
            .map(|v| v.job_name.clone()),
        saved_ready: record
            .and_then(|r| r.ready.as_ref())
            .map(|v| v.identity.clone()),
        resident_process: None,
        process_custody_held: Some(false),
        process_alive: None,
        original_state_held: false,
        stdout_tail: None,
        stderr_tail: None,
        output_incomplete: None,
        capture_complete: None,
        reason: record.and_then(|r| match &r.state {
            ActivationState::CreationFailed { reason }
            | ActivationState::Indeterminate { reason, .. } => Some(reason.clone()),
            _ => None,
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_ready_and_uncertainty_never_claim_live_process_ownership() {
        let (replacement, initial) = crate::activation::activation_fixture();
        assert_eq!(view(None).phase, Phase::NotStarted);
        let mut stages = crate::activation::activation_stages(&initial, &replacement);
        stages.push(
            stages[3]
                .indeterminate("Lost process observation".into(), 209, &replacement)
                .unwrap(),
        );
        stages.push(
            stages[1]
                .creation_failed("OS created no process".into(), 202, &replacement)
                .unwrap(),
        );
        for record in stages {
            let saved = view(Some(&record));
            assert_eq!(saved.process_custody_held, Some(false));
            assert!(!saved.original_state_held);
            assert!(saved.resident_process.is_none() && saved.process_alive.is_none());
            assert_ne!(saved.phase, Phase::ReadyForState);
            if record.created.is_some() {
                assert!(saved.saved_process.is_some());
            }
            if record.ready.is_some() {
                assert!(saved.saved_ready.is_some());
            }
        }
    }
}
