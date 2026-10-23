use rusqlite::Connection;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};
use winbridge_protocol::update::*;
use winbridge_update::journal::{JournalErrorCode as C, UpdateJournal};

const SCOPE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
struct TestChild(std::process::Child);
impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn request(store: &mut UpdateJournal, key: &str) -> UpdateRequest {
    let info = store.info().unwrap();
    UpdateRequest {
        contract_version: 1,
        idempotency_key: key.into(),
        source: UpdateSourcePin {
            archive_name: "winbridge-source.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: info.installation_id,
            scope_sha256: info.scope_sha256,
            worker_build_id: "12345".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    }
}
fn command(key: &str, revision: u64, event: UpdateEvent) -> UpdateCommand {
    UpdateCommand {
        idempotency_key: key.into(),
        expected_revision: revision,
        event,
        reason: format!("Verified step {key}"),
    }
}

#[test]
fn lost_ack_reopen_and_transition_receipts_preserve_identity_before_new_guards() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let mut store = UpdateJournal::open(&root, SCOPE).unwrap();
    let request = request(&mut store, "original");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &request, &request.expected).unwrap();
    assert_eq!(accepted.revision, 0);
    let begin = command("prepare", 0, UpdateEvent::BeginPreparation);
    let preparing = store.advance(&owner, &accepted.update_id, &begin).unwrap();
    store
        .advance(
            &owner,
            &accepted.update_id,
            &command("source", 1, UpdateEvent::SourceVerified),
        )
        .unwrap();
    assert_eq!(
        store.advance(&owner, &accepted.update_id, &begin).unwrap(),
        preparing
    );
    let mut conflict = begin.clone();
    conflict.expected_revision = 200;
    assert_eq!(
        store
            .advance(&owner, &accepted.update_id, &conflict)
            .unwrap_err()
            .code,
        C::KeyConflict
    );
    let epoch = owner.epoch();
    drop(owner);
    drop(store);
    let mut store = UpdateJournal::open(&root, SCOPE).unwrap();
    let owner = store.acquire_owner().unwrap();
    assert_eq!(owner.epoch(), epoch + 1);
    let mut changed = request.expected.clone();
    changed.worker_build_id = "new-build".into();
    let duplicate = store.accept(&owner, &request, &changed).unwrap();
    assert_eq!(duplicate.update_id, accepted.update_id);
    assert_eq!(duplicate.phase, UpdatePhase::Building);
    assert_eq!(duplicate.revision, 2);
    let mut conflict = request.clone();
    conflict.source.source_commit = "0".repeat(40);
    assert_eq!(
        store.accept(&owner, &conflict, &changed).unwrap_err().code,
        C::KeyConflict
    );
    let page = store.transitions(0, 2).unwrap();
    assert_eq!(
        page.iter().map(|t| t.sequence).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(page.iter().all(|t| t.owner_epoch == epoch));
    assert_eq!(store.transitions(2, 2).unwrap()[0].revision, 2);
    assert_eq!(store.info().unwrap().last_sequence, 3);
}

#[test]
fn competing_admissions_and_cas_are_serialized_without_partial_state() {
    let temp = tempfile::tempdir().unwrap();
    let mut first = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let second = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut first, "same-key");
    let owner = Arc::new(first.acquire_owner().unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = [first, second]
        .into_iter()
        .map(|mut store| {
            let owner = owner.clone();
            let barrier = barrier.clone();
            let original = original.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let record = store.accept(&owner, &original, &original.expected).unwrap();
                (store, record)
            })
        })
        .collect();
    let mut results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results[0].1, results[1].1);
    let id = results[0].1.update_id.clone();
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = results
        .drain(..)
        .enumerate()
        .map(|(i, (mut store, _))| {
            let owner = owner.clone();
            let barrier = barrier.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.advance(
                    &owner,
                    &id,
                    &command(&format!("step-{i}"), 0, UpdateEvent::BeginPreparation),
                )
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .code,
        C::RevisionConflict
    );
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    assert_eq!(store.get(&id).unwrap().revision, 1);
    assert_eq!(store.info().unwrap().last_sequence, 2);
    let mut other = original;
    other.idempotency_key = "other".into();
    assert_eq!(
        store
            .accept(&owner, &other, &other.expected)
            .unwrap_err()
            .code,
        C::InstallationBusy
    );
}

