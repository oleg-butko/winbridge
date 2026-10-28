//! Real SQLite/instance locks; synthetic runtime identity, never native readiness.
#[path = "controller_tests.rs"]
mod controller;
use super::*;
use crate::store::{
    activation_open::tests::{bytes, fixture, lock_available},
    JournalActivationAcceptance,
};
use std::path::Path;
use winbridge_protocol::{update::*, ArtifactRole, LoadedWorkerIdentity, RuntimeImageEvidence};

fn activated(
    root: &Path,
) -> (
    ActivationJournal,
    Box<JournalActivationPermit>,
    WorkerServiceActivationCommand,
) {
    activated_with(fixture(root))
}
fn activated_with(
    (old, state, writer, shutdown): (
        Store,
        LockedShutdownState,
        winbridge_protocol::update::WorkerJournalActivationCommand,
        winbridge_protocol::update::WorkerShutdownReceipt,
    ),
) -> (
    ActivationJournal,
    Box<JournalActivationPermit>,
    WorkerServiceActivationCommand,
) {
    let root = std::path::PathBuf::from(&writer.binding.physical.state.state_directory);
    let mut journal = ActivationJournal::open(state, writer.clone(), shutdown, || Ok(())).unwrap();
    let JournalActivationAcceptance::Fresh(fresh) = journal.activate(|| Ok(())).unwrap() else {
        panic!("fresh activation");
    };
    drop(old);
    let mut descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Worker);
    descriptor.build.source_dirty = false;
    descriptor.build.source_commit = Some("d".repeat(40));
    descriptor.build.source_manifest_sha256 = Some("e".repeat(64));
    let command = WorkerServiceActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: crate::store::now_ms().max(fresh.receipt().committed_at_ms),
        deadline_at_ms: writer.deadline_at_ms,
        binding: WorkerServiceActivationBinding {
            writer_command: writer.clone(),
            writer_receipt: fresh.receipt().clone(),
            writer_record_sha256: "f".repeat(64),
            expected_worker: LoadedWorkerIdentity {
                schema_version: 1,
                kind: "loaded_worker_identity".into(),
                process: writer.binding.physical.process.clone(),
                runtime_identity: "synthetic-service-runtime".into(),
                set_id: uuid::Uuid::new_v4().to_string(),
                manifest_sha256: "b".repeat(64),
                descriptor,
                image: RuntimeImageEvidence {
                    path: root.join("worker.exe").to_str().unwrap().into(),
                    file_identity: "synthetic-service-file".into(),
                    bytes: 1,
                    sha256: "a".repeat(64),
                    query_supported: true,
                },
            },
            worker_config_schema: 1,
            protocol_version: winbridge_protocol::PROTOCOL_VERSION,
        },
    };
    (journal, fresh, command)
}

