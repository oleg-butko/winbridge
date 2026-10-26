use std::{
    fs,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::{
    activate_recovery, prepare_recovery, store::Store, PreparationStage, Worker,
};

// Independent in-process fixtures are not independent OS processes: a Unix
// fork in the real-history case can temporarily inherit another case's flock
// until exec. Do not overlap those fixture lifetimes. The production recovery
// lock stays nonblocking and tests of live-owner/concurrent activation stay intact.
fn recovery_case_scope() -> std::sync::MutexGuard<'static, ()> {
    static CASES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CASES.lock().unwrap_or_else(|e| e.into_inner())
}

fn config(root: &Path) -> WorkerConfig {
    fs::create_dir_all(root.join("project")).unwrap();
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("source-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("source"),"maxOutputBytesPerTask":10000,"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root.join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}

#[test]
fn recovery_removes_all_controller_endpoints_until_explicit_repair() {
    let _scope = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let mut cfg = config(temp.path());
    cfg.worker.controller_topology = Some(
        serde_json::from_value(
            serde_json::json!({"schemaVersion":1,"primary":"a","controllers":[
        {"id":"a","url":"ws://192.168.44.203:1143"},{"id":"b","url":"ws://192.168.44.204:1143"}]}),
        )
        .unwrap(),
    );
    drop(Worker::open(cfg.clone()).unwrap());
    retire(&cfg);
    let prepared = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    let target = WorkerConfig::load(&prepared.worker_config_path).unwrap();
    assert!(target.worker.controller_url.is_none());
    assert!(target.worker.controller_topology.is_none());
    assert!(target.worker.topology().unwrap().is_none());
}
fn submission(key: &str) -> LocalRequest {
    LocalRequest::Submit {
        request: TaskSubmission {
            protocol_version: 1,
            request_id: "test".into(),
            project_id: "fixture".into(),
            idempotency_key: key.into(),
            timeout_ms: 5000,
            snapshot_id: None,
            command: CommandSource::Direct {
                executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                arguments: vec!["success".into()],
                working_directory: ".".into(),
            },
        },
    }
}
fn task(response: LocalResponse) -> TaskRecord {
    if let LocalResponse::Task { task } = response {
        *task
    } else {
        panic!("{response:?}")
    }
}
fn finish(worker: &Arc<Worker>, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.schedule().unwrap();
        let value = task(worker.handle(LocalRequest::GetTask { task_id: id.into() }));
        if value.status.is_terminal() && worker.is_idle() {
            return value;
        }
        assert!(Instant::now() < deadline, "{value:?}");
        thread::sleep(Duration::from_millis(10));
    }
}
fn retire(config: &WorkerConfig) {
    drop(Worker::open_recovery(config.clone()).unwrap());
}
fn rows(path: &Path) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT * FROM \"{}\" ORDER BY rowid",
                    name.replace('"', "\"\"")
                ))
                .unwrap();
            let columns = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get(index))
                        .collect::<Result<Vec<rusqlite::types::Value>, _>>()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        })
        .collect()
}

#[test]
fn archive_keeps_real_history_and_prepared_pair_requires_fenced_activation() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let old_scope = worker.request_context().unwrap();
    let accepted = task(worker.handle(submission("logical-operation")));
    assert_eq!(
        finish(&worker, &accepted.task_id).status,
        TaskStatus::Succeeded
    );
    drop(worker);
    retire(&cfg);
    let before = fs::read(cfg.state_directory.join("journal.sqlite")).unwrap();
    let preparation = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    assert_eq!(preparation.source_context, old_scope);
    assert_ne!(preparation.target_context, old_scope);
    assert_eq!(
        rows(&cfg.state_directory.join("journal.sqlite")),
        rows(&preparation.archive_path)
    );
    assert_eq!(
        before,
        fs::read(cfg.state_directory.join("journal.sqlite")).unwrap()
    );
    let target = WorkerConfig::load(&preparation.worker_config_path).unwrap();
    assert!(target.worker.controller_url.is_none());
    let pending = Store::open_read_only(&target.state_directory.join("journal.sqlite")).unwrap();
    assert_eq!(pending.journal_authority().epoch, 0);
    assert!(pending.requires_fenced_requests().unwrap());
    assert!(pending.all().unwrap().is_empty());
    drop(pending);
    assert!(Worker::open(target.clone()).is_err());
    assert!(Worker::open(cfg.clone()).is_err());
    assert!(prepare_recovery(&cfg, &temp.path().join("replacement")).is_err());
    let archived = Store::open_read_only(&preparation.archive_path).unwrap();
    assert_eq!(
        archived.get(&accepted.task_id).unwrap().status,
        TaskStatus::Succeeded
    );
    let output = archived.output(&accepted.task_id, 0).unwrap();
    assert!(output
        .iter()
        .any(|chunk| String::from_utf8_lossy(&chunk.bytes).contains("Привет ✓")));
    drop(archived);
    #[cfg(unix)]
    {
        assert!(matches!(preparation.stage, PreparationStage::Blocked));
        assert!(activate_recovery(&target).is_err());
        assert!(!target
            .state_directory
            .join("recovery-activated.json")
            .exists());
    }
    #[cfg(windows)]
    {
        assert!(matches!(preparation.stage, PreparationStage::Prepared));
        let receipt = activate_recovery(&target).unwrap();
        assert_eq!(
            activate_recovery(&target).unwrap().checked_at_ms,
            receipt.checked_at_ms
        );
        let replacement = Worker::open(target.clone()).unwrap();
        let rejected = replacement.handle_fenced(FencedRequest {
            request_contract_version: 2,
            context: old_scope,
            request: submission("logical-operation"),
        });
        assert!(matches!(rejected.response, LocalResponse::Error { .. }));
        assert_eq!(rejected.outcome, FencedReplyOutcome::RejectedBeforeDispatch);
        assert!(matches!(
            replacement.handle(submission("logical-operation")),
            LocalResponse::Error { .. }
        ));
        let scope = replacement.request_context().unwrap();
        let request = FencedRequest {
            request_contract_version: 2,
            context: scope,
            request: submission("explicit-new-operation"),
        };
        let new_task = task(replacement.handle_fenced(request.clone()).response);
        assert_eq!(
            finish(&replacement, &new_task.task_id).status,
            TaskStatus::Succeeded
        );
        drop(replacement);
        let replacement = Worker::open(target).unwrap();
        assert_eq!(
            task(replacement.handle_fenced(request).response).task_id,
            new_task.task_id
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("project/runs.log"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        assert_eq!(
            rows(&cfg.state_directory.join("journal.sqlite")),
            rows(&preparation.archive_path)
        );
    }
}

#[test]
fn live_worker_or_inspector_cannot_be_archived_or_replaced() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    assert!(prepare_recovery(&cfg, &temp.path().join("replacement")).is_err());
    assert!(!temp.path().join("replacement").exists());
    drop(worker);
    let inspection = Worker::open_recovery(cfg.clone()).unwrap();
    assert!(prepare_recovery(&cfg, &temp.path().join("replacement")).is_err());
    assert!(!temp.path().join("replacement").exists());
    drop(inspection);
}

