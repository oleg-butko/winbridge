use super::*;
use winbridge_platform::infrastructure;
use winbridge_protocol::ResolvedCommand;
use winbridge_update::build::{
    admit_build, execute_build, BuildAdmission, BuildControl, MAX_OUTPUT_BYTES,
};

impl BuildControl for Control {}
#[path = "compiler_report_cases.rs"]
mod report;
// Compiler tests use the local Cargo volume, not a potentially small /tmp tmpfs.
fn temporary() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::current_exe().unwrap().parent().unwrap()).unwrap()
}
fn command(f: &Fixture, mode: &str) -> ResolvedCommand {
    ResolvedCommand {
        executable: std::env::current_exe()
            .unwrap()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        arguments: vec![
            "--exact".into(),
            "compiler::compiler_process_helper".into(),
            "--ignored".into(),
            "--nocapture".into(),
            "--skip".into(),
            mode.into(),
        ],
        working_directory: f
            .config
            .source_directory
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        timeout_ms: if mode == "tree" { 1500 } else { 20000 },
    }
}
fn ready(root: &Path) -> Fixture {
    let mut f = Fixture::new(root);
    f.prepare();
    f.apply().unwrap();
    f
}
fn log(r: &UpdateBuildRecord, stream: &str) -> PathBuf {
    Path::new(&r.spec.runtime_directory)
        .join("update")
        .join(&r.spec.update_id)
        .join("compiler")
        .join(&r.spec.attempt_id)
        .join(format!("{stream}.bin"))
}
#[test]
#[ignore = "Owned compiler process fixture"]
fn compiler_process_helper() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args
        .windows(2)
        .find(|p| p[0] == "--skip")
        .map(|p| p[1].as_str())
        .unwrap();
    #[cfg(unix)]
    if let Some(journal) = args.iter().find_map(|a| a.strip_prefix("report-journal=")) {
        let update = args
            .iter()
            .find_map(|a| a.strip_prefix("report-update="))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let record = UpdateJournal::inspect_build(Path::new(journal), update).unwrap();
            if record.phase == UpdateBuildPhase::Running {
                break;
            }
            assert!(
                matches!(
                    record.phase,
                    UpdateBuildPhase::Intent
                        | UpdateBuildPhase::Spawned
                        | UpdateBuildPhase::ResumeGranted
                ),
                "Unexpected fixture parent state: {record:?}"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "Fixture never received Running receipt"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let mut counter = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("target/compiler-runs.log")
        .unwrap();
    writeln!(counter, "{} {mode}", std::process::id()).unwrap();
    drop(counter);
    match mode {
        "synthetic-candidate" => {
            let path = args
                .windows(2)
                .find_map(|p| {
                    (p[0] == "--skip")
                        .then(|| p[1].strip_prefix("report-file="))
                        .flatten()
                })
                .unwrap();
            let path = Path::new(path);
            let stage = path.parent().unwrap();
            fs::create_dir(stage).unwrap();
            let payload = stage.join("payload");
            fs::create_dir(&payload).unwrap();
            let source = std::env::current_dir().unwrap().canonicalize().unwrap();
            let sha = sha256(&fs::read(source.join("winbridge-source-manifest.json")).unwrap());
            let verified = winbridge_update::verify_source(&source, &sha).unwrap();
            let build = winbridge_protocol::BuildIdentity {
                version: "0.1.0".into(),
                source_commit: Some(verified.source_commit.clone()),
                source_dirty: false,
                source_manifest_sha256: Some(sha.clone()),
                build_id: "synthetic-compiler-contract-fixture".into(),
            };
            let mut artifacts = vec![];
            let mut queries = vec![];
            for (name, role) in [
                (
                    "winbridge-worker.exe",
                    winbridge_protocol::ArtifactRole::Worker,
                ),
                (
                    "winbridge-fixture.exe",
                    winbridge_protocol::ArtifactRole::Fixture,
                ),
                (
                    "winbridge-desktop.exe",
                    winbridge_protocol::ArtifactRole::Desktop,
                ),
                (
                    "winbridge-update.exe",
                    winbridge_protocol::ArtifactRole::Updater,
                ),
            ] {
                let mut bytes = b"synthetic artifact; no executable health proof".to_vec();
                bytes.extend_from_slice(winbridge_protocol::ARTIFACT_QUERY_MARKER);
                bytes.extend_from_slice(name.as_bytes());
                fs::write(payload.join(name), bytes).unwrap();
                let image = winbridge_update::artifact::inspect_image(&payload.join(name)).unwrap();
                artifacts.push(
                    serde_json::json!({"name":name,"bytes":image.bytes,"sha256":image.sha256}),
                );
                queries.push(serde_json::json!({"name":name,"query":{"schemaVersion":1,"kind":"artifact_query","readOnly":true,"candidateVerified":false,"image":image,"descriptor":{"schemaVersion":1,"kind":"winbridge_artifact","queryContract":1,"role":role,"build":build,"targetOs":std::env::consts::OS,"targetArch":std::env::consts::ARCH},"process":infrastructure::stamp(std::process::id()).unwrap(),"cleanupVerified":true}}));
            }
            let report = serde_json::json!({"schemaVersion":1,"kind":"winbridge-update-candidate","status":"built","runtimeApplied":false,"candidateVerified":false,"sourceDirectory":source,"manifestSha256":sha,"storage":{},"commands":[{"name":"synthetic-contract-fixture","executable":std::env::current_exe().unwrap(),"arguments":[],"exitCode":0,"log":"fixture.log"}],"artifacts":artifacts,"error":null,"source":verified,"workerBuild":build,"artifactIdentities":queries,"artifactIdentityChecked":true});
            fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
            std::process::exit(0);
        }
        "report" | "empty-report" | "oversized-report" => {
            let path = args
                .windows(2)
                .find_map(|p| {
                    (p[0] == "--skip")
                        .then(|| p[1].strip_prefix("report-file="))
                        .flatten()
                })
                .unwrap();
            let path = Path::new(path);
            fs::create_dir(path.parent().unwrap()).unwrap();
            let bytes = match mode {
                "empty-report" => vec![],
                "oversized-report" => {
                    vec![b' '; winbridge_update::build_report::MAX_REPORT_BYTES as usize + 1]
                }
                _ => br#"{"status":"built","candidateVerified":false,"runtimeApplied":false}"#
                    .to_vec(),
            };
            fs::write(path, bytes).unwrap();
            std::process::exit(0);
        }
        "live" => {
            println!("live compiler Привет ✓");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_millis(1500));
            std::process::exit(0);
        }
        "success" => {
            std::thread::sleep(Duration::from_millis(50));
            println!("compiler stdout Привет ✓");
            eprintln!("compiler diagnostic");
            std::process::exit(0);
        }
        "failed" => {
            std::thread::sleep(Duration::from_millis(50));
            std::process::exit(7);
        }
        "quota" => {
            let bytes = vec![b'q'; 1024 * 1024];
            let mut out = std::io::stdout().lock();
            let mut err = std::io::stderr().lock();
            for _ in 0..20 {
                out.write_all(&bytes).unwrap();
                err.write_all(&bytes).unwrap();
            }
            std::process::exit(0);
        }
        "tree" => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "compiler::compiler_process_helper",
                    "--ignored",
                    "--nocapture",
                    "--skip",
                    "long",
                ])
                .spawn()
                .unwrap();
            println!(
                "compiler-child {}",
                serde_json::to_string(&infrastructure::stamp(child.id()).unwrap()).unwrap()
            );
            let _ = child.wait();
        }
        "long" => std::thread::sleep(Duration::from_secs(60)),
        _ => panic!("Unknown compiler fixture mode"),
    }
}
#[test]
fn compiler_success_receipts_output_and_duplicate_never_respawn() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build-once",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    assert_eq!(r.phase, UpdateBuildPhase::Completed);
    assert_eq!(r.outcome, Some(UpdateBuildOutcome::Succeeded));
    assert_eq!(r.exit_code, Some(0));
    assert!(r.cleanup_verified);
    let bytes = fs::read(log(&r, "stdout")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("Привет ✓"));
    assert_eq!(r.stdout.as_ref().unwrap().sha256, sha256(&bytes));
    assert_eq!(
        r.stdout.as_ref().unwrap().retained_bytes,
        bytes.len() as u64
    );
    assert!(r.spec.source_available_bytes_before >= 6 * 1024 * 1024 * 1024);
    assert_eq!(f.journal.get(&f.id).unwrap().phase, UpdatePhase::Building); // exit 0 is not candidate approval
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build-once",
            &cmd,
            &mut f.control
        )
        .unwrap(),
        r
    );
    let mut other = cmd.clone();
    other.arguments.push("changed".into());
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build-once",
        &other,
        &mut f.control
    )
    .is_err());
    assert!(f
        .journal
        .advance(
            &f.owner,
            &f.id,
            &UpdateCommand {
                idempotency_key: "forged-success".into(),
                expected_revision: 3,
                event: UpdateEvent::BuildSucceeded,
                reason: "Raw metadata cannot approve compiler".into()
            }
        )
        .is_err());
    assert!(f
        .config
        .runtime_directory
        .join("state")
        .join(winbridge_worker::UPDATE_STARTUP_GUARD)
        .exists());
    let lease: WorkerUpdateLease = serde_json::from_value(
        info(&f.control.worker).update_coordination.unwrap()["lease"].clone(),
    )
    .unwrap();
    assert_eq!(lease.phase, WorkerUpdatePhase::SourceHeld);
    let inspection = winbridge_update::build::inspect_build(
        &f.config.runtime_directory.join("update-state"),
        &f.id,
    )
    .unwrap();
    assert!(inspection.read_only && !inspection.can_replay);
    if let Some(path) = std::env::var_os("WINBRIDGE_UPDATE_BUILD_REPORT") {
        fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion":1,"kind":"update_build_fixture_proof","passed":true,
                "platform":std::env::consts::OS,"build":winbridge_protocol::build_identity(),
                "record":r,"inspection":inspection,"workerLease":lease,
                "compilationFixture":true,"duplicateObserved":true,"workerLeaseRetained":true,
                "runtimeApplied":false,"candidateVerified":false,"windowsOpened":false,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

struct Edited<'a> {
    control: &'a mut Control,
    path: PathBuf,
}
impl SourceControl for Edited<'_> {
    fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
        self.control.observe_worker()
    }
}
impl BuildControl for Edited<'_> {
    fn build_progress(&mut self, r: &UpdateBuildRecord) -> Result<(), String> {
        if r.phase == UpdateBuildPhase::Spawned {
            fs::write(&self.path, b"late local edit").unwrap();
        }
        Ok(())
    }
}
#[test]
fn compiler_source_changed_after_pid_receipt_never_gets_windows_resume() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let mut c = Edited {
        control: &mut f.control,
        path: f.config.source_directory.join("Cargo.toml"),
    };
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut c,
    )
    .unwrap();
    assert_eq!(r.phase, UpdateBuildPhase::Indeterminate);
    assert!(!r.cleanup_verified);
    assert!(r.reason.contains("final source/config/output proof failed"));
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut c
        )
        .unwrap(),
        r
    );
    #[cfg(windows)]
    assert_eq!(fs::metadata(log(&r, "stdout")).unwrap().len(), 0);
}
#[test]
fn compiler_failure_and_timeout_have_verified_owned_tree_cleanup() {
    for mode in ["failed", "tree"] {
        let tmp = temporary();
        let mut f = ready(tmp.path());
        let cmd = command(&f, mode);
        let r = execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut f.control,
        )
        .unwrap();
        assert_eq!(r.phase, UpdateBuildPhase::Completed);
        assert!(r.cleanup_verified);
        if mode == "failed" {
            assert_eq!(r.outcome, Some(UpdateBuildOutcome::Failed));
            assert_eq!(r.exit_code, Some(7));
        } else {
            assert_eq!(r.outcome, Some(UpdateBuildOutcome::TimedOut));
            let out = String::from_utf8(fs::read(log(&r, "stdout")).unwrap()).unwrap();
            let json = out
                .lines()
                .find_map(|l| l.strip_prefix("compiler-child "))
                .expect("Child was launched before timeout");
            let child: SupervisorProcess = serde_json::from_str(json).unwrap();
            assert!(!infrastructure::matches(&child).unwrap_or(false));
        }
        assert_eq!(
            execute_build(
                &mut f.journal,
                &f.owner,
                &f.config,
                &f.id,
                "build",
                &cmd,
                &mut f.control
            )
            .unwrap(),
            r
        );
    }
}
#[test]
fn compiler_output_quota_drains_both_pipes_without_blocking() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "quota");
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    assert_eq!(r.outcome, Some(UpdateBuildOutcome::Succeeded));
    for (name, proof) in [
        ("stdout", r.stdout.as_ref().unwrap()),
        ("stderr", r.stderr.as_ref().unwrap()),
    ] {
        assert_eq!(proof.retained_bytes, MAX_OUTPUT_BYTES);
        assert!(proof.observed_bytes > MAX_OUTPUT_BYTES);
        let bytes = fs::read(log(&r, name)).unwrap();
        assert_eq!(bytes.len() as u64, MAX_OUTPUT_BYTES);
        assert_eq!(proof.sha256, sha256(&bytes));
    }
}

