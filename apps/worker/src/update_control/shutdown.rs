//! v6 stop acceptance. An ordinary result DTO never owns the stop effect.
use super::*;
use crate::store::{ShutdownAcceptance, ShutdownAcceptancePermit};
use winbridge_update::shutdown::{ShutdownDeliveryState, ShutdownStatePins};

pub(super) struct ControlReply {
    pub(super) result: WorkerUpdateControlResult,
    pub(super) stop_after_reply: Option<StopAfterReply>,
}
impl ControlReply {
    pub(super) fn error(message: String) -> Self {
        Self {
            result: WorkerUpdateControlResult::Error { message },
            stop_after_reply: None,
        }
    }
}
/// Created only after fresh durable acceptance. Its owner is the blocking
/// callback's return value, then the async reply attempt. Cancellation cannot
/// discard a committed stop. The existing serve loop joins control callbacks.
pub(super) struct StopAfterReply {
    worker: Arc<Worker>,
    _acceptance: Box<ShutdownAcceptancePermit>,
}
impl Drop for StopAfterReply {
    fn drop(&mut self) {
        self.worker.stop();
    }
}
pub(super) fn prepare_reply(
    worker: Arc<Worker>,
    reg: &Registration,
    q: &WorkerUpdateControlRequest,
    peer: u32,
) -> Result<ControlReply> {
    if worker.activation_read_only() {
        return Err("Activated update-control listener is staged; worker is not Active".into());
    }
    if let WorkerUpdateControlCommand::RequestShutdown { command } = &q.command {
        let acceptance = accept(&worker, reg, q, peer, command)?;
        let (receipt, stop_after_reply) = match acceptance {
            ShutdownAcceptance::Fresh(permit) => (
                permit.receipt().clone(),
                Some(StopAfterReply {
                    worker,
                    _acceptance: permit,
                }),
            ),
            ShutdownAcceptance::Observed(receipt) => (*receipt, None),
        };
        Ok(ControlReply {
            result: WorkerUpdateControlResult::ShutdownReceipt {
                command: command.clone(),
                receipt: Some(Box::new(receipt)),
                observed_at_ms: crate::store::now_ms(),
            },
            stop_after_reply,
        })
    } else {
        worker
            .control_result(reg, q, peer)
            .map(|result| ControlReply {
                result,
                stop_after_reply: None,
            })
    }
}
fn state(worker: &Worker) -> Result<ShutdownStatePins> {
    let pins =
        ShutdownStatePins::capture(&worker.config.state_directory).map_err(|e| e.to_string())?;
    if pins.observation().lock_identity != worker._instance_lock.lock_identity()? {
        return Err(
            "Shutdown state lock differs from the running worker's held instance lock".into(),
        );
    }
    Ok(pins)
}
pub(super) fn observation(
    worker: &Worker,
    reg: &Registration,
    q: &WorkerUpdateControlRequest,
    peer: u32,
) -> Result<WorkerUpdateControlResult> {
    if q.binding.context != worker.request_context()? {
        return Err("Shutdown observation context changed".into());
    }
    let root = worker.control_authority(reg, q, peer)?;
    match &q.command {
        WorkerUpdateControlCommand::ObserveShutdownState => {
            worker.require_control_config_current(reg)?;
            let pins = state(worker)?;
            pins.revalidate().map_err(|e| e.to_string())?;
            worker.control_authority(reg, q, peer)?;
            Ok(WorkerUpdateControlResult::ShutdownState {
                state: pins.observation().clone(),
            })
        }
        WorkerUpdateControlCommand::ObserveShutdownReceipt { command } => {
            let store = worker.store()?;
            let receipt = store.worker_shutdown_receipt(&root.update_id, command)?;
            if let Some(r) = &receipt {
                require_saved_spec(reg, q, &root, &r.lease.spec)?;
            }
            Ok(WorkerUpdateControlResult::ShutdownReceipt {
                command: command.clone(),
                receipt: receipt.map(Box::new),
                observed_at_ms: crate::store::now_ms(),
            })
        }
        _ => Err("Shutdown request requires the reply-retirement owner, not a data handler".into()),
    }
}
fn accept(
    worker: &Worker,
    reg: &Registration,
    q: &WorkerUpdateControlRequest,
    peer: u32,
    command: &WorkerShutdownCommand,
) -> Result<ShutdownAcceptance> {
    let root = worker.control_authority(reg, q, peer)?;
    if q.binding.context != worker.request_context()? || root.phase != UpdatePhase::Stopping {
        return Err("Shutdown acceptance requires exact current context and root Stopping".into());
    }
    let saved = UpdateJournal::inspect_shutdown(&reg.directory, &root.update_id)
        .map_err(|e| e.to_string())?
        .ok_or("Shutdown intent not committed")?;
    let intent = &saved.intent;
    let b = &intent.binding;
    if intent.command != *command
        || b.worker != reg.binding
        || b.launch_id != q.launch_id
        || b.owner_id != q.updater_owner_id
        || b.owner_epoch != q.updater_epoch
        || root.revision != b.parent_revision + 1
        || !matches!(
            saved.delivery,
            ShutdownDeliveryState::Issued { .. }
                | ShutdownDeliveryState::Uncertain { .. }
                | ShutdownDeliveryState::Acknowledged { .. }
        )
    {
        return Err("Shutdown exact immutable issued intent/owner/revision differs".into());
    }
    require_saved_spec(reg, q, &root, &b.lease.spec)?;
    // Historical delivery has no stop permit, including during stop/config change.
    if let Some(receipt) = worker
        .store()?
        .worker_shutdown_receipt(&root.update_id, command)?
    {
        intent.validate_receipt(&receipt)?;
        return Ok(ShutdownAcceptance::Observed(Box::new(receipt)));
    }
    if !matches!(
        saved.delivery,
        ShutdownDeliveryState::Issued { .. } | ShutdownDeliveryState::Uncertain { .. }
    ) {
        return Err("Updater ACK without worker receipt cannot authorize a fresh stop".into());
    }
    worker.require_control_config_current(reg)?;
    let guard = worker
        .runtime_guard
        .lock()
        .map_err(|_| "Runtime guard lock poisoned")?
        .clone()
        .ok_or("Loaded runtime guard required")?;
    guard.require_installation(&root.request.expected.installation_id, &reg.scope)?;
    guard.revalidate()?;
    if guard.identity() != &b.loaded_worker {
        return Err("Shutdown loaded worker identity changed".into());
    }
    let pins = state(worker)?;
    if pins.observation() != &b.worker_state {
        return Err("Shutdown state directory/journal/lock identity changed".into());
    }
    if worker.active_jobs.load(Ordering::Acquire) != 0
        || !worker
            .active_projects
            .lock()
            .map_err(|_| "Process registry lock poisoned")?
            .is_empty()
        || !worker
            .checks
            .lock()
            .map_err(|_| "Readiness check lock poisoned")?
            .is_empty()
        || worker.maintenance_running.load(Ordering::Acquire)
    {
        return Err("Worker effects still own state; shutdown not accepted".into());
    }
    // Reobserve authority before the worker transaction. The two journals have
    // no shared transaction; any later updater failure must reconcile this receipt.
    worker.control_authority(reg, q, peer)?;
    worker.require_control_config_current(reg)?;
    pins.revalidate().map_err(|e| e.to_string())?;
    let mut store = worker.store()?;
    let observed = store.worker_drain_observation(
        &root.update_id,
        reg.binding.context.clone(),
        &worker.generation,
        0,
    )?;
    if observed.lease.as_deref() != Some(&b.lease)
        || observed.accepted_operations != 0
        || observed.logical_owners != 0
        || observed.in_flight_effects != 0
    {
        return Err("Worker no longer has exact quiesced state".into());
    }
    #[cfg(windows)]
    {
        use winbridge_platform::process_identity::ProcessObservation;
        let domain = worker
            .execution_domain
            .lock()
            .map_err(|_| "Execution domain lock poisoned")?;
        if !domain.as_ref().is_some_and(|d| {
            matches!(
                winbridge_platform::execution_domain::observe_domain(d.identity()),
                ProcessObservation::Empty(_)
            )
        }) {
            return Err("Worker execution domain is occupied or unknown".into());
        }
    }
    store.accept_worker_shutdown(WorkerShutdownReceipt {
        schema_version: 1,
        update_id: root.update_id,
        launch_id: q.launch_id.clone(),
        updater_owner_id: q.updater_owner_id.clone(),
        updater_epoch: q.updater_epoch,
        command: command.clone(),
        lease: b.lease.clone(),
        process: reg.binding.process.clone(),
        state: pins.observation().clone(),
        accepted_at_ms: crate::store::now_ms(),
    })
}
