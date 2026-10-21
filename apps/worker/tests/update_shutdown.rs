//! Actual feature-gated product Main and an independent updater supervisor.
//! All four old roles and candidate helpers are real executables. No installed update.
#![cfg(all(windows, feature = "update-validation"))]
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winbridge_protocol::{update::*, ArtifactRole, ResolvedCommand};
use winbridge_update::{
    artifact::{inspect_image, query_image},
    installed_set::{InstalledArtifact, InstalledSetManifest, MANIFEST_NAME},
    journal::UpdateJournal,
    shutdown::{ShutdownDeliveryState, ShutdownInspectionRequest},
};
use winbridge_worker::update_validation::{Fault, Input};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GateInput {
    root: PathBuf,
    target: UpdateSourcePin,
    archive: PathBuf,
    old_images: PathBuf,
    powershell: PathBuf,
    validation_cache: PathBuf,
    #[serde(default)]
    supplemental: bool,
    #[serde(default)]
    final_backup: bool,
}
fn write(path: &Path, value: &impl serde::Serialize) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}
fn adapter(input: &GateInput) -> PathBuf {
    use std::os::windows::process::CommandExt;
    let source = input.root.join("build-adapter.rs");
    let image = input.root.join("build-adapter.exe");
    fs::write(&source, format!(r#"fn main(){{use std::os::windows::process::CommandExt;let s=std::process::Command::new({:?}).creation_flags(0x08000000).args(std::env::args_os().skip(1)).arg("-ValidationCacheDirectory").arg({:?}).status().expect("private build adapter");std::process::exit(s.code().unwrap_or(1));}}"#, input.powershell, input.validation_cache)).unwrap();
    let output = std::process::Command::new("rustc.exe")
        .creation_flags(0x08000000)
        .args(["--edition=2021"])
        .arg(source)
        .arg("-o")
        .arg(&image)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    image
}
fn setup(
    input: &GateInput,
    fault: Fault,
    adapter: &Path,
) -> (PathBuf, PathBuf, InstalledSetManifest) {
    let root = input.root.join(format!("{fault:?}"));
    fs::create_dir(&root).unwrap();
    let runtime = root.join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::create_dir(root.join("state")).unwrap();
    fs::create_dir(root.join("packages")).unwrap();
    fs::copy(
        &input.archive,
        root.join("packages").join(&input.target.archive_name),
    )
    .unwrap();
    let worker_config = runtime.join("winbridge-worker.local.json");
    write(
        &worker_config,
        &json!({"schemaVersion":1,"instanceId":format!("shutdown-{}",uuid::Uuid::new_v4()),"nodeId":"shutdown-main-test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"source","workspaceRoot":input.root.join("source"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]}),
    );
    let config = runtime.join("winbridge-update.local.json");
    write(
        &config,
        &json!({"schemaVersion":3,"sourceDirectory":input.root.join("source"),"runtimeDirectory":runtime,"workerConfigPath":worker_config,"sourcePackagesDirectory":root.join("packages"),"build":{"powershellExecutable":adapter,"timeoutMs":300000}}),
    );
    let cfg = winbridge_update::load_config(&config).unwrap();
    let scope = winbridge_update::installation_scope(&cfg).unwrap();
    let info = UpdateJournal::open(&runtime.join("update-state"), &scope)
        .unwrap()
        .info()
        .unwrap();
    let mut artifacts = Vec::new();
    for (name, role) in [
        ("winbridge-worker.exe", ArtifactRole::Worker),
        ("winbridge-fixture.exe", ArtifactRole::Fixture),
        ("winbridge-desktop.exe", ArtifactRole::Desktop),
        ("winbridge-update.exe", ArtifactRole::Updater),
    ] {
        let original = if role == ArtifactRole::Worker {
            PathBuf::from(env!("CARGO_BIN_EXE_winbridge-worker"))
        } else {
            input.old_images.join(name)
        };
        let path = runtime.join(name);
        fs::copy(original, &path).unwrap();
        let query = query_image(&path, role).unwrap();
        artifacts.push(InstalledArtifact {
            name: name.into(),
            bytes: query.image.bytes,
            sha256: query.image.sha256,
            descriptor: query.descriptor,
        });
    }
    let manifest = InstalledSetManifest {
        schema_version: 1,
        kind: "winbridge_installed_set".into(),
        set_id: uuid::Uuid::new_v4().to_string(),
        installation_id: info.installation_id.clone(),
        scope_sha256: scope.clone(),
        artifacts,
    };
    write(&runtime.join(MANIFEST_NAME), &manifest);
    if input.final_backup {
        seed_history(&worker_config, &runtime, &root);
    }
    let config_sha = winbridge_update::sha256(&fs::read(&worker_config).unwrap());
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: format!("main-shutdown-{fault:?}"),
        source: input.target.clone(),
        expected: UpdateInstallationPin {
            installation_id: info.installation_id,
            scope_sha256: scope,
            worker_build_id: manifest.artifacts[0].descriptor.build.build_id.clone(),
            worker_config_sha256: config_sha.clone(),
            baseline_manifest_sha256: input.target.manifest_sha256.clone(),
        },
    };
    write(
        &root.join("input.json"),
        &Input {
            schema_version: 1,
            kind: "winbridge_shutdown_main_validation".into(),
            root: root.clone(),
            worker_config_sha256: config_sha,
            request,
            fault,
            final_backup: input.final_backup,
        },
    );
    (root, worker_config, manifest)
}
fn capture(mut reader: winbridge_platform::OutputReader) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut result = Vec::new();
        let mut buffer = [0u8; 16384];
        loop {
            let n = reader.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            let keep = n.min((2 * 1024 * 1024usize).saturating_sub(result.len()));
            result.extend_from_slice(&buffer[..keep]);
        }
        result
    })
}
struct LockHolder {
    child: winbridge_platform::ManagedChild,
    output: (
        std::thread::JoinHandle<Vec<u8>>,
        std::thread::JoinHandle<Vec<u8>>,
    ),
}
impl LockHolder {
    fn start(root: &Path) -> Self {
        let command = ResolvedCommand {
            executable: std::env::current_exe().unwrap().to_string_lossy().into(),
            arguments: vec![
                "--exact".into(),
                "instance_lock_holder_process".into(),
                "--ignored".into(),
                "--nocapture".into(),
                "--skip".into(),
                format!("lock-root={}", root.display()),
            ],
            working_directory: root.to_string_lossy().into(),
            timeout_ms: 900000,
        };
        let (mut child, out, err) = winbridge_platform::ManagedChild::prepare_atomic_executable(
            &command,
            Some(&format!(
                "WinBridge-shutdown-lock-holder-{}",
                uuid::Uuid::new_v4()
            )),
            None,
        )
        .unwrap();
        let output = (capture(out), capture(err));
        child.resume().unwrap();
        Self { child, output }
    }
    fn released(&mut self) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                assert!(s.success());
                break;
            }
            assert!(Instant::now() < until, "Owned lock helper did not release");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(self.child.tree_is_empty().unwrap());
    }
    fn finish(mut self, root: &Path) {
        self.child.terminate_tree().unwrap();
        fs::write(
            root.join("lock-helper-stdout.log"),
            self.output.0.join().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("lock-helper-stderr.log"),
            self.output.1.join().unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "Owned lock helper; invoked only by the supplemental native parent"]
fn instance_lock_holder_process() {
    let root = PathBuf::from(
        std::env::args()
            .find_map(|s| s.strip_prefix("lock-root=").map(str::to_owned))
            .expect("Explicit owned lock root"),
    );
    let input: Input = serde_json::from_slice(&fs::read(root.join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input.root.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(input.fault, Fault::DelayedInstanceLock);
    let until = Instant::now() + Duration::from_secs(900);
    while !root.join("request-instance-lock.json").is_file() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    let sup: SupervisorRecord =
        serde_json::from_slice(&fs::read(root.join("supervisor.json")).unwrap()).unwrap();
    let saved =
        UpdateJournal::inspect_shutdown(&root.join("runtime/update-state"), &sup.spec.update_id)
            .unwrap()
            .unwrap();
    assert!(matches!(
        saved.delivery,
        ShutdownDeliveryState::Acknowledged { .. }
    ));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    assert_eq!(
        winbridge_platform::file_identity(&file).unwrap(),
        saved.intent.binding.worker_state.lock_identity
    );
    // Weak::upgrade stops succeeding before the final Arc destructor has
    // necessarily closed every field. Wait for the actual byte lock, retaining
    // the same already-opened original file; never create/rebind a lock.
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => break,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                assert!(
                    Instant::now() < until,
                    "Original instance lock did not retire: {e}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("Original lock acquisition failed: {e}"),
        }
    }
    assert_eq!(
        winbridge_platform::file_identity(&file).unwrap(),
        saved.intent.binding.worker_state.lock_identity
    );
    write(
        &root.join("instance-lock-holder.json"),
        &json!({"process":winbridge_platform::infrastructure::stamp(std::process::id()).unwrap(),"state":saved.intent.binding.worker_state,"attemptId":saved.intent.attempt_id}),
    );
    write(
        &root.join("instance-lock-held.json"),
        &json!({"proofPublished":true}),
    );
    let until = Instant::now() + Duration::from_secs(60);
    while !root.join("release-instance-lock.json").is_file() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(file);
}
fn before_exit_crash(root: &Path, directory: &Path) -> serde_json::Value {
    let before: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("worker-proof.json")).unwrap()).unwrap();
    assert_eq!(before["requestShutdownCount"], 1);
    let sup: SupervisorRecord = serde_json::from_value(before["supervisor"].clone()).unwrap();
    let shutdown = UpdateJournal::inspect_shutdown(directory, &sup.spec.update_id)
        .unwrap()
        .unwrap();
    assert!(matches!(
        shutdown.delivery,
        ShutdownDeliveryState::Acknowledged { .. }
    ));
    assert!(
        winbridge_platform::infrastructure::matches(&shutdown.intent.binding.worker.process)
            .unwrap()
    );
    assert!(winbridge_platform::infrastructure::matches(sup.process.as_ref().unwrap()).unwrap());
    let (_, state) = UpdateJournal::read_snapshot(directory, Some(&sup.spec.update_id)).unwrap();
    let state = state.unwrap();
    assert_eq!(state.phase, UpdatePhase::Stopping);
    json!({"fault":Fault::SupervisorCrashBeforeExit,"beforeMainExit":before,"shutdown":shutdown,"root":state,"mainAndSupervisorAliveBeforeCrash":true,"physicalCustodyAfterExit":false})
}

