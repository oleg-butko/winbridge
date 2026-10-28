//! V6 continues V5 on the same authenticated stream and resident service.
use crate::{Result, RunningLocalService};
use std::{path::Path, time::Instant};
use winbridge_platform::infrastructure::PinnedProcess;
use winbridge_update::{
    activation::{channel, controller::ActivationControllerSnapshot},
    journal::UpdateJournal,
};

pub(super) async fn receive(
    directory: &Path,
    previous: &ActivationControllerSnapshot,
    parent: &PinnedProcess,
    deadline: Instant,
    stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    running: &mut RunningLocalService,
) -> Result<()> {
    super::native::require_handoff_window(&previous.local, deadline)?;
    let request: channel::LifetimeCommand = tokio::select! {
        value = channel::read(stream, deadline.saturating_duration_since(Instant::now())) => value?,
        outcome = super::native::parent_alive(parent, deadline.saturating_duration_since(Instant::now())) => return outcome,
        outcome = running.wait_until_stopped() => return Err(format!("Local IPC stopped before lifetime delivery: {outcome:?}")),
    };
    let source = &previous.local.intent.source;
    let process = &previous
        .local
        .state_owned
        .as_ref()
        .ok_or("Lifetime child missing")?
        .process;
    let snapshot = Box::new(
        UpdateJournal::inspect_activation_lifetime_receiver(
            directory,
            &source.update_id,
            &previous.local.intent.attempt_id,
            process,
        )
        .map_err(|e| e.to_string())?,
    );
    channel::validate_lifetime_command(&request, &snapshot.lifetime.command)?;
    if snapshot.controller.physical != previous.physical
        || snapshot.controller.writer != previous.writer
        || snapshot.controller.service != previous.service
        || snapshot.controller.local != previous.local
        || snapshot.controller.controller.command != previous.controller.command
        || parent.identity() != &request.command.binding.keeper
    {
        return Err("Lifetime changed original controller/local/parent graph".into());
    }
    let mut checkpoint = || {
        parent.require_live().map_err(|e| e.to_string())?;
        super::native::require_handoff_window(&snapshot.controller.local, deadline)?;
        if UpdateJournal::inspect_activation_lifetime_receiver(
            directory,
            &source.update_id,
            &previous.local.intent.attempt_id,
            process,
        )
        .map_err(|e| e.to_string())?
            != *snapshot
        {
            return Err("Lifetime Issued graph changed before entry".into());
        }
        super::native::require_handoff_window(&snapshot.controller.local, deadline)
    };
    checkpoint()?;
    let receipt = running.enter_supervised_lifetime(request.command, parent, &mut checkpoint)?;
    let ready = channel::LifetimeReady {
        contract_version: channel::LIFETIME_CONTRACT,
        receipt,
    };
    // An accepted decision survives an unknown private ACK under the original
    // keeper. Parent process loss/Job closure and service failure remain fatal;
    // the independent normal endpoint can still observe quarantine after EOF.
    if let Err(reason) = channel::write(
        stream,
        &ready,
        deadline.saturating_duration_since(Instant::now()),
    )
    .await
    {
        eprintln!(
            "Lifetime Ready unconfirmed; same-child supervised quarantine retained: {reason}"
        );
    }
    running.hold_supervised_lifetime().await
}