#[test]
fn compiler_readonly_inspection_rejects_corrupt_old_receipt_bindings() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    let db = f.db();
    db.execute_batch("DROP TRIGGER build_receipt_immutable;")
        .unwrap();
    let original: String = db
        .query_row(
            "SELECT record FROM update_build_receipts WHERE id=?1 AND revision=0",
            [&r.spec.attempt_id],
            |r| r.get(0),
        )
        .unwrap();
    let mut damaged: UpdateBuildRecord = serde_json::from_str(&original).unwrap();
    damaged.spec.owner_id = uuid::Uuid::new_v4().to_string();
    damaged.spec.worker_source_spec.updater_owner_id = damaged.spec.owner_id.clone();
    db.execute(
        "UPDATE update_build_receipts SET record=?1 WHERE id=?2 AND revision=0",
        rusqlite::params![serde_json::to_string(&damaged).unwrap(), r.spec.attempt_id],
    )
    .unwrap();
    assert!(
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .is_err()
    );
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control
    )
    .is_err());
}
struct Fault<'a> {
    control: &'a mut Control,
    db: PathBuf,
    phase: UpdateBuildPhase,
    sql: &'static str,
    fired: bool,
}
impl SourceControl for Fault<'_> {
    fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
        self.control.observe_worker()
    }
}
impl BuildControl for Fault<'_> {
    fn build_progress(&mut self, r: &UpdateBuildRecord) -> Result<(), String> {
        if r.phase == self.phase && !self.fired {
            self.fired = true;
            Connection::open(&self.db)
                .unwrap()
                .execute_batch(self.sql)
                .unwrap();
        }
        Ok(())
    }
}
#[test]
fn compiler_ignored_intent_and_pid_receipt_never_authorize_replay() {
    for table in ["update_builds", "update_build_receipts"] {
        let tmp = temporary();
        let mut f = ready(tmp.path());
        let cmd = command(&f, "success");
        f.db().execute_batch(&format!("CREATE TRIGGER compiler_fault BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END;")).unwrap();
        assert!(execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut f.control
        )
        .is_err());
        assert!(UpdateJournal::inspect_build(
            &f.config.runtime_directory.join("update-state"),
            &f.id
        )
        .is_err());
        assert!(!f
            .config
            .runtime_directory
            .join("update")
            .join(&f.id)
            .join("compiler")
            .exists());
    }
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let mut c=Fault {control:&mut f.control,db:f.config.runtime_directory.join("update-state/updates.sqlite"),phase:UpdateBuildPhase::Intent,
        sql:"CREATE TRIGGER compiler_fault BEFORE UPDATE ON update_builds BEGIN SELECT RAISE(IGNORE); END;",fired:false};
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut c
    )
    .is_err());
    let r = UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
        .unwrap();
    assert_eq!(r.phase, UpdateBuildPhase::Intent);
    assert!(r.process.is_none());
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut c
        )
        .unwrap(),
        r
    );
    #[cfg(windows)]
    assert_eq!(fs::metadata(log(&r, "stdout")).unwrap().len(), 0);
}
#[test]
fn compiler_ignored_resume_receipt_cleans_suspended_process_and_keeps_history() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "long");
    let mut c=Fault {control:&mut f.control,db:f.config.runtime_directory.join("update-state/updates.sqlite"),phase:UpdateBuildPhase::Spawned,
        sql:"CREATE TRIGGER compiler_fault BEFORE INSERT ON update_build_receipts BEGIN SELECT RAISE(IGNORE); END;",fired:false};
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut c
    )
    .is_err());
    let r = UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
        .unwrap();
    assert_eq!(r.phase, UpdateBuildPhase::Spawned);
    assert!(!infrastructure::matches(r.process.as_ref().unwrap()).unwrap_or(false));
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut c
        )
        .unwrap(),
        r
    );
    #[cfg(windows)]
    assert_eq!(fs::metadata(log(&r, "stdout")).unwrap().len(), 0);
}

