//! Trusted managed compiler execution; no public command or automatic replay.
use crate::{
    journal::{UpdateAuthority, UpdateJournal},
    source::{live_lease, SourceControl},
    UpdateConfig,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use winbridge_platform::{infrastructure, ManagedChild, OutputReader};
use winbridge_protocol::{update::*, ResolvedCommand};

type Result<T> = std::result::Result<T, String>;
pub const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
const MIN_FREE_BYTES: u64 = 6 * 1024 * 1024 * 1024;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub can_replay: bool,
    pub record: UpdateBuildRecord,
    pub root_process_matches: Option<bool>,
    pub physical_kind: &'static str,
    pub physical_reason: String,
}
pub fn inspect_build(directory: &Path, id: &str) -> Result<BuildInspection> {
    let record = UpdateJournal::inspect_build(directory, id).map_err(error)?;
    let root_process_matches =
        record
            .process
            .as_ref()
            .and_then(|p| match infrastructure::matches(p) {
                Ok(v) => Some(v),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
                Err(_) => None,
            });
    let observation = match (&record.spec.job_name, &record.process) {
        (Some(name), Some(process)) => {
            let creation_time = process.birth.parse().map_err(error)?;
            winbridge_platform::process_identity::observe_process(
                &winbridge_platform::process_identity::ProcessIdentity {
                    job_name: name.clone(),
                    host_id: process.host_id.clone(),
                    session_id: process.session_id,
                    process_id: process.process_id,
                    creation_time,
                },
            )
        }
        _ => winbridge_platform::process_identity::ProcessObservation::Unknown(
            "No recoverable Windows compiler Job/PID receipt; preserve source barrier".into(),
        ),
    };
    use winbridge_platform::process_identity::ProcessObservation as O;
    let (physical_kind, physical_reason) = match observation {
        O::Empty(v) => ("empty", v),
        O::Occupied(v) => ("occupied", v),
        O::Unknown(v) => ("unknown", v),
    };
    Ok(BuildInspection {
        schema_version: 1,
        kind: "update_build_inspection",
        read_only: true,
        can_replay: false,
        record,
        root_process_matches,
        physical_kind,
        physical_reason,
    })
}
pub fn job_name(attempt_id: &str) -> String {
    format!(
        "Global\\WinBridge-UpdateBuild-{}",
        crate::sha256(attempt_id.as_bytes())
    )
}
pub trait BuildControl: SourceControl {
    fn build_progress(&mut self, _record: &UpdateBuildRecord) -> Result<()> {
        Ok(())
    }
}
impl BuildControl for crate::worker_source::WorkerSourceControl {}
fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis(),
    )
    .map_err(|e| e.to_string())
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn source_guard(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    cfg: &UpdateConfig,
    id: &str,
    c: &mut impl SourceControl,
) -> Result<SourceTransactionRecord> {
    j.check_source_authority(a).map_err(error)?;
    let u = j.get(id).map_err(error)?;
    let s = j.source_header(id).map_err(error)?;
    if winbridge_platform::source_files::directory_identity(Path::new(&s.source_directory))
        .map_err(error)?
        != s.source_directory_identity
    {
        return Err("Compiler source directory identity changed".into());
    }
    if u.phase != UpdatePhase::Building
        || s.phase != SourceTransactionPhase::Verified
        || u.request.expected.scope_sha256 != crate::installation_scope(cfg)?
        || crate::sha256(&crate::read(&cfg.worker_config_path, 64 * 1024)?)
            != u.request.expected.worker_config_sha256
        || cfg.source_directory.canonicalize().map_err(error)?.to_str() != Some(&s.source_directory)
        || cfg
            .runtime_directory
            .canonicalize()
            .map_err(error)?
            .to_str()
            != Some(&s.runtime_directory)
    {
        return Err(
            "Verified source/config/scope/Building required before compiler effects".into(),
        );
    }
    let lease = live_lease(c, &u, Some(&s))?;
    if lease.spec.updater_owner_id != a.owner_id() || lease.spec.updater_epoch != a.epoch() {
        return Err("Compiler source grant belongs to another owner/epoch".into());
    }
    Ok(s)
}
fn logs(spec: &UpdateBuildSpec) -> PathBuf {
    Path::new(&spec.runtime_directory)
        .join("update")
        .join(&spec.update_id)
        .join("compiler")
        .join(&spec.attempt_id)
}
fn output_file(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(error)
}
fn drain(
    mut reader: OutputReader,
    mut file: File,
) -> thread::JoinHandle<Result<UpdateBuildOutput>> {
    thread::spawn(move || {
        let mut result = UpdateBuildOutput {
            retained_bytes: 0,
            observed_bytes: 0,
            sha256: crate::sha256(&[]),
            error: None,
        };
        let mut digest = Sha256::new();
        let mut bytes = [0u8; 16384];
        loop {
            let count = match reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(v) => v,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    result.error = Some(error(e));
                    break;
                }
            };
            result.observed_bytes = result.observed_bytes.saturating_add(count as u64);
            let save = count.min(MAX_OUTPUT_BYTES.saturating_sub(result.retained_bytes) as usize);
            if save > 0 && result.error.is_none() {
                match file.write_all(&bytes[..save]) {
                    Ok(()) => {
                        digest.update(&bytes[..save]);
                        result.retained_bytes += save as u64;
                    }
                    Err(e) => result.error = Some(error(e)),
                }
            }
            // Drain both pipes even after quota/write failure: compiler never waits for UI/disk.
        }
        if let Err(e) = file.sync_all() {
            result.error = Some(error(e));
        }
        result.sha256 = format!("{:x}", digest.finalize());
        // A failed write_all can have written a prefix. Report the actual retained
        // file, rather than a guessed byte count/hash of attempted writes.
        if result.error.is_some() {
            file.seek(SeekFrom::Start(0)).map_err(error)?;
            let mut digest = Sha256::new();
            let mut retained = 0;
            loop {
                let count = file.read(&mut bytes).map_err(error)?;
                if count == 0 {
                    break;
                }
                retained += count as u64;
                if retained > MAX_OUTPUT_BYTES || retained > result.observed_bytes {
                    return Err("Compiler retained output changed outside capture".into());
                }
                digest.update(&bytes[..count]);
            }
            result.retained_bytes = retained;
            result.sha256 = format!("{:x}", digest.finalize());
        }
        if result.error.as_ref().is_some_and(|e| e.len() > 4000) {
            result.error = Some("Compiler output capture failed; inspect retained log".into());
        }
        Ok(result)
    })
}
fn complete(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    r: &UpdateBuildRecord,
    outcome: UpdateBuildOutcome,
    exit: Option<i32>,
    outputs: Option<(UpdateBuildOutput, UpdateBuildOutput)>,
    reason: &str,
) -> Result<UpdateBuildRecord> {
    let mut next = r.clone();
    next.phase = UpdateBuildPhase::Completed;
    next.outcome = Some(outcome);
    next.exit_code = exit;
    next.cleanup_verified = true;
    next.reason = reason.into();
    if let Some((stdout, stderr)) = outputs {
        next.stdout = Some(stdout);
        next.stderr = Some(stderr);
    }
    j.advance_build(a, r, next).map_err(error)
}
fn unknown(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    r: &UpdateBuildRecord,
    reason: &str,
) -> Result<UpdateBuildRecord> {
    let mut next = r.clone();
    next.phase = UpdateBuildPhase::Indeterminate;
    next.reason = reason.chars().take(1000).collect();
    j.advance_build(a, r, next).map_err(error)
}
/// Caller supplies a command from trusted local build policy, never agent JSON.
/// Repeating this request returns its saved record, including unfinished intent;
/// it never spawns again. A successful process is not candidate/install approval.
pub fn execute_build(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    cfg: &UpdateConfig,
    id: &str,
    key: &str,
    command: &ResolvedCommand,
    c: &mut impl BuildControl,
) -> Result<UpdateBuildRecord> {
    j.check_source_authority(a).map_err(error)?;
    if let Some(r) = j.existing_build(id, key, command).map_err(error)? {
        return Ok(r);
    }
    let s = source_guard(j, a, cfg, id, c)?;
    crate::verify_source(
        &cfg.source_directory,
        &j.get(id).map_err(error)?.request.source.manifest_sha256,
    )?;
    if command.working_directory != s.source_directory {
        return Err("Compiler cwd must be the frozen source root".into());
    }
    let executable = Path::new(&command.executable);
    winbridge_platform::require_local_path_prefix(executable).map_err(error)?;
    winbridge_platform::require_safe_path(executable).map_err(error)?;
    winbridge_platform::require_local_disk(executable).map_err(error)?;
    if !executable.is_file() {
        return Err("Compiler executable must be an existing local file".into());
    }
    let source_free = fs2::available_space(&s.source_directory).map_err(error)?;
    let runtime_free = fs2::available_space(&s.runtime_directory).map_err(error)?;
    if source_free < MIN_FREE_BYTES || runtime_free < MIN_FREE_BYTES {
        return Err(format!("Compiler deferred: less than 6 GiB free; source={source_free}, runtime={runtime_free}; no cache deleted"));
    }
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let target_manifest = j.get(id).map_err(error)?.request.source.manifest_sha256;
    let mut r = j
        .accept_build(
            a,
            UpdateBuildSpec {
                attempt_id: attempt_id.clone(),
                update_id: id.into(),
                request_key: key.into(),
                platform: std::env::consts::OS.into(),
                owner_id: a.owner_id().into(),
                owner_epoch: a.epoch(),
                worker_source_spec: s.worker_source_spec,
                worker_lease_revision: s.worker_lease_revision,
                source_directory: s.source_directory,
                runtime_directory: s.runtime_directory,
                source_manifest_sha256: target_manifest,
                command: command.clone(),
                job_name: cfg!(windows).then(|| job_name(&attempt_id)),
                source_available_bytes_before: source_free,
                runtime_available_bytes_before: runtime_free,
                accepted_at_ms: now()?,
            },
        )
        .map_err(error)?;
    // No user code can be launched before the durable intent and observer succeed.
    if let Err(e) = c.build_progress(&r) {
        return unknown(
            j,
            a,
            &r,
            &format!("Build intent retained; observer stopped before launch: {e}"),
        );
    }
    let logdir = logs(&r.spec);
    let setup: Result<(File, File)> = (|| {
        winbridge_platform::require_safe_path(&logdir).map_err(error)?;
        fs::create_dir_all(logdir.parent().unwrap()).map_err(error)?;
        fs::create_dir(&logdir).map_err(error)?;
        Ok((
            output_file(&logdir.join("stdout.bin"))?,
            output_file(&logdir.join("stderr.bin"))?,
        ))
    })();
    let (stdout_file, stderr_file) = match setup {
        Ok(v) => v,
        Err(e) => {
            return complete(
                j,
                a,
                &r,
                UpdateBuildOutcome::SpawnFailed,
                None,
                None,
                &format!("Compiler not created: {e}"),
            )
        }
    };
    source_guard(j, a, cfg, id, c)?;
    let (mut child, stdout, stderr) =
        match ManagedChild::prepare_atomic_executable(command, r.spec.job_name.as_deref(), None) {
            Ok(v) => v,
            Err(e) if e.no_process_created => {
                return complete(
                    j,
                    a,
                    &r,
                    UpdateBuildOutcome::SpawnFailed,
                    None,
                    None,
                    &format!("Compiler not created: {e}"),
                )
            }
            Err(e) => {
                return unknown(
                    j,
                    a,
                    &r,
                    &format!("OS process creation outcome unconfirmed: {e}"),
                )
            }
        };
    // Windows creates the suspended process with its compiler Job already assigned.
    // Unix is a development adapter and never claims this suspended-launch proof.
    let process = match infrastructure::stamp(child.id()) {
        Ok(v) => v,
        Err(e) => {
            child.close_unconfirmed();
            return unknown(j, a, &r, &format!("Compiler PID binding unconfirmed: {e}"));
        }
    };
    if cfg!(windows)
        && child.identity().is_none_or(|i| {
            Some(&i.job_name) != r.spec.job_name.as_ref()
                || i.process_id != process.process_id
                || i.creation_time.to_string() != process.birth
                || i.host_id != process.host_id
                || i.session_id != process.session_id
        })
    {
        child.close_unconfirmed();
        return unknown(j, a, &r, "Compiler Job receipt missing; no resume");
    }
    let mut spawned = r.clone();
    spawned.phase = UpdateBuildPhase::Spawned;
    spawned.process = Some(process);
    spawned.reason = "PID/Job receipt сохранён до Windows resume".into();
    r = match j.advance_build(a, &r, spawned) {
        Ok(receipt) => receipt,
        Err(e) => {
            child.close_unconfirmed();
            return Err(format!(
                "Compiler PID receipt unconfirmed; no resume/replay: {e}"
            ));
        }
    };
    let readers = (drain(stdout, stdout_file), drain(stderr, stderr_file));
    let start = (|| {
        c.build_progress(&r)?;
        source_guard(j, a, cfg, id, c)?;
        let mut permit = r.clone();
        permit.phase = UpdateBuildPhase::ResumeGranted;
        permit.reason = "Сохранено разрешение resume; выполнение ещё не подтверждено".into();
        r = j.advance_build(a, &r, permit).map_err(error)?;
        c.build_progress(&r)?;
        crate::verify_source(&cfg.source_directory, &r.spec.source_manifest_sha256)?;
        source_guard(j, a, cfg, id, c)?;
        child.resume().map_err(error)?;
        let mut running = r.clone();
        running.phase = UpdateBuildPhase::Running;
        running.reason = "Windows resume подтверждён; compiler process запущен".into();
        if !cfg!(windows) {
            running.reason =
                "Development compiler process запущен; Windows resume здесь не проверяется".into();
        }
        r = j.advance_build(a, &r, running).map_err(error)?;
        c.build_progress(&r)
    })();
    let started = Instant::now();
    let mut timed_out = false;
    let mut exit = None;
    let mut failure = start.err();
    while failure.is_none() {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit = status.code();
                break;
            }
            Ok(None) => {}
            Err(e) => {
                failure = Some(error(e));
                break;
            }
        }
        if started.elapsed() >= Duration::from_millis(command.timeout_ms) {
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    // Always clean the compiler domain, including helpers left after root exit.
    let cleanup = child.terminate_tree();
    let until = Instant::now() + Duration::from_secs(5);
    let mut empty = false;
    if cleanup.is_ok() {
        loop {
            let root_exited = matches!(child.try_wait(), Ok(Some(_)));
            if root_exited && child.tree_is_empty().unwrap_or(false) {
                empty = true;
                break;
            }
            if Instant::now() >= until {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    if empty {
        drop(child)
    } else {
        child.close_unconfirmed()
    };
    while !(readers.0.is_finished() && readers.1.is_finished()) && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    if !empty || !readers.0.is_finished() || !readers.1.is_finished() {
        return unknown(
            j,
            a,
            &r,
            "Compiler cleanup/output completion unconfirmed; preserve source lease, do not replay",
        );
    }
    let outputs = match (readers.0.join(), readers.1.join()) {
        (Ok(Ok(out)), Ok(Ok(err))) => (out, err),
        _ => {
            return unknown(
                j,
                a,
                &r,
                "Compiler output readers failed; preserve source lease",
            )
        }
    };
    let final_verification: Result<()> = (|| {
        source_guard(j, a, cfg, id, c)?;
        crate::verify_source(&cfg.source_directory, &r.spec.source_manifest_sha256)?;
        for (name, proof) in [("stdout", &outputs.0), ("stderr", &outputs.1)] {
            let bytes = crate::read(&logdir.join(format!("{name}.bin")), MAX_OUTPUT_BYTES)?;
            if bytes.len() as u64 != proof.retained_bytes || crate::sha256(&bytes) != proof.sha256 {
                return Err("Compiler output readback differs from captured proof".into());
            }
        }
        Ok(())
    })();
    if let Err(e) = final_verification {
        return unknown(
            j,
            a,
            &r,
            &format!("Compiler exited, but final source/config/output proof failed: {e}"),
        );
    }
    let outcome = if timed_out {
        UpdateBuildOutcome::TimedOut
    } else if exit == Some(0)
        && failure.is_none()
        && outputs.0.error.is_none()
        && outputs.1.error.is_none()
    {
        UpdateBuildOutcome::Succeeded
    } else {
        UpdateBuildOutcome::Failed
    };
    let reason = if timed_out {
        "Compiler timeout; owned process tree terminated".into()
    } else {
        failure
            .or_else(|| outputs.0.error.clone())
            .or_else(|| outputs.1.error.clone())
            .unwrap_or_else(|| {
                format!("Compiler finished with exit {exit:?}; owned process tree empty")
            })
    };
    let done = complete(j, a, &r, outcome, exit, Some(outputs), &reason)?;
    c.build_progress(&done)?;
    Ok(done)
}
