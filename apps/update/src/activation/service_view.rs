//! Cold facts never reconstruct live service or original process custody.
use super::service::{ActivationServiceRecord, ActivationServiceState};
use winbridge_protocol::update::{
    ActivationServiceDurablePhase as Durable, ActivationServicePhase as Phase,
    ActivationServiceView,
};
pub(crate) fn view(value: Option<&ActivationServiceRecord>) -> ActivationServiceView {
    ActivationServiceView {
        phase: if value.is_some() { Phase::NotResident } else { Phase::NotStarted },
        attempt_id:value.map(|v|v.command.binding.writer_command.binding.attempt_id.clone()),
        command_id:value.map(|v|v.command.command_id.clone()),
        revision:value.map(|v|v.revision),
        durable_phase:value.map(|v|match v.state { ActivationServiceState::Issued{}=>Durable::Issued, ActivationServiceState::Confirmed{..}=>Durable::Confirmed, ActivationServiceState::Indeterminate{..}=>Durable::Indeterminate }),
        saved_receipt:value.and_then(|v|match &v.state { ActivationServiceState::Confirmed{receipt}=>Some(*receipt.clone()), _=>None }),
        resident_process:None,process_alive:None,
        reason:value.map(|v|match &v.state { ActivationServiceState::Indeterminate{reason}=>reason.clone(),_=>"Saved local-service evidence is history; original residency, controller and lifetime are not inferred".into() }),
    }
}
