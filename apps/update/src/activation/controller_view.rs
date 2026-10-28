//! Cold journal facts never promise a resident owner, a live link or a retry.
use super::controller::{ActivationControllerRecord, ActivationControllerState};
use winbridge_protocol::update::*;
pub(crate) fn view(saved: Option<&ActivationControllerRecord>) -> ActivationControllerView {
    ActivationControllerView {
        phase:if saved.is_some(){ActivationControllerPhase::NotResident}else{ActivationControllerPhase::NotStarted},
        attempt_id:saved.map(|v|v.command.binding.service_receipt.attempt_id.clone()),
        command_id:saved.map(|v|v.command.command_id.clone()),revision:saved.map(|v|v.revision),
        durable_phase:saved.map(|v|match v.state {ActivationControllerState::Issued{}=>ActivationControllerDurablePhase::Issued,ActivationControllerState::Confirmed{..}=>ActivationControllerDurablePhase::Confirmed,ActivationControllerState::Indeterminate{..}=>ActivationControllerDurablePhase::Indeterminate}),
        saved_receipt:saved.and_then(|v|match &v.state {ActivationControllerState::Confirmed{receipt}=>Some(receipt.as_ref().clone()),_=>None}),
        resident_process:None,process_alive:None,observed_projection:None,observed_link:None,
        reason:saved.map(|_|"Saved controller observation is history; no resident owner, current link or retry is inferred".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_issued_uncertain_and_confirmed_records_never_claim_current_controller_or_custody() {
        let f = super::super::controller::test_fixture();
        let issued = ActivationControllerRecord::issued(f.command).unwrap();
        let unknown = issued
            .indeterminate("Lost ACK".into(), issued.updated_at_ms + 1)
            .unwrap();
        let confirmed = issued
            .confirmed(f.receipt.clone(), f.receipt.compatibility.observed_at_ms)
            .unwrap();
        assert_eq!(view(None).phase, ActivationControllerPhase::NotStarted);
        for record in [issued, unknown, confirmed] {
            let observed = view(Some(&record));
            assert_eq!(observed.phase, ActivationControllerPhase::NotResident);
            assert!(observed.resident_process.is_none() && observed.process_alive.is_none());
            assert!(observed.observed_projection.is_none() && observed.observed_link.is_none());
        }
    }
}
