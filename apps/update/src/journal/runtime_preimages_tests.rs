//! Real create-only file copies and retained SQLite metadata; descriptors are
//! synthetic fixtures, never candidate health or executable installation.
use super::*;
use crate::runtime_preimages::{self, RuntimePreimagesAdmission, RuntimePreimagesPermit};
fn ready() -> (Fixture, winbridge_protocol::LoadedWorkerIdentity) {
    let mut f = Fixture::new();
    let i = f.intent();
    let receipt = f.receipt(&i);
    let identity = receipt.loaded_worker.clone();
    f.journal
        .complete_runtime_baseline(&f.owner, &f.supervisor, &receipt)
        .unwrap();
    fs::create_dir_all(runtime_preimages::parent_path(
        &f.supervisor.spec.runtime_directory,
        &i.update_id,
    ))
    .unwrap();
    (f, identity)
}
fn admit(
    f: &mut Fixture,
    identity: &winbridge_protocol::LoadedWorkerIdentity,
) -> RuntimePreimagesPermit {
    match runtime_preimages::admit_fixture(
        &mut f.journal,
        &f.owner,
        &f.supervisor,
        identity.clone(),
    )
    .unwrap()
    {
        RuntimePreimagesAdmission::Fresh(p) => *p,
        _ => panic!("Fresh one-use preimages permit required"),
    }
}
fn destination(f: &Fixture) -> PathBuf {
    runtime_preimages::parent_path(
        &f.supervisor.spec.runtime_directory,
        &f.supervisor.spec.update_id,
    )
    .join(runtime_preimages::DIRECTORY)
}
fn original(f: &Fixture) -> Vec<(PathBuf, Vec<u8>)> {
    let b = f
        .journal
        .runtime_baseline(&f.supervisor.spec.update_id)
        .unwrap()
        .receipt
        .unwrap();
    std::iter::once(b.installed.manifest_file)
        .chain(b.installed.images)
        .map(|i| {
            let p = PathBuf::from(i.path);
            let bytes = fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect()
}
#[test]
fn preimages_fresh_intent_and_five_independent_copies_commit_before_source() {
    let (mut f, identity) = ready();
    let before = original(&f);
    let p = admit(&mut f, &identity);
    assert!(!destination(&f).exists());
    assert_eq!(
        f.journal
            .get(&f.supervisor.spec.update_id)
            .unwrap()
            .revision,
        3
    );
    assert!(begin(&mut f)
        .unwrap_err()
        .message
        .contains("Runtime preimages pending"));
    let result =
        runtime_preimages::execute_fixture(&mut f.journal, &f.owner, &f.supervisor, p, || Ok(()))
            .unwrap();
    let receipt = result.receipt.as_ref().unwrap();
    assert_eq!(receipt.files.len(), 5);
    assert_eq!(receipt.parent_revision, 4);
    for ((path, bytes), copy) in before.iter().zip(&receipt.files) {
        assert_eq!(&fs::read(path).unwrap(), bytes);
        assert_eq!(&fs::read(&copy.path).unwrap(), bytes);
        assert_ne!(
            crate::artifact::inspect_image(path).unwrap().file_identity,
            copy.file_identity
        );
    }
    // A completed duplicate returns history even after the old runtime changes.
    fs::write(&before[1].0, b"changed old runtime after completed archive").unwrap();
    match runtime_preimages::admit_fixture(&mut f.journal, &f.owner, &f.supervisor, identity)
        .unwrap()
    {
        RuntimePreimagesAdmission::Existing(saved) => assert_eq!(*saved, result),
        _ => panic!("Completed duplicate must not create a fresh permit"),
    }
    assert_eq!(begin(&mut f).unwrap().phase, UpdatePhase::Preparing);
    assert_eq!(
        UpdateJournal::inspect_runtime_preimages(
            &f.journal.directory,
            &f.supervisor.spec.update_id
        )
        .unwrap(),
        result
    );
}
#[test]
fn preimages_existing_destination_is_never_adopted_or_overwritten() {
    for before_admission in [true, false] {
        let (mut f, identity) = ready();
        let permit = if before_admission {
            None
        } else {
            Some(admit(&mut f, &identity))
        };
        let target = destination(&f);
        fs::create_dir(&target).unwrap();
        fs::write(target.join("user-data"), b"preserve").unwrap();
        if let Some(p) = permit {
            assert!(runtime_preimages::execute_fixture(
                &mut f.journal,
                &f.owner,
                &f.supervisor,
                p,
                || Ok(())
            )
            .is_err());
        } else {
            assert!(runtime_preimages::admit_fixture(
                &mut f.journal,
                &f.owner,
                &f.supervisor,
                identity
            )
            .is_err());
        }
        assert_eq!(fs::read(target.join("user-data")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(target).unwrap().count(), 1);
        assert!(f
            .journal
            .runtime_preimages(&f.supervisor.spec.update_id)
            .unwrap()
            .receipt
            .is_none());
    }
}
#[test]
fn preimages_dropped_permit_cannot_be_recreated_from_pending_history() {
    let (mut f, identity) = ready();
    drop(admit(&mut f, &identity));
    assert!(
        runtime_preimages::admit_fixture(&mut f.journal, &f.owner, &f.supervisor, identity)
            .err()
            .unwrap()
            .contains("no replay")
    );
    assert!(!destination(&f).exists());
    assert!(begin(&mut f).is_err());
}
#[test]
fn preimages_partial_copy_and_extra_files_are_preserved_without_receipt_or_replay() {
    for extra_file in [false, true] {
        let (mut f, identity) = ready();
        let p = admit(&mut f, &identity);
        let target = destination(&f);
        let before = original(&f);
        let mut checks = 0;
        let result =
            runtime_preimages::execute_fixture(&mut f.journal, &f.owner, &f.supervisor, p, || {
                checks += 1;
                if checks == 5 {
                    if extra_file {
                        fs::write(target.join("unexpected"), b"preserve").unwrap();
                    } else {
                        return Err("Fixture fence/config changed during copy".into());
                    }
                }
                Ok(())
            });
        assert!(result.is_err());
        assert!(fs::read_dir(&target).unwrap().count() > 0);
        for (path, bytes) in before {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(f
            .journal
            .runtime_preimages(&f.supervisor.spec.update_id)
            .unwrap()
            .receipt
            .is_none());
        assert_eq!(
            f.journal
                .get(&f.supervisor.spec.update_id)
                .unwrap()
                .revision,
            3
        );
        assert!(runtime_preimages::admit_fixture(
            &mut f.journal,
            &f.owner,
            &f.supervisor,
            identity
        )
        .is_err());
        assert!(begin(&mut f).is_err());
    }
}
#[test]
fn preimages_final_observation_cannot_hide_added_or_changed_copy() {
    for change_bytes in [false, true] {
        if change_bytes && cfg!(windows) {
            continue;
        }
        let (mut f, identity) = ready();
        let p = admit(&mut f, &identity);
        let target = destination(&f);
        let mut checks = 0;
        let result =
            runtime_preimages::execute_fixture(&mut f.journal, &f.owner, &f.supervisor, p, || {
                checks += 1;
                if checks == 13 {
                    if change_bytes {
                        let path = target.join(MANIFEST_NAME);
                        let mut bytes = fs::read(&path).unwrap();
                        bytes[0] ^= 1;
                        fs::write(path, bytes).unwrap();
                    } else {
                        fs::write(target.join("late-extra"), b"preserve").unwrap();
                    }
                }
                Ok(())
            });
        assert!(result.is_err());
        assert!(f
            .journal
            .runtime_preimages(&f.supervisor.spec.update_id)
            .unwrap()
            .receipt
            .is_none());
    }
}

#[cfg(windows)]
#[test]
fn preimages_windows_read_pins_hold_copies_until_receipt_then_release() {
    let (mut f, identity) = ready();
    let p = admit(&mut f, &identity);
    let target = destination(&f);
    let mut checks = 0;
    let result =
        runtime_preimages::execute_fixture(&mut f.journal, &f.owner, &f.supervisor, p, || {
            checks += 1;
            if checks == 13 {
                let files: Vec<_> = fs::read_dir(&target)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .collect();
                assert_eq!(files.len(), 5);
                for path in files {
                    let failure = fs::OpenOptions::new().write(true).open(&path).unwrap_err();
                    assert_eq!(
                        failure.raw_os_error(),
                        Some(32),
                        "Expected Windows sharing violation for {}",
                        path.display()
                    );
                    assert!(!fs::read(path).unwrap().is_empty());
                }
            }
            Ok(())
        })
        .unwrap();
    for file in result.receipt.unwrap().files {
        drop(fs::OpenOptions::new().write(true).open(&file.path).unwrap());
        assert_eq!(
            crate::artifact::inspect_image(Path::new(&file.path)).unwrap(),
            file
        );
    }
}

#[test]
fn preimages_parent_replacement_or_new_epoch_stops_before_copy() {
    for epoch in [false, true] {
        let (mut f, identity) = ready();
        let p = admit(&mut f, &identity);
        if epoch {
            drop(f.owner);
            f.owner = f.journal.acquire_owner().unwrap();
        } else {
            let parent = destination(&f).parent().unwrap().to_owned();
            fs::rename(&parent, parent.with_extension("held")).unwrap();
            fs::create_dir(&parent).unwrap();
        }
        assert!(runtime_preimages::execute_fixture(
            &mut f.journal,
            &f.owner,
            &f.supervisor,
            p,
            || Ok(())
        )
        .is_err());
        assert!(!destination(&f).exists());
    }
}
#[test]
fn preimages_ignored_required_writes_roll_back_parent_and_preserve_copies() {
    for table in ["runtime_preimage_intents", "runtime_preimages"] {
        let (mut f, identity) = ready();
        let p = if table == "runtime_preimages" {
            Some(admit(&mut f, &identity))
        } else {
            None
        };
        f.journal.connection.execute_batch(&format!("CREATE TRIGGER ignore_preimages BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END;")).unwrap();
        if let Some(p) = p {
            assert!(runtime_preimages::execute_fixture(
                &mut f.journal,
                &f.owner,
                &f.supervisor,
                p,
                || Ok(())
            )
            .is_err());
            assert_eq!(
                f.journal
                    .get(&f.supervisor.spec.update_id)
                    .unwrap()
                    .revision,
                3
            );
            assert_eq!(fs::read_dir(destination(&f)).unwrap().count(), 5);
        } else {
            assert!(runtime_preimages::admit_fixture(
                &mut f.journal,
                &f.owner,
                &f.supervisor,
                identity
            )
            .is_err());
            assert_eq!(
                f.journal
                    .get(&f.supervisor.spec.update_id)
                    .unwrap()
                    .revision,
                2
            );
            assert!(!destination(&f).exists());
        }
        assert!(f
            .journal
            .runtime_preimages(&f.supervisor.spec.update_id)
            .unwrap()
            .receipt
            .is_none());
    }
}
#[test]
fn preimages_durable_abort_from_same_owner_fences_writer_before_release() {
    let (mut f, identity) = ready();
    let p = admit(&mut f, &identity);
    let before = original(&f);
    let scope = f.journal.info().unwrap().scope_sha256;
    let mut control = UpdateJournal::open(&f.journal.directory, &scope).unwrap();
    let authority = &f.owner;
    let spec = crate::abort::AbortSpec {
        update_id: f.supervisor.spec.update_id.clone(),
        owner_id: authority.owner_id().into(),
        owner_epoch: authority.epoch(),
        original_phase: UpdatePhase::Requested,
        runtime_directory: f.supervisor.spec.runtime_directory.clone(),
        runtime_identity: winbridge_platform::source_files::directory_identity(Path::new(
            &f.supervisor.spec.runtime_directory,
        ))
        .unwrap(),
        source_digest: None,
        build_digest: None,
        candidate_digest: None,
        accepted_at_ms: now_ms().unwrap(),
    };
    let mut checks = 0;
    assert!(runtime_preimages::execute_fixture(
        &mut f.journal,
        authority,
        &f.supervisor,
        p,
        || {
            checks += 1;
            if checks == 1 {
                let (_, fresh) = control
                    .admit_abort(authority, spec.clone())
                    .map_err(|e| e.to_string())?;
                assert!(fresh);
            }
            Ok(())
        }
    )
    .is_err());
    assert!(!destination(&f).exists());
    assert!(f
        .journal
        .runtime_preimages(&f.supervisor.spec.update_id)
        .unwrap()
        .receipt
        .is_none());
    assert_eq!(
        f.journal.get(&f.supervisor.spec.update_id).unwrap().phase,
        UpdatePhase::Aborting
    );
    let abort =
        UpdateJournal::inspect_abort(&f.journal.directory, &f.supervisor.spec.update_id).unwrap();
    assert_eq!(abort.phase, crate::abort::AbortPhase::Intent);
    assert!(!abort.cleanup_verified);
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn preimages_raw_events_and_late_source_attempt_never_authorize_copies() {
    let (mut f, identity) = ready();
    let id = f.supervisor.spec.update_id.clone();
    for event in [
        UpdateEvent::RuntimePreimagesAdmitted,
        UpdateEvent::RuntimePreimagesVerified,
    ] {
        assert!(f
            .journal
            .advance(
                &f.owner,
                &id,
                &UpdateCommand {
                    idempotency_key: "raw-preimages".into(),
                    expected_revision: 2,
                    event,
                    reason: "Raw bypass".into()
                }
            )
            .is_err());
    }
    begin(&mut f).unwrap();
    assert!(
        runtime_preimages::admit_fixture(&mut f.journal, &f.owner, &f.supervisor, identity)
            .is_err()
    );
    assert!(!destination(&f).exists());
}
#[test]
fn preimages_retained_receipt_rejects_rebound_file_identity() {
    let (mut f, identity) = ready();
    let p = admit(&mut f, &identity);
    let mut result =
        runtime_preimages::execute_fixture(&mut f.journal, &f.owner, &f.supervisor, p, || Ok(()))
            .unwrap();
    assert!(f
        .journal
        .connection
        .execute_batch("DELETE FROM runtime_preimages")
        .is_err());
    assert!(f
        .journal
        .connection
        .execute_batch("UPDATE runtime_preimages SET record='{}'")
        .is_err());
    let r = result.receipt.as_mut().unwrap();
    r.files[0].file_identity = r.files[1].file_identity.clone();
    f.journal
        .connection
        .execute_batch("DROP TRIGGER runtime_preimages_immutable;")
        .unwrap();
    f.journal
        .connection
        .execute(
            "UPDATE runtime_preimages SET record=?1 WHERE id=?2",
            params![encode(r).unwrap(), r.update_id],
        )
        .unwrap();
    assert_eq!(
        UpdateJournal::inspect_runtime_preimages(&f.journal.directory, &r.update_id)
            .unwrap_err()
            .code,
        JournalErrorCode::Corrupt
    );
}
#[test]
fn schema_eight_migration_preserves_baseline_without_adopting_preimages() {
    let (f, _) = ready();
    let id = f.supervisor.spec.update_id.clone();
    let dir = f.journal.directory.clone();
    let baseline = UpdateJournal::inspect_runtime_baseline(&dir, &id).unwrap();
    f.journal.connection.execute_batch("DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; DROP TABLE shutdown_history; DROP TABLE shutdown_records; DROP TABLE shutdown_intents; DROP TABLE drain_history; DROP TABLE drain_commands; DROP TABLE drain_intents; DROP TABLE candidate_health; DROP TABLE runtime_preimages; DROP TABLE runtime_preimage_intents; PRAGMA user_version=8; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let old = UpdateJournal::inspect(&dir).unwrap();
    assert!(UpdateJournal::open(&dir, &old.scope_sha256).is_err());
    drop(f.owner);
    drop(f.journal);
    let mut j = UpdateJournal::open(&dir, &old.scope_sha256).unwrap();
    assert_eq!(j.info().unwrap().schema_version, UPDATE_SCHEMA_VERSION);
    assert_eq!(j.info().unwrap().owner_epoch, old.owner_epoch + 1);
    assert_eq!(
        UpdateJournal::inspect_runtime_baseline(&dir, &id).unwrap(),
        baseline
    );
    assert!(UpdateJournal::inspect_runtime_preimages(&dir, &id)
        .unwrap()
        .intent
        .is_none());
}
