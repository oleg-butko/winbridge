//! Real SQLite transactions with synthetic process metadata. No runtime permit.
use super::*;
use winbridge_protocol::update::{ActivationStateOwned, WorkerJournalActivationBinding};
use winbridge_protocol::{RequestContext, ResolvedCommand, TaskSubmission};

fn fixture(path: &Path) -> (Store, WorkerShutdownReceipt, WorkerJournalActivationCommand) {
    let (mut store, pending, _) = super::super::shutdown_metadata_fixture(path);
    let super::super::ShutdownAcceptance::Fresh(accepted) =
        store.accept_worker_shutdown(pending).unwrap()
    else {
        panic!("fresh")
    };
    let accepted = accepted.receipt().clone();
    let at = now_ms();
    let mut process = accepted.process.clone();
    process.process_id += 1;
    process.birth = (process.birth.parse::<u64>().unwrap() + 1).to_string();
    let command = WorkerJournalActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: at,
        deadline_at_ms: at + 120000,
        binding: WorkerJournalActivationBinding {
            update_id: accepted.update_id.clone(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
            launch_id: accepted.launch_id.clone(),
            updater_owner_id: accepted.updater_owner_id.clone(),
            updater_epoch: accepted.updater_epoch,
            activation_revision: 6,
            activation_record_sha256: "a".repeat(64),
            backup_record_sha256: "b".repeat(64),
            expected_authority: store.state_backup_authority().unwrap(),
            physical: ActivationStateOwned {
                attempt_id: String::new(),
                process,
                context: RequestContext {
                    instance_id: accepted.lease.spec.instance_id.clone(),
                    journal_incarnation: accepted.lease.spec.journal_incarnation.clone(),
                },
                generation: uuid::Uuid::new_v4().to_string(),
                state: accepted.state.clone(),
                observed_at_ms: at,
            },
            worker_journal_schema: JOURNAL_SCHEMA_VERSION,
            updater_journal_schema: 18,
            worker_config_sha256: "c".repeat(64),
        },
    };
    let mut command = command;
    command.binding.physical.attempt_id = command.binding.attempt_id.clone();
    (store, accepted, command)
}
fn unchanged(
    store: &Store,
    old: &JournalAuthority,
    accepted: &WorkerShutdownReceipt,
    command: &WorkerJournalActivationCommand,
) {
    assert_eq!(Store::read_authority(&store.connection).unwrap(), *old);
    assert_eq!(store.journal_authority(), old);
    assert_eq!(
        store.active_worker_update().unwrap().as_ref(),
        Some(&accepted.lease)
    );
    assert!(store.journal_activation_receipt(command).unwrap().is_none());
}