#[test]
fn compiler_lost_actual_start_receipt_keeps_resume_uncertain_and_never_replays() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "long");
    let mut c=Fault {control:&mut f.control,db:f.config.runtime_directory.join("update-state/updates.sqlite"),phase:UpdateBuildPhase::ResumeGranted,
        sql:"CREATE TRIGGER compiler_fault BEFORE INSERT ON update_build_receipts BEGIN SELECT RAISE(IGNORE); END;",fired:false};
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut c
    )
    .is_err());
    let saved =
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .unwrap();
    assert_eq!(saved.phase, UpdateBuildPhase::ResumeGranted);
    assert!(!saved.cleanup_verified);
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut c
        )
        .unwrap(),
        saved
    );
    assert!(f
        .config
        .runtime_directory
        .join("state")
        .join(winbridge_worker::UPDATE_STARTUP_GUARD)
        .exists());
}

#[test]
fn compiler_source_edits_config_changes_and_owner_restart_do_not_start_new_effects() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let original = fs::read(f.config.source_directory.join("Cargo.toml")).unwrap();
    fs::write(f.config.source_directory.join("Cargo.toml"), b"local edit").unwrap();
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control
    )
    .is_err());
    assert!(
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .is_err()
    );
    fs::write(f.config.source_directory.join("Cargo.toml"), original).unwrap();
    let bytes = fs::read(&f.config.worker_config_path).unwrap();
    let mut changed = bytes.clone();
    changed.push(b'\n');
    fs::write(&f.config.worker_config_path, changed).unwrap();
    assert!(execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control
    )
    .is_err());
    fs::write(&f.config.worker_config_path, bytes).unwrap();
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    drop(f.owner);
    let owner = f.journal.acquire_owner().unwrap();
    assert!(owner.epoch() > r.spec.owner_epoch);
    assert_eq!(
        execute_build(
            &mut f.journal,
            &owner,
            &f.config,
            &f.id,
            "build",
            &cmd,
            &mut f.control
        )
        .unwrap(),
        r
    );
    assert_eq!(
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .unwrap(),
        r
    );
}