#[test]
fn partial_preparation_and_partial_receipt_never_activate_a_journal() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    fs::create_dir(&cfg.state_directory).unwrap();
    #[cfg(unix)]
    fs::set_permissions(
        &cfg.state_directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    fs::write(
        cfg.state_directory.join(winbridge_worker::RECOVERY_MARKER),
        b"partial preparation",
    )
    .unwrap();
    assert!(Worker::open(cfg.clone()).is_err());
    assert!(!cfg.state_directory.join("journal.sqlite").exists());
    fs::write(cfg.state_directory.join("recovery-activated.json"), b"{").unwrap();
    assert!(Worker::open(cfg.clone()).is_err());
    assert!(!cfg.state_directory.join("journal.sqlite").exists());
}

#[test]
fn missing_witness_in_empty_restored_journal_does_not_invent_physical_proof() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    fs::remove_file(cfg.state_directory.join("execution-domain.json")).unwrap();
    retire(&cfg);
    let preparation = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    assert!(matches!(preparation.stage, PreparationStage::Blocked));
    assert!(preparation.blocked_reason.is_some());
    assert!(!cfg.state_directory.join("execution-domain.json").exists());
    let target = WorkerConfig::load(&preparation.worker_config_path).unwrap();
    assert!(activate_recovery(&target).is_err());
    assert!(Worker::open(target).is_err());
}

#[test]
fn foreign_witness_is_refused_before_new_owner_epoch_and_archive_stays_readable() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    let path = cfg.state_directory.join("execution-domain.json");
    let mut witness: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    witness["context"]["journalIncarnation"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    fs::write(&path, serde_json::to_vec(&witness).unwrap()).unwrap();
    let epoch = Store::open_read_only(&cfg.state_directory.join("journal.sqlite"))
        .unwrap()
        .journal_authority()
        .epoch;
    assert!(Worker::open(cfg.clone()).is_err());
    assert_eq!(
        Store::open_read_only(&cfg.state_directory.join("journal.sqlite"))
            .unwrap()
            .journal_authority()
            .epoch,
        epoch
    );
    retire(&cfg);
    let preparation = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    assert!(matches!(preparation.stage, PreparationStage::Blocked));
    assert!(Store::open_read_only(&preparation.archive_path).is_ok());
}

#[test]
fn archive_corruption_or_lost_preparation_gate_cannot_activate() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    retire(&cfg);
    let preparation = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    let target = WorkerConfig::load(&preparation.worker_config_path).unwrap();
    let original = fs::read(&preparation.archive_path).unwrap();
    fs::write(&preparation.archive_path, b"corrupt archive").unwrap();
    assert!(activate_recovery(&target).unwrap_err().contains("checksum"));
    fs::write(&preparation.archive_path, original).unwrap();
    fs::remove_file(
        target
            .state_directory
            .join(winbridge_worker::RECOVERY_MARKER),
    )
    .unwrap();
    assert!(Worker::open(target).is_err());
}

