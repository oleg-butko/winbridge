//! Read-only facts never claim a live keeper or current supervised mode.
use super::lifetime::{ActivationLifetimeRecord, ActivationLifetimeState};
use winbridge_protocol::update::*;
pub(crate) fn view(saved: Option<&ActivationLifetimeRecord>) -> ActivationLifetimeView {
    ActivationLifetimeView {
        phase: if saved.is_some() {
            ActivationLifetimePhase::NotResident
        } else {
            ActivationLifetimePhase::NotStarted
        },
        attempt_id: saved.map(|v| v.command.binding.attempt_id.clone()),
        command_id: saved.map(|v| v.command.command_id.clone()),
        revision: saved.map(|v| v.revision),
        durable_phase: saved.map(|v| match v.state {
            ActivationLifetimeState::Issued {} => ActivationLifetimeDurablePhase::Issued,
            ActivationLifetimeState::Confirmed { .. } => ActivationLifetimeDurablePhase::Confirmed,
            ActivationLifetimeState::Indeterminate { .. } => {
                ActivationLifetimeDurablePhase::Indeterminate
            }
        }),
        saved_receipt: saved.and_then(|v| match &v.state {
            ActivationLifetimeState::Confirmed { receipt } => Some(receipt.as_ref().clone()),
            _ => None,
        }),
        resident_process: None,
        process_alive: None,
        original_custody_held: None,
        observed_receipt: None,
        observed_at_ms: None,
        reason: saved.map(|_| {
            "Saved lifetime entry is history; original custody and current mode are unknown".into()
        }),
    }
}
