//! Fresh current-session observation for the authenticated original keeper.
//! No query may enter a mode, commit a root, release a lease or retire a channel.
use super::*;
use winbridge_transport::local_peer::LocalPeer;
use winbridge_update::activation::{
    lifetime::ActivationLifetimeState, resident_observation as model,
};

pub(crate) fn rejected_resident(
    request_id: Option<String>,
    message: String,
) -> ActivationResidentObservation {
    let request_id =
        request_id.filter(|id| uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == *id));
    ActivationResidentObservation {
        resident_observation_version: ACTIVATION_RESIDENT_OBSERVATION_CONTRACT,
        request_id,
        result: ActivationResidentObservationResult::Rejected { message },
    }
}
impl Worker {
    fn observe_resident(
        &self,
        query: &ActivationResidentQuery,
        peer: &LocalPeer,
    ) -> Result<ActivationResidentObservationResult> {
        model::validate_query(query)?;
        if self.stop.load(Ordering::Acquire) {
            return Err("Resident service is stopping".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No original resident service; no cold adoption")?;
        let resident = {
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::Verification => {
                    return Err(
                        "Resident observation requires an already supervised runtime".into(),
                    )
                }
                ServiceLifetime::Supervised(value) => Arc::clone(value),
                ServiceLifetime::Active(_) => {
                    return Err("Worker is Active; V7 observes quarantine only".into())
                }
                ServiceLifetime::ReleasePending(_) => {
                    return Err(
                        "Finalization pending; V7 remains observation of quarantine only".into(),
                    );
                }
            }
        };
        if query.command != resident.command
            || peer.process_id() != resident.command.binding.keeper.process_id
        {
            return Err(
                "Resident observation requires the actual original keeper peer and command".into(),
            );
        }
        resident.keeper.require_live()?;
        // Reuse the real runtime/config/Job/Store checks without manufacturing
        // another service/controller receipt outside its verification window.
        let observed = self.observe_lifetime(ActivationLifetimeQuery {
            lifetime_observation_version: 1,
            command: query.command.clone(),
        })?;
        if !matches!(&observed, ActivationLifetimeObservationResult::Observed { receipt, .. } if receipt.as_ref() == &resident.receipt)
        {
            return Err("Original resident lifetime observation differs".into());
        }
        let directory = Path::new(&resident.receipt.compatibility.identity.image.path)
            .parent()
            .ok_or("Resident runtime parent missing")?
            .join("update-state");
        let graph =
            winbridge_update::journal::UpdateJournal::inspect_activation_lifetime_continuation(
                &directory,
                &resident.command.binding.update_id,
                &resident.command.binding.attempt_id,
                &resident.receipt.compatibility.identity.process,
                &resident.command,
                &resident.receipt,
            )
            .map_err(|e| e.to_string())?;
        if !matches!(&graph.lifetime.state, ActivationLifetimeState::Confirmed { receipt } if receipt.as_ref() == &resident.receipt)
        {
            return Err("Resident current-session observation requires the saved independent lifetime confirmation".into());
        }
        // Validate health locally: serializing full Info/catalog on the bounded
        // control wire would reintroduce project-count dependent frame sizes.
        let winbridge_protocol::LocalResponse::Info { info } =
            self.handle(winbridge_protocol::LocalRequest::Info)
        else {
            return Err("Resident readonly runtime Info unavailable".into());
        };
        winbridge_update::activation::service::validate_readonly_info(
            &graph.controller.service.command,
            &info,
        )?;
        let topology = &graph.controller.controller.command.binding.topology;
        // Capture the accepted current ticket only after expensive validation.
        let (projection, link) = match self.activation_controller_frame(
            &topology.confirmation_endpoint_id,
            &resident.command.binding.compatibility.context,
        ) {
            Ok(value) => value,
            Err(message) => {
                return Ok(ActivationResidentObservationResult::Unavailable { message })
            }
        };
        let frame = model::summarize_projection(&projection)?;
        self.check_local_service_scope()?;
        self._instance_lock.revalidate()?;
        resident.keeper.require_live()?;
        let observed_at_ms = store::now_ms();
        model::validate_current_connection(
            &resident.command,
            topology,
            &frame,
            &link,
            observed_at_ms,
        )?;
        Ok(ActivationResidentObservationResult::Observed {
            entry: Box::new(resident.receipt.clone()),
            frame: Box::new(frame),
            link: Box::new(link),
            observed_at_ms,
        })
    }
    pub(crate) fn handle_resident_observation(
        &self,
        query: ActivationResidentQuery,
        peer: LocalPeer,
    ) -> ActivationResidentObservation {
        // Only an implemented receiver retains this verbatim marker. V7 is
        // observation only; no mutable finalization support is advertised.
        std::hint::black_box(ACTIVATION_RESIDENT_OBSERVATION_MARKER);
        match self.observe_resident(&query, &peer) {
            Ok(result) => ActivationResidentObservation {
                resident_observation_version: ACTIVATION_RESIDENT_OBSERVATION_CONTRACT,
                request_id: Some(query.request_id),
                result,
            },
            Err(message) => rejected_resident(Some(query.request_id), message),
        }
    }
}