fn restart_refused(worker: &Path, config: &Path, root: &Path, update_id: &str) {
    let command = ResolvedCommand {
        executable: worker.to_string_lossy().into(),
        arguments: vec![
            "--config".into(),
            config.to_string_lossy().into(),
            "serve".into(),
        ],
        working_directory: root.to_string_lossy().into(),
        timeout_ms: 30000,
    };
    let (mut child, stdout, stderr) = winbridge_platform::ManagedChild::prepare_atomic_executable(
        &command,
        Some(&format!(
            "WinBridge-shutdown-restart-denied-{}",
            uuid::Uuid::new_v4()
        )),
        None,
    )
    .unwrap();
    let capture = (capture(stdout), capture(stderr));
    child.resume().unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if Instant::now() >= until {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    child.terminate_tree().unwrap();
    let stdout = capture.0.join().unwrap();
    let stderr = capture.1.join().unwrap();
    assert!(!status
        .expect("Ordinary Main restart did not refuse pending update")
        .success());
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&stderr).contains(update_id)
            && !String::from_utf8_lossy(&stderr).contains("WinBridge worker ready:"),
        "{}",
        String::from_utf8_lossy(&stderr)
    );
}

fn seed_history(config: &Path, runtime: &Path, root: &Path) {
    use winbridge_protocol::{
        CommandSource, LocalRequest, LocalResponse, TaskStatus, TaskSubmission,
    };
    let worker = winbridge_worker::Worker::open_file(config).unwrap();
    // fixture writes runs.log in cwd. Keep that generated side effect in the
    // explicitly preserved build cache, outside verified source inputs.
    fs::create_dir_all(root.parent().unwrap().join("source/target/history-seed")).unwrap();
    let submission = TaskSubmission {
        protocol_version: 1,
        request_id: "backup-history".into(),
        project_id: "source".into(),
        idempotency_key: "backup-history".into(),
        timeout_ms: 10000,
        snapshot_id: None,
        command: CommandSource::Direct {
            executable: runtime
                .join("winbridge-fixture.exe")
                .to_string_lossy()
                .into(),
            arguments: vec!["success".into()],
            working_directory: "target/history-seed".into(),
        },
    };
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
        request: submission.clone(),
    }) else {
        panic!("History task rejected")
    };
    let id = task.task_id.clone();
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        worker.schedule().unwrap();
        let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask {
            task_id: id.clone(),
        }) else {
            panic!("History task missing")
        };
        if task.status.is_terminal() && worker.is_idle() {
            assert_eq!(task.status, TaskStatus::Succeeded);
            break;
        }
        assert!(Instant::now() < until, "History seed task did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let LocalResponse::Task { task } = worker.handle(LocalRequest::Submit {
        request: submission,
    }) else {
        panic!("History duplicate rejected")
    };
    assert_eq!(task.task_id, id);
    worker.stop();
    drop(worker);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => {
                fs2::FileExt::unlock(&lock).unwrap();
                break;
            }
            Err(_) => {
                assert!(
                    Instant::now() < until,
                    "History seed retained instance lock"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
fn journal_rows(path: &Path) -> (String, usize, usize) {
    let c = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let mut q = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut data = Vec::new();
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut q = c
            .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
            .unwrap();
        let count = q.column_count();
        let rows = q
            .query_map([], |r| {
                (0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        data.push((name, format!("{rows:?}")));
    }
    let tasks: i64 = c
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    let output: i64 = c
        .query_row("SELECT count(*) FROM output", [], |r| r.get(0))
        .unwrap();
    (
        winbridge_update::sha256(&serde_json::to_vec(&data).unwrap()),
        usize::try_from(tasks).unwrap(),
        usize::try_from(output).unwrap(),
    )
}
fn journal_bytes(root: &Path) -> serde_json::Value {
    let mut files = Vec::new();
    for name in ["journal.sqlite", "journal.sqlite-wal"] {
        let p = root.join("state").join(name);
        // A readonly SQLite connection can create an empty WAL coordination
        // file. Only that absent/empty distinction is ignored; every frame and
        // every byte of the main journal remains covered by its exact digest.
        let digest = p.is_file().then(|| fs::read(&p).unwrap()).and_then(|b| {
            if name.ends_with("-wal") && b.is_empty() {
                None
            } else {
                Some(winbridge_update::sha256(&b))
            }
        });
        files.push((name, digest));
    }
    serde_json::to_value(files).unwrap()
}
#[test]
fn journal_fingerprint_allows_only_empty_wal_creation_and_detects_data_changes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("state")).unwrap();
    let main = root.path().join("state/journal.sqlite");
    let wal = root.path().join("state/journal.sqlite-wal");
    fs::write(&main, b"main bytes").unwrap();
    let before = journal_bytes(root.path());
    fs::write(&wal, []).unwrap();
    assert_eq!(journal_bytes(root.path()), before);
    fs::write(&wal, b"committed frame").unwrap();
    let framed = journal_bytes(root.path());
    assert_ne!(framed, before);
    fs::write(&wal, b"changed frame").unwrap();
    assert_ne!(journal_bytes(root.path()), framed);
    fs::write(&wal, []).unwrap();
    fs::write(&main, b"changed main").unwrap();
    assert_ne!(journal_bytes(root.path()), before);
}
fn final_backup(root: &Path, sup: &SupervisorRecord) -> serde_json::Value {
    let mut request: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    assert_eq!(request.contract_version, 15);
    assert_eq!(
        winbridge_update::sha256(request.token.as_bytes()),
        sup.spec.token_sha256
    );
    assert_eq!(request.launch_id, sup.spec.launch_id);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let call = |q: &SupervisorRequest| -> UpdatePreparationView {
        let response: SupervisorResponse = rt
            .block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                sup.process.as_ref().unwrap().process_id,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ))
            .unwrap();
        assert_eq!(response.contract_version, 15);
        assert!(response.error.is_none(), "{:?}", response.error);
        let view = response.preparation.unwrap();
        assert_eq!(view.schema_version, 7);
        view
    };
    let source_path = root.join("state/journal.sqlite");
    let bytes = journal_bytes(root);
    let rows = journal_rows(&source_path);
    assert!(
        rows.1 >= 1 && rows.2 >= 2,
        "Real task/output history missing"
    );
    request.action = SupervisorAction::BackupWorkerState;
    let admitted = call(&request);
    assert!(admitted
        .candidate_state_backup
        .as_ref()
        .unwrap()
        .attempt_id
        .is_some());
    request.action = SupervisorAction::ObserveStateBackup;
    let until = Instant::now() + Duration::from_secs(150);
    let done = loop {
        let v = call(&request);
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if !v.driver_active {
            break v;
        }
        assert!(Instant::now() < until, "Final backup did not settle");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        done.candidate_state_backup.as_ref().unwrap().phase,
        CandidateStateBackupPhase::Verified
    );
    assert!(
        done.candidate_shutdown
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    let directory = root.join("runtime/update-state");
    let record = UpdateJournal::inspect_state_backup(&directory, &sup.spec.update_id)
        .unwrap()
        .unwrap();
    let report = record.report.as_ref().unwrap();
    report.validate_envelope().unwrap();
    assert_eq!(
        winbridge_update::state_backup::pin_backup_file(Path::new(&report.backup.path))
            .unwrap()
            .evidence(),
        &report.backup
    );
    assert_eq!(journal_rows(Path::new(&report.backup.path)), rows);
    assert_eq!(journal_bytes(root), bytes);
    request.action = SupervisorAction::BackupWorkerState;
    let duplicate = call(&request);
    assert_eq!(
        duplicate.candidate_state_backup,
        done.candidate_state_backup
    );
    assert_eq!(
        UpdateJournal::inspect_state_backup(&directory, &sup.spec.update_id)
            .unwrap()
            .unwrap(),
        record
    );
    assert_eq!(journal_bytes(root), bytes);
    json!({"record":record,"view":done,"duplicate":duplicate,"sourceRowsSha256":rows.0,"backupRowsSha256":rows.0,"sourceTaskCount":rows.1,"sourceOutputCount":rows.2,"sourceBytesUnchanged":true,"allLogicalRowsPreserved":true,"duplicateNoNewAttempt":true,"privateAuthorityNotExported":true})
}
fn case(input: &GateInput, fault: Fault, adapter: &Path) -> serde_json::Value {
    let (root, config, manifest) = setup(input, fault, adapter);
    let worker = root.join("runtime/winbridge-worker.exe");
    let input_path = root.join("input.json");
    // Only the suspended creation inherits these two variables. Restore before
    // running any helper; cold inspection never initializes this fixture driver.
    let vars = [
        (
            "WINBRIDGE_SHUTDOWN_MAIN_INPUT",
            input_path.as_os_str().to_owned(),
        ),
        (
            "WINBRIDGE_SHUTDOWN_MAIN_INPUT_SHA256",
            winbridge_update::sha256(&fs::read(&input_path).unwrap()).into(),
        ),
    ];
    let previous: Vec<_> = vars
        .iter()
        .map(|(k, _)| (*k, std::env::var_os(k)))
        .collect();
    for (k, v) in &vars {
        std::env::set_var(k, v);
    }
    let command = ResolvedCommand {
        executable: worker.to_string_lossy().into(),
        arguments: vec![
            "--config".into(),
            config.to_string_lossy().into(),
            "serve".into(),
        ],
        working_directory: root.to_string_lossy().into(),
        timeout_ms: 900000,
    };
    let prepared = winbridge_platform::ManagedChild::prepare_atomic_executable(
        &command,
        Some(&format!(
            "WinBridge-shutdown-main-validation-{}",
            uuid::Uuid::new_v4()
        )),
        None,
    );
    for (k, v) in previous {
        if let Some(v) = v {
            std::env::set_var(k, v)
        } else {
            std::env::remove_var(k)
        }
    }
    let (mut child, stdout, stderr) = prepared.unwrap();
    let capture = (capture(stdout), capture(stderr));
    let mut holder = (fault == Fault::DelayedInstanceLock).then(|| LockHolder::start(&root));
    child.resume().unwrap();
    let until = Instant::now() + Duration::from_secs(900);
    let mut progress = Instant::now();
    let status = loop {
        if fault == Fault::SupervisorCrashBeforeExit
            && root.join("awaiting-owned-crash.json").is_file()
        {
            break None;
        }
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= until {
            break None;
        }
        if progress.elapsed() >= Duration::from_secs(5) {
            let last = fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                .filter(|n| n.starts_with(|c: char| c.is_ascii_digit()))
                .max();
            println!(
                "Actual Main shutdown validation {fault:?}: {}",
                last.unwrap_or_else(|| "startup".into())
            );
            progress = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let directory = root.join("runtime/update-state");
    // Keep cleanup unconditional on assertions below; killing this validation
    // Job owns only the copied worker/supervisor descendants, never installed ones.
    let proof = || {
        if fault == Fault::SupervisorCrashBeforeExit {
            assert!(status.is_none());
            return before_exit_crash(&root, &directory);
        }
        let status = status.expect("Owned Main validation deadline exceeded");
        assert!(
            status.success(),
            "Main failed; inspect retained case stdout/stderr/validation-error files"
        );
        let before: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("worker-proof.json")).unwrap()).unwrap();
        assert_eq!(before["requestShutdownCount"], 1);
        assert_eq!(before["normalMain"], true);
        let sup: SupervisorRecord = serde_json::from_value(before["supervisor"].clone()).unwrap();
        let id = &sup.spec.update_id;
        let process = sup.process.as_ref().unwrap();
        if let Some(holder) = &mut holder {
            let held: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("instance-lock-holder.json")).unwrap())
                    .unwrap();
            let stamp: SupervisorProcess = serde_json::from_value(held["process"].clone()).unwrap();
            let expected = holder.child.identity().unwrap();
            assert_eq!(stamp.process_id, expected.process_id);
            assert_eq!(stamp.birth, expected.creation_time.to_string());
            assert_eq!(stamp.host_id, expected.host_id);
            assert_eq!(stamp.session_id, expected.session_id);
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                let saved = UpdateJournal::inspect_shutdown(&directory, id)
                    .unwrap()
                    .unwrap();
                assert!(matches!(
                    saved.delivery,
                    ShutdownDeliveryState::Acknowledged { .. }
                ));
                assert_eq!(
                    serde_json::to_value(&saved.intent.binding.worker_state).unwrap(),
                    held["state"]
                );
                assert_eq!(saved.intent.attempt_id, held["attemptId"].as_str().unwrap());
                let (_, state) = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
                assert_eq!(state.unwrap().phase, UpdatePhase::Stopping);
                assert!(winbridge_platform::infrastructure::matches(&stamp).unwrap());
                std::thread::sleep(Duration::from_millis(50));
            }
            write(
                &root.join("release-instance-lock.json"),
                &json!({"updateId":id}),
            );
            holder.released();
        }
        let until = Instant::now() + Duration::from_secs(65);
        let shutdown = loop {
            let s = UpdateJournal::inspect_shutdown(&directory, id)
                .unwrap()
                .unwrap();
            if fault == Fault::ReceiptWriteDenied
                || matches!(s.delivery, ShutdownDeliveryState::Exited { .. })
            {
                break s;
            }
            assert!(
                Instant::now() < until,
                "Supervisor did not confirm exact exit; inspect retained supervisor stderr"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(
            winbridge_platform::infrastructure::matches(process).unwrap(),
            "Supervisor did not outlive Main"
        );
        let (_, root_state) = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
        let root_state = root_state.unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("state/worker.lock"))
            .unwrap();
        if fault == Fault::ReceiptWriteDenied {
            assert_eq!(root_state.phase, UpdatePhase::Stopping);
            assert!(matches!(
                shutdown.delivery,
                ShutdownDeliveryState::Uncertain { .. }
            ));
            fs2::FileExt::try_lock_exclusive(&lock).unwrap();
            fs2::FileExt::unlock(&lock).unwrap();
        } else {
            assert_eq!(root_state.phase, UpdatePhase::Replacing);
            let error = fs2::FileExt::try_lock_exclusive(&lock).unwrap_err();
            assert!(
                error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            );
            if fault == Fault::LostAck {
                assert_eq!(
                    shutdown.revision, 4,
                    "Lost ACK must retain Uncertain edge before readonly reconciliation"
                );
                assert!(
                    root.join("runtime/update")
                        .join(id)
                        .join(format!(
                            "shutdown-receipt-{}.json",
                            shutdown.intent.attempt_id
                        ))
                        .is_file(),
                    "Closed control channel must exercise integrated cold reconciliation"
                );
            }
            if fault == Fault::DelayedReply {
                assert!(root.join("09-delayed-reply.json").is_file());
            }
        }
        let final_state_backup = input.final_backup.then(|| final_backup(&root, &sup));
        let input = ShutdownInspectionRequest {
            schema_version: 1,
            intent: shutdown.intent.clone(),
            worker_config: inspect_image(&config).unwrap(),
            helper_image: shutdown.intent.binding.loaded_worker.image.clone(),
            worker_journal_schema: winbridge_worker::store::JOURNAL_SCHEMA_VERSION,
        };
        let path = root.join("cold-after-main.json");
        write(&path, &input);
        let cold = winbridge_update::shutdown::inspect_saved_receipt(
            &path,
            &winbridge_update::sha256(&fs::read(&path).unwrap()),
            &input,
        )
        .unwrap();
        assert_eq!(cold.receipt.is_none(), fault == Fault::ReceiptWriteDenied);
        if let ShutdownDeliveryState::Exited { receipt, .. } = &shutdown.delivery {
            assert_eq!(cold.receipt.as_deref(), Some(receipt.as_ref()));
        }
        restart_refused(&worker, &config, &root, id);
        assert!(winbridge_worker::store::Store::reject_pending_update_start(
            &root.join("state/journal.sqlite")
        )
        .unwrap_err()
        .contains(id));
        assert_eq!(
            serde_json::from_slice::<InstalledSetManifest>(
                &fs::read(root.join("runtime").join(MANIFEST_NAME)).unwrap()
            )
            .unwrap(),
            manifest
        );
        for image in &manifest.artifacts {
            assert_eq!(
                winbridge_update::sha256(
                    &fs::read(root.join("runtime").join(&image.name)).unwrap()
                ),
                image.sha256
            );
        }
        json!({"fault":fault,"finalStateBackup":final_state_backup,"beforeMainExit":before,"shutdown":shutdown,"root":root_state,"coldAfterMain":cold,"supervisorOutlivedMain":true,"physicalCustodyAfterExit":fault!=Fault::ReceiptWriteDenied,"startupStillBlocked":true,"oldRuntimeUnchanged":true})
    };
    // The closure may panic; an RAII owner still closes the test Job. On success
    // explicitly verify its complete termination and the post-crash history.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(proof));
    child.terminate_tree().unwrap();
    if let Some(holder) = holder {
        holder.finish(&root);
    }
    let until = Instant::now() + Duration::from_secs(10);
    while !child.tree_is_empty().unwrap() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(child.tree_is_empty().unwrap());
    fs::write(root.join("stdout.log"), capture.0.join().unwrap()).unwrap();
    fs::write(root.join("stderr.log"), capture.1.join().unwrap()).unwrap();
    let mut proof = match result {
        Ok(proof) => proof,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    let id = proof["root"]["updateId"].as_str().unwrap();
    let view = UpdateJournal::shutdown_preparation_snapshot(&directory, id, false, None).unwrap();
    let phase = view.candidate_shutdown.unwrap();
    assert!(!phase.physical_custody_held);
    assert_eq!(
        phase.phase,
        if matches!(
            fault,
            Fault::ReceiptWriteDenied | Fault::SupervisorCrashBeforeExit
        ) {
            CandidateShutdownPhase::Indeterminate
        } else {
            CandidateShutdownPhase::Exited
        }
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    if fault == Fault::SupervisorCrashBeforeExit {
        let after = UpdateJournal::inspect_shutdown(&directory, id)
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::to_value(&after).unwrap(), proof["shutdown"]);
        let (_, state) = UpdateJournal::read_snapshot(&directory, Some(id)).unwrap();
        assert_eq!(serde_json::to_value(state.unwrap()).unwrap(), proof["root"]);
        let input = ShutdownInspectionRequest {
            schema_version: 1,
            intent: after.intent.clone(),
            worker_config: inspect_image(&config).unwrap(),
            helper_image: after.intent.binding.loaded_worker.image.clone(),
            worker_journal_schema: winbridge_worker::store::JOURNAL_SCHEMA_VERSION,
        };
        let path = root.join("cold-after-owned-crash.json");
        write(&path, &input);
        let cold = winbridge_update::shutdown::inspect_saved_receipt(
            &path,
            &winbridge_update::sha256(&fs::read(&path).unwrap()),
            &input,
        )
        .unwrap();
        let ShutdownDeliveryState::Acknowledged { receipt, .. } = &after.delivery else {
            panic!("Crash fabricated an exit");
        };
        assert_eq!(cold.receipt.as_deref(), Some(receipt.as_ref()));
        fs2::FileExt::unlock(&lock).unwrap();
        restart_refused(&worker, &config, &root, id);
        proof["coldAfterMain"] = serde_json::to_value(cold).unwrap();
        proof["startupStillBlocked"] = json!(true);
        for image in &manifest.artifacts {
            assert_eq!(
                winbridge_update::sha256(
                    &fs::read(root.join("runtime").join(&image.name)).unwrap()
                ),
                image.sha256
            );
        }
        proof["oldRuntimeUnchanged"] = json!(true);
    }
    if fault == Fault::DelayedInstanceLock {
        proof["delayedLockHolder"] =
            serde_json::from_slice(&fs::read(root.join("instance-lock-holder.json")).unwrap())
                .unwrap();
    }
    proof["delayedOriginalLockVerified"] = json!(fault == Fault::DelayedInstanceLock);
    proof["ownedSupervisorCrashRetainsHistoryWithoutCustody"] = json!(true);
    proof
}
#[test]
#[ignore = "Explicit clean native Main shutdown gate; builds a real candidate and retains case directories"]
fn actual_main_and_supervisor_confirm_exact_shutdown_without_runtime_replacement() {
    let path = PathBuf::from(
        std::env::var_os("WINBRIDGE_SHUTDOWN_GATE_INPUT").expect("Explicit native gate input"),
    );
    let input: GateInput = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(input.root.is_dir());
    assert_ne!(
        winbridge_protocol::build_identity().source_commit.as_ref(),
        Some(&input.target.source_commit)
    );
    // Sequential cases share one verified source directory and compiler cache.
    // Each worker/runtime/state/installation remains separate. The previous
    // owned Job must be empty before the next case can lease these inputs.
    let stage = input.root.join("seed");
    winbridge_update::archive::stage_archive(&input.archive, &input.target, &stage).unwrap();
    fs::rename(stage.join("source"), input.root.join("source")).unwrap();
    let adapter = adapter(&input);
    let mut cases = Vec::new();
    let selected: &[Fault] = if input.final_backup {
        &[Fault::None]
    } else if input.supplemental {
        &[Fault::DelayedInstanceLock, Fault::SupervisorCrashBeforeExit]
    } else {
        &[
            Fault::None,
            Fault::LostAck,
            Fault::DelayedReply,
            Fault::ReceiptWriteDenied,
        ]
    };
    for &fault in selected {
        cases.push(case(&input, fault, &adapter));
        write(
            &input.root.join(format!("case-{}.json", cases.len())),
            cases.last().unwrap(),
        );
    }
    let proof = json!({"schemaVersion":1,"kind":if input.final_backup {"state_backup_main_native_proof"} else if input.supplemental {"shutdown_main_supplemental_proof"} else {"shutdown_main_native_proof"},"passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),"target":input.target,"cases":cases,"normalMain":true,"actualOldRoles":true,"actualCandidateBuild":true,"validationFeature":true,"runtimeApplied":false,"windowsOpened":false});
    write(&input.root.join("proof.json"), &proof);
}
