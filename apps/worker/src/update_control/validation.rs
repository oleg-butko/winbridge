//! Explicit opt-in native validation build. Absent from ordinary worker builds.
//! Main still opens its normal runtime guard, worker journal and serve loop.
//! Only this private fixture driver may retain the enclosing validation Job.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    sync::Weak,
    time::{Duration, Instant},
};
use winbridge_update::shutdown::ShutdownDeliveryState;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    None,
    LostAck,
    DelayedReply,
    ReceiptWriteDenied,
    DelayedInstanceLock,
    SupervisorCrashBeforeExit,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupFault {
    #[default]
    None,
    ExistingDestination,
    ExitAfterCopy,
    WrongInputReport,
    HoldBeforeCopy,
}
pub const BACKUP_COLLISION: &[u8] = b"preserve existing validation backup destination";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub schema_version: u16,
    pub kind: String,
    pub root: PathBuf,
    pub worker_config_sha256: String,
    pub request: UpdateRequest,
    pub fault: Fault,
    #[serde(default)]
    pub final_backup: bool,
    #[serde(default)]
    pub backup_fault: BackupFault,
}
fn publish(root: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
/// Called only by feature-gated Main; missing fixture input changes no startup.
/// Inputs live beside this copied runtime, never in an installed configuration.
pub fn attach(worker: &Arc<Worker>) -> Result<Option<tokio::task::JoinHandle<Result<()>>>> {
    let input = std::env::var_os("WINBRIDGE_SHUTDOWN_MAIN_INPUT");
    let sha = std::env::var("WINBRIDGE_SHUTDOWN_MAIN_INPUT_SHA256").ok();
    let (path, sha) = match (input, sha) {
        (None, None) => return Ok(None),
        (Some(path), Some(sha)) => (PathBuf::from(path), sha),
        _ => return Err("Both exact Main validation input and SHA are required".into()),
    };
    if !cfg!(windows) {
        return Err("Main shutdown validation requires native Windows".into());
    }
    let build = build_identity();
    if build.source_dirty || build.source_manifest_sha256.is_none() {
        return Err("Main validation requires a clean exported source build".into());
    }
    winbridge_platform::require_local_path_prefix(&path).map_err(|e| e.to_string())?;
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(&path).map_err(|e| e.to_string())?;
    let file = fs::File::open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Regular validation input required".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Bounded validation input required".into());
    }
    if winbridge_update::sha256(&bytes) != sha {
        return Err("Main validation input SHA changed".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let root = input.root.canonicalize().map_err(|e| e.to_string())?;
    let runtime = root.join("runtime");
    let config = runtime.join("winbridge-update.local.json");
    let cfg = winbridge_update::load_config(&config)?;
    let actual_config = worker
        .config_path
        .lock()
        .map_err(|_| "Config lock poisoned")?
        .clone()
        .ok_or("Main validation requires file-backed startup")?;
    if input.schema_version != 1
        || input.kind != "winbridge_shutdown_main_validation"
        || path.canonicalize().map_err(|e| e.to_string())? != root.join("input.json")
        || std::env::current_exe()
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?
            != runtime.join("winbridge-worker.exe")
        || cfg
            .runtime_directory
            .canonicalize()
            .map_err(|e| e.to_string())?
            != runtime
        || cfg
            .source_directory
            .canonicalize()
            .map_err(|e| e.to_string())?
            != root
                .parent()
                .ok_or("Validation root has no gate parent")?
                .join("source")
        || actual_config.canonicalize().map_err(|e| e.to_string())?
            != runtime.join("winbridge-worker.local.json")
        || worker
            .config
            .state_directory
            .canonicalize()
            .map_err(|e| e.to_string())?
            != root.join("state")
        || worker.config.worker.controller_url.is_some()
        || worker
            .runtime_guard
            .lock()
            .map_err(|_| "Runtime guard lock poisoned")?
            .is_none()
        || hash_config(&actual_config)? != input.worker_config_sha256
        || input.request.expected.worker_config_sha256 != input.worker_config_sha256
    {
        return Err(
            "Main validation requires its own exact copied runtime/source/state and no controller"
                .into(),
        );
    }
    let image_sha =
        winbridge_update::supervisor::image_sha256(&runtime.join("winbridge-update.exe"))?;
    worker.enable_update_control(&config, &image_sha)?;
    let reg = worker.control_registration()?;
    if input.fault == Fault::LostAck {
        reg.drop_shutdown_ack.store(true, Ordering::Release);
    }
    if input.fault == Fault::ReceiptWriteDenied {
        worker.store()?.connection.execute_batch("CREATE TRIGGER validation_deny_shutdown BEFORE INSERT ON worker_shutdown_receipts BEGIN SELECT RAISE(ABORT,'validation receipt write denied');END;")
            .map_err(|e| e.to_string())?;
    }
    let worker = worker.clone();
    let weak = Arc::downgrade(&worker);
    Ok(Some(tokio::spawn(async move {
        let result = drive(worker, &reg, &input).await;
        if let Err(error) = &result {
            let _ = publish(
                &input.root,
                "validation-error.json",
                &serde_json::json!({"error":error,"runtimeApplied":false}),
            );
            if let Some(worker) = weak.upgrade() {
                worker.stop();
            }
        }
        result
    })))
}
async fn idle(
    h: &SupervisorHandle,
    expected: Option<UpdatePhase>,
    seconds: u64,
) -> Result<UpdatePreparationView> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let v = h.candidate_shutdown(false).await?;
        if let Some(e) = &v.driver_error {
            return Err(e.clone());
        }
        if !v.driver_active {
            if expected.is_some_and(|p| p != v.update.phase) {
                return Err(format!(
                    "Unexpected pipeline phase {:?}, expected {expected:?}",
                    v.update.phase
                ));
            }
            return Ok(v);
        }
        if Instant::now() >= until {
            return Err("Main validation pipeline deadline exceeded".into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
async fn drive(worker: Arc<Worker>, reg: &Arc<Registration>, input: &Input) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(10);
    while !reg.ready.load(Ordering::Acquire) {
        if Instant::now() >= until {
            return Err("Normal serve control listener did not become ready".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let weak: Weak<Worker> = Arc::downgrade(&worker);
    let WorkerSupervisorLaunch::Started(mut h) =
        worker.launch_supervisor_inner(&input.request, true).await?
    else {
        return Err("Fresh Main validation must not resume an existing attempt".into());
    };
    // This fixture task must not retain the instance lock through an Arc while
    // the supervisor waits for Main to exit. Main joins us only until admission.
    drop(worker);
    publish(&input.root, "supervisor.json", &h.record)?;
    if input.final_backup {
        // Local fixture-only capability, deliberately excluded from shared
        // report whitelists. It is useless after this owned supervisor exits.
        publish(
            &input.root,
            "supervisor-control.private",
            &h.validation_private_client()?,
        )?;
    }
    h.runtime_baseline(true).await?;
    publish(&input.root, "01-baseline.json", &idle(&h, None, 60).await?)?;
    h.runtime_preimages(true).await?;
    publish(&input.root, "02-preimages.json", &idle(&h, None, 60).await?)?;
    h.preparation(true).await?;
    publish(
        &input.root,
        "03-source.json",
        &idle(&h, Some(UpdatePhase::Building), 600).await?,
    )?;
    h.build_candidate().await?;
    publish(
        &input.root,
        "04-build.json",
        &idle(&h, Some(UpdatePhase::Building), 360).await?,
    )?;
    h.verify_candidate_artifacts().await?;
    publish(
        &input.root,
        "05-artifacts.json",
        &idle(&h, Some(UpdatePhase::ValidatingCandidate), 60).await?,
    )?;
    h.candidate_health(true).await?;
    publish(
        &input.root,
        "06-health.json",
        &idle(&h, Some(UpdatePhase::CandidateVerified), 90).await?,
    )?;
    h.candidate_drain(true).await?;
    publish(
        &input.root,
        "07-drain.json",
        &idle(&h, Some(UpdatePhase::Quiesced), 60).await?,
    )?;
    let gate = if input.fault == Fault::DelayedReply {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *reg.shutdown_test_gate
            .lock()
            .map_err(|_| "Validation gate lock poisoned")? = Some(Arc::new(ControlTestGate {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
        }));
        Some((entered_rx, release_tx))
    } else {
        None
    };
    let start = if input.final_backup {
        h.shutdown_for_state_backup().await?
    } else {
        h.candidate_shutdown(true).await?
    };
    publish(&input.root, "08-shutdown-admitted.json", &start)?;
    if let Some((entered, release)) = gate {
        tokio::time::timeout(Duration::from_secs(5), entered)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let v = h.candidate_shutdown(false).await?;
        if v.update.phase != UpdatePhase::Stopping
            || v.candidate_shutdown.as_ref().is_none_or(|s| {
                s.phase != CandidateShutdownPhase::AwaitingAcceptance
                    || s.physical_custody_held
                    || s.exited_at_ms.is_some()
            })
        {
            return Err("Held acceptance callback incorrectly authorized process exit".into());
        }
        let worker = weak
            .upgrade()
            .ok_or("Worker disappeared while callback held")?;
        let saved = UpdateJournal::inspect_shutdown(&reg.directory, &v.update.update_id)
            .map_err(|e| e.to_string())?
            .ok_or("Shutdown missing")?;
        if worker.stop.load(Ordering::Acquire)
            || worker
                .store()?
                .worker_shutdown_receipt(&v.update.update_id, &saved.intent.command)?
                .is_none()
        {
            return Err("Receipt was not durable before reply retirement".into());
        }
        drop(worker);
        publish(&input.root, "09-delayed-reply.json", &v)?;
        release.send(()).map_err(|e| e.to_string())?;
    }
    let until = Instant::now() + Duration::from_secs(55);
    loop {
        let view = h.candidate_shutdown(false).await?;
        let saved = UpdateJournal::inspect_shutdown(&reg.directory, &view.update.update_id)
            .map_err(|e| e.to_string())?
            .ok_or("Shutdown intent missing")?;
        if input.fault == Fault::ReceiptWriteDenied {
            if view.driver_error.is_some() && !view.driver_active {
                if !matches!(saved.delivery, ShutdownDeliveryState::Uncertain { .. })
                    || view.update.phase != UpdatePhase::Stopping
                {
                    return Err("Denied receipt unexpectedly approved shutdown".into());
                }
                let worker = weak.upgrade().ok_or("Receipt failure stopped worker")?;
                if worker.stop.load(Ordering::Acquire)
                    || worker
                        .store()?
                        .worker_shutdown_receipt(&view.update.update_id, &saved.intent.command)?
                        .is_some()
                {
                    return Err("Receipt failure stopped worker or committed an acceptance".into());
                }
                // Explicit fixture teardown, not production shutdown acceptance.
                worker.stop();
                break;
            }
        } else {
            if let Some(e) = &view.driver_error {
                return Err(e.clone());
            }
            if matches!(saved.delivery, ShutdownDeliveryState::Acknowledged { .. }) {
                if view
                    .candidate_shutdown
                    .as_ref()
                    .is_none_or(|s| s.exited_at_ms.is_some() || s.physical_custody_held)
                {
                    return Err("Live Main was prematurely reported exited".into());
                }
                break;
            }
        }
        if Instant::now() >= until {
            return Err("Shutdown acceptance did not settle".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if input.fault == Fault::DelayedInstanceLock {
        // The helper is owned by the external test parent, outside this Main
        // process. Wait until serve has dropped all Worker Arcs/instance lock.
        let until = Instant::now() + Duration::from_secs(10);
        while weak.upgrade().is_some() {
            if Instant::now() >= until {
                return Err("Normal serve retained Worker beyond stop".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        publish(
            &input.root,
            "request-instance-lock.json",
            &serde_json::json!({"updateId":h.record.spec.update_id}),
        )?;
        while !input
            .root
            .join("instance-lock-held.json")
            .try_exists()
            .map_err(|e| e.to_string())?
        {
            if Instant::now() >= until {
                return Err("Owned external lock holder did not become ready".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    let duplicate = if input.final_backup {
        h.shutdown_for_state_backup().await?
    } else {
        h.candidate_shutdown(true).await?
    };
    let count = reg.shutdown_requests.load(Ordering::Acquire);
    if count != 1 {
        return Err(format!("Expected one RequestShutdown, observed {count}"));
    }
    publish(
        &input.root,
        "worker-proof.json",
        &serde_json::json!({"schemaVersion":1,"kind":"shutdown_main_before_exit","passed":true,"fault":input.fault,"build":build_identity(),"worker":reg.binding,"supervisor":h.record,"requestShutdownCount":count,"duplicate":duplicate,"normalMain":true,"normalServe":true,"validationFeature":true,"receiptDeniedFixtureStop":input.fault==Fault::ReceiptWriteDenied,"runtimeApplied":false,"windowsOpened":false}),
    )?;
    if input.fault == Fault::SupervisorCrashBeforeExit {
        // Keep Main alive after acceptance until the parent crashes only its
        // owned validation Job. This is never a production shutdown path.
        publish(
            &input.root,
            "awaiting-owned-crash.json",
            &serde_json::json!({"proofPublished":true}),
        )?;
        std::future::pending::<()>().await;
    }
    Ok(())
}

/// Validation-only fault input, authenticated by the same local case path/SHA
/// and checked export scope. Default builds have no hook or environment reader.
pub(crate) fn export_before_copy(
    guard: &winbridge_update::state_backup::StateBackupExportGuard,
    destination: &Path,
) -> Result<BackupFault> {
    let path = std::env::var_os("WINBRIDGE_SHUTDOWN_MAIN_INPUT");
    let expected = std::env::var("WINBRIDGE_SHUTDOWN_MAIN_INPUT_SHA256").ok();
    let (path, expected) = match (path, expected) {
        (None, None) => return Ok(BackupFault::None),
        (Some(p), Some(s)) => (PathBuf::from(p), s),
        _ => return Err("Both validation input and SHA required".into()),
    };
    let raw = winbridge_workspace_sync::read_bounded(&path, 65536)?;
    if winbridge_update::sha256(&raw) != expected {
        return Err("Export validation input changed".into());
    }
    let input: Input =
        serde_json::from_slice(&raw).map_err(|_| "Invalid export validation fields")?;
    let root = input.root.canonicalize().map_err(|e| e.to_string())?;
    let state = guard
        .worker_configuration()
        .state_directory
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let runtime = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !input.final_backup
        || input.kind != "winbridge_shutdown_main_validation"
        || input.schema_version != 1
        || path.canonicalize().map_err(|e| e.to_string())? != root.join("input.json")
        || state != root.join("state")
        || runtime.parent() != Some(root.join("runtime").as_path())
        || input.worker_config_sha256 != guard.intent().binding.worker_config.sha256
        || input.request.expected.worker_build_id
            != guard
                .intent()
                .binding
                .shutdown
                .intent
                .binding
                .loaded_worker
                .descriptor
                .build
                .build_id
    {
        return Err("Export validation scope rejected".into());
    }
    if input.backup_fault != BackupFault::None {
        publish(
            &root,
            "backup-helper-fault.json",
            &serde_json::json!({"fault":input.backup_fault,"attemptId":guard.intent().attempt_id,"process":guard.process()}),
        )?;
    }
    match input.backup_fault {
        BackupFault::ExistingDestination => {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|e| e.to_string())?;
            f.write_all(BACKUP_COLLISION)
                .and_then(|_| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        BackupFault::HoldBeforeCopy => {
            publish(
                &root,
                "backup-helper-held.json",
                &serde_json::json!({"attemptId":guard.intent().attempt_id,"process":guard.process()}),
            )?;
            let until = Instant::now() + Duration::from_secs(90);
            while !root.join("release-backup-helper.json").is_file() {
                if Instant::now() >= until {
                    return Err("Validation backup hold expired".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        _ => {}
    }
    Ok(input.backup_fault)
}
pub(crate) fn export_after_copy(fault: BackupFault) {
    if fault == BackupFault::ExitAfterCopy {
        std::process::exit(79);
    }
}
pub(crate) fn export_report(
    fault: BackupFault,
    mut report: WorkerStateBackupReport,
) -> WorkerStateBackupReport {
    if fault == BackupFault::WrongInputReport {
        report.input_sha256 = "0".repeat(64);
    }
    report
}