#[test]
fn failed_transaction_does_not_accept_key_or_advance_state() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "failure");
    let owner = store.acquire_owner().unwrap();
    let injection = Connection::open(temp.path().join("updates.sqlite")).unwrap();
    injection.execute_batch("CREATE TRIGGER fail_transition BEFORE INSERT ON transitions BEGIN SELECT RAISE(ABORT,'injected fault'); END;").unwrap();
    assert_eq!(
        store
            .accept(&owner, &original, &original.expected)
            .unwrap_err()
            .code,
        C::Storage
    );
    let info = store.info().unwrap();
    assert_eq!(info.last_sequence, 0);
    assert!(info.active_update_id.is_none());
    injection
        .execute_batch("DROP TRIGGER fail_transition;")
        .unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    injection.execute_batch("CREATE TRIGGER fail_transition BEFORE INSERT ON transitions BEGIN SELECT RAISE(ABORT,'injected fault'); END;").unwrap();
    let begin = command("begin", 0, UpdateEvent::BeginPreparation);
    assert_eq!(
        store
            .advance(&owner, &accepted.update_id, &begin)
            .unwrap_err()
            .code,
        C::Storage
    );
    assert_eq!(store.get(&accepted.update_id).unwrap(), accepted);
    assert_eq!(store.info().unwrap().last_sequence, 1);
    injection
        .execute_batch("DROP TRIGGER fail_transition;")
        .unwrap();
    assert_eq!(
        store
            .advance(&owner, &accepted.update_id, &begin)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn silently_ignored_writes_never_ack_acceptance_transition_or_owner() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "ignored-write");
    let injection = Connection::open(temp.path().join("updates.sqlite")).unwrap();
    injection
        .execute_batch(
            "CREATE TRIGGER ignored BEFORE UPDATE ON installation BEGIN SELECT RAISE(IGNORE); END;",
        )
        .unwrap();
    assert!(matches!(store.acquire_owner(),Err(e) if e.code==C::Storage));
    assert_eq!(store.info().unwrap().owner_epoch, 0);
    injection.execute_batch("DROP TRIGGER ignored;").unwrap();
    let owner = store.acquire_owner().unwrap();
    for sql in [
        "CREATE TRIGGER ignored BEFORE INSERT ON updates BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER ignored BEFORE INSERT ON transitions BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER ignored BEFORE UPDATE ON transitions BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER ignored AFTER UPDATE ON transitions BEGIN UPDATE transitions SET transition='{}'; END;",
    ] {
        injection.execute_batch(sql).unwrap();
        assert!(store.accept(&owner,&original,&original.expected).is_err());
        assert_eq!(store.info().unwrap().last_sequence,0);
        assert!(store.info().unwrap().active_update_id.is_none());
        injection.execute_batch("DROP TRIGGER ignored;").unwrap();
    }
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    injection
        .execute_batch(
            "CREATE TRIGGER ignored BEFORE UPDATE ON updates BEGIN SELECT RAISE(IGNORE); END;",
        )
        .unwrap();
    assert_eq!(
        store
            .advance(
                &owner,
                &accepted.update_id,
                &command("start", 0, UpdateEvent::BeginPreparation)
            )
            .unwrap_err()
            .code,
        C::Storage
    );
    assert_eq!(store.get(&accepted.update_id).unwrap(), accepted);
    assert_eq!(store.info().unwrap().last_sequence, 1);
}

#[test]
fn exclusive_lock_and_epoch_both_fence_callbacks_including_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "owner");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    let mut competitor = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    assert!(matches!(competitor.acquire_owner(),Err(e) if e.code==C::OwnerBusy));
    let inspector = UpdateJournal::read_snapshot(temp.path(), None).unwrap();
    assert_eq!(inspector.0.owner_epoch, owner.epoch());
    assert_eq!(inspector.1, Some(accepted.clone()));
    // Simulate a stale asynchronous callback after the durable authority changed.
    let connection = Connection::open(temp.path().join("updates.sqlite")).unwrap();
    connection
        .execute("UPDATE installation SET epoch=epoch+1", [])
        .unwrap();
    assert_eq!(
        store
            .accept(&owner, &original, &original.expected)
            .unwrap_err()
            .code,
        C::StaleOwner
    );
    assert_eq!(
        store
            .advance(
                &owner,
                &accepted.update_id,
                &command("old", 0, UpdateEvent::BeginPreparation)
            )
            .unwrap_err()
            .code,
        C::StaleOwner
    );
    assert_eq!(store.get(&accepted.update_id).unwrap(), accepted);
    assert_eq!(store.info().unwrap().last_sequence, 1);
}

