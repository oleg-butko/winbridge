//! Read a sealed in-memory Active fact. No updater-journal epoch/config hash
//! dependency after activation, no SQL writes and no mode adoption from data.
use super::*;
use winbridge_transport::local_peer::LocalPeer;
use winbridge_update::activation::{finalization, finalization_observation as model};

pub(crate) fn rejected_finalization_observation(
    request_id: Option<String>,
    message: String,
) -> ActivationFinalizationObservation {
    ActivationFinalizationObservation {
        finalization_observation_version: ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT,
        request_id: request_id
            .filter(|id| uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == *id)),
        result: ActivationFinalizationObservationResult::Rejected { message },
    }
}
impl Worker {
    fn observe_finalization(
        &self,
        query: &ActivationFinalizationQuery,
        peer: &LocalPeer,
    ) -> Result<ActivationFinalizationObservationResult> {
        model::validate_query(query)?;
        if self.stop.load(Ordering::Acquire) {
            return Err("Worker is stopping".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No original resident service; no cold Active adoption")?;
        let (resident, pending, active) = {
            // Same order as the mutable seal. The snapshot is data, no permit.
            let _state = self.store()?;
            let mode = scope
                .lifetime
                .lock()
                .map_err(|_| "Lifetime mode lock poisoned")?;
            match &*mode {
                ServiceLifetime::Verification => return Err("No resident lifetime owner".into()),
                ServiceLifetime::Supervised(v) => (Arc::clone(v), None, None),
                ServiceLifetime::ReleasePending(v) => {
                    (Arc::clone(&v.resident), Some(Arc::clone(v)), None)
                }
                ServiceLifetime::Active(v) => (Arc::clone(&v.resident), None, Some(Arc::clone(v))),
            }
        };
        if query.command.binding.lifetime_command != resident.command
            || peer.process_id() != resident.command.binding.keeper.process_id
        {
            return Err(
                "Finalization observation requires actual original keeper peer and lifetime".into(),
            );
        }
        resident.keeper.require_live()?;
        self._instance_lock.revalidate()?;
        let guard = self
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .clone()
            .ok_or("Actual loaded runtime guard missing; metadata is not Active evidence")?;
        guard.revalidate()?;
        guard.require_installation(
            &query.command.binding.installation_id,
            &query.command.binding.scope_sha256,
        )?;
        if guard.identity() != &resident.command.binding.compatibility.identity {
            return Err("Finalization observation changed actual loaded worker".into());
        }
        #[cfg(windows)]
        winbridge_platform::infrastructure::PinnedProcess::live(&guard.identity().process)
            .and_then(|p| p.require_job_membership(&resident.command.binding.job_name))
            .map_err(|e| e.to_string())?;
        if let Some(pending) = pending {
            if pending.command != query.command {
                return Err("Pending finalization command changed".into());
            }
            self.check_local_service_scope()?;
            return Ok(ActivationFinalizationObservationResult::Pending {
                command_id: pending.command.command_id.clone(),
                message: "Original finalization remains pending; query grants no Active".into(),
            });
        }
        let Some(active) = active else {
            return Ok(ActivationFinalizationObservationResult::Unavailable {
                message: "No in-memory finalization decision; observe without resending a mutation"
                    .into(),
            });
        };
        if active.receipt.command_id != query.command.command_id
            || active.receipt.command_sha256 != finalization::command_sha256(&query.command)?
        {
            return Err("Active query changed the sealed finalization decision".into());
        }
        // No Empty-domain check: legitimate work may already be running. No
        // old updater graph/config SHA check: settings/next updates are legal.
        self._instance_lock.revalidate()?;
        let state = self.store()?;
        state.check_authority()?;
        let mode = scope
            .lifetime
            .lock()
            .map_err(|_| "Lifetime mode lock poisoned")?;
        if !matches!(&*mode, ServiceLifetime::Active(current) if Arc::ptr_eq(current, &active))
            || self.stop.load(Ordering::Acquire)
        {
            return Err("Original Active mode no longer observable".into());
        }
        resident.keeper.require_live()?;
        let at = store::now_ms();
        if at < active.receipt.compatibility.observed_at_ms
            || at > winbridge_core::update::MAX_UPDATE_COUNTER
        {
            return Err("Active observation clock moved backwards/outside contract".into());
        }
        Ok(ActivationFinalizationObservationResult::Observed {
            receipt: Box::new(active.receipt.clone()),
            observed_at_ms: at,
        })
    }
    pub(crate) fn handle_finalization_observation(
        &self,
        query: ActivationFinalizationQuery,
        peer: LocalPeer,
    ) -> ActivationFinalizationObservation {
        std::hint::black_box(ACTIVATION_FINALIZATION_OBSERVATION_MARKER);
        match self.observe_finalization(&query, &peer) {
            Ok(result) => ActivationFinalizationObservation {
                finalization_observation_version: ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT,
                request_id: Some(query.request_id),
                result,
            },
            Err(message) => rejected_finalization_observation(Some(query.request_id), message),
        }
    }
}
