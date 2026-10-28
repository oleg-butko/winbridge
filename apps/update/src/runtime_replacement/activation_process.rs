//! Consuming suspended launch. Ready retains original state; no writer handoff.
use super::*;
use crate::activation::{channel, ActivationCreated, ActivationRecord, ActivationState};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use winbridge_platform::{infrastructure, ManagedChild, OutputReader};
use winbridge_transport::supervisor as ipc;
#[path = "activation_controller.rs"]
mod controller;
pub use controller::ActivationControllerPreparation;
#[path = "activation_service.rs"]
mod service;
#[cfg(feature = "update-validation")]
#[path = "activation_process_validation.rs"]
mod validation;
#[path = "activation_writer.rs"]
mod writer;
pub use service::ActivationServicePreparation;

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    incomplete: bool,
}
fn drain(reader: OutputReader, capture: Arc<Mutex<Capture>>) -> Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("activation-output".into())
        .spawn(move || {
            let mut reader = reader;
            let mut chunk = [0; 4096];
            loop {
                let result = reader.read(&mut chunk);
                let mut state = capture.lock().unwrap_or_else(|p| p.into_inner());
                match result {
                    Ok(0) => break,
                    Ok(n) => {
                        let keep = n.min(65536usize.saturating_sub(state.bytes.len()));
                        state.bytes.extend_from_slice(&chunk[..keep]);
                        state.incomplete |= keep != n;
                    }
                    Err(_) => {
                        state.incomplete = true;
                        break;
                    }
                }
            }
        })
        .map_err(|_| "Activation output reader could not start".into())
}
/// Original file/state custody, one actual process/Job and its private channel.
/// Cannot be deserialized, cloned, or constructed from a saved Created receipt.
pub struct ActivationProcessGuard {
    files: Box<ActivationFilesGuard>,
    record: ActivationRecord,
    child: Option<ManagedChild>,
    channel: Option<ipc::Stream>,
    captures: [Arc<Mutex<Capture>>; 2],
    readers: Vec<thread::JoinHandle<()>>,
    released: Option<crate::shutdown::ReleasedExitCustody>,
    _controller_config_pin: Option<std::fs::File>,
    controller_observation: Option<winbridge_protocol::update::ActivationControllerObservation>,
}
/// Retain the same OS resources after error, including an uncertain live child.
/// No method converts this object into a second creation attempt.
pub struct ActivationLaunchFailure {
    pub reason: String,
    custody: Box<ActivationProcessGuard>,
}
impl std::fmt::Debug for ActivationLaunchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivationLaunchFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ActivationLaunchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for ActivationLaunchFailure {}
impl ActivationLaunchFailure {
    pub fn record(&self) -> &ActivationRecord {
        &self.custody.record
    }
    pub fn process_id(&self) -> Option<u32> {
        self.custody.child.as_ref().map(ManagedChild::id)
    }
    pub(crate) fn resident_process(&self) -> Option<winbridge_protocol::update::SupervisorProcess> {
        self.custody.resident_process()
    }
    pub(crate) fn process_alive(&self) -> Option<bool> {
        self.custody.process_alive()
    }
    pub(crate) fn diagnostics(&self) -> (String, String, bool, bool) {
        self.custody.diagnostics()
    }
    pub(crate) fn original_lock_held(&self) -> Option<bool> {
        self.custody.original_lock_held()
    }
}
impl Drop for ActivationProcessGuard {
    fn drop(&mut self) {
        // This requests kill-on-close without an unbounded ManagedChild::drop
        // wait. Dropping is not positive evidence of process/tree cleanup.
        if let Some(child) = self.child.take() {
            child.close_unconfirmed();
        }
    }
}
impl ActivationFilesGuard {
    /// Poll on a dedicated stack using the ORIGINAL supervisor IO driver. A
    /// temporary Runtime would strand the retained named pipe on its shutdown.
    pub fn launch_receiver_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<ActivationProcessGuard>, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |files| files.launch_receiver(j, a, s)) {
            Ok(result) => result,
            Err((files, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: ActivationProcessGuard::new(files),
            }),
        }
    }

    pub async fn launch_receiver(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<ActivationProcessGuard>, ActivationLaunchFailure> {
        let mut owner = ActivationProcessGuard::new(self);
        match owner.launch(j, a, s).await {
            Ok(()) => Ok(owner),
            Err(reason) => {
                owner.record_uncertainty(j, a, s);
                Err(ActivationLaunchFailure {
                    reason,
                    custody: owner,
                })
            }
        }
    }
}
impl ActivationProcessGuard {
    fn new(files: Box<ActivationFilesGuard>) -> Box<Self> {
        Box::new(Self {
            record: files.preparation().clone(),
            files,
            child: None,
            channel: None,
            captures: [
                Arc::new(Mutex::new(Capture::default())),
                Arc::new(Mutex::new(Capture::default())),
            ],
            readers: Vec::new(),
            released: None,
            _controller_config_pin: None,
            controller_observation: None,
        })
    }
    pub(crate) fn resident_process(&self) -> Option<winbridge_protocol::update::SupervisorProcess> {
        let identity = self.child.as_ref()?.identity()?;
        Some(winbridge_protocol::update::SupervisorProcess {
            process_id: identity.process_id,
            birth: identity.creation_time.to_string(),
            host_id: identity.host_id.clone(),
            session_id: identity.session_id,
        })
    }
    /// Readonly liveness of an actually retained child; never reconstructs an
    /// owner from a PID. A denied/inconclusive observation remains unknown.
    pub(crate) fn process_alive(&self) -> Option<bool> {
        let actual = self.resident_process()?;
        match infrastructure::PinnedProcess::live(&actual) {
            Ok(_) => Some(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
            Err(_) => None,
        }
    }
    pub fn record(&self) -> &ActivationRecord {
        &self.record
    }
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(ManagedChild::id)
    }
    pub(crate) fn original_lock_held(&self) -> Option<bool> {
        self.released.as_ref().map_or_else(
            || Some(self.files.original.signal.held()),
            crate::shutdown::ReleasedExitCustody::original_lock_held,
        )
    }
    /// Consume the same Ready owner once. There is no path from a failure or
    /// cold record back to this factory. StateOwned still grants no Store writer.
    pub fn handoff_state_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<Self>, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.handoff_state(j, a, s).await {
                Ok(()) => Ok(owner),
                Err(reason) => {
                    owner.record_uncertainty(j, a, s);
                    Err(ActivationLaunchFailure {
                        reason,
                        custody: owner,
                    })
                }
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }
    /// Bounded diagnostics. The private challenge is never written to stdout.
    pub fn output(&self) -> (Vec<u8>, Vec<u8>, bool) {
        let stdout = self.captures[0].lock().unwrap_or_else(|p| p.into_inner());
        let stderr = self.captures[1].lock().unwrap_or_else(|p| p.into_inner());
        (
            stdout.bytes.clone(),
            stderr.bytes.clone(),
            stdout.incomplete || stderr.incomplete,
        )
    }
    pub(crate) fn diagnostics(&self) -> (String, String, bool, bool) {
        fn tail(bytes: &[u8]) -> (String, bool) {
            let text = String::from_utf8_lossy(bytes);
            let count = text.chars().count();
            (
                text.chars().skip(count.saturating_sub(512)).collect(),
                count > 512,
            )
        }
        let (stdout, stderr, incomplete) = self.output();
        let (stdout, cut_out) = tail(&stdout);
        let (stderr, cut_err) = tail(&stderr);
        (
            stdout,
            stderr,
            incomplete || cut_out || cut_err,
            self.readers.len() == 2 && self.readers.iter().all(|r| r.is_finished()),
        )
    }
    fn at(&self) -> Result<u64> {
        crate::journal::now_ms()
            .map(|at| at.max(self.record.updated_at_ms))
            .map_err(|e| e.to_string())
    }
    fn require_custody(
        &self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        grant: bool,
    ) -> Result<RuntimeReplacementRecord> {
        let mut timing = crate::activation::ActivationTiming::new(
            "process_custody",
            Some(self.files.preparation.deadline),
        );
        let r = j
            .require_activation_owned_current(a, s, &self.files.preparation, &self.record)
            .map_err(|e| e.to_string())?;
        timing.mark("graph_checked", None, None);
        self.require_fixed_custody(grant)?;
        timing.mark("physical_checked", None, None);
        Ok(r)
    }
    fn require_fixed_custody(&self, grant: bool) -> Result<()> {
        match &self.released {
            Some(custody) => {
                custody.require_released(self.files.original.permit.shutdown_intent())?
            }
            None => self.files.original.require_exit_custody()?,
        }
        self.files
            .original
            .anchors
            .as_ref()
            .ok_or("Activation anchors missing")?
            .revalidate()?;
        self.files
            .original
            .retained
            .as_ref()
            .ok_or("Activation retained directory missing")?
            .revalidate()
            .map_err(|e| e.to_string())?;
        self.files.installed.revalidate()?;
        if grant {
            self.files.preparation.require_live_window()?;
        }
        Ok(())
    }
    async fn handoff_state(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "handoff",
            Some(self.files.preparation.deadline),
        );
        if self.released.is_some()
            || !matches!(self.record.state, ActivationState::ReadyForState {})
            || self.record.revision != 4
            || self.channel.is_none()
            || self.resident_process().as_ref() != self.record.created.as_ref().map(|c| &c.process)
        {
            return Err(
                "State handoff needs the original live Ready process/channel; no replay".into(),
            );
        }
        // This is still a readonly capability query. Full physical validation
        // follows it immediately before HandoffIssued, and again before unlock.
        let r = j
            .require_activation_owned_current(a, s, &self.files.preparation, &self.record)
            .map_err(|e| e.to_string())?;
        let process = &self
            .record
            .created
            .as_ref()
            .ok_or("Created child missing")?
            .process;
        let live = infrastructure::PinnedProcess::live(process).map_err(|e| e.to_string())?;
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&worker, channel::HANDOFF_MARKER)?
        {
            return Err(
                "Ready worker lacks private state handoff V2; original lock retained".into(),
            );
        }
        timing.mark("capability_checked", None, None);
        self.require_custody(j, a, s, true)?;
        let next = self.record.issue_handoff(self.at()?, &r)?;
        self.save(j, a, s, next)?; // Commit/readback before relinquishing ownership.
        timing.mark("handoff_intent_saved", None, None);
        let request = channel::state_handoff(&self.record)?;
        self.require_custody(j, a, s, true)?;
        #[cfg(feature = "update-validation")]
        validation::state_boundary(&self.record, validation::StateBoundary::Held, || {
            j.require_activation_owned_current(a, s, &self.files.preparation, &self.record)
                .map_err(|e| e.to_string())?;
            live.require_live().map_err(|e| e.to_string())?;
            self.files.preparation.require_live_window()
        })?;
        let exited = self
            .files
            .original
            .exited
            .take()
            .ok_or("Original exited-state owner missing")?;
        let released = exited.release(|| {
            j.require_activation_owned_current(a, s, &self.files.preparation, &self.record)
                .map_err(std::io::Error::other)?;
            live.require_live()?;
            self.files
                .preparation
                .require_live_window()
                .map_err(std::io::Error::other)
        });
        self.files.original.signal.0.store(
            released.original_lock_held() == Some(true),
            std::sync::atomic::Ordering::Release,
        );
        self.released = Some(released); // Retain residual handles even on unlock error.
        timing.mark("original_lock_released", None, None);
        self.require_custody(j, a, s, false)?;
        #[cfg(feature = "update-validation")]
        validation::state_boundary(&self.record, validation::StateBoundary::Released, || {
            j.require_activation_owned_current(a, s, &self.files.preparation, &self.record)
                .map_err(|e| e.to_string())?;
            live.require_live().map_err(|e| e.to_string())?;
            self.files.preparation.require_live_window()
        })?;
        live.require_live().map_err(|e| e.to_string())?;
        self.files.preparation.require_live_window()?;
        let remaining = self
            .files
            .preparation
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        let timeout = Duration::from_millis(self.record.intent.context.startup_timeout_ms);
        let stream = self
            .channel
            .as_mut()
            .ok_or("Original state handoff channel missing")?;
        channel::write(stream, &request, remaining).await?;
        timing.mark("handoff_sent", None, None);
        let message: channel::StateOwned = channel::read(stream, timeout).await?;
        timing.mark("state_owned_received", None, None);
        live.require_live().map_err(|e| e.to_string())?;
        channel::validate_state_owned(&message, &self.record)?;
        self.require_custody(j, a, s, false)?;
        let next = self
            .record
            .state_owned(message.state_owned, self.at()?, &r)?;
        self.save(j, a, s, next)?;
        timing.mark("state_owned_saved", None, None);
        Ok(())
    }
    fn save(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        next: ActivationRecord,
    ) -> Result<()> {
        self.record = j
            .append_activation_owned(a, s, &self.files.preparation, &self.record, next)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn launch(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "launch",
            Some(self.files.preparation.deadline),
        );
        // No progressed/cold record may enter this one-shot factory.
        self.files.require_current(j, a, s)?;
        timing.mark("files_current", None, None);
        let pin = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if pin.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&pin, channel::RECEIVER_MARKER)?
        {
            return Err(
                "Installed worker lacks the exact private activation receiver; no process created"
                    .into(),
            );
        }
        timing.mark("capability_checked", None, None);
        let source = &self.record.intent.source;
        let address = channel::endpoint(
            &Path::new(&source.runtime_directory).join("update-state"),
            &self.record.intent.attempt_id,
        )?;
        let mut listener = ipc::Listener::bind(&address).map_err(|e| e.to_string())?;
        let command = winbridge_protocol::ResolvedCommand {
            executable: source.worker.path.clone(),
            arguments: vec![
                "--activate-update".into(),
                self.record.intent.context.worker_config.path.clone(),
                source.update_id.clone(),
                self.record.intent.attempt_id.clone(),
            ],
            working_directory: source.runtime_directory.clone(),
            timeout_ms: self.record.intent.context.startup_timeout_ms,
        };
        let r = self.require_custody(j, a, s, true)?;
        let next = self.record.issue_create(self.at()?, &r)?;
        self.save(j, a, s, next)?; // Durable Creating before any CreateProcess.
        timing.mark("creating_saved", None, None);
        #[cfg(feature = "update-validation")]
        validation::hold(&self.record, "creating", || {
            self.files.preparation.require_live_window()?;
            j.check_source_authority(a).map_err(|e| e.to_string())
        })?;
        self.require_custody(j, a, s, true)?;
        let name = self.record.intent.job_name()?;
        // The platform creates the suspended process and Job together. There
        // is no detached root, post-create assignment gap, or automatic resume.
        let spawned = ManagedChild::prepare_atomic_executable(&command, Some(&name), None);
        let (child, stdout, stderr) = match spawned {
            Ok(value) => value,
            Err(error) => {
                if error.no_process_created {
                    let next = self.record.creation_failed(
                        "OS confirmed no activation process was created".into(),
                        self.at()?,
                        &r,
                    )?;
                    self.save(j, a, s, next)?;
                }
                return Err(format!("Activation process creation stopped: {error}"));
            }
        };
        self.child = Some(child); // Retain ownership before any fallible receipt.
        timing.mark("child_created_suspended", None, None);
        let identity = self
            .child
            .as_ref()
            .unwrap()
            .identity()
            .ok_or("Atomic child identity missing")?;
        let process = winbridge_protocol::update::SupervisorProcess {
            process_id: identity.process_id,
            birth: identity.creation_time.to_string(),
            host_id: identity.host_id.clone(),
            session_id: identity.session_id,
        };
        let created = ActivationCreated {
            process,
            job_name: identity.job_name.clone(),
            observed_at_ms: self.at()?,
        };
        let next = self.record.created(created, self.at()?, &r)?;
        // Actual PID/Job fact may arrive late.
        self.save(j, a, s, next)?;
        // Still suspended: output cannot block it yet. Persist the real PID/Job
        timing.mark("created_saved", None, None);
        // before a fallible allocation of either output reader thread.
        #[cfg(feature = "update-validation")]
        validation::hold(&self.record, "created", || {
            self.files.preparation.require_live_window()?;
            j.check_source_authority(a).map_err(|e| e.to_string())
        })?;
        self.readers.push(drain(stdout, self.captures[0].clone())?);
        self.readers.push(drain(stderr, self.captures[1].clone())?);
        self.require_custody(j, a, s, true)?;
        let next = self.record.grant_resume(self.at()?, &r)?;
        self.save(j, a, s, next)?;
        self.require_custody(j, a, s, true)?;
        let child = self.child.as_mut().unwrap();
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Suspended activation child exited; no resume".into());
        }
        child.resume().map_err(|e| e.to_string())?;
        timing.mark("child_resumed", None, None);
        let process = self.record.created.as_ref().unwrap().process.clone();
        let pinned = infrastructure::PinnedProcess::live(&process).map_err(|e| e.to_string())?;
        let timeout = Duration::from_millis(self.record.intent.context.startup_timeout_ms);
        // This bound waits for facts only; it never renews the original grant
        // window or permits state handoff after its expiry.
        let until = tokio::time::Instant::now() + timeout;
        let mut stream = loop {
            let (stream, pid) = tokio::time::timeout_at(until, listener.accept())
                .await
                .map_err(|_| "Activation receiver connection expired; observe original child")?
                .map_err(|_| "Activation receiver connection failed; observe original child")?;
            pinned.require_live().map_err(|e| e.to_string())?;
            if pid == process.process_id {
                break stream;
            }
            // Never disclose nonce to an unrelated same-user pipe client.
        };
        let challenge = channel::Challenge {
            contract_version: channel::CONTRACT,
            update_id: self.record.intent.source.update_id.clone(),
            attempt_id: self.record.intent.attempt_id.clone(),
            nonce: self.files.preparation.channel_nonce.clone(),
        };
        timing.mark("child_connected", None, None);
        let remaining = until.saturating_duration_since(tokio::time::Instant::now());
        channel::write(&mut stream, &challenge, remaining).await?;
        let ready: channel::Ready = channel::read(
            &mut stream,
            until.saturating_duration_since(tokio::time::Instant::now()),
        )
        .await?;
        timing.mark("ready_received", None, None);
        pinned.require_live().map_err(|e| e.to_string())?;
        channel::validate_ready(&ready, &self.record)?;
        self.require_custody(j, a, s, false)?;
        let next = self.record.ready(ready.ready, self.at()?, &r)?;
        self.save(j, a, s, next)?;
        self.channel = Some(stream);
        timing.mark("ready_saved", None, None);
        Ok(())
    }
    fn record_uncertainty(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) {
        let directory =
            Path::new(&self.record.intent.source.runtime_directory).join("update-state");
        let Ok(Some(saved)) = UpdateJournal::inspect_activation(&directory, &s.spec.update_id)
        else {
            return;
        };
        if saved.intent != self.record.intent
            || matches!(
                saved.state,
                ActivationState::Prepared {}
                    | ActivationState::CreationFailed { .. }
                    | ActivationState::Indeterminate { .. }
                    | ActivationState::LocalVerified {}
            )
        {
            return;
        }
        self.record = saved;
        let Ok(r) = j.require_activation_owned_current(a, s, &self.files.preparation, &self.record)
        else {
            return;
        };
        let Ok(at) = self.at() else { return };
        if let Ok(next)=self.record.indeterminate("Activation interrupted; retain original resources and reconcile this attempt; never launch another child".into(),at,&r) {
            let _=self.save(j,a,s,next);
        }
    }
}