#[test]
fn service_transfer_retains_same_lock_bytes_epoch_generation_and_quiesced_lease() {
    eprintln!(
        "SERVICE_ADMISSION_BUILD {}",
        serde_json::to_string(&winbridge_protocol::build_identity()).unwrap()
    );
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let before = bytes(root.path());
    let expected = fresh.receipt().clone();
    let expected_state = journal.state_observation().clone();
    let mut checks = 0;
    let prepared = journal
        .prepare_local_service(fresh, command.clone(), || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(checks, 2);
    assert!(!lock_available(root.path()));
    assert_eq!(prepared.receipt().unwrap(), Some(expected));
    assert_eq!(prepared.command(), &command);
    assert_eq!(prepared.state_observation(), &expected_state);
    prepared.revalidate_state().unwrap();
    assert_eq!(bytes(root.path()), before);
    // No new journal/schema/authority transaction or hidden recovery has run.
    drop(prepared);
    assert!(lock_available(root.path()));
}

#[test]
fn service_transfer_failure_consumes_fresh_permit_but_retains_exact_state_owner() {
    for failed_check in [1, 2] {
        let root = tempfile::tempdir().unwrap();
        let (journal, fresh, command) = activated(root.path());
        let before = bytes(root.path());
        let expected_state = journal.state_observation().clone();
        let mut checks = 0;
        let failure = journal
            .prepare_local_service(fresh, command, || {
                checks += 1;
                if checks == failed_check {
                    Err("Original parent/window/graph rejected".into())
                } else {
                    Ok(())
                }
            })
            .err()
            .unwrap();
        assert_eq!(checks, failed_check);
        assert!(failure.reason().contains("rejected"));
        assert_eq!(failure.state_observation(), &expected_state);
        failure.revalidate_state().unwrap();
        assert!(!lock_available(root.path()));
        assert_eq!(bytes(root.path()), before);
        drop(failure);
        assert!(lock_available(root.path()));
    }
}

#[test]
fn service_transfer_rejects_foreign_fresh_permit_without_reopening_or_unlocking() {
    let root = tempfile::tempdir().unwrap();
    let foreign_root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let (foreign, foreign_fresh, _) = activated(foreign_root.path());
    let before = bytes(root.path());
    let failure = journal
        .prepare_local_service(foreign_fresh, command, || {
            panic!("foreign receipt rejected first")
        })
        .err()
        .unwrap();
    assert!(!lock_available(root.path()));
    assert_eq!(bytes(root.path()), before);
    failure.revalidate_state().unwrap();
    drop((fresh, foreign));
}

#[test]
fn service_transfer_rejects_rebound_command_authority_scope_and_window() {
    for mutation in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let (journal, fresh, mut command) = activated(root.path());
        let before = bytes(root.path());
        match mutation {
            0 => command.binding.writer_command.command_id = uuid::Uuid::new_v4().to_string(),
            1 => command.binding.writer_receipt.authority.epoch += 1,
            2 => command.binding.expected_worker.process.birth.push('0'),
            _ => command.issued_at_ms = command.deadline_at_ms + 1,
        }
        let failure = journal
            .prepare_local_service(fresh, command, || Ok(()))
            .err()
            .unwrap();
        failure.revalidate_state().unwrap();
        assert!(!lock_available(root.path()));
        assert_eq!(bytes(root.path()), before);
    }
}

#[test]
fn service_transfer_rejects_a_superseded_header_while_retaining_the_same_lock() {
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let c = rusqlite::Connection::open(root.path().join("journal.sqlite")).unwrap();
    c.execute(
        "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
        [uuid::Uuid::new_v4().to_string()],
    )
    .unwrap();
    drop(c);
    let before = bytes(root.path());
    let failure = journal
        .prepare_local_service(fresh, command, || Ok(()))
        .err()
        .unwrap();
    assert!(
        failure
            .reason()
            .contains("stale journal incarnation/owner epoch"),
        "{}",
        failure.reason()
    );
    assert!(!lock_available(root.path()));
    failure.revalidate_state().unwrap();
    assert_eq!(bytes(root.path()), before);
}

