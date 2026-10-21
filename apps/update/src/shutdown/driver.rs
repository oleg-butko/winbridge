//! Blocking supervisor-only driver. Readonly preflight precedes admission;
//! durable Issued precedes one request, and acceptance never substitutes for exit.
use super::*;
use crate::{
    artifact::{self, Pinned},
    installed_set::PinnedInstalledSet,
    journal::{UpdateAuthority, UpdateJournal},
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::runtime::Handle;
use winbridge_platform::infrastructure::PinnedProcess;
use winbridge_protocol::WorkerRuntimeCompatibility;

pub(crate) struct Prepared {
    permit: Box<ShutdownPermit>,
    process: PinnedProcess,
    state: ShutdownStatePins,
    runtime: PinnedInstalledSet,
    config: Pinned,
    _candidate: Vec<Pinned>,
    cfg: crate::UpdateConfig,
    worker_schema: u32,
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn call(
    io: &Handle,
    s: &SupervisorRecord,
    c: WorkerUpdateControlCommand,
) -> Result<WorkerUpdateControlResponse> {
    io.block_on(crate::supervisor::forward_worker(s, c))
}
fn current(
    j: &UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: &ShutdownPermit,
) -> Result<()> {
    j.require_shutdown_current(a, s, p).map_err(error)
}
fn compatibility(io: &Handle, s: &SupervisorRecord) -> Result<WorkerRuntimeCompatibility> {
    let value = call(io, s, WorkerUpdateControlCommand::ObserveCompatibility)?
        .into_runtime_compatibility()?;
    value.validate_binding(
        s.spec
            .worker
            .as_ref()
            .ok_or("Shutdown worker binding missing")?,
    )?;
    Ok(value)
}
/// Called on the owned blocking worker, never the async mailbox. No stop is
/// issued here; a failed/aborted preflight leaves no shutdown admission.
pub(crate) fn prepare(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    io: Handle,
) -> Result<Prepared> {
    j.check_source_authority(a).map_err(error)?;
    let id = &s.spec.update_id;
    if j.shutdown(id).map_err(error)?.is_some() {
        return Err("Shutdown already admitted; observe only, never recreate a permit".into());
    }
    let root = j.get(id).map_err(error)?;
    if root.phase != UpdatePhase::Quiesced {
        return Err("Exact quiesced root required before shutdown preflight".into());
    }
    let cfg = crate::preparation::configuration(s)?;
    let source = crate::build::saved_source_guard_for(j, a, &cfg, id, UpdatePhase::Quiesced)?;
    crate::verify_source(
        &cfg.source_directory,
        &source.worker_source_spec.target_manifest_sha256,
    )?;
    let health = j
        .health(id)
        .map_err(error)?
        .ok_or("Shutdown health missing")?;
    crate::health::validate_fresh_success(&health)?;
    crate::health::physically_quiescent(&health)?;
    let context = health
        .context
        .as_ref()
        .ok_or("Shutdown health context missing")?;
    let build = UpdateJournal::inspect_build(&cfg.runtime_directory.join("update-state"), id)
        .map_err(error)?;
    crate::abort::compiler_empty(&build)?;
    let baseline = j
        .runtime_baseline(id)
        .map_err(error)?
        .receipt
        .ok_or("Shutdown baseline missing")?;
    let runtime = crate::installed_set::pin_installed_set(
        &cfg.runtime_directory,
        &root.request.expected.installation_id,
        &root.request.expected.scope_sha256,
    )?;
    if runtime.observation() != &baseline.installed {
        return Err("Shutdown old runtime differs from its baseline".into());
    }
    let (candidate, candidate_pins) = crate::candidate_artifacts::inspect_pinned(&cfg, id)?;
    if candidate != context.artifacts.inspection {
        return Err("Shutdown candidate differs from approved health".into());
    }
    let config = artifact::pin(&cfg.worker_config_path)?;
    if config.evidence != context.worker_config {
        return Err("Shutdown config differs from approved health".into());
    }
    let worker_config: winbridge_core::config::WorkerConfig =
        serde_json::from_slice(&crate::read(&cfg.worker_config_path, 65536)?)
            .map_err(|_| "Invalid shutdown worker config")?;
    worker_config.validate()?;
    let state = ShutdownStatePins::capture(&worker_config.state_directory).map_err(error)?;
    let binding = s.spec.worker.as_ref().ok_or("Shutdown worker missing")?;
    let process = PinnedProcess::live(&binding.process).map_err(error)?;
    let mut live = compatibility(&io, s)?;
    let worker_schema = live.worker_journal_schema;
    live.observed_at_ms = context.compatibility.observed_at_ms;
    if live != context.compatibility
        || live.identity != baseline.loaded_worker
        || live.updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
    {
        return Err("Shutdown live worker/config/schema differs from health".into());
    }
    // Both contracts must exist before accepting a stop, including lost-ACK recovery.
    let old_image = artifact::pin(Path::new(&baseline.loaded_worker.image.path))?;
    if old_image.evidence != baseline.loaded_worker.image
        || !crate::worker_rehearsal::supports(&old_image, WORKER_SHUTDOWN_INSPECTION_MARKER)?
    {
        return Err(
            "Old worker lacks the exact readonly shutdown receipt inspector; no stop admitted"
                .into(),
        );
    }
    let observed =
        call(&io, s, WorkerUpdateControlCommand::ObserveShutdownState)?.into_shutdown_state()?;
    if &observed != state.observation() {
        return Err("Worker-computed state differs from local pinned files".into());
    }
    let logical =
        call(&io, s, WorkerUpdateControlCommand::ObserveDrain)?.into_drain_observation()?;
    let lease = logical
        .lease
        .as_deref()
        .ok_or("Shutdown worker lease missing")?;
    if logical.accepted_operations != 0
        || logical.logical_owners != 0
        || logical.in_flight_effects != 0
        || lease.phase != WorkerUpdatePhase::Quiesced
    {
        return Err("Shutdown worker is not freshly quiesced".into());
    }
    runtime.revalidate()?;
    state.revalidate().map_err(error)?;
    process.require_live().map_err(error)?;
    crate::preparation::configuration(s)?;
    let admission = j
        .admit_shutdown(a, s, lease, &baseline.loaded_worker, observed)
        .map_err(error)?;
    let ShutdownAdmission::Fresh(permit) = admission else {
        return Err("Shutdown was already admitted; no replay".into());
    };
    Ok(Prepared {
        permit,
        process,
        state,
        runtime,
        config,
        _candidate: candidate_pins,
        cfg,
        worker_schema,
    })
}
fn cold_input(p: &Prepared) -> Result<(PathBuf, String, ShutdownInspectionRequest)> {
    let request = ShutdownInspectionRequest {
        schema_version: 1,
        intent: p.permit.intent.clone(),
        worker_config: p.config.evidence.clone(),
        helper_image: p.permit.intent.binding.loaded_worker.image.clone(),
        worker_journal_schema: p.worker_schema,
    };
    request.validate()?;
    let path = p
        .cfg
        .runtime_directory
        .join("update")
        .join(&request.intent.binding.update_id)
        .join(format!(
            "shutdown-receipt-{}.json",
            request.intent.attempt_id
        ));
    winbridge_platform::require_local_path_prefix(&path).map_err(error)?;
    winbridge_platform::require_safe_path(&path).map_err(error)?;
    winbridge_platform::require_local_disk(&path).map_err(error)?;
    let raw = serde_json::to_vec(&request).map_err(error)?;
    if raw.len() > 65536 {
        return Err("Shutdown readonly input exceeds its bound".into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(error)?;
    file.write_all(&raw)
        .and_then(|_| file.sync_all())
        .map_err(error)?;
    winbridge_platform::source_files::sync_parent(&path).map_err(error)?;
    Ok((path, crate::sha256(&raw), request))
}
fn reconcile(p: &Prepared, io: &Handle, s: &SupervisorRecord) -> Result<WorkerShutdownReceipt> {
    let command = &p.permit.intent.command;
    if let Ok(response) = call(
        io,
        s,
        WorkerUpdateControlCommand::ObserveShutdownReceipt {
            command: command.clone(),
        },
    ) {
        if let Some(receipt) = response.into_shutdown_receipt(command)? {
            return Ok(receipt);
        }
    }
    let (path, sha, input) = cold_input(p)?;
    inspect_saved_receipt(&path, &sha, &input)?
        .receipt
        .map(|r| *r)
        .ok_or("Exact shutdown receipt absent; no resend, restart or exit approval".into())
}
/// The supervisor keeps `custody` after this driver returns. U2 must consume a
/// real guard; a persisted Exited record cannot reconstruct one after a crash.
pub(crate) fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: Prepared,
    io: Handle,
    custody: Custody,
) -> Result<()> {
    current(j, a, s, &p.permit)?;
    crate::preparation::configuration(s)?;
    p.runtime.revalidate()?;
    p.state.revalidate().map_err(error)?;
    p.process.require_live().map_err(error)?;
    if artifact::inspect_image(Path::new(&p.config.evidence.path))? != p.config.evidence {
        return Err("Shutdown config changed before request".into());
    }
    let issued = match j.issue_shutdown(a, s, &p.permit).map_err(error)? {
        ShutdownIssue::Fresh(permit) => permit.into_record(),
        ShutdownIssue::Observed(_) => {
            return Err("Shutdown was already issued; no delivery replay".into())
        }
    };
    let command = p.permit.intent.command.clone();
    let receipt = match call(
        &io,
        s,
        WorkerUpdateControlCommand::RequestShutdown {
            command: command.clone(),
        },
    )
    .and_then(|r| r.into_shutdown_receipt(&command))
    {
        Ok(Some(receipt)) => receipt,
        result => {
            let reason = match result {
                Ok(None) => "Shutdown request returned no acceptance".into(),
                Err(e) => e,
                _ => unreachable!(),
            };
            let _ = j.mark_shutdown_uncertain(
                a,
                s,
                &p.permit,
                &issued,
                reason.chars().take(900).collect(),
            );
            reconcile(&p, &io, s).map_err(|e| format!("{reason}; {e}"))?
        }
    };
    j.acknowledge_shutdown(a, s, &p.permit, receipt)
        .map_err(error)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while !p.process.exited().map_err(error)? {
        current(j, a, s, &p.permit)?;
        if Instant::now() >= deadline {
            return Err("Worker accepted shutdown but exact process exit is unconfirmed; no force kill or restart".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut pins = p.state;
    let locked = loop {
        current(j, a, s, &p.permit)?;
        match pins.try_lock() {
            Ok(locked) => break locked,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                if Instant::now() >= deadline {
                    return Err("Exact worker exited but original instance lock remains occupied; no replacement approved".into());
                }
                std::thread::sleep(Duration::from_millis(50));
                pins = ShutdownStatePins::capture(Path::new(
                    &p.permit.intent.binding.worker_state.state_directory,
                ))
                .map_err(error)?;
                if pins.observation() != &p.permit.intent.binding.worker_state {
                    return Err("Shutdown state rebound while awaiting lock".into());
                }
            }
            Err(e) => return Err(error(e)),
        }
    };
    let guard = ShutdownExitGuard::observe(&p.permit.intent, p.process, locked, p.runtime)?;
    j.confirm_shutdown_exit(a, s, &p.permit, &guard)
        .map_err(error)?;
    custody.hold(guard)
}
