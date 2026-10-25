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
        &mut self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        grant: bool,
    ) -> Result<RuntimeReplacementRecord> {
        let r = j
            .require_activation_owned_current(a, s, &self.files.preparation, &self.record)
            .map_err(|e| e.to_string())?;
        self.files.original.require_exit_custody()?;
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
        Ok(r)
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
        // No progressed/cold record may enter this one-shot factory.
        self.files.require_current(j, a, s)?;
        let pin = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if pin.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&pin, channel::RECEIVER_MARKER)?
        {
            return Err(
                "Installed worker lacks the exact private activation receiver; no process created"
                    .into(),
            );
        }
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
        // before a fallible allocation of either output reader thread.
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
        let remaining = until.saturating_duration_since(tokio::time::Instant::now());
        channel::write(&mut stream, &challenge, remaining).await?;
        let ready: channel::Ready = channel::read(
            &mut stream,
            until.saturating_duration_since(tokio::time::Instant::now()),
        )
        .await?;
        pinned.require_live().map_err(|e| e.to_string())?;
        channel::validate_ready(&ready, &self.record)?;
        self.require_custody(j, a, s, false)?;
        let next = self.record.ready(ready.ready, self.at()?, &r)?;
        self.save(j, a, s, next)?;
        self.channel = Some(stream);
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