#[test]
fn preconditions_and_scope_fail_before_admission_without_reserving_keys() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "guarded");
    let owner = store.acquire_owner().unwrap();
    let mut observed = original.expected.clone();
    observed.worker_config_sha256 = "0".repeat(64);
    assert_eq!(
        store.accept(&owner, &original, &observed).unwrap_err().code,
        C::PreconditionsChanged
    );
    let mut wrong = original.clone();
    wrong.expected.installation_id = "different".into();
    assert_eq!(
        store
            .accept(&owner, &wrong, &wrong.expected)
            .unwrap_err()
            .code,
        C::ScopeMismatch
    );
    assert!(
        matches!(UpdateJournal::open(temp.path(),&"0".repeat(64)),Err(e) if e.code==C::ScopeMismatch)
    );
    assert_eq!(store.info().unwrap().last_sequence, 0);
    assert!(store.accept(&owner, &original, &original.expected).is_ok());
    let another = tempfile::tempdir().unwrap();
    let mut other = UpdateJournal::open(another.path(), SCOPE).unwrap();
    let request = request(&mut other, "other-installation");
    assert_eq!(
        other
            .accept(&owner, &request, &request.expected)
            .unwrap_err()
            .code,
        C::StaleOwner
    );
    assert_eq!(other.info().unwrap().last_sequence, 0);
}

#[test]
fn safe_failure_releases_admission_but_never_forgets_the_original_key() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "original");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    let failed = store
        .advance(
            &owner,
            &accepted.update_id,
            &command("failure", 0, UpdateEvent::Fail),
        )
        .unwrap();
    assert_eq!(failed.phase, UpdatePhase::FailedBeforeCutover);
    assert!(store.info().unwrap().active_update_id.is_none());
    let mut next = original.clone();
    next.idempotency_key = "next".into();
    let accepted_next = store.accept(&owner, &next, &next.expected).unwrap();
    assert_ne!(accepted_next.update_id, accepted.update_id);
    assert_eq!(
        store.accept(&owner, &original, &original.expected).unwrap(),
        failed
    );
    assert_eq!(store.info().unwrap().last_sequence, 3);
}

#[test]
fn manual_intervention_blocks_a_new_update_across_owner_restart() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "manual");
    let owner = store.acquire_owner().unwrap();
    let mut record = store.accept(&owner, &original, &original.expected).unwrap();
    for (i, event) in [
        UpdateEvent::BeginPreparation,
        UpdateEvent::BeginSourceApply,
        UpdateEvent::Fail,
    ]
    .into_iter()
    .enumerate()
    {
        record = store
            .advance(
                &owner,
                &record.update_id,
                &command(&format!("step-{i}"), record.revision, event),
            )
            .unwrap();
    }
    assert_eq!(record.phase, UpdatePhase::ManualIntervention);
    drop(owner);
    drop(store);
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let owner = store.acquire_owner().unwrap();
    assert_eq!(
        store.info().unwrap().active_update_id,
        Some(record.update_id)
    );
    let mut next = original;
    next.idempotency_key = "next".into();
    assert_eq!(
        store
            .accept(&owner, &next, &next.expected)
            .unwrap_err()
            .code,
        C::InstallationBusy
    );
}

#[test]
fn source_transaction_failure_also_blocks_new_installation_before_binary_cutover() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "source-write");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    store
        .advance(
            &owner,
            &accepted.update_id,
            &command("prepare", 0, UpdateEvent::BeginPreparation),
        )
        .unwrap();
    let applying = store
        .advance(
            &owner,
            &accepted.update_id,
            &command("intent", 1, UpdateEvent::BeginSourceApply),
        )
        .unwrap();
    assert_eq!(applying.phase, UpdatePhase::SourceApplying);
    let failed = store
        .advance(
            &owner,
            &accepted.update_id,
            &command("partial-source", 2, UpdateEvent::Fail),
        )
        .unwrap();
    assert_eq!(failed.phase, UpdatePhase::ManualIntervention);
    let mut next = original;
    next.idempotency_key = "next-package".into();
    assert_eq!(
        store
            .accept(&owner, &next, &next.expected)
            .unwrap_err()
            .code,
        C::InstallationBusy
    );
}

#[test]
fn unsupported_foreign_or_unversioned_databases_are_never_migrated() {
    for sql in [
        "PRAGMA user_version=99; PRAGMA application_id=1463965008;",
        "CREATE TABLE user_data(value TEXT);",
        "PRAGMA user_version=1; PRAGMA application_id=123;",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("updates.sqlite");
        let connection = Connection::open(&db).unwrap();
        connection.execute_batch(sql).unwrap();
        drop(connection);
        let before = fs::read(&db).unwrap();
        assert!(
            matches!(UpdateJournal::open(temp.path(),SCOPE),Err(e) if e.code==C::UnsupportedSchema)
        );
        assert_eq!(fs::read(&db).unwrap(), before);
        assert!(!temp.path().join("updates.sqlite-wal").exists());
        assert!(!temp.path().join("owner.lock").exists());
    }
}

#[test]
fn readonly_inspection_does_not_initialize_missing_state_or_take_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    assert!(UpdateJournal::inspect(&missing).is_err());
    assert!(!missing.exists());
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let before = store.info().unwrap();
    for _ in 0..2 {
        assert_eq!(UpdateJournal::inspect(temp.path()).unwrap(), before);
    }
    assert!(!temp.path().join("owner.lock").exists());
}