#[test]
fn service_transfer_rechecks_authority_after_the_last_external_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let mut checks = 0;
    let failure = journal
        .prepare_local_service(fresh, command, || {
            checks += 1;
            if checks == 2 {
                let c = rusqlite::Connection::open(root.path().join("journal.sqlite")).unwrap();
                c.execute(
                    "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            Ok(())
        })
        .err()
        .unwrap();
    assert_eq!(checks, 2);
    assert!(
        failure
            .reason()
            .contains("stale journal incarnation/owner epoch"),
        "{}",
        failure.reason()
    );
    assert!(!lock_available(root.path()));
    failure.revalidate_state().unwrap();
}

// The graph/process metadata stays synthetic. These tests additionally exercise
// shared Worker recovery and real local IPC (named pipe on Windows), never TCP.
pub(crate) fn service_fixture(root: &Path) -> (PreparedServiceJournal, std::path::PathBuf, String) {
    service_fixture_with(
        root,
        serde_json::json!({"controllerUrl":"ws://127.0.0.1:9","keepRunningOnUiExit":true}),
    )
}
fn service_fixture_with(
    root: &Path,
    settings: serde_json::Value,
) -> (PreparedServiceJournal, std::path::PathBuf, String) {
    use winbridge_protocol::*;
    let state = root.join("state");
    std::fs::create_dir(&state).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let project = root.join("project");
    std::fs::create_dir(&project).unwrap();
    let instance = format!("svc-{}", uuid::Uuid::new_v4().simple());
    let config = serde_json::json!({"schemaVersion":1,"instanceId":instance,"nodeId":"service-test",
        "stateDirectory":state,"worker":settings,
        "projects":[{"id":"source","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]});
    let config_path = root.join("worker.json");
    let config_bytes = serde_json::to_vec(&config).unwrap();
    std::fs::write(&config_path, &config_bytes).unwrap();
    let mut task_id = String::new();
    let parts = crate::store::activation_open::tests::fixture_with(
        &state,
        &instance,
        &winbridge_update::sha256(&config_bytes),
        |store| {
            let task = store
                .accept(
                    submission("old"),
                    ResolvedCommand {
                        executable: "synthetic-never-run".into(),
                        arguments: vec![],
                        working_directory: ".".into(),
                        timeout_ms: 5000,
                    },
                    None,
                )
                .unwrap();
            task_id = task.task_id.clone();
            store
                .append_output(
                    &task_id,
                    OutputStream::Stdout,
                    0,
                    b"old output: \xe2\x9c\x93",
                    1024,
                )
                .unwrap();
            store
                .finish(
                    &task_id,
                    TaskStatus::Cancelled,
                    None,
                    "Metadata fixture; no process started",
                )
                .unwrap();
            let context = RequestContext {
                instance_id: instance.clone(),
                journal_incarnation: store.journal_authority().incarnation.clone(),
            };
            let observation = store.queue_observation(context.clone()).unwrap();
            assert!(matches!(
                store
                    .queue_control(
                        context,
                        QueueControlCommand::Set {
                            command_id: "old-pause".into(),
                            expected_revision: observation.revision,
                            mode: QueueMode::Paused
                        }
                    )
                    .unwrap(),
                QueueControlResult::Applied { .. }
            ));
        },
    );
    let (journal, fresh, command) = activated_with(parts);
    (
        journal
            .prepare_local_service(fresh, command, || Ok(()))
            .unwrap(),
        config_path,
        task_id,
    )
}
fn submission(key: &str) -> winbridge_protocol::TaskSubmission {
    winbridge_protocol::TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "source".into(),
        idempotency_key: key.into(),
        timeout_ms: 5000,
        snapshot_id: None,
        command: winbridge_protocol::CommandSource::Direct {
            executable: "synthetic-never-run".into(),
            arguments: vec![],
            working_directory: ".".into(),
        },
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_service_recovers_same_owner_and_exposes_readonly_history_over_real_ipc() {
    use winbridge_protocol::*;
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, task_id) = service_fixture(root.path());
    let expected = prepared.command().clone();
    let state = root.path().join("state");
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    assert!(!lock_available(&state));
    let running = service.start().await.unwrap();
    let endpoint = running.endpoint().unwrap();
    let response: LocalResponse =
        winbridge_transport::supervisor::call(&endpoint, std::process::id(), &LocalRequest::Info)
            .await
            .unwrap();
    let LocalResponse::Info { info } = response else {
        panic!("Info")
    };
    assert_eq!(
        info.generation,
        expected.binding.writer_command.binding.physical.generation
    );
    assert!(
        !info.maintenance_available && !info.snapshot_import_available && !info.workflow_available
    );
    assert!(
        !info
            .fenced_requests
            .as_ref()
            .unwrap()
            .legacy_mutations_allowed
    );
    assert!(info.journal_inspection.is_none());
    assert!(info
        .runtime_validation
        .contains("Локальная проверка обновления"));
    let connection = info.controller_connection.unwrap();
    assert_eq!(connection.url, "ws://127.0.0.1:9");
    assert!(matches!(connection.state, ConnectionState::Disconnected));
    assert!(connection.session_id.is_none() && connection.last_confirmed_at_ms.is_none());
    let catalog = info.contract_catalog.unwrap();
    for key in ["workScheduling", "queueControl", "resourceScheduling"] {
        assert_eq!(catalog[key]["readOnly"], true, "{key}");
    }
    let response: LocalResponse = winbridge_transport::supervisor::call(
        &endpoint,
        std::process::id(),
        &LocalRequest::GetTask {
            task_id: task_id.clone(),
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(response, LocalResponse::Task { task } if task.status == TaskStatus::Cancelled)
    );
    let response: LocalResponse = winbridge_transport::supervisor::call(
        &endpoint,
        std::process::id(),
        &LocalRequest::ReadOutput {
            task_id: task_id.clone(),
            after_sequence: 0,
        },
    )
    .await
    .unwrap();
    let LocalResponse::Output { chunks } = response else {
        panic!("output")
    };
    assert_eq!(
        chunks
            .iter()
            .flat_map(|c| c.bytes.clone())
            .collect::<Vec<_>>(),
        b"old output: \xe2\x9c\x93"
    );
    let response: LocalResponse = winbridge_transport::supervisor::call(
        &endpoint,
        std::process::id(),
        &LocalRequest::Submit {
            request: submission("new"),
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(response, LocalResponse::Error { message } if message.contains("Локальная проверка обновления"))
    );
    assert!(winbridge_transport::supervisor::call::<_, LocalResponse>(
        &endpoint,
        std::process::id() + 1,
        &LocalRequest::Info
    )
    .await
    .is_err());
    let context = info.fenced_requests.unwrap().context;
    let observe = QueueControlRequest {
        queue_control_version: 1,
        context: context.clone(),
        command: QueueControlCommand::Observe {
            command_id: Some("old-pause".into()),
        },
    };
    let response: QueueControlResponse =
        winbridge_transport::supervisor::call(&endpoint, std::process::id(), &observe)
            .await
            .unwrap();
    let QueueControlResult::Observed {
        observation,
        receipt,
    } = response.result
    else {
        panic!("queue observation")
    };
    assert_eq!(observation.mode, QueueMode::Paused);
    assert!(observation.read_only && receipt.is_some());
    let set = QueueControlRequest {
        queue_control_version: 1,
        context,
        command: QueueControlCommand::Set {
            command_id: "new-unpause".into(),
            expected_revision: observation.revision,
            mode: QueueMode::Running,
        },
    };
    let response: QueueControlResponse =
        winbridge_transport::supervisor::call(&endpoint, std::process::id(), &set)
            .await
            .unwrap();
    assert!(matches!(
        response.result,
        QueueControlResult::Rejected {
            code: QueueControlError::Unavailable,
            ..
        }
    ));
    let observed = Store::open_read_only(&state.join("journal.sqlite")).unwrap();
    assert_eq!(
        observed.state_backup_authority().unwrap(),
        expected.binding.writer_receipt.authority
    );
    assert!(observed.update_frozen().unwrap());
    assert_eq!(observed.recent().unwrap().len(), 1);
    assert!(!lock_available(&state));
    drop(observed);
    running.close().await.unwrap();
    assert!(lock_available(&state));
}
#[test]
fn local_service_failed_initialization_retains_same_lock_and_never_binds_ipc() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let mut calls = 0;
    let failure = crate::LocalService::prepare_fixture(prepared, &config, || {
        calls += 1;
        if calls == 5 {
            Err("Parent scope changed during recovery".into())
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert!(
        failure.reason().contains("Parent scope changed"),
        "{}",
        failure.reason()
    );
    failure.revalidate_state().unwrap();
    assert!(!lock_available(&root.path().join("state")));
    drop(failure);
    assert!(lock_available(&root.path().join("state")));
}
#[test]
fn local_service_rejects_changed_config_without_initializing_or_releasing_state() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let before = bytes(&root.path().join("state"));
    std::fs::write(&config, b"{}").unwrap();
    let failure = crate::LocalService::prepare_fixture(prepared, &config, || {
        panic!("No checkpoint before pinned config check")
    })
    .err()
    .unwrap();
    assert!(
        failure.reason().contains("SHA mismatch"),
        "{}",
        failure.reason()
    );
    failure.revalidate_state().unwrap();
    assert!(!lock_available(&root.path().join("state")));
    assert_eq!(bytes(&root.path().join("state")), before);
}
#[test]
fn local_service_initialization_cannot_renew_the_original_monotonic_window() {
    let root = tempfile::tempdir().unwrap();
    let (mut prepared, config, _) = service_fixture(root.path());
    prepared
        .journal
        .constrain_startup_deadline(std::time::Instant::now());
    let failure = crate::LocalService::prepare_fixture(prepared, &config, || Ok(()))
        .err()
        .unwrap();
    assert!(
        failure.reason().contains("window expired"),
        "{}",
        failure.reason()
    );
    failure.revalidate_state().unwrap();
    assert!(!lock_available(&root.path().join("state")));
}

#[test]
fn sealed_initialization_fence_allows_recovery_but_keeps_admission_admin_and_epoch_closed() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, _, _) = service_fixture(root.path());
    let mut state = prepared.into_runtime(|| Ok(())).unwrap();
    state
        .store
        .write_transaction(rusqlite::TransactionBehavior::Immediate)
        .unwrap()
        .commit()
        .unwrap();
    assert!(state.store.require_update_administration().is_err());
    assert!(state.store.begin_authority().is_err());
    assert!(state
        .store
        .accept(
            submission("closed"),
            winbridge_protocol::ResolvedCommand {
                executable: "synthetic-never-run".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 5000
            },
            None
        )
        .unwrap_err()
        .contains("обновление WinBridge"));
    state.store.finish_service_initialization().unwrap();
    assert!(state
        .store
        .write_transaction(rusqlite::TransactionBehavior::Immediate)
        .err()
        .unwrap()
        .contains("frozen"));
    assert!(state.store.finish_service_initialization().is_err());
    assert!(!lock_available(&root.path().join("state")));
}
#[test]
fn sealed_initialization_fence_rechecks_header_on_each_transaction() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, _, _) = service_fixture(root.path());
    let mut state = prepared.into_runtime(|| Ok(())).unwrap();
    let other = rusqlite::Connection::open(root.path().join("state/journal.sqlite")).unwrap();
    other
        .execute(
            "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
            [uuid::Uuid::new_v4().to_string()],
        )
        .unwrap();
    assert!(state
        .store
        .write_transaction(rusqlite::TransactionBehavior::Immediate)
        .err()
        .unwrap()
        .contains("stale"));
    assert!(state.store.finish_service_initialization().is_err());
    assert!(!lock_available(&root.path().join("state")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_service_bind_conflict_is_not_readiness_and_keeps_original_state_owned() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    let endpoint = service.endpoint().unwrap();
    #[cfg(unix)]
    let occupied = tokio::net::UnixListener::bind(&endpoint).unwrap();
    #[cfg(windows)]
    let occupied = winbridge_transport::create_pipe(&endpoint, true).unwrap();
    let failure = service.start().await.err().unwrap();
    failure.revalidate_state().unwrap();
    assert!(!lock_available(&root.path().join("state")));
    drop((occupied, failure));
    assert!(lock_available(&root.path().join("state")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_observation_never_promotes_fixture_metadata_or_rebinds_a_command() {
    use winbridge_protocol::update::*;
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let command = prepared.command().clone();
    let mut normal = winbridge_core::config::WorkerConfig::load(&config).unwrap();
    normal.state_directory = root.path().join("normal-state");
    normal.instance_id = format!("normal-{}", uuid::Uuid::new_v4().simple());
    let normal = crate::Worker::open(normal).unwrap();
    let query = ActivationServiceQuery {
        service_observation_version: 1,
        command: command.clone(),
    };
    assert!(matches!(
        normal.handle_service_observation(query.clone()).result,
        ActivationServiceObservationResult::Rejected { .. }
    ));
    drop(normal);
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    let running = service.start().await.unwrap();
    let endpoint = running.endpoint().unwrap();
    for mutation in 0..3 {
        let mut query = query.clone();
        if mutation == 1 {
            query.service_observation_version = 2;
        }
        if mutation == 2 {
            query.command.command_id = uuid::Uuid::new_v4().to_string();
        }
        let response: ActivationServiceObservation =
            winbridge_transport::supervisor::call(&endpoint, std::process::id(), &query)
                .await
                .unwrap();
        let ActivationServiceObservationResult::Rejected { message } = response.result else {
            panic!("No installed-set guard; cannot fabricate actual runtime identity")
        };
        assert!(
            message.contains(if mutation == 0 {
                "No actual loaded runtime guard"
            } else {
                "binding differs"
            }),
            "{message}"
        );
    }
    running.close().await.unwrap();
    assert!(lock_available(&root.path().join("state")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_monitor_observes_listener_exit_and_close_never_polls_a_completed_join_twice() {
    let root = tempfile::tempdir().unwrap();
    let (prepared, config, _) = service_fixture(root.path());
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    let mut running = service.start().await.unwrap();
    running.stop_fixture();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        running.wait_until_stopped(),
    )
    .await
    .unwrap()
    .unwrap();
    // This is retained state, not a handoff: actual close/drain is still needed.
    assert!(!lock_available(&root.path().join("state")));
    running.wait_until_stopped().await.unwrap();
    running.close().await.unwrap();
    assert!(lock_available(&root.path().join("state")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn staged_service_preserves_multi_controller_diagnostics_without_network_or_dispatch() {
    use winbridge_protocol::*;
    let root = tempfile::tempdir().unwrap();
    let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = serde_json::json!({"keepRunningOnUiExit":true,"controllerTopology":{
        "schemaVersion":1,"primary":"a","controllers":[
            {"id":"a","url":format!("ws://{}",a.local_addr().unwrap())},
            {"id":"b","url":format!("ws://{}",b.local_addr().unwrap())}]}});
    let (prepared, config, _) = service_fixture_with(root.path(), settings);
    let service = crate::LocalService::prepare_fixture(prepared, &config, || Ok(())).unwrap();
    let running = service.start().await.unwrap();
    let endpoint = running.endpoint().unwrap();
    let LocalResponse::Info { info } =
        winbridge_transport::supervisor::call(&endpoint, std::process::id(), &LocalRequest::Info)
            .await
            .unwrap()
    else {
        panic!("Info")
    };
    assert!(!info.workflow_available && !info.snapshot_import_available);
    let context = info.fenced_requests.as_ref().unwrap().context.clone();
    let query = ControllerConnectionsRequest {
        context: context.clone(),
    };
    let request = controller_connections_envelope(&info, &query).unwrap();
    let response = winbridge_transport::supervisor::call(&endpoint, std::process::id(), &request)
        .await
        .unwrap();
    let links = decode_controller_connections(response, &info, &query).unwrap();
    assert_eq!(links.primary.as_deref(), Some("a"));
    assert_eq!(links.connections.len(), 2);
    for link in links.connections {
        assert_eq!(link.attempts, "0");
        assert!(matches!(
            link.connection.state,
            ConnectionState::Disconnected
        ));
        assert!(link.connection.error.is_some());
        assert!(link.connection.session_id.is_none());
    }
    let response: LocalResponse = winbridge_transport::supervisor::call(
        &endpoint,
        std::process::id(),
        &LocalRequest::Submit {
            request: submission("must-not-run"),
        },
    )
    .await
    .unwrap();
    assert!(matches!(response, LocalResponse::Error { .. }));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(120), async {
            tokio::select! { result=a.accept()=>result,result=b.accept()=>result }
        })
        .await
        .is_err(),
        "Staged service connected a controller"
    );
    running.close().await.unwrap();
    assert!(lock_available(&root.path().join("state")));
}
