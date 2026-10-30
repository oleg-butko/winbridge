//! One original outbound exchange. No saved record constructs this state owner.
use super::*;
use crate::supervisor::keeper_launch::{startup_remaining, ticket_sha};

struct Exchange {
    binding: KeeperBinding,
    ticket: KeeperLaunchTicket,
    fingerprint: String,
    start_attempted: bool,
    confirmation_attempted: bool,
    status: Option<KeeperLaunchStatus>,
}
impl Exchange {
    fn new(binding: KeeperBinding, ticket: KeeperLaunchTicket, at: u64) -> Result<Self> {
        crate::supervisor::keeper_launch::validate(&ticket, &binding)?;
        startup_remaining(ticket.prepared_at_ms, at)?;
        let fingerprint = ticket_sha(&ticket)?;
        Ok(Self {
            binding,
            ticket,
            fingerprint,
            start_attempted: false,
            confirmation_attempted: false,
            status: None,
        })
    }
    fn request(&self, command: KeeperLaunchCommand) -> KeeperLaunchRequest {
        KeeperLaunchRequest {
            keeper_launch_version: KEEPER_LAUNCH_CONTRACT,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: self.binding.clone(),
            command,
        }
    }
    fn start(&mut self, at: u64) -> Result<KeeperLaunchRequest> {
        if self.start_attempted {
            return Err(
                "Original keeper Start already attempted; observe only, never resend".into(),
            );
        }
        startup_remaining(self.ticket.prepared_at_ms, at)?;
        // Set BEFORE the first await/write. Dropping a borrowed send future does
        // not lose this flag or the caller's actual files/process resources.
        self.start_attempted = true;
        Ok(self.request(KeeperLaunchCommand::Start {
            ticket: Box::new(self.ticket.clone()),
        }))
    }
    fn observe(&self) -> Result<KeeperLaunchRequest> {
        if !self.start_attempted {
            return Err("No original keeper Start was attempted".into());
        }
        Ok(self.request(KeeperLaunchCommand::Observe {
            launch_id: self.ticket.launch_id.clone(),
            ticket_sha256: self.fingerprint.clone(),
        }))
    }
    fn confirm(&mut self, process: &SupervisorProcess) -> Result<KeeperLaunchRequest> {
        let status = self
            .status
            .as_ref()
            .ok_or("Original keeper creation is unconfirmed")?;
        if status.process.as_ref() != Some(process)
            || !matches!(
                status.phase,
                KeeperLaunchPhase::Created | KeeperLaunchPhase::ReadyConfirmed
            )
        {
            return Err("Ready confirmation differs from the original created process".into());
        }
        self.confirmation_attempted = true;
        Ok(self.request(KeeperLaunchCommand::ConfirmReady {
            launch_id: self.ticket.launch_id.clone(),
            ticket_sha256: self.fingerprint.clone(),
            process: process.clone(),
        }))
    }
    fn accept(
        &mut self,
        q: &KeeperLaunchRequest,
        reply: KeeperLaunchResponse,
        mut live: impl FnMut(&SupervisorProcess) -> Result<()>,
    ) -> Result<KeeperLaunchStatus> {
        if !self.start_attempted
            || q.binding != self.binding
            || q.keeper_launch_version != KEEPER_LAUNCH_CONTRACT
            || reply.keeper_launch_version != KEEPER_LAUNCH_CONTRACT
            || reply.request_id.as_ref() != Some(&q.request_id)
        {
            return Err(
                "Keeper launch reply changed version, original binding or correlation".into(),
            );
        }
        let status = match reply.result {
            KeeperLaunchResult::Status { status } => *status,
            KeeperLaunchResult::Rejected { message } => {
                return Err(format!("Original keeper rejected request: {message}"))
            }
        };
        if status.launch_id != self.ticket.launch_id
            || status.ticket_sha256 != self.fingerprint
            || status.expires_at_ms != self.ticket.prepared_at_ms + KEEPER_LAUNCH_STARTUP_MS
            || status
                .reason
                .as_ref()
                .is_some_and(|s| s.chars().count() > 1000)
        {
            return Err(
                "Keeper launch status changed ticket, process scope or original deadline".into(),
            );
        }
        let needs_process = matches!(
            status.phase,
            KeeperLaunchPhase::Created
                | KeeperLaunchPhase::ReadyConfirmed
                | KeeperLaunchPhase::Exited
        );
        if needs_process != status.process.is_some()
            || (status.phase == KeeperLaunchPhase::ReadyConfirmed && !self.confirmation_attempted)
        {
            return Err("Keeper launch status has no original process/confirmation".into());
        }
        if let Some(p) = &status.process {
            if p.process_id == 0
                || p.process_id == self.binding.worker.process_id
                || p.process_id == self.binding.coordinator.process_id
                || p.host_id != self.binding.worker.host_id
                || p.session_id != self.binding.worker.session_id
                || !p
                    .birth
                    .parse::<u64>()
                    .is_ok_and(|v| v > 0 && v.to_string() == p.birth)
                || self
                    .ticket
                    .predecessor
                    .as_ref()
                    .is_some_and(|a| a.process_id == p.process_id)
            {
                return Err("Keeper launch returned a foreign/aliased process identity".into());
            }
        }
        if let Some(old) = &self.status {
            let allowed = match old.phase {
                KeeperLaunchPhase::Preparing => true,
                KeeperLaunchPhase::Created => matches!(
                    status.phase,
                    KeeperLaunchPhase::Created
                        | KeeperLaunchPhase::ReadyConfirmed
                        | KeeperLaunchPhase::Exited
                ),
                KeeperLaunchPhase::ReadyConfirmed => matches!(
                    status.phase,
                    KeeperLaunchPhase::ReadyConfirmed | KeeperLaunchPhase::Exited
                ),
                KeeperLaunchPhase::Failed
                | KeeperLaunchPhase::Indeterminate
                | KeeperLaunchPhase::Exited => old == &status,
            };
            if !allowed || old.process.is_some() && old.process != status.process {
                return Err(
                    "Keeper launch observation regressed or replaced its original attempt".into(),
                );
            }
        }
        if matches!(
            status.phase,
            KeeperLaunchPhase::Created | KeeperLaunchPhase::ReadyConfirmed
        ) {
            live(status.process.as_ref().expect("Validated process presence"))?;
        }
        self.status = Some(status.clone());
        Ok(status)
    }
}
#[cfg(windows)]
mod native;
#[cfg(windows)]
pub(crate) use native::BootstrapSpawnedProcess;
#[cfg(windows)]
pub use native::{
    BootstrapLaunchAttempt, BootstrapLaunchFailure, BootstrapOwnerReleaseFailure, BootstrapSession,
};
#[cfg(test)]
mod tests;