#[test]
fn corrupted_aggregate_and_linked_lock_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    let original = request(&mut store, "corrupt");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    let injection = Connection::open(temp.path().join("updates.sqlite")).unwrap();
    injection
        .execute("UPDATE updates SET revision=revision+1", [])
        .unwrap();
    assert_eq!(store.get(&accepted.update_id).unwrap_err().code, C::Corrupt);
    assert_eq!(
        UpdateJournal::read_snapshot(temp.path(), None)
            .unwrap_err()
            .code,
        C::Corrupt
    );
    drop(owner);
    drop(store);
    drop(injection);
    fs::hard_link(
        temp.path().join("owner.lock"),
        temp.path().join("alias.lock"),
    )
    .unwrap();
    assert!(UpdateJournal::open(temp.path(), SCOPE).is_err());
}

// Invoked by the parent test in a separate process; no production hidden CLI commands.
#[test]
fn subprocess_owner_helper() {
    let Some(directory) = std::env::var_os("WINBRIDGE_UPDATE_TEST_OWNER") else {
        return;
    };
    let directory = Path::new(&directory);
    let mut store = UpdateJournal::open(directory, SCOPE).unwrap();
    let original = request(&mut store, "crash-key");
    let owner = store.acquire_owner().unwrap();
    let accepted = store.accept(&owner, &original, &original.expected).unwrap();
    fs::write(
        directory.join("accepted.json"),
        serde_json::to_vec(&accepted).unwrap(),
    )
    .unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn abrupt_process_exit_releases_os_lock_without_repeating_or_abandoning_update() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = TestChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "subprocess_owner_helper", "--nocapture"])
            .env("WINBRIDGE_UPDATE_TEST_OWNER", temp.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let marker = temp.path().join("accepted.json");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !marker.exists() && Instant::now() < deadline {
        if let Some(exit) = child.0.try_wait().unwrap() {
            panic!("Owner child exited early: {exit}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if !marker.exists() {
        panic!("Owner child never accepted request");
    }
    let accepted: UpdateRecord = serde_json::from_slice(&fs::read(marker).unwrap()).unwrap();
    let mut store = UpdateJournal::open(temp.path(), SCOPE).unwrap();
    assert!(matches!(store.acquire_owner(),Err(e) if e.code==C::OwnerBusy));
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let owner = store.acquire_owner().unwrap();
    assert_eq!(owner.epoch(), 2);
    assert_eq!(store.get(&accepted.update_id).unwrap(), accepted);
    assert_eq!(
        store
            .accept(&owner, &accepted.request, &accepted.request.expected)
            .unwrap(),
        accepted
    );
    let mut next = accepted.request;
    next.idempotency_key = "new-key".into();
    assert_eq!(
        store
            .accept(&owner, &next, &next.expected)
            .unwrap_err()
            .code,
        C::InstallationBusy
    );
    assert_eq!(store.info().unwrap().last_sequence, 1);
}

#[test]
fn cli_journal_init_and_inspection_preserve_source_config_and_runtime_payload() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let runtime = temp.path().join("runtime");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&runtime).unwrap();
    let worker = runtime.join("worker.local.json");
    fs::write(&worker, "preserve config").unwrap();
    fs::write(source.join("main.rs"), "preserve source").unwrap();
    fs::write(runtime.join("worker.exe"), "preserve binary").unwrap();
    let config = temp.path().join("updater.json");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":worker})).unwrap()).unwrap();
    let call = |action| {
        Command::new(env!("CARGO_BIN_EXE_winbridge-update"))
            .arg("--config")
            .arg(&config)
            .args(["journal", action])
            .output()
            .unwrap()
    };
    assert!(!call("status").status.success());
    assert!(!runtime.join("update-state").exists());
    let first = call("init");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let second = call("init");
    assert!(second.status.success());
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(first, second);
    let status = call("status");
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["info"], first["info"]);
    assert_eq!(status["readOnly"], true);
    assert_eq!(status["schemaVersion"], 2);
    // No update is selected: readonly diagnostics cannot infer the state of
    // runtime files from an absent record. Actual preservation is checked below.
    assert_eq!(status.get("runtimeApplied"), Some(&serde_json::Value::Null));
    assert_eq!(fs::read_to_string(worker).unwrap(), "preserve config");
    assert_eq!(
        fs::read_to_string(source.join("main.rs")).unwrap(),
        "preserve source"
    );
    assert_eq!(
        fs::read_to_string(runtime.join("worker.exe")).unwrap(),
        "preserve binary"
    );
    assert!(!runtime.join("update-state/owner.lock").exists());
}
