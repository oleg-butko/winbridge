//! Actual readonly CLI with synthetic intent metadata. This does not prove
//! shutdown of worker Main or grant runtime replacement.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{artifact_descriptor, update::*, ArtifactRole, LoadedWorkerIdentity};
use winbridge_update::{
    artifact::inspect_image,
    shutdown::{ShutdownBinding, ShutdownInspectionRequest, ShutdownIntent, ShutdownStatePins},
};
use winbridge_worker::{store::Store, Worker};

fn call(image: &Path, input: &Path, sha: &str) -> std::process::Output {
    let mut command = Command::new(image);
    command
        .args(["--inspect-update-shutdown", input.to_str().unwrap(), sha])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Owned readonly helper exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
fn write_input(path: &Path, request: &ShutdownInspectionRequest) -> String {
    let raw = serde_json::to_vec(request).unwrap();
    fs::write(path, &raw).unwrap();
    winbridge_update::sha256(&raw)
}
fn report(output: std::process::Output) -> WorkerShutdownInspectionReport {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fixture(root: &Path) -> (std::sync::Arc<Worker>, PathBuf, ShutdownInspectionRequest) {
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    let cargo = b"[package]\nname='readonly-fixture'\nversion='0.1.0'\n";
    fs::write(source.join("Cargo.toml"), cargo).unwrap();
    let manifest = SourceManifest {
        schema_version: 1,
        source_commit: "a".repeat(40),
        dirty: false,
        created_at: "2026-10-21T00:00:00Z".into(),
        includes_built_ui: false,
        files: vec![SourceFile {
            path: "Cargo.toml".into(),
            bytes: cargo.len() as u64,
            sha256: winbridge_update::sha256(cargo),
        }],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let hash = winbridge_update::sha256(&manifest_bytes);
    fs::write(
        source.join("winbridge-source-manifest.json"),
        manifest_bytes,
    )
    .unwrap();
    let config:WorkerConfig=serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"readonly-shutdown","nodeId":"fixture","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"source","workspaceRoot":source,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap();
    let config_path = root.join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let worker = Worker::open_file(&config_path).unwrap();
    let spec = worker
        .update_source_spec(
            &uuid::Uuid::new_v4().to_string(),
            &uuid::Uuid::new_v4().to_string(),
            2,
            &source,
            (&hash, &hash),
        )
        .unwrap();
    worker.begin_source_update(spec.clone()).unwrap();
    let mut lease = None;
    for (n, action) in [
        WorkerUpdateAction::AcquireSource,
        WorkerUpdateAction::BeginDrain,
        WorkerUpdateAction::CloseDispatch,
        WorkerUpdateAction::ConfirmQuiesced,
    ]
    .into_iter()
    .enumerate()
    {
        lease = Some(
            worker
                .advance_source_update(
                    &spec,
                    &WorkerUpdateCommand {
                        command_id: format!("fixture-{n}"),
                        expected_revision: n as u64,
                        action,
                        source_manifest_sha256: None,
                        reason: format!("Fixture {n}"),
                    },
                )
                .unwrap(),
        );
    }
    let image = root.join("reader-worker.exe");
    fs::copy(env!("CARGO_BIN_EXE_winbridge-worker"), &image).unwrap();
    let helper = inspect_image(&image).unwrap();
    let descriptor = artifact_descriptor(ArtifactRole::Worker);
    let process = winbridge_platform::infrastructure::stamp(std::process::id()).unwrap();
    let binding = SupervisorWorkerBinding {
        context: worker.request_context().unwrap(),
        generation: spec.worker_generation.clone(),
        build: descriptor.build.clone(),
        process: process.clone(),
        config_sha256: inspect_image(&config_path).unwrap().sha256,
        endpoint: "metadata-fixture".into(),
    };
    let loaded_worker = LoadedWorkerIdentity {
        schema_version: 1,
        kind: "loaded_worker_identity".into(),
        descriptor,
        process,
        set_id: uuid::Uuid::new_v4().to_string(),
        manifest_sha256: "a".repeat(64),
        runtime_identity: winbridge_platform::source_files::directory_identity(root).unwrap(),
        image: helper.clone(),
    };
    let intent = ShutdownIntent::from_quiesced(
        ShutdownBinding {
            update_id: spec.update_id,
            launch_id: uuid::Uuid::new_v4().to_string(),
            owner_id: spec.updater_owner_id,
            owner_epoch: 2,
            parent_revision: 10,
            admitted_at_ms: 1,
            drain_sha256: "b".repeat(64),
            worker: binding,
            lease: lease.unwrap(),
            loaded_worker,
            worker_state: ShutdownStatePins::capture(&config.state_directory)
                .unwrap()
                .observation()
                .clone(),
        },
        uuid::Uuid::new_v4().to_string(),
    )
    .unwrap();
    (
        worker,
        config_path.clone(),
        ShutdownInspectionRequest {
            schema_version: 1,
            intent,
            worker_config: inspect_image(&config_path).unwrap(),
            helper_image: helper,
            worker_journal_schema: winbridge_worker::store::JOURNAL_SCHEMA_VERSION,
        },
    )
}
#[test]
fn exact_readonly_cli_preserves_pending_guard_authority_and_receipt_before_and_after_owner_drop() {
    let root = tempfile::tempdir().unwrap();
    let (worker, _, request) = fixture(root.path());
    let input = root.path().join("input.json");
    let sha = write_input(&input, &request);
    let image = Path::new(&request.helper_image.path);
    let state = Path::new(&request.intent.binding.worker_state.state_directory);
    let journal = state.join("journal.sqlite");
    let marker = state.join(winbridge_worker::UPDATE_STARTUP_GUARD);
    let marker_bytes = fs::read(&marker).unwrap();
    let authority = Store::read_admission_header(&journal).unwrap();
    let empty = report(call(image, &input, &sha));
    assert!(empty.receipt.is_none());
    // With no receipt, a foreign state binding must be an error rather than a
    // plausible "not found" observation from a different file generation.
    let mut foreign_binding = request.intent.binding.clone();
    foreign_binding.worker_state.journal_identity = "different-journal-file".into();
    let foreign = ShutdownInspectionRequest {
        intent: ShutdownIntent::from_quiesced(foreign_binding, uuid::Uuid::new_v4().to_string())
            .unwrap(),
        ..request.clone()
    };
    let foreign_sha = write_input(&input, &foreign);
    assert!(!call(image, &input, &foreign_sha).status.success());
    assert_eq!(write_input(&input, &request), sha);
    assert!(ShutdownStatePins::capture(state)
        .unwrap()
        .try_lock()
        .is_err());
    let b = &request.intent.binding;
    let mut store = Store::open(&journal).unwrap();
    let accepted = store
        .accept_worker_shutdown(WorkerShutdownReceipt {
            schema_version: 1,
            update_id: b.update_id.clone(),
            launch_id: b.launch_id.clone(),
            updater_owner_id: b.owner_id.clone(),
            updater_epoch: b.owner_epoch,
            command: request.intent.command.clone(),
            lease: b.lease.clone(),
            process: b.worker.process.clone(),
            state: b.worker_state.clone(),
            accepted_at_ms: 1,
        })
        .unwrap();
    let winbridge_worker::store::ShutdownAcceptance::Fresh(permit) = accepted else {
        panic!("Fresh fixture receipt")
    };
    let receipt = permit.receipt().clone();
    drop(permit);
    drop(store);
    let actual = report(call(image, &input, &sha));
    assert_eq!(actual.receipt.as_deref(), Some(&receipt));
    assert!(
        actual.journal_opened_read_only
            && !actual.worker_constructed
            && !actual.scheduler_started
            && !actual.recovery_marker_changed
            && !actual.runtime_applied
    );
    assert_eq!(actual.request_sha256, sha);
    assert_eq!(actual.helper_image, request.helper_image);
    drop(worker);
    let c = rusqlite::Connection::open(&journal).unwrap();
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(c);
    let before = fs::read(&journal).unwrap();
    assert_eq!(
        report(call(image, &input, &sha)).receipt.as_deref(),
        Some(&receipt)
    );
    let queried =
        winbridge_update::shutdown::inspect_saved_receipt(&input, &sha, &request).unwrap();
    assert_eq!(queried.receipt.as_deref(), Some(&receipt));
    assert_eq!(fs::read(&journal).unwrap(), before);
    assert_eq!(fs::read(&marker).unwrap(), marker_bytes);
    assert_eq!(Store::read_admission_header(&journal).unwrap(), authority);
    assert!(!state.join(winbridge_worker::RECOVERY_MARKER).exists());
    let bad = call(image, &input, &"0".repeat(64));
    assert!(!bad.status.success());
    let changed = ShutdownInspectionRequest {
        intent: ShutdownIntent::from_quiesced(
            request.intent.binding.clone(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap(),
        ..request.clone()
    };
    let changed_sha = write_input(&input, &changed);
    assert!(!call(image, &input, &changed_sha).status.success());
    assert_eq!(fs::read(journal).unwrap(), before);
    assert_eq!(fs::read(marker).unwrap(), marker_bytes);
}