#[test]
fn journal_activation_commits_header_and_receipt_once_without_releasing_maintenance() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let (mut store, accepted, command) = fixture(&path);
    let stale = Store::open(&path).unwrap();
    let before = store.journal_authority().clone();
    let mut checks = 0;
    let JournalActivationAcceptance::Fresh(permit) = store
        .activate_journal_authority_checked(&command, &accepted, || {
            checks += 1;
            Ok(())
        })
        .unwrap()
    else {
        panic!("fresh")
    };
    assert_eq!(checks, 2);
    let receipt = permit.receipt().clone();
    assert_eq!(store.journal_authority().epoch, before.epoch + 1);
    assert_eq!(store.journal_authority().incarnation, before.incarnation);
    assert_ne!(store.journal_authority().authority_id, before.authority_id);
    assert_eq!(receipt.authority, store.state_backup_authority().unwrap());
    assert!(stale.check_authority().is_err());
    assert_eq!(
        store.active_worker_update().unwrap().as_ref(),
        Some(&accepted.lease)
    );
    assert_eq!(
        store
            .worker_shutdown_receipt(&accepted.update_id, &accepted.command)
            .unwrap(),
        Some(accepted.clone())
    );
    assert!(store.begin_authority().is_err());
    assert!(Store::require_update_writes(&store.connection).is_err());
    assert!(store.require_update_administration().is_err());
    assert!(store
        .advance_worker_update(
            &accepted.lease.spec,
            &winbridge_protocol::update::WorkerUpdateCommand {
                command_id: "ordinary-release".into(),
                expected_revision: accepted.lease.revision,
                action: winbridge_protocol::update::WorkerUpdateAction::Release,
                source_manifest_sha256: None,
                reason: "Ordinary release remains blocked".into(),
            },
            winbridge_protocol::update::WorkerUpdateBlockers {
                accepted_operations: 0,
                logical_owners: 0,
                in_flight_effects: 0,
                physical_owners: 0,
                uncertain_owners: 0,
            }
        )
        .is_err());
    let header = store.journal_authority().clone();
    let JournalActivationAcceptance::Observed(same) = store
        .activate_journal_authority_checked(&command, &accepted, || {
            panic!("No fresh checkpoint/permission for duplicate")
        })
        .unwrap()
    else {
        panic!("observed")
    };
    assert_eq!(*same, receipt);
    assert_eq!(store.journal_authority(), &header);
    let reader = Store::open_read_only(&path).unwrap();
    assert_eq!(
        reader.journal_activation_receipt(&command).unwrap(),
        Some(receipt)
    );
    assert!(reader.read_only);
    drop(store);
    let mut cold = Store::open(&path).unwrap();
    assert!(matches!(
        cold.activate_journal_authority_checked(&command, &accepted, || panic!(
            "Cold receipt is not a permit"
        ))
        .unwrap(),
        JournalActivationAcceptance::Observed(_)
    ));
    assert_eq!(cold.journal_authority(), &header);
    assert!(cold.begin_authority().is_err());
    assert!(cold
        .connection
        .execute("DELETE FROM worker_journal_activations", [])
        .is_err());
    assert!(cold
        .connection
        .execute("UPDATE worker_journal_activations SET receipt=receipt", [])
        .is_err());
}
#[test]
fn journal_activation_rolls_back_ignored_header_receipt_and_failed_live_check() {
    for fault in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let (mut store, accepted, command) = fixture(&root.path().join("journal.sqlite"));
        let old = store.journal_authority().clone();
        if fault < 2 {
            store.connection.execute_batch(if fault==0 {"CREATE TRIGGER fault BEFORE UPDATE ON journal_authority BEGIN SELECT RAISE(IGNORE);END;"} else {"CREATE TRIGGER fault BEFORE INSERT ON worker_journal_activations BEGIN SELECT RAISE(IGNORE);END;"}).unwrap();
        }
        let mut checks = 0;
        assert!(store
            .activate_journal_authority_checked(&command, &accepted, || {
                checks += 1;
                if fault == 2 && checks == 1 || fault == 3 && checks == 2 {
                    Err("Stale live owner".into())
                } else {
                    Ok(())
                }
            })
            .is_err());
        unchanged(&store, &old, &accepted, &command);
    }
}
#[test]
fn journal_activation_refuses_stale_header_foreign_scope_and_expired_window() {
    for fault in 0..7 {
        let root = tempfile::tempdir().unwrap();
        let (mut store, accepted, mut command) = fixture(&root.path().join("journal.sqlite"));
        let old = store.journal_authority().clone();
        match fault {
            0 => command.binding.expected_authority.epoch += 1,
            1 => command.binding.expected_authority.fingerprint_sha256 = "d".repeat(64),
            2 => command.binding.physical.state.lock_identity.push('x'),
            3 => command.binding.launch_id = uuid::Uuid::new_v4().to_string(),
            4 => {
                command.binding.physical.generation = accepted.lease.spec.worker_generation.clone()
            }
            5 => command.binding.worker_journal_schema -= 1,
            _ => {
                command.deadline_at_ms = command.issued_at_ms;
            }
        }
        assert!(store
            .activate_journal_authority_checked(&command, &accepted, || Ok(()))
            .is_err());
        assert_eq!(Store::read_authority(&store.connection).unwrap(), old);
        assert_eq!(
            store
                .connection
                .query_row("SELECT count(*) FROM worker_journal_activations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn journal_activation_checks_actual_schema_and_stored_shutdown_before_any_epoch_change() {
    for fault in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let (mut store, accepted, command) = fixture(&root.path().join("journal.sqlite"));
        let before = store.journal_authority().clone();
        // Deliberate metadata corruption in this owned temporary database.
        store
            .connection
            .execute_batch(&match fault {
                0 => "DROP TRIGGER worker_shutdown_retained; DELETE FROM worker_shutdown_receipts;"
                    .into(),
                1 => "PRAGMA user_version=16;".into(),
                _ => format!("PRAGMA user_version={};", JOURNAL_SCHEMA_VERSION + 1),
            })
            .unwrap();
        assert!(store
            .activate_journal_authority_checked(&command, &accepted, || Ok(()))
            .is_err());
        assert_eq!(Store::read_authority(&store.connection).unwrap(), before);
        assert_eq!(
            store
                .connection
                .query_row("SELECT count(*) FROM worker_journal_activations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn cold_activation_receipt_rejects_consistent_tampering_outside_the_accepted_stop_scope() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let (mut store, accepted, command) = fixture(&path);
    let JournalActivationAcceptance::Fresh(permit) = store
        .activate_journal_authority_checked(&command, &accepted, || Ok(()))
        .unwrap()
    else {
        panic!("fresh")
    };
    let mut changed = command.clone();
    changed.binding.physical.state.lock_identity.push('x');
    let mut receipt = permit.receipt().clone();
    receipt.command_sha256 = command_sha256(&changed).unwrap();
    validate_receipt(&receipt, &changed).unwrap(); // Hash/types alone are insufficient.
    store
        .connection
        .execute_batch("DROP TRIGGER worker_journal_activation_immutable;")
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE worker_journal_activations SET command_sha256=?1,command=?2,receipt=?3",
            params![
                receipt.command_sha256,
                encode(&changed).unwrap(),
                encode(&receipt).unwrap()
            ],
        )
        .unwrap();
    let reader = Store::open_read_only(&path).unwrap();
    assert!(reader.journal_activation_receipt(&changed).is_err());
}
#[test]
fn backup_authority_fingerprint_keeps_the_frozen_v1_tuple_encoding() {
    let authority = JournalAuthority {
        incarnation: "11111111-1111-4111-8111-111111111111".into(),
        epoch: 7,
        authority_id: "22222222-2222-4222-8222-222222222222".into(),
    };
    assert_eq!(
        stamp(&authority).unwrap().fingerprint_sha256,
        "8711ef5aa9addab8693825f7bbc25025e9976173d087c3c31de1a046c8502c29"
    );
}

#[test]
fn journal_activation_receipt_survives_consistent_archive_without_minting_an_owner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let (mut store, accepted, command) = fixture(&path);
    let JournalActivationAcceptance::Fresh(permit) = store
        .activate_journal_authority_checked(&command, &accepted, || Ok(()))
        .unwrap()
    else {
        panic!("fresh")
    };
    let receipt = permit.receipt().clone();
    let reader = Store::open_read_only(&path).unwrap();
    let backup = root.path().join("archive.sqlite");
    reader.archive_read_only(&backup).unwrap();
    let archived = Store::open_read_only(&backup).unwrap();
    assert_eq!(
        archived.journal_activation_receipt(&command).unwrap(),
        Some(receipt)
    );
    assert_eq!(
        archived
            .worker_shutdown_receipt(&accepted.update_id, &accepted.command)
            .unwrap(),
        Some(accepted.clone())
    );
    assert_eq!(
        archived.active_worker_update().unwrap().as_ref(),
        Some(&accepted.lease)
    );
    assert_eq!(archived.journal_authority(), reader.journal_authority());
    assert!(archived.read_only);
}

#[test]
fn activation_migration_backup_retains_exact_file_identity_until_commit() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    store.begin_authority().unwrap();
    store.connection.execute_batch("DROP TABLE operation_cancellations; DROP TABLE operation_creators; DROP TABLE worker_journal_activations;PRAGMA user_version=16;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let guard = Store::backup_activation_migration(&path, &store.connection).unwrap();
    guard.revalidate().unwrap();
    #[cfg(windows)]
    {
        assert!(std::fs::rename(&guard.path, root.path().join("moved.sqlite")).is_err());
        assert!(std::fs::remove_file(&guard.path).is_err());
    }
    #[cfg(unix)]
    {
        let moved = root.path().join("moved.sqlite");
        std::fs::rename(&guard.path, &moved).unwrap();
        std::fs::copy(moved, &guard.path).unwrap(); // Same bytes, different FileID.
        assert!(guard.revalidate().is_err());
    }
}
#[test]
fn journal_activation_conflicting_key_payload_and_second_key_do_not_advance_epoch() {
    let root = tempfile::tempdir().unwrap();
    let (mut store, accepted, command) = fixture(&root.path().join("journal.sqlite"));
    assert!(matches!(
        store
            .activate_journal_authority_checked(&command, &accepted, || Ok(()))
            .unwrap(),
        JournalActivationAcceptance::Fresh(_)
    ));
    let header = store.journal_authority().clone();
    for changed_key in [false, true] {
        let mut changed = command.clone();
        if changed_key {
            changed.command_id = uuid::Uuid::new_v4().to_string();
        } else {
            changed.binding.worker_config_sha256 = "d".repeat(64);
        }
        assert!(store
            .activate_journal_authority_checked(&changed, &accepted, || panic!("No second grant"))
            .is_err());
        assert_eq!(store.journal_authority(), &header);
    }
    let mut readonly = Store::open_read_only(&root.path().join("journal.sqlite")).unwrap();
    assert!(readonly
        .activate_journal_authority_checked(&command, &accepted, || Ok(()))
        .is_err());
}
#[test]
fn activation_migration_sixteen_preserves_history_keys_header_and_verified_backup() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let (store, accepted, _) = fixture(&path);
    // Metadata-only final history; no actual release/OS proof is claimed.
    let mut released = accepted.lease.clone();
    released.revision += 1;
    released.phase = WorkerUpdatePhase::Released;
    released.updated_at_ms += 1;
    store
        .connection
        .execute(
            "UPDATE worker_update_leases SET revision=?1,phase=?2,active=0,record=?3 WHERE id=?4",
            params![
                released.revision as i64,
                encode(&released.phase).unwrap(),
                encode(&released).unwrap(),
                accepted.update_id
            ],
        )
        .unwrap();
    let mut store = store;
    let request:TaskSubmission=serde_json::from_value(serde_json::json!({"protocolVersion":1,"requestId":"migration",
        "projectId":"source","idempotencyKey":"migration-key","timeoutMs":1000,
        "command":{"kind":"direct","executable":"fixture.exe","arguments":[],"workingDirectory":"."}})).unwrap();
    let task = store
        .accept(
            request.clone(),
            ResolvedCommand {
                executable: "fixture.exe".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 1000,
            },
            None,
        )
        .unwrap();
    let old = store.journal_authority().clone();
    store.connection.execute_batch("DROP TABLE operation_cancellations; DROP TABLE operation_creators; DROP TABLE worker_journal_activations;PRAGMA user_version=16;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(store);
    let migrated = Store::open(&path).unwrap();
    assert_eq!(migrated.journal_authority(), &old);
    assert_eq!(
        migrated.existing(&request).unwrap().unwrap().task_id,
        task.task_id
    );
    assert_eq!(
        migrated
            .worker_shutdown_receipt(&accepted.update_id, &accepted.command)
            .unwrap(),
        Some(accepted)
    );
    assert_eq!(
        migrated
            .connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        JOURNAL_SCHEMA_VERSION
    );
    let backups: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v16-")
                && p.extension().is_some_and(|extension| extension == "sqlite")
        })
        .collect();
    // WAL/SHM sidecars share the prefix, but are not SQLite backup databases.
    assert_eq!(backups.len(), 1);
    let c = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        16
    );
    assert_eq!(Store::read_authority(&c).unwrap(), old);
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM tasks WHERE key='migration-key'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn activation_migration_refuses_active_sixteen_before_any_writable_open() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let (store, _, _) = fixture(&path);
    store.connection.execute_batch("DROP TABLE operation_cancellations; DROP TABLE operation_creators; DROP TABLE worker_journal_activations;PRAGMA user_version=16;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(store);
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open(&path).err().unwrap().contains("unfinished"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("journal-v16-")));
    assert!(Store::open_read_only(&path).is_err());
}

#[cfg(unix)]
#[test]
fn activation_preflight_rejects_dangling_symlink_before_database_creation() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("must-not-create.sqlite");
    let linked = root.path().join("journal.sqlite");
    std::os::unix::fs::symlink(&target, &linked).unwrap();
    let error = Store::open(&linked).err().unwrap();
    assert!(
        error.contains("Linked/reparse/special path refused"),
        "{error}"
    );
    assert!(!target.exists());
    assert!(std::fs::symlink_metadata(linked)
        .unwrap()
        .file_type()
        .is_symlink());
}