#[test]
fn schema_three_migration_backs_up_history_and_does_not_rebind_source_owner() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let directory = f.config.runtime_directory.join("update-state");
    f.db().execute_batch("DROP TABLE IF EXISTS activation_lifetime_history;DROP TABLE IF EXISTS activation_lifetime_records;DROP TABLE IF EXISTS activation_lifetime_intents;DROP TABLE IF EXISTS activation_controller_history;DROP TABLE IF EXISTS activation_controller_records;DROP TABLE IF EXISTS activation_controller_intents;DROP TABLE activation_service_history;DROP TABLE activation_service_records;DROP TABLE activation_service_intents;DROP TABLE activation_writer_history;DROP TABLE activation_writer_records;DROP TABLE activation_writer_intents;DROP TABLE activation_history; DROP TABLE activation_records; DROP TABLE activation_intents; DROP TABLE compatible_drain_records; DROP TABLE candidate_support_history; DROP TABLE candidate_support_records; DROP TABLE candidate_support_intents; DROP TABLE runtime_replacement_history; DROP TABLE runtime_replacement_records; DROP TABLE runtime_replacement_intents; DROP TABLE runtime_preparation_history; DROP TABLE runtime_preparation_records; DROP TABLE runtime_preparation_intents; DROP TABLE state_backup_history; DROP TABLE state_backup_records; DROP TABLE state_backup_intents; DROP TABLE shutdown_history; DROP TABLE shutdown_records; DROP TABLE shutdown_intents; DROP TABLE drain_history; DROP TABLE drain_commands; DROP TABLE drain_intents; DROP TABLE candidate_health; DROP TABLE runtime_preimages; DROP TABLE runtime_preimage_intents; DROP TABLE runtime_baselines; DROP TABLE runtime_baseline_intents; DROP TABLE candidate_artifact_sets; DROP TABLE update_build_reports; DROP TABLE update_build_report_intents; DROP TABLE update_abort_receipts; DROP TABLE update_aborts; DROP TABLE update_build_receipts; DROP TABLE update_builds; PRAGMA user_version=3; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let old = UpdateJournal::inspect(&directory).unwrap();
    assert_eq!(old.schema_version, 3);
    assert!(UpdateJournal::open(&directory, &old.scope_sha256).is_err()); // owner lock still held
    assert_eq!(UpdateJournal::inspect(&directory).unwrap(), old);
    drop(f.owner);
    drop(f.journal);
    let mut j = UpdateJournal::open(&directory, &old.scope_sha256).unwrap();
    assert_eq!(
        j.info().unwrap().schema_version,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    );
    assert!(j.info().unwrap().owner_epoch > old.owner_epoch);
    let backup = fs::read_dir(&directory)
        .unwrap()
        .map(|v| v.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v3-")
        })
        .unwrap();
    let db =
        Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM transitions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let owner = j.acquire_owner().unwrap();
    assert!(execute_build(
        &mut j,
        &owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control
    )
    .is_err());
    assert!(UpdateJournal::inspect_build(&directory, &f.id).is_err());
}

