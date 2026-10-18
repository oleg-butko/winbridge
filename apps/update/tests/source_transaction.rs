use rusqlite::Connection;
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::{config::WorkerConfig, update::SOURCE_MANIFEST_NAME};
use winbridge_protocol::{update::*, LocalRequest, LocalResponse, WorkerInfo};
use winbridge_update::{
    archive::stage_archive,
    installation_scope,
    journal::{UpdateAuthority, UpdateJournal},
    sha256,
    source::*,
    UpdateConfig,
};
use winbridge_worker::Worker;
use zip::{write::SimpleFileOptions, ZipWriter};

#[path = "support/compiler_cases.rs"]
mod compiler;

fn info(w: &Worker) -> WorkerInfo {
    let LocalResponse::Info { info } = w.handle(LocalRequest::Info) else {
        panic!("Info")
    };
    *info
}
struct Control {
    worker: Arc<Worker>,
    events: Vec<SourceProgress>,
    fault: Option<(SourceCheckpoint, u32, &'static str)>,
    source: PathBuf,
    crash: Option<(SourceCheckpoint, Option<u32>)>,
}
impl SourceControl for Control {
    fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
        Ok(info(&self.worker))
    }
    fn progress(&mut self, e: &SourceProgress) -> Result<(), String> {
        self.events.push(e.clone());
        if self.crash == Some((e.checkpoint, e.ordinal)) {
            std::process::exit(79);
        }
        if self
            .fault
            .is_some_and(|(c, i, _)| e.checkpoint == c && e.ordinal == Some(i))
        {
            let (_, _, kind) = self.fault.take().unwrap();
            match kind {
                "edit" | "reappear" => {
                    fs::write(self.source.join(e.path.as_ref().unwrap()), b"user edit").unwrap()
                }
                _ => return Err("injected progress failure".into()),
            }
        }
        Ok(())
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.worker.stop();
    }
}
struct Fixture {
    config: UpdateConfig,
    journal: UpdateJournal,
    owner: UpdateAuthority,
    id: String,
    control: Control,
    root: PathBuf,
}
fn tree(root: &Path, commit: char, files: &[(&str, &[u8])]) -> Vec<u8> {
    for (p, b) in files {
        let path = root.join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b).unwrap();
    }
    let m = SourceManifest {
        schema_version: 1,
        source_commit: commit.to_string().repeat(40),
        dirty: false,
        created_at: "2026-10-17T00:00:00Z".into(),
        includes_built_ui: false,
        files: files
            .iter()
            .map(|(p, b)| SourceFile {
                path: (*p).into(),
                bytes: b.len() as u64,
                sha256: sha256(b),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&m).unwrap();
    fs::write(root.join(SOURCE_MANIFEST_NAME), &bytes).unwrap();
    bytes
}
impl Fixture {
    fn new(root: &Path) -> Self {
        let source = root.join("source");
        let runtime = root.join("runtime");
        let incoming = root.join("incoming");
        for p in [&source, &runtime, &incoming] {
            fs::create_dir_all(p).unwrap();
        }
        let baseline = sha256(&tree(
            &source,
            'a',
            &[
                ("Cargo.toml", b"old"),
                ("removed.rs", b"removed"),
                ("src/main.rs", b"keep"),
            ],
        ));
        let target = sha256(&tree(
            &incoming,
            'b',
            &[
                ("Cargo.toml", b"new"),
                ("src/main.rs", b"keep"),
                ("src/new.rs", b"added"),
            ],
        ));
        // Reproducible fixture bytes, never a copy of real credentials/configuration.
        for p in ["target/cache", ".git/fixture", ".env", "fixture.local.json"] {
            let path = source.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"preserved fixture").unwrap();
        }
        let cfg:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("source-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":runtime.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"source","workspaceRoot":source,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
        let worker_config_path = runtime.join("winbridge-worker.local.json");
        fs::write(&worker_config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
        let worker = Worker::open_file(&worker_config_path).unwrap();
        let config = UpdateConfig {
            schema_version: 1,
            source_directory: source.clone(),
            runtime_directory: runtime.clone(),
            worker_config_path,
        };
        fs::write(root.join("update.local.json"),serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":config.worker_config_path})).unwrap()).unwrap();
        let zip_path = root.join("source.zip");
        let mut zip = ZipWriter::new(File::create(&zip_path).unwrap());
        for p in [
            "Cargo.toml",
            "src/main.rs",
            "src/new.rs",
            SOURCE_MANIFEST_NAME,
        ] {
            zip.start_file(format!("winbridge/{p}"), SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&fs::read(incoming.join(p)).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        let pin = UpdateSourcePin {
            archive_name: "source.zip".into(),
            archive_sha256: sha256(&fs::read(&zip_path).unwrap()),
            manifest_sha256: target.clone(),
            source_commit: "b".repeat(40),
        };
        let mut journal = UpdateJournal::open(
            &runtime.join("update-state"),
            &installation_scope(&config).unwrap(),
        )
        .unwrap();
        let owner = journal.acquire_owner().unwrap();
        let j = journal.info().unwrap();
        let request = UpdateRequest {
            contract_version: 1,
            idempotency_key: "source-test".into(),
            source: pin.clone(),
            expected: UpdateInstallationPin {
                installation_id: j.installation_id,
                scope_sha256: j.scope_sha256,
                worker_build_id: info(&worker).build.unwrap().build_id,
                worker_config_sha256: sha256(&fs::read(&config.worker_config_path).unwrap()),
                baseline_manifest_sha256: baseline.clone(),
            },
        };
        let accepted = journal.accept(&owner, &request, &request.expected).unwrap();
        let id = accepted.update_id;
        journal
            .advance(
                &owner,
                &id,
                &UpdateCommand {
                    idempotency_key: "prepare".into(),
                    expected_revision: 0,
                    event: UpdateEvent::BeginPreparation,
                    reason: "Prepare fixture".into(),
                },
            )
            .unwrap();
        let spec = worker
            .update_source_spec(
                &id,
                owner.owner_id(),
                owner.epoch(),
                &source,
                (&baseline, &target),
            )
            .unwrap();
        worker.begin_source_update(spec.clone()).unwrap();
        assert_eq!(
            worker
                .advance_source_update(
                    &spec,
                    &WorkerUpdateCommand {
                        command_id: "acquire".into(),
                        expected_revision: 0,
                        action: WorkerUpdateAction::AcquireSource,
                        source_manifest_sha256: None,
                        reason: "Acquire fixture".into()
                    }
                )
                .unwrap()
                .phase,
            WorkerUpdatePhase::SourceHeld
        );
        fs::create_dir_all(runtime.join("update").join(&id)).unwrap();
        stage_archive(
            &zip_path,
            &pin,
            &runtime.join("update").join(&id).join("archive"),
        )
        .unwrap();
        Self {
            config,
            journal,
            owner,
            id,
            control: Control {
                worker,
                events: vec![],
                fault: None,
                source,
                crash: None,
            },
            root: root.into(),
        }
    }
    fn prepare(&mut self) -> SourceTransactionRecord {
        prepare_source(
            &mut self.journal,
            &self.owner,
            &self.config,
            &self.id,
            &mut self.control,
        )
        .unwrap()
    }
    fn apply(&mut self) -> Result<SourceTransactionRecord, String> {
        apply_source(
            &mut self.journal,
            &self.owner,
            &self.config,
            &self.id,
            &mut self.control,
        )
    }
    fn status(&self) -> SourceTransactionStatus {
        UpdateJournal::read_source_status(
            &self.config.runtime_directory.join("update-state"),
            &self.id,
            0,
            100,
        )
        .unwrap()
    }
    fn db(&self) -> Connection {
        Connection::open(
            self.config
                .runtime_directory
                .join("update-state/updates.sqlite"),
        )
        .unwrap()
    }
    fn assert_manual(&mut self) {
        assert_eq!(
            self.journal.get(&self.id).unwrap().phase,
            UpdatePhase::ManualIntervention
        );
        assert_eq!(
            self.status().transaction.phase,
            SourceTransactionPhase::Blocked
        );
        assert!(self.apply().is_err());
        assert!(self
            .config
            .runtime_directory
            .join("state")
            .join(winbridge_worker::UPDATE_STARTUP_GUARD)
            .exists());
    }
}

#[test]
fn source_success_preserves_old_files_and_caches_and_manifest_is_last() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    let r = f.prepare();
    assert_eq!(r.total_files, 4);
    assert_eq!(f.status().applied_files, 0);
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
    let applied = f.apply().unwrap();
    assert_eq!(applied.phase, SourceTransactionPhase::Verified);
    assert_eq!(f.journal.get(&f.id).unwrap().phase, UpdatePhase::Building);
    assert_eq!(f.status().applied_files, 4);
    assert_eq!(
        f.control
            .events
            .iter()
            .rfind(|e| e.checkpoint == SourceCheckpoint::IntentCommitted)
            .unwrap()
            .path
            .as_deref(),
        Some(SOURCE_MANIFEST_NAME)
    );
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"new"
    );
    assert!(!f.config.source_directory.join("removed.rs").exists());
    for p in ["target/cache", ".git/fixture", ".env", "fixture.local.json"] {
        assert_eq!(
            fs::read(f.config.source_directory.join(p)).unwrap(),
            b"preserved fixture"
        );
    }
    let status = f.status();
    for file in &status.files {
        if file.change.previous.is_some() {
            let p = Path::new(&r.transaction_directory);
            assert_eq!(
                fs::read(p.join("held").join(format!("{:05}", file.ordinal))).unwrap(),
                fs::read(p.join("preimages").join(format!("{:05}", file.ordinal))).unwrap()
            );
        }
    }
    let count = f.control.events.len();
    assert_eq!(f.apply().unwrap(), applied);
    assert_eq!(f.prepare(), applied);
    assert_eq!(f.control.events.len(), count);
    let inspected = inspect_source(
        &f.config.runtime_directory.join("update-state"),
        &f.id,
        0,
        2,
    )
    .unwrap();
    assert!(inspected.source_matches_target);
    assert!(!inspected.source_matches_baseline);
    assert!(!inspected.can_replay);
    assert!(inspected.status.has_more);
    for action in ["source-status", "source-inspect"] {
        let out = Command::new(env!("CARGO_BIN_EXE_winbridge-update"))
            .arg("--config")
            .arg(f.root.join("update.local.json"))
            .args(["journal", action, &f.id, "0", "2"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["readOnly"], true);
    }
    let wi = info(&f.control.worker);
    assert_eq!(
        wi.update_coordination.as_ref().unwrap()["lease"]["phase"],
        "source_held"
    );
    if let Ok(path) = std::env::var("WINBRIDGE_SOURCE_TRANSACTION_REPORT") {
        fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"source_transaction_proof","passed":true,"platform":std::env::consts::OS,"build":wi.build,"sourceVerified":true,"runtimeApplied":false,"windowsOpened":false,"workerLeaseRetained":true,"status":status,"inspection":inspected,"progress":f.control.events})).unwrap()).unwrap();
    }
}

