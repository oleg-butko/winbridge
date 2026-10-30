//! Historical SQL state never implies a live process, retired pin or grant.
use super::finalization::{ActivationFinalizationRecord, ActivationFinalizationState};
use winbridge_protocol::update::*;
pub(crate) fn view(saved: Option<&ActivationFinalizationRecord>) -> ActivationFinalizationView {
    ActivationFinalizationView {
        phase: if saved.is_some() {
            ActivationFinalizationPhase::NoOriginalCustody
        } else {
            ActivationFinalizationPhase::NotStarted
        },
        command_id: saved.map(|r| r.command.command_id.clone()),
        durable_phase: saved.map(|r| match r.state {
            ActivationFinalizationState::ReleasePending {} => {
                ActivationFinalizationDurablePhase::ReleasePending
            }
            ActivationFinalizationState::Indeterminate { .. } => {
                ActivationFinalizationDurablePhase::Indeterminate
            }
            ActivationFinalizationState::Confirmed { .. } => {
                ActivationFinalizationDurablePhase::Confirmed
            }
        }),
        resident_process: None,
        process_alive: None,
        original_custody_held: None,
        verification_pins_retired: None,
        delivery_attempted: None,
        interrupted_reply_prefix: None,
        observation: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        output_incomplete: None,
        readers_finished: None,
        reason: Some(
            "Saved finalization data are not original process/Job custody or a new delivery permit"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_confirmed_success_never_claims_a_live_owner_or_retired_pins() {
        let f = crate::activation::finalization::test_fixture();
        let saved = ActivationFinalizationRecord::pending(f.command)
            .unwrap()
            .confirmed(
                f.receipt.clone(),
                &f.shutdown,
                f.receipt.compatibility.observed_at_ms,
            )
            .unwrap();
        let view = view(Some(&saved));
        assert_eq!(view.phase, ActivationFinalizationPhase::NoOriginalCustody);
        assert_eq!(
            view.durable_phase,
            Some(ActivationFinalizationDurablePhase::Confirmed)
        );
        assert!(
            view.resident_process.is_none()
                && view.process_alive.is_none()
                && view.original_custody_held.is_none()
                && view.verification_pins_retired.is_none()
                && view.delivery_attempted.is_none()
                && view.observation.is_none()
        );
        assert!(serde_json::to_vec(&view).unwrap().len() < 2048);
    }
}