#[cfg(windows)]
struct Crash<'a>(&'a mut Control);
#[cfg(windows)]
impl SourceControl for Crash<'_> {
    fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
        self.0.observe_worker()
    }
}
#[cfg(windows)]
impl BuildControl for Crash<'_> {
    fn build_progress(&mut self, r: &UpdateBuildRecord) -> Result<(), String> {
        if r.phase == UpdateBuildPhase::Running {
            std::process::exit(83)
        }
        Ok(())
    }
}
#[cfg(windows)]
#[test]
#[ignore = "Owned compiler owner-crash helper"]
fn compiler_crash_helper() {
    let root = PathBuf::from(std::env::var_os("WINBRIDGE_COMPILER_CRASH_ROOT").unwrap());
    let mut f = ready(&root);
    let cmd = command(&f, "long");
    let mut c = Crash(&mut f.control);
    execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut c,
    )
    .unwrap();
    panic!("Crash checkpoint not reached");
}
#[cfg(windows)]
#[test]
fn compiler_owner_crash_keeps_durable_attempt_and_never_replays_it() {
    let tmp = temporary();
    let exit = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "compiler::compiler_crash_helper",
            "--ignored",
            "--nocapture",
        ])
        .env("WINBRIDGE_COMPILER_CRASH_ROOT", tmp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(exit.code(), Some(83));
    let cfg = winbridge_update::load_config(&tmp.path().join("update.local.json")).unwrap();
    let dir = cfg.runtime_directory.join("update-state");
    let info = UpdateJournal::inspect(&dir).unwrap();
    let id = info.active_update_id.unwrap();
    let saved = UpdateJournal::inspect_build(&dir, &id).unwrap();
    assert_eq!(saved.phase, UpdateBuildPhase::Running);
    assert!(!saved.cleanup_verified);
    let until = Instant::now() + Duration::from_secs(5);
    while infrastructure::matches(saved.process.as_ref().unwrap()).unwrap_or(false) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    let inspection = winbridge_update::build::inspect_build(&dir, &id).unwrap();
    assert_eq!(inspection.physical_kind, "empty");
    assert!(!inspection.can_replay);
    struct Offline;
    impl SourceControl for Offline {
        fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
            panic!("Duplicate must not need a new grant")
        }
    }
    impl BuildControl for Offline {}
    let mut j = UpdateJournal::open(&dir, &info.scope_sha256).unwrap();
    let owner = j.acquire_owner().unwrap();
    assert_eq!(
        execute_build(
            &mut j,
            &owner,
            &cfg,
            &id,
            "build",
            &saved.spec.command,
            &mut Offline
        )
        .unwrap(),
        saved
    );
    assert!(cfg
        .runtime_directory
        .join("state")
        .join(winbridge_worker::UPDATE_STARTUP_GUARD)
        .exists());
}