#[test]
fn source_transaction_uses_fresh_process_authenticated_worker_ipc() {
    use winbridge_platform::infrastructure;
    use winbridge_update::worker_source::WorkerSourceControl;
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    let io = tokio::runtime::Runtime::new().unwrap();
    let server = io.spawn(winbridge_worker::serve(f.control.worker.clone()));
    let info = info(&f.control.worker);
    let binding = SupervisorWorkerBinding {
        context: info.fenced_requests.unwrap().context,
        generation: info.generation,
        build: info.build.unwrap(),
        process: infrastructure::stamp(std::process::id()).unwrap(),
        config_sha256: sha256(&fs::read(&f.config.worker_config_path).unwrap()),
        endpoint: "unused-source-readonly-adapter".into(),
    };
    let mut control = WorkerSourceControl::new(
        binding.clone(),
        &f.config.worker_config_path,
        io.handle().clone(),
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while control.observe_worker().is_err() {
        assert!(
            Instant::now() < until,
            "Readonly worker IPC did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    prepare_source(&mut f.journal, &f.owner, &f.config, &f.id, &mut control).unwrap();
    let applied = apply_source(&mut f.journal, &f.owner, &f.config, &f.id, &mut control).unwrap();
    assert_eq!(applied.phase, SourceTransactionPhase::Verified);
    assert_eq!(f.journal.get(&f.id).unwrap().phase, UpdatePhase::Building);
    assert!(control.observe_worker().is_ok());

    // Same IPC endpoint/process is insufficient: every reply must match the
    // frozen build, journal incarnation and worker generation as well.
    for field in ["generation", "build", "incarnation", "birth"] {
        let mut wrong = binding.clone();
        match field {
            "generation" => wrong.generation = uuid::Uuid::new_v4().to_string(),
            "build" => wrong.build.build_id = "different-build".into(),
            "incarnation" => wrong.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
            "birth" => wrong.process.birth.push_str("-changed"),
            _ => unreachable!(),
        }
        let mut other =
            WorkerSourceControl::new(wrong, &f.config.worker_config_path, io.handle().clone())
                .unwrap();
        assert!(other.observe_worker().is_err(), "{field}");
    }
    let bytes = fs::read(&f.config.worker_config_path).unwrap();
    let mut changed = bytes.clone();
    changed.push(b'\n');
    fs::write(&f.config.worker_config_path, changed).unwrap();
    assert!(control.observe_worker().is_err());
    assert!(
        WorkerSourceControl::new(binding, &f.config.worker_config_path, io.handle().clone())
            .is_err()
    );
    fs::write(&f.config.worker_config_path, bytes).unwrap();
    assert!(control.observe_worker().is_ok());
    f.control.worker.stop();
    io.block_on(server).unwrap().unwrap();
}
#[test]
fn local_edit_and_staged_tampering_stop_before_source_effects() {
    for tamper in ["source", "staged", "config"] {
        let tmp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(tmp.path());
        match tamper {
            "source" => {
                fs::write(f.config.source_directory.join("Cargo.toml"), b"user edit").unwrap()
            }
            "staged" => fs::write(
                f.config
                    .runtime_directory
                    .join("update")
                    .join(&f.id)
                    .join("archive/source/src/new.rs"),
                b"tampered",
            )
            .unwrap(),
            _ => fs::write(&f.config.worker_config_path, b"changed fixture").unwrap(),
        };
        assert!(
            prepare_source(&mut f.journal, &f.owner, &f.config, &f.id, &mut f.control).is_err()
        );
        assert!(!f
            .config
            .runtime_directory
            .join("update")
            .join(&f.id)
            .join("source-transaction")
            .exists());
        assert_eq!(
            fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
            if tamper == "source" {
                b"user edit".as_slice()
            } else {
                b"old".as_slice()
            }
        );
    }
}
#[test]
fn racing_edit_is_preserved_in_held_and_never_silently_overwritten() {
    for (checkpoint, kind) in [
        (SourceCheckpoint::IntentCommitted, "edit"),
        (SourceCheckpoint::OldPreserved, "reappear"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(tmp.path());
        let r = f.prepare();
        f.control.fault = Some((checkpoint, 0, kind));
        assert!(f.apply().is_err());
        f.assert_manual();
        let p = Path::new(&r.transaction_directory);
        assert_eq!(fs::read(p.join("preimages/00000")).unwrap(), b"old");
        if kind == "edit" {
            assert_eq!(fs::read(p.join("held/00000")).unwrap(), b"user edit");
            assert!(!f.config.source_directory.join("Cargo.toml").exists());
        } else {
            assert_eq!(fs::read(p.join("held/00000")).unwrap(), b"old");
            assert_eq!(
                fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
                b"user edit"
            );
        }
        assert_eq!(f.status().files[0].state, SourceFileState::Intent);
    }
}
#[test]
fn ignored_writes_never_ack_intent_receipt_or_final_verification() {
    for point in ["intent", "receipt", "verified"] {
        let tmp = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(tmp.path());
        f.prepare();
        let sql=match point{"intent"=>"CREATE TRIGGER fault BEFORE INSERT ON source_file_intents BEGIN SELECT RAISE(IGNORE); END;","receipt"=>"CREATE TRIGGER fault BEFORE INSERT ON source_file_receipts BEGIN SELECT RAISE(IGNORE); END;",_=>"CREATE TRIGGER fault BEFORE UPDATE ON source_transactions WHEN NEW.phase='\"verified\"' BEGIN SELECT RAISE(IGNORE); END;"};
        f.db().execute_batch(sql).unwrap();
        assert!(f.apply().is_err());
        f.assert_manual();
        let s = f.status();
        match point {
            "intent" => {
                assert_eq!(s.files[0].state, SourceFileState::Planned);
                assert_eq!(
                    fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
                    b"old"
                );
            }
            "receipt" => {
                assert_eq!(s.files[0].state, SourceFileState::Intent);
                assert_eq!(
                    fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
                    b"new"
                );
            }
            _ => {
                assert_eq!(s.applied_files, 4);
                assert!(
                    inspect_source(
                        &f.config.runtime_directory.join("update-state"),
                        &f.id,
                        0,
                        100
                    )
                    .unwrap()
                    .source_matches_target
                );
            }
        }
    }
}
#[test]
fn changed_root_and_stale_owner_cannot_apply_a_prepared_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    f.prepare();
    let old = f.config.source_directory.with_file_name("saved-source");
    fs::rename(&f.config.source_directory, &old).unwrap();
    fs::create_dir(&f.config.source_directory).unwrap();
    assert!(f.apply().is_err());
    assert_eq!(fs::read(old.join("Cargo.toml")).unwrap(), b"old");
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    f.prepare();
    let epoch = f.owner.epoch();
    drop(f.owner);
    f.owner = f.journal.acquire_owner().unwrap();
    assert!(f.owner.epoch() > epoch);
    assert!(f.apply().is_err());
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
}
#[test]
fn raw_lifecycle_cannot_bypass_a_source_transaction() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    f.prepare();
    for event in [
        UpdateEvent::SourceVerified,
        UpdateEvent::BeginSourceApply,
        UpdateEvent::Fail,
    ] {
        assert!(f
            .journal
            .advance(
                &f.owner,
                &f.id,
                &UpdateCommand {
                    idempotency_key: format!("bypass-{event:?}"),
                    expected_revision: 1,
                    event,
                    reason: "Bypass rejected".into()
                }
            )
            .is_err());
    }
    assert_eq!(f.journal.get(&f.id).unwrap().phase, UpdatePhase::Preparing);
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn crash_point(i: usize) -> (SourceCheckpoint, Option<u32>) {
    match i {
        0 => (SourceCheckpoint::Applying, None),
        1 => (SourceCheckpoint::IntentCommitted, Some(0)),
        2 => (SourceCheckpoint::OldPreserved, Some(0)),
        3 => (SourceCheckpoint::NewPlaced, Some(0)),
        4 => (SourceCheckpoint::ReceiptCommitted, Some(0)),
        _ => (SourceCheckpoint::NewPlaced, Some(3)),
    }
}
#[test]
#[ignore = "Owned subprocess crash helper"]
fn source_crash_helper() {
    let root = PathBuf::from(std::env::var_os("WINBRIDGE_SOURCE_CRASH_ROOT").unwrap());
    let point = std::env::var("WINBRIDGE_SOURCE_CRASH_POINT")
        .unwrap()
        .parse()
        .unwrap();
    let mut f = Fixture::new(&root);
    fs::write(
        root.join("crash-scope"),
        installation_scope(&f.config).unwrap(),
    )
    .unwrap();
    fs::write(root.join("crash-id"), &f.id).unwrap();
    f.prepare();
    f.control.crash = Some(crash_point(point));
    let _ = f.apply();
    panic!("Crash point not reached");
}
#[test]
fn process_crash_never_replays_effects_or_releases_worker_guard() {
    for point in 0..6 {
        let tmp = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "source_crash_helper", "--ignored", "--nocapture"])
            .env("WINBRIDGE_SOURCE_CRASH_ROOT", tmp.path())
            .env("WINBRIDGE_SOURCE_CRASH_POINT", point.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = Child(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(exit) = child.0.try_wait().unwrap() {
                assert_eq!(exit.code(), Some(79), "crash point {point}");
                break;
            }
            assert!(Instant::now() < deadline, "child deadline");
            std::thread::sleep(Duration::from_millis(20));
        }
        let root = tmp.path();
        let id = fs::read_to_string(root.join("crash-id")).unwrap();
        let scope = fs::read_to_string(root.join("crash-scope")).unwrap();
        let config = winbridge_update::load_config(&root.join("update.local.json")).unwrap();
        let dir = config.runtime_directory.join("update-state");
        let mut j = UpdateJournal::open(&dir, &scope).unwrap();
        let owner = j.acquire_owner().unwrap();
        let before = UpdateJournal::read_source_status(&dir, &id, 0, 100).unwrap();
        assert_eq!(before.transaction.phase, SourceTransactionPhase::Applying);
        assert_eq!(j.get(&id).unwrap().phase, UpdatePhase::SourceApplying);
        assert!(config
            .runtime_directory
            .join("state")
            .join(winbridge_worker::UPDATE_STARTUP_GUARD)
            .exists());
        assert!(Worker::open_file(&config.worker_config_path).is_err());
        struct NoControl;
        impl SourceControl for NoControl {
            fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
                panic!("Must refuse before observing worker");
            }
        }
        assert!(apply_source(&mut j, &owner, &config, &id, &mut NoControl).is_err());
        let inspected = inspect_source(&dir, &id, 0, 100).unwrap();
        assert!(!inspected.can_replay);
        assert_eq!(inspected.source_matches_target, point == 5);
        assert_eq!(
            UpdateJournal::read_source_status(&dir, &id, 0, 100)
                .unwrap()
                .files,
            before.files
        );
    }
}

#[test]
fn progress_failure_blocks_before_source_changes_and_lease_change_stops_apply() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    f.prepare();
    f.control.fault = Some((SourceCheckpoint::IntentCommitted, 0, "fail"));
    assert!(f.apply().is_err());
    f.assert_manual();
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    let r = f.prepare();
    f.control
        .worker
        .advance_source_update(
            &r.worker_source_spec,
            &WorkerUpdateCommand {
                command_id: "drain-before-source".into(),
                expected_revision: r.worker_lease_revision,
                action: WorkerUpdateAction::BeginDrain,
                source_manifest_sha256: None,
                reason: "Changed lease phase".into(),
            },
        )
        .unwrap();
    assert!(f.apply().is_err());
    assert_eq!(
        f.journal.get(&f.id).unwrap().phase,
        UpdatePhase::FailedBeforeCutover
    );
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
}

