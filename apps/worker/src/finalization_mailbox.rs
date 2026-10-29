//! Bounded delivery to the original RunningLocalService owner. Disconnecting
//! a caller drops only its reply, never an accepted command or known COMMIT.
use super::*;
use tokio::sync::{mpsc, oneshot};
use winbridge_transport::local_peer::LocalPeer;
use winbridge_update::activation::finalization_wire;

#[cfg_attr(not(any(windows, test)), allow(dead_code))] // Native owner loop is Windows-only.
pub(in crate::local_service) struct FinalizationDelivery {
    pub(super) request: ActivationFinalizationRequest,
    pub(super) peer: LocalPeer,
    pub(super) reply: oneshot::Sender<ActivationFinalizationResponse>,
    queued_at: Instant,
}
pub(crate) fn rejected_finalization(
    command_id: Option<String>,
    message: String,
) -> ActivationFinalizationResponse {
    ActivationFinalizationResponse {
        finalization_version: ACTIVATION_FINALIZATION_CONTRACT,
        command_id: command_id
            .filter(|s| uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == *s)),
        result: ActivationFinalizationResult::Rejected { message },
    }
}
fn uncertain(command_id: String, message: String) -> ActivationFinalizationResponse {
    ActivationFinalizationResponse {
        finalization_version: ACTIVATION_FINALIZATION_CONTRACT,
        command_id: Some(command_id),
        result: ActivationFinalizationResult::Indeterminate { message },
    }
}
impl Worker {
    fn authorize_finalization_delivery(
        &self,
        request: &ActivationFinalizationRequest,
        peer: &LocalPeer,
    ) -> Result<()> {
        finalization_wire::validate_request(request)?;
        if self.stop.load(Ordering::Acquire) {
            return Err("Original worker is stopping".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No original local owner; cold V8 adoption forbidden")?;
        let mode = scope
            .lifetime
            .lock()
            .map_err(|_| "Lifetime mode lock poisoned")?;
        let resident = match &*mode {
            ServiceLifetime::Verification => {
                return Err("V8 requires an already supervised original service".into())
            }
            ServiceLifetime::Supervised(value) => value,
            ServiceLifetime::ReleasePending(value) => {
                if request.command != value.command {
                    return Err("Another pending finalization decision; no new release".into());
                }
                &value.resident
            }
            ServiceLifetime::Active(value) => {
                if value.receipt.command_id != request.command.command_id
                    || value.receipt.command_sha256
                        != winbridge_update::activation::finalization::command_sha256(
                            &request.command,
                        )?
                {
                    return Err("Active decision is immutable".into());
                }
                &value.resident
            }
        };
        if request.command.binding.lifetime_command != resident.command
            || peer.process_id() != resident.command.binding.keeper.process_id
        {
            return Err("V8 requires actual original keeper peer and exact lifetime".into());
        }
        resident.keeper.require_live()
    }
    pub(crate) async fn handle_finalization_request(
        &self,
        request: ActivationFinalizationRequest,
        peer: LocalPeer,
    ) -> ActivationFinalizationResponse {
        let key = request.command.command_id.clone();
        if let Err(message) = self.authorize_finalization_delivery(&request, &peer) {
            return rejected_finalization(Some(key), message);
        }
        let (reply, observed) = oneshot::channel();
        let queued = self.local_activation.as_ref().and_then(|scope| {
            scope
                .finalization_sender
                .lock()
                .ok()
                .and_then(|v| v.clone())
        });
        let Some(sender) = queued else {
            return rejected_finalization(
                Some(key),
                "Original finalization coordinator unavailable".into(),
            );
        };
        if let Err(error) = sender.try_send(FinalizationDelivery {
            request,
            peer,
            reply,
            queued_at: Instant::now(),
        }) {
            let message = match error {
                mpsc::error::TrySendError::Full(_) => {
                    "Finalization mailbox busy; this delivery was not accepted"
                }
                mpsc::error::TrySendError::Closed(_) => {
                    "Original coordinator stopped; this delivery was not accepted"
                }
            };
            return rejected_finalization(Some(key), message.into());
        }
        eprintln!(
            "WinBridge finalization {}",
            serde_json::json!({"event":"delivery_accepted", "processId":std::process::id(), "generation":self.generation, "commandId":key})
        );
        // The outer IPC timeout may drop this receiver. Ownership of the
        // request/peer/known SQL commit remains entirely in the coordinator.
        observed.await.unwrap_or_else(|_| uncertain(key, "Accepted finalization outcome unknown; use readonly observation, never repeat SQL release".into()))
    }
}

#[cfg(windows)]
#[derive(Default)]
pub(in crate::local_service) struct FinalizationRecovery {
    known: Option<finalization_service::active::ActiveSealFailure>,
    unconfirmed: Option<finalization_service::active::ActiveReadiness>,
}
#[cfg(windows)]
enum DeliveryPhase {
    Begin,
    Pending,
    Active,
}
#[cfg(windows)]
impl FinalizationRecovery {
    pub(in crate::local_service) async fn close(self) {
        if let Some(value) = self.known {
            let (_, ready) = value.into_parts();
            ready.close().await;
        }
        if let Some(ready) = self.unconfirmed {
            ready.close().await;
        }
    }
}
impl RunningLocalService {
    #[cfg(any(windows, test))]
    pub(super) fn process_finalization_delivery(&mut self, delivery: FinalizationDelivery) {
        let FinalizationDelivery {
            request,
            peer,
            reply,
            queued_at,
        } = delivery;
        let started = Instant::now();
        let queue_wait_ms = started.duration_since(queued_at).as_millis();
        let key = request.command.command_id.clone();
        let result = match self
            .service
            .worker
            .authorize_finalization_delivery(&request, &peer)
        {
            Err(message) => uncertain(key, format!("Accepted finalization revalidation failed: {message}; inspect original outcome")),
            Ok(()) => {
                #[cfg(windows)]
                {
                    self.finalize_original_service(request, peer)
                }
                #[cfg(not(windows))]
                {
                    rejected_finalization(
                        Some(key),
                        "Native V8 finalization requires Windows; no metadata fallback".into(),
                    )
                }
            }
        };
        let outcome = match &result.result {
            ActivationFinalizationResult::Active { .. } => "active",
            ActivationFinalizationResult::Indeterminate { .. } => "indeterminate",
            ActivationFinalizationResult::Rejected { .. } => "rejected",
        };
        eprintln!(
            "WinBridge finalization {}",
            serde_json::json!({"event":"delivery_finished", "processId":std::process::id(), "generation":self.service.worker.generation, "commandId":result.command_id, "outcome":outcome, "queueWaitMs":queue_wait_ms, "executionMs":started.elapsed().as_millis()})
        );
        if reply.send(result).is_err() {
            eprintln!("WinBridge V8 reply lost; original coordinator retains outcome; observe Active over normal IPC");
        }
    }
    #[cfg(windows)]
    fn finalize_original_service(
        &mut self,
        request: ActivationFinalizationRequest,
        peer: LocalPeer,
    ) -> ActivationFinalizationResponse {
        use store::activation_finalization::ActivatedReleaseAdmission;
        std::hint::black_box(ACTIVATION_FINALIZATION_MARKER);
        let key = request.command.command_id.clone();
        let worker = Arc::clone(&self.service.worker);
        let mut timing =
            winbridge_update::activation::ActivationTiming::new("finalization_receiver", None);
        timing.mark("coordinator_accepted", None, None);
        let phase = {
            let Some(scope) = &worker.local_activation else {
                return uncertain(key, "Accepted finalization lost original scope".into());
            };
            let Ok(mode) = scope.lifetime.lock() else {
                return uncertain(key, "Accepted finalization lifetime lock poisoned".into());
            };
            match &*mode {
                ServiceLifetime::Supervised(_) => DeliveryPhase::Begin,
                ServiceLifetime::ReleasePending(_) => DeliveryPhase::Pending,
                ServiceLifetime::Active(_) => DeliveryPhase::Active,
                ServiceLifetime::Verification => {
                    return uncertain(key, "Accepted finalization has no supervised owner".into())
                }
            }
        };
        if matches!(phase, DeliveryPhase::Active) {
            let query = ActivationFinalizationQuery {
                finalization_observation_version: ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT,
                request_id: uuid::Uuid::new_v4().to_string(),
                command: request.command,
            };
            return match worker.handle_finalization_observation(query, peer).result {
                ActivationFinalizationObservationResult::Observed { receipt, .. } => {
                    ActivationFinalizationResponse {
                        finalization_version: ACTIVATION_FINALIZATION_CONTRACT,
                        command_id: Some(key),
                        result: ActivationFinalizationResult::Active { receipt },
                    }
                }
                _ => uncertain(
                    key,
                    "Active decision exists but current independent observation unavailable".into(),
                ),
            };
        }
        if matches!(phase, DeliveryPhase::Pending) {
            // Only retained known COMMIT/readiness can finish the same seal.
            // Saved/Observed SQL rows and repeated JSON cannot recreate it.
            let Some(failure) = self.finalization_recovery.known.take() else {
                return uncertain(key, "Original finalization pending without known COMMIT permit; observe only, no SQL retry".into());
            };
            let (commit, ready) = failure.into_parts();
            return self.finish_delivery_commit(key, commit, ready, &peer);
        }
        if let Err(message) = self.enter_finalization_pending(request.command, &peer) {
            // It may have sealed Pending before the last checkpoint failed.
            return uncertain(
                key,
                format!("Finalization entry not confirmed: {message}; inspect original mode"),
            );
        }
        timing.mark("pending_runtime_checked", None, None);
        let ready = match self.prepare_active_readiness(&peer) {
            Ok(ready) => ready,
            Err(message) => {
                return uncertain(
                    key,
                    format!("Finalization Pending, Active readiness unavailable: {message}"),
                )
            }
        };
        timing.mark("active_readiness_staged", None, None);
        let commit = match self.release_pending_finalization(&peer) {
            Ok(ActivatedReleaseAdmission::Committed(commit)) => commit,
            Ok(ActivatedReleaseAdmission::Observed(saved)) => {
                self.finalization_recovery.unconfirmed = Some(ready);
                return uncertain(
                    key,
                    format!(
                        "Saved release revision {} is not a known COMMIT; no Active adoption",
                        saved.revision
                    ),
                );
            }
            Err(message) => {
                self.finalization_recovery.unconfirmed = Some(ready);
                return uncertain(key, format!("SQL release unconfirmed: {message}; no retry"));
            }
        };
        timing.mark("same_store_release_committed", None, None);
        let response = self.finish_delivery_commit(key, commit, ready, &peer);
        timing.mark("active_seal_finished_or_retained", None, None);
        response
    }
    #[cfg(windows)]
    fn finish_delivery_commit(
        &mut self,
        key: String,
        commit: Box<store::activation_finalization::ActivatedReleaseCommit>,
        ready: finalization_service::active::ActiveReadiness,
        peer: &LocalPeer,
    ) -> ActivationFinalizationResponse {
        match self.finish_active_release(commit, ready, peer) {
            Ok(receipt) => ActivationFinalizationResponse {
                finalization_version: ACTIVATION_FINALIZATION_CONTRACT,
                command_id: Some(key),
                result: ActivationFinalizationResult::Active {
                    receipt: Box::new(receipt),
                },
            },
            Err(failure) => {
                let message = format!(
                    "Known SQL COMMIT retained, Active seal not confirmed: {}",
                    failure.reason
                );
                self.finalization_recovery.known = Some(failure);
                uncertain(key, message)
            }
        }
    }
}
