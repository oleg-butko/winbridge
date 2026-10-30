//! Windows owner retains the original pins across every borrowed async call.
use super::*;
use crate::{
    journal::{UpdateAuthority, UpdateJournal},
    supervisor::SupervisorHandle,
};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use winbridge_platform::infrastructure::PinnedProcess;
use winbridge_transport::supervisor as ipc;
fn clock() -> Result<u64> {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    if at > winbridge_core::update::MAX_UPDATE_COUNTER as u128 {
        return Err("Bootstrap clock exceeds supported counter".into());
    }
    Ok(at as u64)
}
#[must_use = "Retain this original attempt after timeout/cancellation; only Observe may follow a Start"]
pub struct BootstrapLaunchAttempt {
    prepared: Box<PreparedBootstrap>,
    exchange: Exchange,
    endpoint: String,
    directory: PathBuf,
    process: Option<Arc<PinnedProcess>>,
    publish_attempted: bool,
    spawned: Option<Box<crate::journal::BootstrapSpawnedCommit>>,
    reason: Option<String>,
}
#[must_use = "Failed launch preparation retains original pins and known SQL fact"]
pub struct BootstrapLaunchFailure {
    pub reason: String,
    _prepared: Box<PreparedBootstrap>,
}
#[must_use = "Retain the original owner and process after failed release; no takeover/replay"]
pub struct BootstrapOwnerReleaseFailure {
    pub reason: String,
    attempt: Box<BootstrapLaunchAttempt>,
    owner: UpdateAuthority,
}
impl BootstrapOwnerReleaseFailure {
    pub fn into_parts(self) -> (Box<BootstrapLaunchAttempt>, UpdateAuthority, String) {
        (self.attempt, self.owner, self.reason)
    }
}
/// Minted only after the original Spawned COMMIT and final physical checks.
/// It cannot be deserialized or constructed from a PID/saved supervisor record.
pub(crate) struct BootstrapSpawnedProcess {
    record: SupervisorRecord,
    pin: Arc<PinnedProcess>,
    directory: PathBuf,
    token: String,
}
impl BootstrapSpawnedProcess {
    pub(crate) fn into_parts(self) -> (SupervisorRecord, Arc<PinnedProcess>, PathBuf, String) {
        (self.record, self.pin, self.directory, self.token)
    }
}
#[must_use = "Keep this session until Ready/Handoff is known; dropping never kills the independent coordinator"]
pub struct BootstrapSession {
    attempt: Box<BootstrapLaunchAttempt>,
    handle: SupervisorHandle,
    ready_confirmed: bool,
}
impl PreparedBootstrap {
    pub fn begin_launch(
        self: Box<Self>,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Box<BootstrapLaunchAttempt>, Box<BootstrapLaunchFailure>> {
        let mut trace = PhaseTrace::new("begin_launch", &self.reservation().intent.launch_id);
        let result = (|| {
            self.require_current(j, a, &mut own_state)?;
            let reservation = self.reservation();
            let directory =
                PathBuf::from(&reservation.supervisor.spec.runtime_directory).join("update-state");
            let ancestor = UpdateJournal::inspect_bootstrap(
                &directory,
                &reservation.intent.previous.launch_id,
            )
            .map_err(|e| e.to_string())?;
            let predecessor = ancestor.map(|v| v.intent.previous.coordinator);
            let ticket = self.files.launch_ticket(reservation, predecessor)?;
            let exchange = Exchange::new(self.capture.binding().clone(), ticket, clock()?)?;
            let endpoint = ipc::endpoint(&directory, &exchange.binding.launch_id)
                .map_err(|e| e.to_string())?;
            self.capture.require_current()?;
            own_state()?;
            Ok((exchange, endpoint, directory))
        })();
        trace.finish(result.is_ok());
        match result {
            Ok((exchange, endpoint, directory)) => Ok(Box::new(BootstrapLaunchAttempt {
                prepared: self,
                exchange,
                endpoint,
                directory,
                process: None,
                publish_attempted: false,
                spawned: None,
                reason: None,
            })),
            Err(reason) => Err(Box::new(BootstrapLaunchFailure {
                reason,
                _prepared: self,
            })),
        }
    }
}
impl BootstrapLaunchAttempt {
    pub fn status(&self) -> Option<&KeeperLaunchStatus> {
        self.exchange.status.as_ref()
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    fn retain<T>(&mut self, result: Result<T>) -> Result<T> {
        self.reason = result
            .as_ref()
            .err()
            .map(|v| v.chars().take(1000).collect());
        result
    }
    fn require_original(&self) -> Result<()> {
        self.prepared.capture.require_original_processes()?;
        self.prepared.files.require_current()
    }
    fn require_created(&self) -> Result<()> {
        let status = self
            .status()
            .ok_or("Original creation has no observed status")?;
        let pin = self
            .process
            .as_ref()
            .ok_or("Original created process pin missing")?;
        if !matches!(
            status.phase,
            KeeperLaunchPhase::Created | KeeperLaunchPhase::ReadyConfirmed
        ) || status.process.as_ref() != Some(pin.identity())
        {
            return Err("Original keeper creation is not live/confirmed".into());
        }
        pin.require_outside_job(&self.exchange.binding.job_name)
            .map_err(|e| e.to_string())?;
        pin.require_outside_job(&self.exchange.ticket.excluded_job)
            .map_err(|e| e.to_string())
    }
    async fn exchange(&mut self, q: KeeperLaunchRequest) -> Result<KeeperLaunchStatus> {
        let started = Instant::now();
        let action = match &q.command {
            KeeperLaunchCommand::Start { .. } => "start",
            KeeperLaunchCommand::Observe { .. } => "observe",
            KeeperLaunchCommand::ConfirmReady { .. } => "confirm_ready",
        };
        let result = async {
            let reply: KeeperLaunchResponse = ipc::call(
                &self.endpoint,
                self.exchange.binding.coordinator.process_id,
                &q,
            )
            .await
            .map_err(|e| format!("Keeper exchange unconfirmed; retain original attempt and Observe, never resend Start: {e}"))?;
            let binding = self.exchange.binding.clone();
            let excluded = self.exchange.ticket.excluded_job.clone();
            let pin = &mut self.process;
            self.exchange.accept(&q, reply, |p| {
                if let Some(held) = pin.as_ref() {
                    if held.identity() != p {
                        return Err("Keeper changed the exact held coordinator process".into());
                    }
                } else {
                    *pin = Some(Arc::new(PinnedProcess::live(p).map_err(|e| e.to_string())?));
                }
                let held = pin.as_ref().expect("Retained original process");
                held.require_outside_job(&binding.job_name).map_err(|e| e.to_string())?;
                held.require_outside_job(&excluded).map_err(|e| e.to_string())
            })
        }.await;
        eprintln!(
            "WinBridge bootstrap {}",
            serde_json::json!({"event":"keeper_exchange","action":action,"launchId":self.exchange.ticket.launch_id,"elapsedMs":started.elapsed().as_millis(),"phase":result.as_ref().ok().map(|s|s.phase),"error":result.as_ref().err()})
        );
        result
    }
    /// Exactly one Start. A dropped/timed-out borrowed future leaves this attempt
    /// resident with start_attempted=true and all original file/process pins.
    pub async fn send_start(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> Result<KeeperLaunchStatus> {
        let mut trace = PhaseTrace::new(
            "start_preflight_and_exchange",
            &self.exchange.ticket.launch_id,
        );
        // Finish the synchronous SQL preflight before constructing the I/O
        // future. A shared &UpdateJournal is not Send (SQLite is not Sync).
        let request = (|| {
            if self.exchange.start_attempted {
                return Err("Keeper Start already attempted; only Observe is permitted".into());
            }
            self.prepared.require_current(j, a, &mut own_state)?;
            self.exchange.start(clock()?)
        })();
        let result = match request {
            Ok(q) => {
                self.reason=Some("Keeper Start outcome unconfirmed; Observe the original attempt, never resend Start".into());
                self.exchange(q).await
            }
            Err(reason) => Err(reason),
        };
        trace.finish(result.is_ok());
        self.retain(result)
    }
    /// Read-only continuation, including after the original 3s probe expires.
    /// It never refreshes a deadline, writes SQL or reconstructs custody.
    pub async fn observe(&mut self) -> Result<KeeperLaunchStatus> {
        let result = async {
            self.prepared.capture.require_original_processes()?;
            let q = self.exchange.observe()?;
            self.exchange(q).await
        }
        .await;
        self.retain(result)
    }
    pub fn publish_spawned(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> Result<SupervisorRecord> {
        let mut trace = PhaseTrace::new("publish_spawned", &self.exchange.ticket.launch_id);
        let result = (|| {
            if self.publish_attempted {
                return Err("Original Spawned write already attempted; no replay/adoption".into());
            }
            self.require_original()?;
            self.require_created()?;
            own_state()?;
            j.require_known_bootstrap(a, &self.prepared.known)
                .map_err(|e| e.to_string())?;
            startup_remaining(self.exchange.ticket.prepared_at_ms, clock()?)?;
            self.publish_attempted = true;
            let process = self
                .process
                .as_ref()
                .expect("Checked process")
                .identity()
                .clone();
            let fact=j.commit_bootstrap_spawned(a,&self.prepared.known,&process,|| {
                self.require_original()?;self.require_created()?;own_state()?;
                startup_remaining(self.exchange.ticket.prepared_at_ms,clock()?).map(|_|())
            }).map_err(|e|format!("Bootstrap Spawned write unconfirmed; retain original owner/attempt, no replay: {e}"))?;
            let r = fact.record().clone();
            self.spawned = Some(fact); // Keep the opaque COMMIT before later checks may fail.
            self.require_original()?;
            self.require_created()?;
            own_state()?;
            Ok(r)
        })();
        trace.finish(result.is_ok());
        self.retain(result)
    }
    pub fn release_owner(
        self: Box<Self>,
        j: &UpdateJournal,
        owner: UpdateAuthority,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> std::result::Result<Box<BootstrapSession>, Box<BootstrapOwnerReleaseFailure>> {
        let mut trace = PhaseTrace::new("release_owner", &self.exchange.ticket.launch_id);
        let result = (|| {
            self.require_original()?;
            self.require_created()?;
            own_state()?;
            let r = self
                .spawned
                .as_ref()
                .ok_or("Original Spawned COMMIT not known; no owner release")?;
            j.require_bootstrap_spawned(&owner, &self.prepared.known, r)
                .map_err(|e| e.to_string())?;
            startup_remaining(self.exchange.ticket.prepared_at_ms, clock()?)?;
            Ok(())
        })();
        if let Err(reason) = result {
            trace.finish(false);
            return Err(Box::new(BootstrapOwnerReleaseFailure {
                reason,
                attempt: self,
                owner,
            }));
        }
        let attempt = self;
        let sealed = BootstrapSpawnedProcess {
            record: attempt
                .spawned
                .as_ref()
                .expect("Checked Spawned")
                .record()
                .clone(),
            pin: Arc::clone(attempt.process.as_ref().expect("Checked original process")),
            directory: attempt.directory.clone(),
            token: attempt.exchange.ticket.token.clone(),
        };
        let handle = SupervisorHandle::from_bootstrap(sealed);
        drop(owner); // Only after the exact original Spawned fact and physical checks.
        trace.finish(true);
        Ok(Box::new(BootstrapSession {
            attempt,
            handle,
            ready_confirmed: false,
        }))
    }
}
impl BootstrapSession {
    pub fn record(&self) -> &SupervisorRecord {
        &self.handle.record
    }
    pub fn reason(&self) -> Option<&str> {
        self.attempt.reason()
    }
    pub async fn wait_ready_and_confirm(&mut self) -> Result<SupervisorRecord> {
        let result = async {
            self.attempt.require_original()?;
            let r = self.handle.wait_ready().await?;
            if r.spec != self.attempt.prepared.reservation().supervisor.spec
                || r.revision != 2
                || r.updated_at_ms
                    >= self.attempt.exchange.ticket.prepared_at_ms + KEEPER_LAUNCH_STARTUP_MS
            {
                return Err("Ready changed original bootstrap spec".into());
            }
            let p = r
                .process
                .as_ref()
                .ok_or("Ready lost original coordinator identity")?;
            let q = self.attempt.exchange.confirm(p)?;
            self.attempt.reason = Some(
                "Keeper Ready confirmation unconfirmed; Observe the original keeper attempt".into(),
            );
            let status = self.attempt.exchange(q).await?;
            if status.phase != KeeperLaunchPhase::ReadyConfirmed {
                return Err("Original keeper did not confirm Ready".into());
            }
            self.ready_confirmed = true;
            Ok(r)
        }
        .await;
        self.attempt.retain(result)
    }
    /// Resolve a lost Ready-confirmation reply without resending any mutation.
    pub async fn observe_keeper(&mut self) -> Result<KeeperLaunchStatus> {
        let status = self.attempt.observe().await?;
        if status.phase == KeeperLaunchPhase::ReadyConfirmed {
            if status.process != self.handle.record.process {
                return Err("Keeper observation changed this session's Ready process".into());
            }
            self.ready_confirmed = true;
        }
        Ok(status)
    }
    pub async fn handoff(
        &mut self,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> Result<SupervisorRecord> {
        let result = async {
            if !self.ready_confirmed {
                return Err("Original keeper Ready must be confirmed before Handoff".into());
            }
            self.attempt.require_original()?;
            own_state()?;
            self.attempt.reason=Some("Handoff outcome unconfirmed; Observe this same supervisor session, never resend Handoff".into());
            self.handle.command(SupervisorAction::Handoff).await
        }
        .await;
        self.attempt.retain(result)
    }
    pub async fn observe_handoff(&mut self) -> Result<SupervisorRecord> {
        let result = async {
            self.attempt.prepared.capture.require_original_processes()?;
            self.handle.command(SupervisorAction::Observe).await
        }
        .await;
        self.attempt.retain(result)
    }
    pub fn finish(mut self: Box<Self>) -> std::result::Result<SupervisorHandle, Box<Self>> {
        if let Err(reason) = self.attempt.require_created() {
            self.attempt.reason = Some(reason);
            return Err(self);
        }
        if !self.ready_confirmed || self.handle.record.phase != SupervisorPhase::Active {
            self.attempt.reason = Some(
                "Handoff is not known Active; retain this original session and observe".into(),
            );
            return Err(self);
        }
        // The already-independent handle survives retiring all original pins.
        let BootstrapSession { handle, .. } = *self;
        Ok(handle)
    }
}

#[cfg(test)]
mod send_tests {
    use super::*;
    #[test]
    fn borrowed_bootstrap_futures_can_run_in_the_worker_send_task() {
        fn send<T: Send>(_: T) {}
        // Compile the actual public futures, without constructing fake live
        // custody or claiming this is a Windows IPC/runtime proof.
        let _signatures = |attempt: &mut BootstrapLaunchAttempt,
                           journal: &mut UpdateJournal,
                           authority: &UpdateAuthority,
                           session: &mut BootstrapSession| {
            send(attempt.send_start(journal, authority, || Ok(())));
            send(attempt.observe());
            send(session.wait_ready_and_confirm());
            send(session.observe_keeper());
            send(session.handoff(|| Ok(())));
            send(session.observe_handoff());
        };
    }
}