#[test]
fn compiler_admission_commits_intent_before_dispatch_and_never_mints_a_second_grant() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let BuildAdmission::New(permit) = admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "admitted-build",
        &cmd,
    )
    .unwrap() else {
        panic!("Fresh compiler permission")
    };
    assert_eq!(permit.record().phase, UpdateBuildPhase::Intent);
    assert!(permit.record().process.is_none());
    let saved =
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .unwrap();
    assert_eq!(saved, *permit.record());
    assert!(!f
        .config
        .source_directory
        .join("target/compiler-runs.log")
        .exists());
    assert!(!f
        .config
        .runtime_directory
        .join("update")
        .join(&f.id)
        .join("compiler")
        .exists());
    assert!(matches!(
        admit_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "admitted-build",
            &cmd
        )
        .unwrap(),
        BuildAdmission::Observed(_)
    ));
    let done = permit
        .execute(&mut f.journal, &f.owner, &f.config, &mut f.control)
        .unwrap();
    assert_eq!(done.outcome, Some(UpdateBuildOutcome::Succeeded));
    let observed = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "admitted-build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    assert_eq!(observed, done);
    assert_eq!(
        fs::read_to_string(f.config.source_directory.join("target/compiler-runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn dropped_compiler_permission_remains_an_observation_after_owner_restart() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let BuildAdmission::New(permit) = admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "dropped-build",
        &cmd,
    )
    .unwrap() else {
        panic!("Fresh compiler permission")
    };
    let expected = permit.record().clone();
    drop(permit);
    drop(f.owner);
    f.owner = f.journal.acquire_owner().unwrap();
    let BuildAdmission::Observed(saved) = admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "dropped-build",
        &cmd,
    )
    .unwrap() else {
        panic!("Restart cannot recreate execution permission")
    };
    assert_eq!(*saved, expected);
    assert_eq!(saved.phase, UpdateBuildPhase::Intent);
    assert_eq!(
        execute_build(
            &mut f.journal,
            &f.owner,
            &f.config,
            &f.id,
            "dropped-build",
            &cmd,
            &mut f.control
        )
        .unwrap(),
        expected
    );
    assert!(!f
        .config
        .source_directory
        .join("target/compiler-runs.log")
        .exists());
    assert!(f
        .config
        .runtime_directory
        .join("state")
        .join(winbridge_worker::UPDATE_STARTUP_GUARD)
        .exists());
}

