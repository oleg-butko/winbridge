//! V4 consumes the original activated connection/Fresh and starts local IPC only.
use crate::{
    runtime_guard::RuntimeGuard,
    store::{ActivationJournal, JournalActivationPermit},
    LocalService, Result,
};
use std::{fs::File, path::PathBuf, time::Instant};
use winbridge_platform::infrastructure::PinnedProcess;
use winbridge_protocol::update::*;
use winbridge_update::{
    activation::{channel, ActivationWriterSnapshot},
    journal::UpdateJournal,
};

pub(super) struct Receiver {
    pub config: PathBuf,
    pub config_pin: File,
    pub directory: PathBuf,
    pub writer: ActivationWriterSnapshot,
    pub parent: PinnedProcess,
    pub guard: RuntimeGuard,
    pub journal: ActivationJournal,
    pub fresh: Box<JournalActivationPermit>,
    pub deadline: Instant,
    pub stream: tokio::net::windows::named_pipe::NamedPipeClient,
}
impl Receiver {
    pub async fn run(self) -> Result<()> {
        let Self {
            config,
            config_pin,
            directory,
            writer,
            parent,
            guard,
            journal,
            fresh,
            deadline,
            mut stream,
        } = self;
        let mut timing =
            winbridge_update::activation::ActivationTiming::new("receiver_service", Some(deadline));
        let update = &writer.activation.intent.source.update_id;
        let attempt = &writer.activation.intent.attempt_id;
        let process = &writer
            .activation
            .state_owned
            .as_ref()
            .ok_or("Service physical ownership missing")?
            .process;
        super::native::require_handoff_window(&writer.activation, deadline)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let request: channel::ServiceCommand = tokio::select! {
            value=channel::read(&mut stream,remaining)=>value?,
            outcome=super::native::parent_alive(&parent,remaining)=>return outcome,
        };
        timing.mark("service_command_received", None, None);
        let snapshot = UpdateJournal::inspect_activation_service_receiver(
            &directory, update, attempt, process,
        )
        .map_err(|e| e.to_string())?;
        channel::validate_service_command(&request, &snapshot.service.command)?;
        if snapshot.writer.activation != writer.activation
            || snapshot.writer.shutdown_receipt != writer.shutdown_receipt
            || snapshot.writer.writer.command != writer.writer.command
            || request.command.binding.writer_receipt != *fresh.receipt()
            || request.command.binding.expected_worker != *guard.identity()
        {
            return Err("Service changed original same-child writer/runtime/physical scope".into());
        }
        let mut checkpoint = || {
            parent.require_live().map_err(|e| e.to_string())?;
            super::native::require_handoff_window(&snapshot.writer.activation, deadline)?;
            if UpdateJournal::inspect_activation_service_receiver(
                &directory, update, attempt, process,
            )
            .map_err(|e| e.to_string())?
                != snapshot
            {
                return Err("Service admission changed during same-state startup".into());
            }
            super::native::require_handoff_window(&snapshot.writer.activation, deadline)
        };
        checkpoint()?;
        timing.mark("service_graph_checked", None, None);
        let prepared = journal
            .prepare_local_service(fresh, request.command.clone(), &mut checkpoint)
            .map_err(|e| e.to_string())?;
        timing.mark("service_state_transferred", None, None);
        let service = LocalService::prepare(prepared, &config, guard, &mut checkpoint)
            .map_err(|e| e.to_string())?;
        // LocalService now owns its independently checked share-read-only pin.
        // The initial native receiver must not retain a hidden second pin for
        // the entire supervised/Active lifetime. No writable gap is introduced.
        service.retire_receiver_config_pin(config_pin)?;
        timing.mark("worker_initialized", None, None);
        let mut running = service.start().await.map_err(|e| e.to_string())?;
        timing.mark("local_ipc_bound", None, None);
        let result = async {
            checkpoint()?;
            let query = ActivationServiceQuery {
                service_observation_version: 1,
                command: request.command.clone(),
            };
            let observed: ActivationServiceObservation =
                winbridge_transport::supervisor::call_with_timeout(
                    &running.endpoint()?,
                    process.process_id,
                    &query,
                    deadline.saturating_duration_since(Instant::now()),
                )
                .await
                .map_err(|e| e.to_string())?;
            let ActivationServiceObservationResult::Observed { receipt } = observed.result else {
                return Err("Local IPC did not confirm actual staged runtime".into());
            };
            if observed.service_observation_version != 1 {
                return Err("Local IPC service observation version differs".into());
            }
            timing.mark("local_ipc_observed", None, None);
            let ready = channel::ServiceReady {
                contract_version: channel::SERVICE_CONTRACT,
                receipt: *receipt,
            };
            channel::validate_service_ready(&ready, &request.command)?;
            checkpoint()?;
            channel::write(
                &mut stream,
                &ready,
                deadline.saturating_duration_since(Instant::now()),
            )
            .await?;
            timing.mark("service_ready_sent", None, None);
            // V5 still retains the same local owner and original window.
            super::controller::receive(
                &directory,
                &snapshot,
                &parent,
                deadline,
                &mut stream,
                &mut running,
            )
            .await
        }
        .await;
        let stopped = running.close().await.map_err(|e| e.to_string());
        match (result, stopped) {
            (Err(reason), Err(cleanup)) => Err(format!(
                "{reason}; local IPC cleanup unconfirmed: {cleanup}"
            )),
            (result, Ok(())) => result,
            (Ok(()), Err(cleanup)) => Err(cleanup),
        }
    }
}
