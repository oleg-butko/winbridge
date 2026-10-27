//! Real file locks/SQLite, synthetic child/graph metadata. No runtime exit proof.
use super::*;
use crate::store::ShutdownAcceptance;
use std::{fs, fs::OpenOptions};
use winbridge_protocol::{
    update::{ActivationStateOwned, WorkerJournalActivationBinding},
    RequestContext,
};
use winbridge_update::shutdown::ShutdownStatePins;

pub(crate) fn fixture(
    root: &Path,
) -> (
    Store,
    LockedShutdownState,
    WorkerJournalActivationCommand,
    WorkerShutdownReceipt,
) {
    fixture_with(root, "shutdown-test", &"c".repeat(64), |_| {})
}
pub(crate) fn fixture_with(
    root: &Path,
    instance: &str,
    config_sha: &str,
    seed: impl FnOnce(&mut Store),
) -> (
    Store,
    LockedShutdownState,
    WorkerJournalActivationCommand,
    WorkerShutdownReceipt,
) {
    let (mut store, mut pending, _) =
        crate::store::shutdown::tests::ready_with(&root.join("journal.sqlite"), instance, seed);
    fs::write(root.join("worker.lock"), b"existing lock").unwrap();
    let pins = ShutdownStatePins::capture(root).unwrap();
    pending.state = pins.observation().clone();
    let ShutdownAcceptance::Fresh(accepted) = store.accept_worker_shutdown(pending).unwrap() else {
        panic!("fresh shutdown")
    };
    let shutdown = accepted.receipt().clone();
    let mut process = shutdown.process.clone();
    process.process_id += 1;
    process.birth = (process.birth.parse::<u64>().unwrap() + 1).to_string();
    let at = super::super::now_ms();
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let command = WorkerJournalActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        binding: WorkerJournalActivationBinding {
            update_id: shutdown.update_id.clone(),
            attempt_id: attempt_id.clone(),
            launch_id: shutdown.launch_id.clone(),
            updater_owner_id: shutdown.updater_owner_id.clone(),
            updater_epoch: shutdown.updater_epoch,
            activation_revision: 6,
            activation_record_sha256: "a".repeat(64),
            backup_record_sha256: "b".repeat(64),
            expected_authority: store.state_backup_authority().unwrap(),
            physical: ActivationStateOwned {
                attempt_id,
                process,
                context: RequestContext {
                    instance_id: shutdown.lease.spec.instance_id.clone(),
                    journal_incarnation: shutdown.lease.spec.journal_incarnation.clone(),
                },
                generation: uuid::Uuid::new_v4().to_string(),
                state: shutdown.state.clone(),
                observed_at_ms: at,
            },
            worker_journal_schema: JOURNAL_SCHEMA_VERSION,
            updater_journal_schema: 18,
            worker_config_sha256: config_sha.into(),
        },
        issued_at_ms: at,
        deadline_at_ms: at + 120000,
    };
    (store, pins.try_lock().unwrap(), command, shutdown)
}
pub(crate) fn bytes(root: &Path) -> (Vec<u8>, Option<Vec<u8>>) {
    (
        fs::read(root.join("journal.sqlite")).unwrap(),
        fs::read(root.join("journal.sqlite-wal")).ok(),
    )
}
pub(crate) fn lock_available(root: &Path) -> bool {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("worker.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).is_ok()
}

#[test]
fn existing_open_keeps_the_same_lock_header_bytes_and_maintenance_until_one_activation() {
    let root = tempfile::tempdir().unwrap();
    let (old, state, command, shutdown) = fixture(root.path());
    let header = old.journal_authority().clone();
    let before = bytes(root.path());
    let mut checks = 0;
    let mut journal = ActivationJournal::open(state, command.clone(), shutdown.clone(), || {
        checks += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(checks, 3);
    assert!(!lock_available(root.path()));
    assert_eq!(journal.state_observation(), &command.binding.physical.state);
    assert_eq!(journal.store.journal_authority(), &header);
    assert_eq!(bytes(root.path()), before);
    assert!(journal.receipt().unwrap().is_none());
    assert_eq!(
        journal.store.active_worker_update().unwrap(),
        Some(shutdown.lease.clone())
    );
    for (name, value) in [("query_only", 0), ("foreign_keys", 1), ("synchronous", 2)] {
        assert_eq!(
            journal
                .store
                .connection
                .query_row(&format!("PRAGMA {name}"), [], |r| r.get::<_, u32>(0))
                .unwrap(),
            value
        );
    }
    assert!(journal.store.begin_authority().is_err());
    let JournalActivationAcceptance::Fresh(permit) = journal.activate(|| Ok(())).unwrap() else {
        panic!("one fresh activation")
    };
    let receipt = permit.receipt().clone();
    assert_eq!(receipt.authority.epoch, header.epoch + 1);
    assert!(old.check_authority().is_err());
    assert!(matches!(
        journal
            .activate(|| panic!("no duplicate live grant"))
            .unwrap(),
        JournalActivationAcceptance::Observed(_)
    ));
    assert_eq!(journal.receipt().unwrap(), Some(receipt.clone()));
    assert!(!lock_available(root.path()));
    assert_eq!(
        journal.store.active_worker_update().unwrap(),
        Some(shutdown.lease)
    );
    drop(journal);
    assert!(lock_available(root.path()));
    assert_eq!(
        Store::open_read_only(&root.path().join("journal.sqlite"))
            .unwrap()
            .journal_activation_receipt(&command)
            .unwrap(),
        Some(receipt)
    );
}

#[test]
fn existing_open_refuses_old_future_and_non_wal_before_any_mode_or_schema_write() {
    for schema in [16, JOURNAL_SCHEMA_VERSION + 1] {
        let root = tempfile::tempdir().unwrap();
        let (old, state, command, shutdown) = fixture(root.path());
        old.connection
            .execute_batch(&format!(
                "PRAGMA user_version={schema};PRAGMA wal_checkpoint(TRUNCATE);"
            ))
            .unwrap();
        let before = bytes(root.path());
        let failure = ActivationJournal::open(state, command, shutdown, || Ok(()))
            .err()
            .unwrap();
        assert!(failure.reason().contains("schema"));
        assert_eq!(bytes(root.path()), before);
        assert!(!lock_available(root.path()));
        failure.revalidate_state().unwrap();
        assert!(!fs::read_dir(root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("journal-v")));
    }
    let root = tempfile::tempdir().unwrap();
    let (old, state, command, shutdown) = fixture(root.path());
    // All fixture connections close before changing journal mode.
    drop(old);
    let c = Connection::open(root.path().join("journal.sqlite")).unwrap();
    c.execute_batch("PRAGMA journal_mode=DELETE;").unwrap();
    drop(c);
    let before = bytes(root.path());
    let failure = ActivationJournal::open(state, command, shutdown, || Ok(()))
        .err()
        .unwrap();
    assert!(failure.reason().contains("WAL mode"));
    assert_eq!(bytes(root.path()), before);
    assert!(!lock_available(root.path()));
}

#[test]
fn existing_open_rejects_foreign_state_header_scope_and_expired_window_retaining_lock() {
    for fault in 0..6 {
        let root = tempfile::tempdir().unwrap();
        let (old, state, mut command, mut shutdown) = fixture(root.path());
        let before = bytes(root.path());
        match fault {
            0 => command.binding.expected_authority.fingerprint_sha256 = "d".repeat(64),
            1 => command.binding.physical.state.journal_identity.push('x'),
            2 => {
                command.binding.physical.generation = shutdown.lease.spec.worker_generation.clone()
            }
            3 => command.binding.worker_journal_schema = 16,
            4 => shutdown.command.reason.push('x'),
            _ => {
                command.issued_at_ms = 1;
                command.deadline_at_ms = 2;
                command.binding.physical.observed_at_ms = 1;
            }
        }
        let failure = ActivationJournal::open(state, command, shutdown, || Ok(()))
            .err()
            .unwrap();
        failure.revalidate_state().unwrap();
        assert_eq!(bytes(root.path()), before);
        assert!(!lock_available(root.path()));
        assert!(old.check_authority().is_ok());
        drop(failure);
        assert!(lock_available(root.path()));
    }
}

#[test]
fn every_preparation_checkpoint_failure_keeps_the_lock_and_unmodified_database() {
    for fail_at in 1..=3 {
        let root = tempfile::tempdir().unwrap();
        let (old, state, command, shutdown) = fixture(root.path());
        let before = bytes(root.path());
        let mut checks = 0;
        let failure = ActivationJournal::open(state, command, shutdown, || {
            checks += 1;
            if checks == fail_at {
                Err("Original live custody expired".into())
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap();
        assert_eq!(checks, fail_at);
        assert_eq!(failure.reason(), "Original live custody expired");
        assert_eq!(bytes(root.path()), before);
        assert!(!lock_available(root.path()));
        assert!(old.check_authority().is_ok());
    }
}

#[test]
fn failed_activation_retains_connection_and_lock_but_never_replays_the_transaction() {
    let root = tempfile::tempdir().unwrap();
    let (old, state, command, shutdown) = fixture(root.path());
    let header = old.journal_authority().clone();
    let mut journal = ActivationJournal::open(state, command, shutdown, || Ok(())).unwrap();
    let mut checks = 0;
    assert!(journal
        .activate(|| {
            checks += 1;
            if checks == 2 {
                Err("Final live check failed".into())
            } else {
                Ok(())
            }
        })
        .is_err());
    assert_eq!(checks, 2);
    assert_eq!(journal.store.journal_authority(), &header);
    assert!(journal.receipt().unwrap().is_none());
    assert!(journal
        .activate(|| panic!("failed attempt must not replay"))
        .is_err());
    assert!(journal.receipt().unwrap().is_none());
    assert!(!lock_available(root.path()));
    assert!(old.check_authority().is_ok());
    drop(journal);
    assert!(lock_available(root.path()));
}

#[test]
fn cold_open_cannot_adopt_a_committed_receipt_as_a_new_owner() {
    let root = tempfile::tempdir().unwrap();
    let (_old, state, command, shutdown) = fixture(root.path());
    let mut journal =
        ActivationJournal::open(state, command.clone(), shutdown.clone(), || Ok(())).unwrap();
    journal.activate(|| Ok(())).unwrap();
    let receipt = journal.receipt().unwrap();
    drop(journal);
    let state = ShutdownStatePins::capture(root.path())
        .unwrap()
        .try_lock()
        .unwrap();
    let failure = ActivationJournal::open(state, command.clone(), shutdown, || Ok(()))
        .err()
        .unwrap();
    assert!(!lock_available(root.path()));
    assert_eq!(
        Store::open_read_only(&root.path().join("journal.sqlite"))
            .unwrap()
            .journal_activation_receipt(&command)
            .unwrap(),
        receipt
    );
    failure.revalidate_state().unwrap();
}

#[test]
fn last_connection_preparation_and_failure_do_not_checkpoint_or_remove_the_retained_wal() {
    use rusqlite::config::DbConfig;
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (old, state, command, shutdown) = fixture(root.path());
        // Real stopped-source shape: no other SQLite connection keeps the WAL
        // open for us. The existing physical pins are ordinary file handles.
        assert!(old
            .connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
            .unwrap());
        drop(old);
        let before = bytes(root.path());
        assert!(!before.1.as_ref().unwrap().is_empty());
        let mut checks = 0;
        let result = ActivationJournal::open(state, command, shutdown, || {
            checks += 1;
            if fail && checks == 3 {
                Err("Last preparation check failed".into())
            } else {
                Ok(())
            }
        });
        if fail {
            let failure = result.err().unwrap();
            assert!(
                bytes(root.path()) == before,
                "Failed preparation changed DB/WAL before authority acceptance"
            );
            assert!(!lock_available(root.path()));
            failure.revalidate_state().unwrap();
        } else {
            let journal = result.unwrap();
            assert!(
                bytes(root.path()) == before,
                "Preparation changed DB/WAL before authority acceptance"
            );
            drop(journal);
            assert!(
                bytes(root.path()) == before,
                "Unactivated connection Drop changed DB/WAL"
            );
            assert!(lock_available(root.path()));
        }
    }
}

#[cfg(unix)]
#[test]
fn existing_open_detects_rebinding_and_never_recreates_a_missing_journal() {
    for replace in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (_old, state, command, shutdown) = fixture(root.path());
        let path = root.path().join("journal.sqlite");
        let mut checks = 0;
        let failure = ActivationJournal::open(state, command, shutdown, || {
            checks += 1;
            if checks == 2 {
                fs::rename(&path, root.path().join("retained.sqlite")).unwrap();
                if replace {
                    fs::copy(root.path().join("retained.sqlite"), &path).unwrap();
                }
            }
            Ok(())
        })
        .err()
        .unwrap();
        assert!(failure.reason().contains("rebound") || failure.reason().contains("No such file"));
        assert_eq!(path.exists(), replace);
        assert!(!lock_available(root.path()));
    }
}