// Build a real v1 fixture by removing only the v2 additions from our owned DB.
fn downgrade_fixture(dir: &Path) {
    let db = Connection::open(dir.join("updates.sqlite")).unwrap();
    db.execute_batch("DROP TABLE update_build_receipts; DROP TABLE update_builds; DROP TABLE supervisor_receipts; DROP TABLE supervisor_launches; DROP TABLE source_file_receipts; DROP TABLE source_file_intents; DROP TABLE source_plans; DROP TABLE source_transactions; PRAGMA user_version=1; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
}
#[test]
fn schema_one_migration_requires_owner_lock_and_retains_consistent_history_backup() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    let dir = f.config.runtime_directory.join("update-state");
    let before = f.journal.get(&f.id).unwrap();
    let old_info = f.journal.info().unwrap();
    downgrade_fixture(&dir);
    assert_eq!(UpdateJournal::inspect(&dir).unwrap().schema_version, 1);
    assert!(UpdateJournal::read_source_status(&dir, &f.id, 0, 100).is_err());
    assert!(
        matches!(UpdateJournal::open(&dir,&old_info.scope_sha256),Err(e) if e.code==winbridge_update::journal::JournalErrorCode::OwnerBusy)
    );
    assert!(!fs::read_dir(&dir).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("updates-v1-")));
    drop(f.owner);
    let mut migrated = UpdateJournal::open(&dir, &old_info.scope_sha256).unwrap();
    assert_eq!(
        migrated.info().unwrap().schema_version,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    );
    assert_eq!(
        migrated.info().unwrap().owner_epoch,
        old_info.owner_epoch + 1
    );
    assert_eq!(migrated.get(&f.id).unwrap(), before);
    let backup = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v1-")
        })
        .unwrap();
    let db =
        Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT record FROM updates WHERE id=?1", [&f.id], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        serde_json::to_string(&before).unwrap()
    );
    assert_eq!(
        db.query_row("SELECT epoch FROM installation", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        old_info.owner_epoch as i64
    );
    let owner = migrated.acquire_owner().unwrap();
    assert_eq!(owner.epoch(), old_info.owner_epoch + 2);
}
#[test]
fn failed_schema_migration_rolls_back_epoch_and_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    let dir = f.config.runtime_directory.join("update-state");
    let before = f.journal.info().unwrap();
    downgrade_fixture(&dir);
    drop(f.owner);
    let db = Connection::open(dir.join("updates.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE source_plans(block_migration TEXT);")
        .unwrap();
    drop(db);
    assert!(UpdateJournal::open(&dir, &before.scope_sha256).is_err());
    let after = UpdateJournal::inspect(&dir).unwrap();
    assert_eq!(after.schema_version, 1);
    assert_eq!(after.owner_epoch, before.owner_epoch);
    assert_eq!(after.active_update_id, before.active_update_id);
}
#[test]
#[ignore = "Explicit retained previous updater required; never touches installed runtime"]
fn previous_updater_refuses_current_schema_without_writes() {
    let reference = PathBuf::from(
        std::env::var_os("WINBRIDGE_UPDATE_REFERENCE_UPDATER")
            .expect("Pinned isolated reference updater"),
    );
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::new(tmp.path());
    let dir = f.config.runtime_directory.join("update-state");
    let before = UpdateJournal::read_snapshot(&dir, Some(&f.id)).unwrap();
    let db = dir.join("updates.sqlite");
    f.db()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    let bytes = fs::read(&db).unwrap();
    let out = Command::new(reference)
        .arg("--config")
        .arg(tmp.path().join("update.local.json"))
        .args(["journal", "init"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("UnsupportedSchema"));
    assert_eq!(fs::read(db).unwrap(), bytes);
    assert_eq!(
        UpdateJournal::read_snapshot(&dir, Some(&f.id)).unwrap(),
        before
    );
}

#[test]
fn failed_prepared_notification_keeps_source_unchanged_and_blocks_replay() {
    struct FailPrepared<'a>(&'a mut Control);
    impl SourceControl for FailPrepared<'_> {
        fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
            self.0.observe_worker()
        }
        fn progress(&mut self, e: &SourceProgress) -> Result<(), String> {
            if e.checkpoint == SourceCheckpoint::Prepared {
                Err("notification unavailable".into())
            } else {
                self.0.progress(e)
            }
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    assert!(prepare_source(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        &mut FailPrepared(&mut f.control)
    )
    .is_err());
    assert_eq!(
        f.status().transaction.phase,
        SourceTransactionPhase::Blocked
    );
    assert_eq!(
        f.journal.get(&f.id).unwrap().phase,
        UpdatePhase::FailedBeforeCutover
    );
    assert!(f.apply().is_err());
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
}

