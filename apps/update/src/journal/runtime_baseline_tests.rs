//! Database/file fixtures only. Synthetic descriptors never prove executable health.
use super::*;
use crate::installed_set::{InstalledArtifact, InstalledSetManifest, MANIFEST_NAME};
use winbridge_protocol::{update::*, ArtifactRole, RequestContext};

struct Fixture {
    journal: UpdateJournal,
    owner: UpdateAuthority,
    supervisor: SupervisorRecord,
    // Windows cannot remove SQLite/owner files until their handles close.
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        // Archive tests enforce the real 6-GiB disk guard. /tmp can be a small
        // tmpfs; use local disk, just like the production previous-runtime path.
        #[cfg(not(windows))]
        let root = tempfile::tempdir_in("/var/tmp").unwrap();
        #[cfg(windows)]
        let root =
            tempfile::tempdir_in(std::env::current_exe().unwrap().parent().unwrap()).unwrap();
        let runtime = root.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        let runtime = runtime.canonicalize().unwrap();
        let mut journal =
            UpdateJournal::open(&root.path().join("update-state"), &"a".repeat(64)).unwrap();
        let info = journal.info().unwrap();
        let owner = journal.acquire_owner().unwrap();
        let request = UpdateRequest {
            contract_version: 1,
            idempotency_key: "baseline-test".into(),
            source: UpdateSourcePin {
                archive_name: "fixture.zip".into(),
                archive_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
                source_commit: "d".repeat(40),
            },
            expected: UpdateInstallationPin {
                installation_id: info.installation_id,
                scope_sha256: info.scope_sha256,
                worker_build_id: "worker-build".into(),
                worker_config_sha256: "e".repeat(64),
                baseline_manifest_sha256: "f".repeat(64),
            },
        };
        let update = journal.accept(&owner, &request, &request.expected).unwrap();
        let process = winbridge_platform::infrastructure::stamp(std::process::id()).unwrap();
        let mut build = winbridge_protocol::build_identity();
        build.source_dirty = false;
        build.source_commit = Some("1".repeat(40));
        build.source_manifest_sha256 = Some("2".repeat(64));
        build.build_id = "worker-build".into();
        let binding = SupervisorWorkerBinding {
            context: RequestContext {
                instance_id: "fixture".into(),
                journal_incarnation: Uuid::new_v4().to_string(),
            },
            generation: Uuid::new_v4().to_string(),
            build,
            process: process.clone(),
            config_sha256: request.expected.worker_config_sha256,
            endpoint: "fixture-local".into(),
        };
        let spec = SupervisorSpec {
            launch_id: Uuid::new_v4().to_string(),
            update_id: update.update_id,
            parent: process.clone(),
            parent_owner_id: owner.owner_id().into(),
            parent_epoch: owner.epoch(),
            token_sha256: "3".repeat(64),
            config_path: "fixture.json".into(),
            config_sha256: "4".repeat(64),
            runtime_directory: runtime.to_string_lossy().into(),
            image_path: "fixture-updater.exe".into(),
            image_sha256: "5".repeat(64),
            image_identity: "file".into(),
            excluded_domain: None,
            endpoint: "fixture-local".into(),
            accepted_at_ms: now_ms().unwrap(),
            worker: Some(binding),
        };
        let s = journal.prepare_supervisor(&owner, spec).unwrap();
        let s = journal
            .advance_supervisor(
                &owner,
                &s.spec.launch_id,
                0,
                SupervisorPhase::Spawned,
                Some(process),
                "Fixture PID receipt",
            )
            .unwrap();
        // Other parallel tests spawn processes. A Unix fork may briefly retain
        // a reference to this same lock description until exec, so close alone
        // need not release it synchronously. This synthetic fixture launches no
        // supervisor; explicitly finish its handoff before the next owner.
        fs2::FileExt::unlock(&owner.lock).unwrap();
        drop(owner);
        let owner = journal.acquire_owner().unwrap();
        let s = journal
            .advance_supervisor(
                &owner,
                &s.spec.launch_id,
                s.revision,
                SupervisorPhase::Ready,
                None,
                "Fixture Ready",
            )
            .unwrap();
        let supervisor = journal
            .advance_supervisor(
                &owner,
                &s.spec.launch_id,
                s.revision,
                SupervisorPhase::Active,
                None,
                "Fixture Active",
            )
            .unwrap();
        Self {
            root,
            journal,
            owner,
            supervisor,
        }
    }
    fn intent(&mut self) -> RuntimeBaselineIntent {
        self.journal
            .admit_runtime_baseline(&self.owner, &self.supervisor)
            .unwrap()
            .intent
            .unwrap()
    }
    fn receipt(&self, i: &RuntimeBaselineIntent) -> RuntimeBaselineReceipt {
        let runtime = self.root.path().join("runtime");
        assert!(runtime.is_dir());
        let mut artifacts = vec![];
        for (name, role) in [
            ("winbridge-worker.exe", ArtifactRole::Worker),
            ("winbridge-fixture.exe", ArtifactRole::Fixture),
            ("winbridge-desktop.exe", ArtifactRole::Desktop),
            ("winbridge-update.exe", ArtifactRole::Updater),
        ] {
            let bytes = [winbridge_protocol::ARTIFACT_QUERY_MARKER, name.as_bytes()].concat();
            fs::write(runtime.join(name), &bytes).unwrap();
            let mut descriptor = winbridge_protocol::artifact_descriptor(role);
            descriptor.build = i.worker.build.clone();
            artifacts.push(InstalledArtifact {
                name: name.into(),
                bytes: bytes.len() as u64,
                sha256: crate::sha256(&bytes),
                descriptor,
            });
        }
        let expected = &self.journal.get_readonly_expected(&i.update_id);
        let manifest = InstalledSetManifest {
            schema_version: 1,
            kind: "winbridge_installed_set".into(),
            set_id: Uuid::new_v4().to_string(),
            installation_id: expected.installation_id.clone(),
            scope_sha256: expected.scope_sha256.clone(),
            artifacts,
        };
        fs::write(
            runtime.join(MANIFEST_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let pin = crate::installed_set::pin_installed_set(
            &runtime,
            &manifest.installation_id,
            &manifest.scope_sha256,
        )
        .unwrap();
        let o = pin.observation().clone();
        let loaded_worker = winbridge_protocol::LoadedWorkerIdentity {
            schema_version: 1,
            kind: "loaded_worker_identity".into(),
            process: i.worker.process.clone(),
            runtime_identity: o.runtime_identity.clone(),
            set_id: manifest.set_id,
            manifest_sha256: o.manifest_file.sha256.clone(),
            descriptor: o.manifest.artifacts[0].descriptor.clone(),
            image: o.images[0].clone(),
        };
        RuntimeBaselineReceipt {
            schema_version: 1,
            update_id: i.update_id.clone(),
            intent_sha256: crate::runtime_baseline::digest(i).unwrap(),
            parent_revision: i.parent_revision + 1,
            observed_at_ms: now_ms().unwrap().max(i.admitted_at_ms),
            installed: o,
            loaded_worker,
        }
    }
}
// Keep fixture reads on a consistent readonly snapshot through the same reader.
impl UpdateJournal {
    fn get_readonly_expected(&self, id: &str) -> UpdateInstallationPin {
        record(&self.connection, id).unwrap().request.expected
    }
}
fn begin(f: &mut Fixture) -> Result<UpdateRecord> {
    let update = f.journal.get(&f.supervisor.spec.update_id)?;
    f.journal.advance(
        &f.owner,
        &update.update_id,
        &UpdateCommand {
            idempotency_key: "prepare".into(),
            expected_revision: update.revision,
            event: UpdateEvent::BeginPreparation,
            reason: "Fixture source preparation".into(),
        },
    )
}
#[test]
fn fixture_closes_journal_and_owner_before_removing_its_directory() {
    let f = Fixture::new();
    let path = f.root.path().to_owned();
    assert!(path.is_dir());
    drop(f);
    assert!(!path.exists());
}

#[test]
fn baseline_intent_requires_the_exact_active_supervisor() {
    let mut f = Fixture::new();
    let id = f.supervisor.spec.update_id.clone();
    for field in ["phase", "epoch", "generation", "config", "launch"] {
        let mut s = f.supervisor.clone();
        match field {
            "phase" => s.phase = SupervisorPhase::Ready,
            "epoch" => s.owner_epoch = Some(s.owner_epoch.unwrap() + 1),
            "generation" => s.spec.worker.as_mut().unwrap().generation = Uuid::new_v4().to_string(),
            "config" => s.spec.config_sha256 = "0".repeat(64),
            "launch" => s.spec.launch_id = Uuid::new_v4().to_string(),
            _ => unreachable!(),
        }
        assert!(
            f.journal.admit_runtime_baseline(&f.owner, &s).is_err(),
            "{field}"
        );
        assert_eq!(f.journal.get(&id).unwrap().revision, 0);
        assert!(f.journal.runtime_baseline(&id).unwrap().intent.is_none());
    }
    assert_eq!(f.intent().parent_revision, 1);
}

#[test]
fn baseline_intent_receipt_and_parent_history_are_atomic_before_source() {
    let mut f = Fixture::new();
    let i = f.intent();
    assert_eq!(i.parent_revision, 1);
    assert!(begin(&mut f).is_err());
    let receipt = f.receipt(&i);
    let view = f
        .journal
        .complete_runtime_baseline(&f.owner, &f.supervisor, &receipt)
        .unwrap();
    assert_eq!(view.receipt.as_ref(), Some(&receipt));
    let mut conflict = receipt.clone();
    conflict.observed_at_ms += 1;
    assert_eq!(
        f.journal
            .complete_runtime_baseline(&f.owner, &f.supervisor, &conflict)
            .unwrap_err()
            .code,
        JournalErrorCode::KeyConflict
    );
    assert_eq!(f.journal.get(&i.update_id).unwrap().revision, 2);
    assert_eq!(
        f.journal
            .admit_runtime_baseline(&f.owner, &f.supervisor)
            .unwrap(),
        view
    );
    assert_eq!(
        f.journal
            .complete_runtime_baseline(&f.owner, &f.supervisor, &receipt)
            .unwrap(),
        view
    );
    assert_eq!(begin(&mut f).unwrap().phase, UpdatePhase::Preparing);
    assert_eq!(
        UpdateJournal::inspect_runtime_baseline(&f.journal.directory, &i.update_id).unwrap(),
        view
    );
}
#[test]
fn baseline_ignored_intent_and_receipt_writes_roll_back_parent() {
    for table in ["runtime_baseline_intents", "runtime_baselines"] {
        let mut f = Fixture::new();
        let i = if table == "runtime_baselines" {
            Some(f.intent())
        } else {
            None
        };
        let before = f.journal.get(&f.supervisor.spec.update_id).unwrap();
        f.journal.connection.execute_batch(&format!("CREATE TRIGGER ignore_baseline BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END;")).unwrap();
        if let Some(i) = i {
            let r = f.receipt(&i);
            assert!(f
                .journal
                .complete_runtime_baseline(&f.owner, &f.supervisor, &r)
                .is_err());
        } else {
            assert!(f
                .journal
                .admit_runtime_baseline(&f.owner, &f.supervisor)
                .is_err());
        }
        assert_eq!(f.journal.get(&f.supervisor.spec.update_id).unwrap(), before);
        assert!(f
            .journal
            .runtime_baseline(&before.update_id)
            .unwrap()
            .receipt
            .is_none());
    }
}
#[test]
fn baseline_never_adopts_late_source_attempt_or_raw_lifecycle_event() {
    let mut f = Fixture::new();
    let id = f.supervisor.spec.update_id.clone();
    assert!(f
        .journal
        .advance(
            &f.owner,
            &id,
            &UpdateCommand {
                idempotency_key: "raw".into(),
                expected_revision: 0,
                event: UpdateEvent::RuntimeBaselineAdmitted,
                reason: "Raw baseline bypass".into()
            }
        )
        .is_err());
    begin(&mut f).unwrap();
    assert!(f
        .journal
        .admit_runtime_baseline(&f.owner, &f.supervisor)
        .is_err());
    assert!(f.journal.runtime_baseline(&id).unwrap().intent.is_none());
}
#[test]
fn baseline_loaded_peer_and_set_forgery_cannot_commit_a_receipt() {
    let mut f = Fixture::new();
    let i = f.intent();
    let original = f.receipt(&i);
    for field in [
        "process",
        "set",
        "image",
        "build",
        "duplicate",
        "manifest",
        "root",
    ] {
        let mut r = original.clone();
        match field {
            "process" => r.loaded_worker.process.birth.push('0'),
            "set" => r.loaded_worker.set_id = Uuid::new_v4().to_string(),
            "image" => r.loaded_worker.image.sha256 = "0".repeat(64),
            "build" => r.loaded_worker.descriptor.build.build_id.push('x'),
            "duplicate" => {
                r.installed.images[1].file_identity = r.installed.images[0].file_identity.clone()
            }
            "manifest" => r.installed.manifest_file.sha256 = "0".repeat(64),
            "root" => {
                // Keep all returned paths internally consistent; the frozen
                // supervisor root must still prevent adopting another runtime.
                r.installed.runtime_directory.push_str("-other");
                r.installed.manifest_file.path = Path::new(&r.installed.runtime_directory)
                    .join(MANIFEST_NAME)
                    .to_string_lossy()
                    .into();
                for (image, artifact) in r
                    .installed
                    .images
                    .iter_mut()
                    .zip(&r.installed.manifest.artifacts)
                {
                    image.path = Path::new(&r.installed.runtime_directory)
                        .join(&artifact.name)
                        .to_string_lossy()
                        .into();
                }
                r.loaded_worker.image = r.installed.images[0].clone();
            }
            _ => unreachable!(),
        }
        assert!(
            f.journal
                .complete_runtime_baseline(&f.owner, &f.supervisor, &r)
                .is_err(),
            "{field}"
        );
        assert_eq!(f.journal.get(&i.update_id).unwrap().revision, 1);
        assert!(f
            .journal
            .runtime_baseline(&i.update_id)
            .unwrap()
            .receipt
            .is_none());
    }
    f.journal
        .complete_runtime_baseline(&f.owner, &f.supervisor, &original)
        .unwrap();
}

#[test]
fn baseline_owner_restart_keeps_history_but_cannot_complete_the_old_intent() {
    let mut f = Fixture::new();
    let i = f.intent();
    let receipt = f.receipt(&i);
    let old_epoch = f.owner.epoch();
    drop(f.owner);
    let owner = f.journal.acquire_owner().unwrap();
    assert_eq!(owner.epoch(), old_epoch + 1);
    assert!(f
        .journal
        .complete_runtime_baseline(&owner, &f.supervisor, &receipt)
        .is_err());
    let view = f.journal.runtime_baseline(&i.update_id).unwrap();
    assert_eq!(view.intent, Some(i));
    assert!(view.receipt.is_none());
}

#[test]
fn baseline_retention_and_transition_corruption_are_detected() {
    let mut f = Fixture::new();
    let i = f.intent();
    let receipt = f.receipt(&i);
    f.journal
        .complete_runtime_baseline(&f.owner, &f.supervisor, &receipt)
        .unwrap();
    for table in ["runtime_baseline_intents", "runtime_baselines"] {
        assert!(f
            .journal
            .connection
            .execute_batch(&format!("DELETE FROM {table}"))
            .is_err());
        assert!(f
            .journal
            .connection
            .execute_batch(&format!("UPDATE {table} SET record='{{}}'"))
            .is_err());
    }
    let (raw, revision): (String, i64) = f
        .journal
        .connection
        .query_row(
            "SELECT transition,revision FROM transitions WHERE update_id=?1 AND command_key=?2",
            params![&i.update_id, RECEIPT_KEY],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut t: UpdateTransition = decode(&raw).unwrap();
    t.owner_epoch += 1;
    f.journal
        .connection
        .execute(
            "UPDATE transitions SET transition=?1 WHERE update_id=?2 AND revision=?3",
            params![encode(&t).unwrap(), &i.update_id, revision],
        )
        .unwrap();
    assert_eq!(
        f.journal.runtime_baseline(&i.update_id).unwrap_err().code,
        JournalErrorCode::Corrupt
    );
}

#[test]
fn schema_seven_migration_preserves_old_history_without_inventing_baseline() {
    let mut f = Fixture::new();
    let id = f.supervisor.spec.update_id.clone();
    let before = f.journal.get(&id).unwrap();
    f.journal.connection.execute_batch("DROP TABLE IF EXISTS activation_finalization_history;DROP TABLE IF EXISTS activation_finalization_records;DROP TABLE IF EXISTS activation_finalization_intents;DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history; DROP TABLE activation_records; DROP TABLE activation_intents; DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; DROP TABLE shutdown_history; DROP TABLE shutdown_records; DROP TABLE shutdown_intents; DROP TABLE drain_history; DROP TABLE drain_commands; DROP TABLE drain_intents; DROP TABLE candidate_health; DROP TABLE runtime_preimages; DROP TABLE runtime_preimage_intents; DROP TABLE runtime_baselines;DROP TABLE runtime_baseline_intents;PRAGMA user_version=7;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let directory = f.journal.directory.clone();
    let old = UpdateJournal::inspect(&directory).unwrap();
    assert!(UpdateJournal::open(&directory, &old.scope_sha256).is_err());
    drop(f.owner);
    drop(f.journal);
    let mut journal = UpdateJournal::open(&directory, &old.scope_sha256).unwrap();
    assert_eq!(
        journal.info().unwrap().schema_version,
        UPDATE_SCHEMA_VERSION
    );
    assert_eq!(journal.info().unwrap().owner_epoch, old.owner_epoch + 1);
    assert_eq!(journal.get(&id).unwrap(), before);
    let view = UpdateJournal::inspect_runtime_baseline(&directory, &id).unwrap();
    assert!(view.intent.is_none() && view.receipt.is_none());
    let backup = fs::read_dir(&directory)
        .unwrap()
        .map(|r| r.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v7-")
        })
        .unwrap();
    let db = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[path = "runtime_preimages_tests.rs"]
mod preimages;

#[test]
fn health_acceptance_hook_checks_new_atomic_parent_and_rolls_back_wrong_authority() {
    use crate::health::{HealthIntent, HealthRecord};
    // Test the actual transaction/hook ordering and its fences, not executable
    // health. Complete approval still requires the Windows full-pipeline gate.
    let mut f = Fixture::new();
    let id = f.supervisor.spec.update_id.clone();
    for (n, event) in [
        UpdateEvent::BeginPreparation,
        UpdateEvent::SourceVerified,
        UpdateEvent::BuildSucceeded,
    ]
    .into_iter()
    .enumerate()
    {
        let current = f.journal.get(&id).unwrap();
        f.journal
            .advance(
                &f.owner,
                &id,
                &UpdateCommand {
                    idempotency_key: format!("hook-fixture-{n}"),
                    expected_revision: current.revision,
                    event,
                    reason: "Metadata-only hook fixture".into(),
                },
            )
            .unwrap();
    }
    let before = f.journal.get(&id).unwrap();
    let pending = HealthRecord {
        schema_version: 1,
        revision: 0,
        context: None,
        probes: vec![],
        result: None,
        intent: HealthIntent {
            schema_version: 1,
            update_id: id.clone(),
            attempt_id: Uuid::new_v4().to_string(),
            launch_id: f.supervisor.spec.launch_id.clone(),
            owner_id: f.owner.owner_id().into(),
            owner_epoch: f.owner.epoch(),
            parent_revision: before.revision,
            admitted_at_ms: now_ms().unwrap(),
            artifacts_sha256: "a".repeat(64),
            preimages_sha256: "b".repeat(64),
            directory: "metadata-only".into(),
            worker_config_path: "metadata-only.json".into(),
        },
    };
    assert!(super::super::health::require_acceptance(
        &f.journal.connection,
        &f.owner,
        &f.supervisor,
        &pending,
        &before
    )
    .is_err());
    let command = UpdateCommand {
        idempotency_key: "candidate-health-verified-v1".into(),
        expected_revision: before.revision,
        event: UpdateEvent::CandidateAccepted,
        reason: "Metadata-only atomic parent edge".into(),
    };
    let sequence = f.journal.info().unwrap().last_sequence;
    for fault in ["owner", "epoch", "parent", "launch", "key"] {
        let mut h = pending.clone();
        let mut cmd = command.clone();
        match fault {
            "owner" => h.intent.owner_id = Uuid::new_v4().to_string(),
            "epoch" => h.intent.owner_epoch += 1,
            "parent" => h.intent.parent_revision += 1,
            "launch" => h.intent.launch_id = Uuid::new_v4().to_string(),
            "key" => cmd.idempotency_key = "different-acceptance-key".into(),
            _ => unreachable!(),
        }
        assert!(
            f.journal
                .advance_with_driver(&f.owner, &id, &cmd, Driver::Health, |c, parent| {
                    super::super::health::require_acceptance(c, &f.owner, &f.supervisor, &h, parent)
                })
                .is_err(),
            "{fault}"
        );
        assert_eq!(f.journal.get(&id).unwrap(), before);
        assert_eq!(f.journal.info().unwrap().last_sequence, sequence);
    }
    let accepted = f
        .journal
        .advance_with_driver(&f.owner, &id, &command, Driver::Health, |c, parent| {
            assert_eq!(parent.phase, UpdatePhase::CandidateVerified);
            super::super::health::require_acceptance(c, &f.owner, &f.supervisor, &pending, parent)
        })
        .unwrap();
    assert_eq!(accepted.phase, UpdatePhase::CandidateVerified);
    assert_eq!(accepted.revision, before.revision + 1);
    assert_eq!(f.journal.info().unwrap().last_sequence, sequence + 1);
}
#[test]
fn driver_transition_proof_accepts_both_health_edges_and_rejects_cross_phase_key_owner_epoch() {
    // Retained transition rows only: no artifact health or executable approval is claimed.
    let mut f = Fixture::new();
    let id = f.supervisor.spec.update_id.clone();
    for (n, event) in [
        UpdateEvent::BeginPreparation,
        UpdateEvent::SourceVerified,
        UpdateEvent::BuildSucceeded,
    ]
    .into_iter()
    .enumerate()
    {
        let r = f.journal.get(&id).unwrap();
        f.journal
            .advance(
                &f.owner,
                &id,
                &UpdateCommand {
                    idempotency_key: format!("metadata-{n}"),
                    expected_revision: r.revision,
                    event,
                    reason: "Metadata-only transition fixture".into(),
                },
            )
            .unwrap();
    }
    for (event, key) in [
        (
            UpdateEvent::CandidateHealthAdmitted,
            "health-intent-fixture",
        ),
        (UpdateEvent::CandidateAccepted, "health-result-fixture"),
    ] {
        let before = f.journal.get(&id).unwrap();
        let after = f
            .journal
            .advance_with_driver(
                &f.owner,
                &id,
                &UpdateCommand {
                    idempotency_key: key.into(),
                    expected_revision: before.revision,
                    event,
                    reason: "Metadata-only health transition proof fixture".into(),
                },
                Driver::Health,
                |_, _| Ok(()),
            )
            .unwrap();
        assert_eq!(
            transition(
                &f.journal.connection,
                &id,
                after.revision,
                event,
                key,
                f.owner.owner_id(),
                f.owner.epoch()
            )
            .unwrap(),
            after.updated_at_ms
        );
        for (wrong_event, wrong_key, wrong_owner, wrong_epoch) in [
            (
                UpdateEvent::RuntimeBaselineAdmitted,
                key,
                f.owner.owner_id(),
                f.owner.epoch(),
            ),
            (event, "different-key", f.owner.owner_id(), f.owner.epoch()),
            (event, key, "different-owner", f.owner.epoch()),
            (event, key, f.owner.owner_id(), f.owner.epoch() + 1),
            (UpdateEvent::Fail, key, f.owner.owner_id(), f.owner.epoch()),
        ] {
            assert!(transition(
                &f.journal.connection,
                &id,
                after.revision,
                wrong_event,
                wrong_key,
                wrong_owner,
                wrong_epoch
            )
            .is_err());
        }
    }
}