#[test]
fn compiler_permission_rechecks_source_before_any_process_or_log_creation() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let BuildAdmission::New(permit) = admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "changed-source",
        &cmd,
    )
    .unwrap() else {
        panic!("Fresh compiler permission")
    };
    fs::write(
        f.config.source_directory.join("Cargo.toml"),
        b"preserved local edit",
    )
    .unwrap();
    assert!(permit
        .execute(&mut f.journal, &f.owner, &f.config, &mut f.control)
        .is_err());
    let saved = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "changed-source",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    assert_eq!(saved.phase, UpdateBuildPhase::Intent);
    assert!(saved.process.is_none());
    assert!(!f
        .config
        .source_directory
        .join("target/compiler-runs.log")
        .exists());
    assert!(!f
        .config
        .runtime_directory
        .join("update")
        .join(&f.id)
        .join("compiler")
        .exists());
    assert_eq!(
        fs::read(f.config.source_directory.join("Cargo.toml")).unwrap(),
        b"preserved local edit"
    );
}

#[test]
fn compiler_admission_never_treats_a_saved_source_grant_as_live_permission() {
    struct Offline;
    impl SourceControl for Offline {
        fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
            Err("Fixture worker unavailable".into())
        }
    }
    impl BuildControl for Offline {}
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let BuildAdmission::New(permit) = admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "offline-worker",
        &cmd,
    )
    .unwrap() else {
        panic!("Fresh metadata admission")
    };
    assert!(permit
        .execute(&mut f.journal, &f.owner, &f.config, &mut Offline)
        .unwrap_err()
        .contains("unavailable"));
    assert!(!f
        .config
        .source_directory
        .join("target/compiler-runs.log")
        .exists());
    assert!(!f
        .config
        .runtime_directory
        .join("update")
        .join(&f.id)
        .join("compiler")
        .exists());
}