#[test]
fn partial_execution_domain_member_refuses_dispatch_before_epoch_changes() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    let before = Store::open_read_only(&cfg.state_directory.join("journal.sqlite"))
        .unwrap()
        .journal_authority()
        .epoch;
    let registry = cfg.state_directory.join("execution-domains");
    fs::create_dir(&registry).unwrap();
    fs::write(registry.join("session-55.json"), b"{").unwrap();
    assert!(Worker::open(cfg.clone()).is_err());
    assert_eq!(
        Store::open_read_only(&cfg.state_directory.join("journal.sqlite"))
            .unwrap()
            .journal_authority()
            .epoch,
        before
    );
    assert!(!temp.path().join("project/runs.log").exists());
}

#[test]
fn old_activation_receipt_cannot_remove_a_retired_source_latch() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    retire(&cfg);
    fs::write(cfg.state_directory.join("recovery-activated.json"), b"{}").unwrap();
    assert!(activate_recovery(&cfg).unwrap_err().contains("retires"));
    assert!(Worker::open(cfg).is_err());
}

#[cfg(unix)]
#[test]
fn linked_recovery_destination_is_refused_without_writing_outside() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    retire(&cfg);
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(prepare_recovery(&cfg, &link.join("replacement")).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn canonical_drive_path_keeps_the_journal_and_network_prefixes_are_refused() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let mut cfg = config(temp.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let context = worker.request_context().unwrap();
    drop(worker);
    cfg.state_directory = fs::canonicalize(&cfg.state_directory).unwrap();
    assert!(matches!(
        cfg.state_directory.components().next(),
        Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::VerbatimDisk(_))
    ));
    let worker = Worker::open(cfg).unwrap();
    assert_eq!(worker.request_context().unwrap(), context);
    for path in [
        r"\\server\share\state",
        r"\\?\UNC\server\share\state",
        r"\\.\pipe\state",
    ] {
        // Pure syntax check: never probe a remote server or named device.
        assert!(winbridge_platform::require_local_path_prefix(Path::new(path)).is_err());
    }
}

#[cfg(windows)]
#[test]
fn an_unlisted_live_process_blocks_cutover_even_with_empty_history() {
    let _case = recovery_case_scope();
    use winbridge_platform::{
        execution_domain::{observe_domain, DomainIdentity, ExecutionDomain},
        process_identity::ProcessObservation,
        ManagedChild,
    };
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    let worker = Worker::open(cfg.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(
        &fs::read(cfg.state_directory.join("execution-domain.json")).unwrap(),
    )
    .unwrap();
    let identity: DomainIdentity = serde_json::from_value(json["identity"].clone()).unwrap();
    let domain = ExecutionDomain::open(&identity).unwrap();
    let command = ResolvedCommand {
        executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
        arguments: vec!["long".into()],
        working_directory: temp.path().join("project").to_string_lossy().into(),
        timeout_ms: 60000,
    };
    let name = format!("WinBridge-Unlisted-{}", uuid::Uuid::new_v4());
    let (mut child, stdout, stderr) =
        ManagedChild::prepare_in_domain(&command, Some(&name), Some(&domain)).unwrap();
    child.resume().unwrap();
    assert!(matches!(
        observe_domain(&identity),
        ProcessObservation::Occupied(_)
    ));
    // The domain still owns this real process while its journal has no task row.
    drop(worker);
    retire(&cfg);
    assert!(
        Store::open_read_only(&cfg.state_directory.join("journal.sqlite"))
            .unwrap()
            .all()
            .unwrap()
            .is_empty()
    );
    let preparation = prepare_recovery(&cfg, &temp.path().join("replacement")).unwrap();
    assert!(matches!(preparation.stage, PreparationStage::Blocked));
    let target = WorkerConfig::load(&preparation.worker_config_path).unwrap();
    assert!(activate_recovery(&target).is_err());
    assert!(!target
        .state_directory
        .join("recovery-activated.json")
        .exists());
    child.terminate_tree().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() || !child.tree_is_empty().unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    drop((child, stdout, stderr, domain));
    activate_recovery(&target).unwrap();
    drop(Worker::open(target).unwrap());
}

#[cfg(windows)]
#[test]
fn a_second_recovery_retires_the_old_activation_receipt() {
    let _case = recovery_case_scope();
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    drop(Worker::open(cfg.clone()).unwrap());
    retire(&cfg);
    let first = prepare_recovery(&cfg, &temp.path().join("first")).unwrap();
    let first_config = WorkerConfig::load(&first.worker_config_path).unwrap();
    activate_recovery(&first_config).unwrap();
    drop(Worker::open(first_config.clone()).unwrap());
    retire(&first_config);
    assert!(activate_recovery(&first_config)
        .unwrap_err()
        .contains("retires"));
    assert!(first_config
        .state_directory
        .join("recovery-activated.json")
        .exists());
    assert!(Worker::open(first_config.clone()).is_err());
    let second = prepare_recovery(&first_config, &temp.path().join("second")).unwrap();
    let second_config = WorkerConfig::load(&second.worker_config_path).unwrap();
    activate_recovery(&second_config).unwrap();
    drop(Worker::open(second_config).unwrap());
    assert!(Worker::open(first_config).is_err());
}
