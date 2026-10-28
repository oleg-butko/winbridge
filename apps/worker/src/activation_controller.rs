//! V5 follows V4 on the original authenticated stream; no cold entry point.
use crate::{Result, RunningLocalService};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use winbridge_platform::infrastructure::PinnedProcess;
use winbridge_protocol::update::*;
use winbridge_update::{
    activation::{channel, controller, ActivationServiceSnapshot},
    journal::UpdateJournal,
};

pub(super) async fn receive(
    directory: &Path,
    previous: &ActivationServiceSnapshot,
    parent: &PinnedProcess,
    deadline: Instant,
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    running: &mut RunningLocalService,
) -> Result<()> {
    let physical = &previous.writer.activation;
    let source = &physical.intent.source;
    let process = &physical
        .state_owned
        .as_ref()
        .ok_or("Controller physical owner missing")?
        .process;
    let request: channel::ControllerCommand = tokio::select! {
        value=channel::read(stream,deadline.saturating_duration_since(Instant::now()))=>value?,
        outcome=super::native::parent_alive(parent,deadline.saturating_duration_since(Instant::now()))=>return outcome,
        outcome=running.wait_until_stopped()=>return Err(format!("Local IPC stopped before controller delivery: {outcome:?}")),
    };
    let snapshot = UpdateJournal::inspect_activation_controller_receiver(
        directory,
        &source.update_id,
        &physical.intent.attempt_id,
        process,
    )
    .map_err(|e| e.to_string())?;
    channel::validate_controller_command(&request, &snapshot.controller.command)?;
    if snapshot.physical != *physical
        || snapshot.writer.command != previous.writer.writer.command
        || snapshot.service.command != previous.service.command
    {
        return Err("Controller changed original same-child writer/service graph".into());
    }
    let mut checkpoint = || {
        parent.require_live().map_err(|e| e.to_string())?;
        super::native::require_handoff_window(&snapshot.local, deadline)?;
        if UpdateJournal::inspect_activation_controller_receiver(
            directory,
            &source.update_id,
            &physical.intent.attempt_id,
            process,
        )
        .map_err(|e| e.to_string())?
            != snapshot
        {
            return Err("Controller Issued graph changed before dispatch/Ready".into());
        }
        super::native::require_handoff_window(&snapshot.local, deadline)
    };
    checkpoint()?;
    running.start_controller(request.command.clone(), &mut checkpoint)?;
    loop {
        parent.require_live().map_err(|e| e.to_string())?;
        super::native::require_handoff_window(&snapshot.local, deadline)?;
        let observed: ActivationControllerObservation =
            winbridge_transport::supervisor::call_with_timeout(
                &running.endpoint()?,
                process.process_id,
                &ActivationControllerQuery {
                    controller_observation_version: 1,
                    command: request.command.clone(),
                },
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(winbridge_transport::IPC_TIMEOUT),
            )
            .await
            .map_err(|e| e.to_string())?;
        if observed.controller_observation_version != 1 {
            return Err("Controller normal IPC observation version differs".into());
        }
        match observed.result {
            ActivationControllerObservationResult::Observed {
                receipt,
                projection,
                link,
            } => {
                controller::validate_projection_evidence(&receipt, &request.command, &projection)?;
                controller::validate_link_evidence(&receipt, &request.command, &link)?;
                checkpoint()?;
                channel::write(
                    stream,
                    &channel::ControllerReady {
                        contract_version: channel::CONTROLLER_CONTRACT,
                        receipt: *receipt,
                    },
                    deadline.saturating_duration_since(Instant::now()),
                )
                .await?;
                // Still verification-owned. No transfer, release, dispatch or
                // final root commit follows from a controller acknowledgement.
                return tokio::select! {
                    value=running.wait_until_stopped()=>Err(format!("Controller verification IPC stopped: {value:?}")),
                    _=channel::read::<_,serde::de::IgnoredAny>(stream,deadline.saturating_duration_since(Instant::now()))=>Err("Controller parent channel closed or unsupported next phase; no lifetime transfer".into()),
                    outcome=super::native::parent_alive(parent,deadline.saturating_duration_since(Instant::now()))=>outcome,
                };
            }
            ActivationControllerObservationResult::Rejected { message } => return Err(message),
            ActivationControllerObservationResult::Waiting { .. } => {}
        }
        tokio::select! {
            _=tokio::time::sleep(Duration::from_millis(100))=>{},
            value=running.wait_until_stopped()=>return Err(format!("Local IPC stopped while awaiting controller: {value:?}")),
            outcome=super::native::parent_alive(parent,deadline.saturating_duration_since(Instant::now()))=>return outcome,
        }
    }
}