#[test]
fn concurrent_compiler_admissions_grant_exactly_one_execution() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let directory = f.config.runtime_directory.join("update-state");
    let scope = installation_scope(&f.config).unwrap();
    let results = std::thread::scope(|threads| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let directory = &directory;
                let scope = &scope;
                let owner = &f.owner;
                let cfg = &f.config;
                let id = &f.id;
                let cmd = &cmd;
                threads.spawn(move || {
                    let mut journal = UpdateJournal::open(directory, scope).unwrap();
                    barrier.wait();
                    admit_build(&mut journal, owner, cfg, id, "racing-build", cmd)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut permits = results.into_iter().filter_map(|r| match r {
        Ok(BuildAdmission::New(p)) => Some(p),
        Ok(BuildAdmission::Observed(_)) | Err(_) => None,
    });
    let permit = permits.next().expect("One fresh commit");
    assert!(permits.next().is_none());
    let done = permit
        .execute(&mut f.journal, &f.owner, &f.config, &mut f.control)
        .unwrap();
    assert_eq!(done.outcome, Some(UpdateBuildOutcome::Succeeded));
    assert_eq!(
        fs::read_to_string(f.config.source_directory.join("target/compiler-runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

fn output_query(r: &UpdateBuildRecord, limit_bytes: u32) -> UpdateCompilerOutputRequest {
    UpdateCompilerOutputRequest {
        attempt_id: r.spec.attempt_id.clone(),
        stream: winbridge_protocol::OutputStream::Stdout,
        offset: 0,
        limit_bytes,
        file_identity: None,
    }
}
#[test]
fn compiler_output_pages_preserve_utf8_bytes_scope_and_terminal_proof() {
    use winbridge_update::build_output::read_compiler_output;
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    let before = f.journal.get(&f.id).unwrap();
    let mut q = output_query(&r, 3);
    let mut bytes = vec![];
    let mut pages = 0;
    loop {
        let page = read_compiler_output(&f.config, &f.id, &q).unwrap();
        assert!(page.read_only && page.proof_verified && page.capture_complete);
        assert!(page.bytes.len() <= 3);
        assert_eq!(page.quota_truncated, Some(false));
        bytes.extend_from_slice(&page.bytes);
        pages += 1;
        if !page.has_more {
            break;
        }
        q.offset = page.next_offset;
        q.file_identity = page.file_identity;
    }
    assert!(String::from_utf8(bytes).unwrap().contains("Привет ✓"));
    assert!(pages > 3);
    assert_eq!(f.journal.get(&f.id).unwrap(), before);
    let mut bad = output_query(&r, 0);
    assert!(read_compiler_output(&f.config, &f.id, &bad).is_err());
    bad.limit_bytes = 65537;
    assert!(read_compiler_output(&f.config, &f.id, &bad).is_err());
    bad.limit_bytes = 1;
    bad.attempt_id = uuid::Uuid::new_v4().to_string();
    assert!(read_compiler_output(&f.config, &f.id, &bad).is_err());
    bad.attempt_id = "../outside".into();
    assert!(read_compiler_output(&f.config, &f.id, &bad).is_err());
    let mut other = f.config.clone();
    other.runtime_directory = tmp.path().into();
    assert!(read_compiler_output(&other, &f.id, &output_query(&r, 3)).is_err());
}
#[test]
fn compiler_output_rejects_same_size_tamper_replacement_and_cursor_rebind() {
    use winbridge_update::build_output::read_compiler_output;
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut f.control,
    )
    .unwrap();
    let mut q = output_query(&r, 3);
    let first = read_compiler_output(&f.config, &f.id, &q).unwrap();
    q.offset = first.next_offset;
    q.file_identity = first.file_identity;
    let path = log(&r, "stdout");
    let original = fs::read(&path).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&path, changed).unwrap();
    assert!(read_compiler_output(&f.config, &f.id, &q)
        .unwrap_err()
        .contains("SHA-256"));
    fs::write(&path, &original).unwrap();
    fs::rename(&path, path.with_extension("original")).unwrap();
    fs::write(&path, &original).unwrap();
    assert!(read_compiler_output(&f.config, &f.id, &q)
        .unwrap_err()
        .contains("replaced"));
    let mut no_identity = output_query(&r, 3);
    no_identity.offset = 3;
    assert!(read_compiler_output(&f.config, &f.id, &no_identity).is_err());
    let mut wrong_stream = q.clone();
    wrong_stream.stream = winbridge_protocol::OutputStream::Stderr;
    assert!(read_compiler_output(&f.config, &f.id, &wrong_stream).is_err());
    fs::remove_file(&path).unwrap();
    assert!(read_compiler_output(&f.config, &f.id, &q).is_err());
    assert_eq!(f.journal.get(&f.id).unwrap().phase, UpdatePhase::Building);
}
#[test]
fn compiler_live_output_is_readonly_and_does_not_claim_capture_completed() {
    struct Live<'a> {
        control: &'a mut Control,
        cfg: UpdateConfig,
        id: String,
        page: Option<UpdateCompilerOutputPage>,
    }
    impl SourceControl for Live<'_> {
        fn observe_worker(&mut self) -> Result<WorkerInfo, String> {
            self.control.observe_worker()
        }
    }
    impl BuildControl for Live<'_> {
        fn build_progress(&mut self, r: &UpdateBuildRecord) -> Result<(), String> {
            if r.phase == UpdateBuildPhase::Running {
                let until = Instant::now() + Duration::from_secs(5);
                loop {
                    let page = winbridge_update::build_output::read_compiler_output(
                        &self.cfg,
                        &self.id,
                        &output_query(r, 65536),
                    )?;
                    if String::from_utf8_lossy(&page.bytes).contains("Привет ✓") {
                        assert!(!page.capture_complete && !page.proof_verified);
                        assert!(page.observed_bytes.is_none() && page.quota_truncated.is_none());
                        self.page = Some(page);
                        break;
                    }
                    assert!(Instant::now() < until);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Ok(())
        }
    }
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "live");
    let mut live = Live {
        control: &mut f.control,
        cfg: f.config.clone(),
        id: f.id.clone(),
        page: None,
    };
    let r = execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "build",
        &cmd,
        &mut live,
    )
    .unwrap();
    assert_eq!(r.outcome, Some(UpdateBuildOutcome::Succeeded));
    let final_page = winbridge_update::build_output::read_compiler_output(
        &f.config,
        &f.id,
        &output_query(&r, 65536),
    )
    .unwrap();
    assert!(final_page.capture_complete && final_page.proof_verified);
    if let Some(path) = std::env::var_os("WINBRIDGE_COMPILER_OUTPUT_REPORT") {
        fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"compiler_output_proof","passed":true,"platform":std::env::consts::OS,"build":winbridge_protocol::build_identity(),"live":live.page.unwrap(),"final":final_page,"runtimeApplied":false,"windowsOpened":false})).unwrap()).unwrap();
    }
}