#[test]
fn schema_two_source_inspection_stays_readonly_and_migration_fences_old_grant() {
    let tmp = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(tmp.path());
    f.prepare();
    let dir = f.config.runtime_directory.join("update-state");
    let before = f.status();
    let info = f.journal.info().unwrap();
    let db = f.db();
    db.execute_batch(
        "DROP TABLE update_build_receipts; DROP TABLE update_builds; DROP TABLE supervisor_receipts; DROP TABLE supervisor_launches; PRAGMA user_version=2;",
    )
    .unwrap();
    drop(db);
    assert_eq!(
        UpdateJournal::read_source_status(&dir, &f.id, 0, 100)
            .unwrap()
            .files,
        before.files
    );
    assert_eq!(UpdateJournal::inspect(&dir).unwrap().schema_version, 2);
    assert!(
        matches!(UpdateJournal::open(&dir,&info.scope_sha256),Err(e) if e.code==winbridge_update::journal::JournalErrorCode::OwnerBusy)
    );
    drop(f.owner);
    let mut migrated = UpdateJournal::open(&dir, &info.scope_sha256).unwrap();
    assert_eq!(
        migrated.info().unwrap().schema_version,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    );
    assert_eq!(migrated.info().unwrap().owner_epoch, info.owner_epoch + 1);
    let owner = migrated.acquire_owner().unwrap();
    assert!(apply_source(&mut migrated, &owner, &f.config, &f.id, &mut f.control).is_err());
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"old"
    );
    assert!(fs::read_dir(&dir).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("updates-v2-")));
}
