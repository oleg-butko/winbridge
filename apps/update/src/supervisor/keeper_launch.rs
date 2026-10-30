//! One original-child delegation attempt. This host never opens an updater DB.
use super::*;
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s)
}
#[cfg(any(windows, test))]
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[cfg(any(windows, test))]
fn text(s: &str, n: usize) -> bool {
    !s.is_empty() && s.len() <= n && !s.chars().any(char::is_control)
}
#[cfg(any(windows, test))]
pub(crate) fn ticket_sha(t: &KeeperLaunchTicket) -> Result<String> {
    let mut value = serde_json::to_value(t).map_err(error)?;
    value["token"] = sha256(t.token.as_bytes()).into();
    let mut bytes = b"winbridge.keeper.launch.v1\n".to_vec();
    bytes.extend(winbridge_protocol::canonical_evidence_json(&value).map_err(error)?);
    Ok(sha256(&bytes))
}
#[cfg(any(windows, test))]
pub(crate) fn startup_remaining(prepared: u64, at: u64) -> Result<Duration> {
    let expires = prepared
        .checked_add(KEEPER_LAUNCH_STARTUP_MS)
        .filter(|v| *v <= winbridge_core::update::MAX_UPDATE_COUNTER)
        .ok_or("Invalid original launch startup deadline")?;
    if at < prepared || at >= expires {
        return Err("Original keeper launch startup budget expired".into());
    }
    Ok(Duration::from_millis(expires - at))
}
#[cfg(any(windows, test))]
pub(crate) fn validate(t: &KeeperLaunchTicket, b: &KeeperBinding) -> Result<()> {
    let ids = [
        &t.update_id,
        &t.launch_id,
        &b.update_id,
        &b.launch_id,
        &b.generation,
    ];
    if !ids.iter().all(|v| uuid(v))
        || !(0..ids.len()).all(|i| ((i + 1)..ids.len()).all(|j| ids[i] != ids[j]))
        || !hash(&t.token)
        || !text(&t.excluded_job, 1024)
        || t.excluded_job == b.job_name
        || !text(&t.stdout_identity, 512)
        || !text(&t.stderr_identity, 512)
        || !hash(&t.image.sha256)
        || !hash(&t.config.sha256)
        || !(1..=256 * 1024 * 1024).contains(&t.image.bytes)
        || !(1..=65536).contains(&t.config.bytes)
        || !text(&t.image.file_identity, 512)
        || !text(&t.config.file_identity, 512)
        || t.prepared_at_ms > winbridge_core::update::MAX_UPDATE_COUNTER - KEEPER_LAUNCH_STARTUP_MS
    {
        return Err("Invalid keeper launch scope/files/budget".into());
    }
    let files = [
        &t.image.file_identity,
        &t.config.file_identity,
        &t.stdout_identity,
        &t.stderr_identity,
    ];
    if !(0..files.len()).all(|i| ((i + 1)..files.len()).all(|j| files[i] != files[j])) {
        return Err("Keeper launch files alias".into());
    }
    Ok(())
}
#[cfg(any(windows, test))]
struct CreationFailure {
    reason: String,
    may_have_created: bool,
}
#[cfg(any(windows, test))]
#[derive(Default)]
struct Machine {
    slot: Option<KeeperLaunchStatus>,
    successor_confirmed: bool,
}
#[cfg(any(windows, test))]
impl Machine {
    fn start(
        &mut self,
        b: &KeeperBinding,
        t: &KeeperLaunchTicket,
        at: u64,
        create: impl FnOnce() -> std::result::Result<SupervisorProcess, CreationFailure>,
    ) -> Result<KeeperLaunchStatus> {
        validate(t, b)?;
        let fingerprint = ticket_sha(t)?;
        if let Some(s) = &self.slot {
            if s.launch_id != t.launch_id || s.ticket_sha256 != fingerprint {
                return Err("Original keeper already has another launch attempt; no replay".into());
            }
            return Ok(s.clone());
        }
        startup_remaining(t.prepared_at_ms, at)?;
        self.slot = Some(KeeperLaunchStatus {
            launch_id: t.launch_id.clone(),
            ticket_sha256: fingerprint,
            phase: KeeperLaunchPhase::Preparing,
            process: None,
            expires_at_ms: t.prepared_at_ms + KEEPER_LAUNCH_STARTUP_MS,
            reason: None,
        });
        // Reserve BEFORE any OS create. Every error remains this same attempt.
        let outcome = create();
        let s = self.slot.as_mut().unwrap();
        match outcome {
            Ok(p) => {
                s.process = Some(p);
                s.phase = KeeperLaunchPhase::Created;
            }
            Err(e) => {
                s.phase = if e.may_have_created {
                    KeeperLaunchPhase::Indeterminate
                } else {
                    KeeperLaunchPhase::Failed
                };
                s.reason = Some(e.reason.chars().take(1000).collect());
            }
        }
        Ok(s.clone())
    }
    fn observe(&self, id: &str, fingerprint: &str) -> Result<KeeperLaunchStatus> {
        let s = self
            .slot
            .as_ref()
            .ok_or("No original keeper launch attempt exists")?;
        if s.launch_id != id || s.ticket_sha256 != fingerprint {
            return Err("Keeper launch observation changed correlation".into());
        }
        Ok(s.clone())
    }
    fn ready(
        &mut self,
        id: &str,
        fingerprint: &str,
        p: &SupervisorProcess,
    ) -> Result<KeeperLaunchStatus> {
        let s = self.observe(id, fingerprint)?;
        if s.process.as_ref() != Some(p)
            || !matches!(
                s.phase,
                KeeperLaunchPhase::Created | KeeperLaunchPhase::ReadyConfirmed
            )
        {
            return Err("Ready confirmation lacks the same created process".into());
        }
        let s = self.slot.as_mut().unwrap();
        s.phase = KeeperLaunchPhase::ReadyConfirmed;
        self.successor_confirmed = true;
        Ok(s.clone())
    }
}
pub(super) fn rejected(
    q: Option<&KeeperLaunchRequest>,
    message: impl Into<String>,
) -> KeeperLaunchResponse {
    std::hint::black_box(KEEPER_LAUNCH_MARKER);
    KeeperLaunchResponse {
        keeper_launch_version: 1,
        request_id: q
            .filter(|q| uuid(&q.request_id))
            .map(|q| q.request_id.clone()),
        result: KeeperLaunchResult::Rejected {
            message: message.into(),
        },
    }
}
#[cfg(windows)]
#[derive(Default)]
pub(super) struct Actor {
    machine: Machine,
    child: Option<InfrastructureChild>,
    excluded_job: Option<String>,
}
#[cfg(windows)]
impl Actor {
    /// Fixture starts a real outside-Job helper, not a real updater coordinator.
    /// Only native loop tests use this constructor; production still creates
    /// exclusively through authenticated Start and fixed pinned helper files.
    #[cfg(test)]
    pub(super) fn exit_test_actor(
        mut child: InfrastructureChild,
        b: &KeeperBinding,
        t: &KeeperLaunchTicket,
    ) -> Result<Self> {
        let pin = infrastructure::PinnedProcess::live(&child.identity).map_err(error)?;
        pin.require_outside_job(&b.job_name).map_err(error)?;
        pin.require_outside_job(&t.excluded_job).map_err(error)?;
        child.detached();
        let mut machine = Machine::default();
        machine.start(b, t, now()?, || Ok(child.identity.clone()))?;
        Ok(Self {
            machine,
            child: Some(child),
            excluded_job: Some(t.excluded_job.clone()),
        })
    }
    pub(super) fn successor_confirmed(&self) -> bool {
        self.machine.successor_confirmed
    }
    pub(super) fn handle(
        &mut self,
        r: &SupervisorRecord,
        c: &crate::activation::ActivationKeeperCustody,
        q: &KeeperLaunchRequest,
        pid: u32,
    ) -> KeeperLaunchResponse {
        std::hint::black_box(KEEPER_LAUNCH_MARKER);
        let outcome = (|| {
            if q.keeper_launch_version != 1 || !uuid(&q.request_id) {
                return Err("Invalid keeper launch version/correlation".into());
            }
            c.require_bootstrap_peer(pid, &q.binding)?;
            if let Some(child) = self.child.as_mut() {
                if child.exited().map_err(error)? {
                    if let Some(s) = self.machine.slot.as_mut() {
                        s.phase = KeeperLaunchPhase::Exited;
                        s.reason = Some("Created coordinator exited; no replay".into());
                    }
                }
            }
            match &q.command {
                KeeperLaunchCommand::Start { ticket } => {
                    let child = &mut self.child;
                    let excluded = &mut self.excluded_job;
                    let original = self.machine.slot.is_none();
                    let status = self.machine.start(&q.binding, ticket, now()?, || {
                        let made = create(r, c, &q.binding, ticket, pid)?;
                        let p = made.identity.clone();
                        *child = Some(made);
                        *excluded = Some(ticket.excluded_job.clone());
                        Ok(p)
                    })?;
                    if original {
                        eprintln!(
                            "WinBridge keeper {}",
                            serde_json::json!({"event":"coordinator_launch_attempt","keeperPid":std::process::id(),"workerPid":pid,"launchId":status.launch_id,"phase":status.phase,"process":status.process,"expiresAtMs":status.expires_at_ms,"reason":status.reason})
                        );
                    }
                    Ok(status)
                }
                KeeperLaunchCommand::Observe {
                    launch_id,
                    ticket_sha256,
                } => self.machine.observe(launch_id, ticket_sha256),
                KeeperLaunchCommand::ConfirmReady {
                    launch_id,
                    ticket_sha256,
                    process,
                } => {
                    let saved = self.machine.observe(launch_id, ticket_sha256)?;
                    if saved.process.as_ref() != Some(process)
                        || self.child.as_ref().map(|c| &c.identity) != Some(process)
                    {
                        return Err(
                            "Ready confirmation changed the original created process".into()
                        );
                    }
                    let pin = infrastructure::PinnedProcess::live(process).map_err(error)?;
                    pin.require_outside_job(&q.binding.job_name)
                        .map_err(error)?;
                    pin.require_outside_job(
                        self.excluded_job
                            .as_deref()
                            .ok_or("Original excluded domain missing")?,
                    )
                    .map_err(error)?;
                    self.machine.ready(launch_id, ticket_sha256, process)
                }
            }
        })();
        match outcome {
            Ok(status) => KeeperLaunchResponse {
                keeper_launch_version: 1,
                request_id: Some(q.request_id.clone()),
                result: KeeperLaunchResult::Status {
                    status: Box::new(status),
                },
            },
            Err(e) => rejected(Some(q), e),
        }
    }
}
#[cfg(windows)]
fn create(
    r: &SupervisorRecord,
    c: &crate::activation::ActivationKeeperCustody,
    b: &KeeperBinding,
    t: &KeeperLaunchTicket,
    pid: u32,
) -> std::result::Result<InfrastructureChild, CreationFailure> {
    let prepare = (|| -> Result<_> {
        c.require_bootstrap_peer(pid, b)?;
        c.require_predecessor_exited(t.predecessor.as_ref())?;
        let runtime = local(Path::new(&r.spec.runtime_directory))?;
        let helper = runtime.join("update").join(&t.update_id).join("supervisor");
        let image = crate::artifact::pin(&helper.join("winbridge-update.exe"))?;
        let config = crate::artifact::pin(&runtime.join("winbridge-update.local.json"))?;
        for (actual, wanted) in [(&image, &t.image), (&config, &t.config)] {
            winbridge_platform::require_single_link_handle(&actual.file).map_err(error)?;
            if actual.evidence.sha256 != wanted.sha256
                || actual.evidence.bytes != wanted.bytes
                || actual.evidence.file_identity != wanted.file_identity
            {
                return Err("Keeper launch pinned file differs".into());
            }
        }
        let output = |name: &str, id: &str| -> Result<File> {
            let path = helper.join(name);
            winbridge_platform::require_safe_path(&path).map_err(error)?;
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
            let f = OpenOptions::new()
                .append(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(path)
                .map_err(error)?;
            winbridge_platform::require_single_link_handle(&f).map_err(error)?;
            if winbridge_platform::file_identity(&f).map_err(error)? != id {
                return Err("Keeper launch output identity differs".into());
            }
            Ok(f)
        };
        let outputs = [
            output("stdout.log", &t.stdout_identity)?,
            output("stderr.log", &t.stderr_identity)?,
        ];
        let parent = infrastructure::PinnedProcess::live(&b.coordinator).map_err(error)?;
        startup_remaining(t.prepared_at_ms, now()?)?;
        c.require_bootstrap_peer(pid, b)?;
        c.require_predecessor_exited(t.predecessor.as_ref())?;
        Ok((runtime, image, config, outputs, parent))
    })()
    .map_err(|reason| CreationFailure {
        reason,
        may_have_created: false,
    })?;
    let (runtime, image, config, outputs, parent) = prepare;
    let args = vec![
        "--config".into(),
        config.evidence.path.clone(),
        "supervisor-internal".into(),
        t.launch_id.clone(),
        t.token.clone(),
    ];
    let mut child = InfrastructureChild::spawn_excluding_jobs(
        &parent,
        [&b.job_name, &t.excluded_job],
        Path::new(&image.evidence.path),
        &args,
        &runtime,
        outputs,
    )
    .map_err(|e| CreationFailure {
        reason: format!("Keeper creation unconfirmed; no replay: {e}"),
        may_have_created: true,
    })?;
    child.detached(); // BEFORE any reply or durable Spawned/start marker.
    Ok(child)
}
#[cfg(test)]
mod tests;
