use super::*;
use winbridge_update::build_report::read_verified;
fn report_command(f: &mut Fixture, mode: &str) -> (ResolvedCommand, PathBuf) {
    let mut cmd = command(f, mode);
    f.config.build = Some(winbridge_update::PowerShellBuildPolicy {
        powershell_executable: PathBuf::from(&cmd.executable),
        timeout_ms: cmd.timeout_ms,
    });
    let path = f
        .config
        .runtime_directory
        .join("update")
        .join(&f.id)
        .join("candidate/candidate-report.json");
    cmd.arguments
        .extend(["--skip".into(), format!("report-file={}", path.display())]);
    (cmd, path)
}
fn run(f: &mut Fixture, cmd: &ResolvedCommand) -> Result<UpdateBuildRecord, String> {
    execute_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "report-build",
        cmd,
        &mut f.control,
    )
}
fn snapshot(f: &Fixture) -> winbridge_update::build_report::ReportSnapshot {
    UpdateJournal::inspect_build_report(&f.config.runtime_directory.join("update-state"), &f.id)
        .unwrap()
}
#[test]
fn compiler_report_is_admitted_before_dispatch_and_committed_with_completion() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let (cmd, path) = report_command(&mut f, "report");
    let permit = match admit_build(
        &mut f.journal,
        &f.owner,
        &f.config,
        &f.id,
        "report-build",
        &cmd,
    )
    .unwrap()
    {
        BuildAdmission::New(p) => p,
        _ => panic!("Fresh report intent required"),
    };
    let view = snapshot(&f);
    let (build, intent, evidence) = (view.build, view.intent, view.evidence);
    assert_eq!(build.phase, UpdateBuildPhase::Intent);
    assert!(intent.is_some());
    assert!(evidence.is_none());
    assert!(!path.exists());
    let done = permit
        .execute(&mut f.journal, &f.owner, &f.config, &mut f.control)
        .unwrap();
    assert_eq!(done.outcome, Some(UpdateBuildOutcome::Succeeded));
    let (proof, bytes) = read_verified(&f.config, &f.id).unwrap();
    assert_eq!(proof.sha256, sha256(&bytes));
    assert_eq!(proof.bytes, bytes.len() as u64);
    assert_eq!(bytes, fs::read(&path).unwrap());
    assert_eq!(snapshot(&f).evidence, Some(proof));
    assert_eq!(run(&mut f, &cmd).unwrap(), done);
    assert_eq!(
        fs::read_to_string(f.config.source_directory.join("target/compiler-runs.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    for sql in [
        "DELETE FROM update_build_reports",
        "UPDATE update_build_reports SET record='{}'",
        "DELETE FROM update_build_report_intents",
        "UPDATE update_build_report_intents SET record='{}'",
    ] {
        assert!(f.db().execute_batch(sql).is_err());
    }
    assert!(read_verified(&f.config, &f.id).is_ok());
    if let Some(path) = std::env::var_os("WINBRIDGE_COMPILER_REPORT_PROOF") {
        fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"kind":"compiler_report_binding_proof","passed":true,"platform":std::env::consts::OS,"build":winbridge_protocol::build_identity(),"snapshot":snapshot(&f),"receipt":read_verified(&f.config,&f.id).unwrap().0,"intentBeforeDispatch":true,"atomicCompletion":true,"duplicateObserved":true,"runtimeApplied":false,"candidateVerified":false,"windowsOpened":false})).unwrap()).unwrap();
    }
}
#[test]
fn compiler_report_rejects_same_size_edits_file_and_parent_replacements() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let (cmd, path) = report_command(&mut f, "report");
    run(&mut f, &cmd).unwrap();
    let bytes = fs::read(&path).unwrap();
    let proof = read_verified(&f.config, &f.id).unwrap().0;
    let mut changed = bytes.clone();
    changed[0] = b' ';
    fs::write(&path, &changed).unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("differs"));
    fs::write(&path, &bytes).unwrap();
    assert_eq!(read_verified(&f.config, &f.id).unwrap().0, proof);
    let alias = path.with_extension("alias");
    fs::hard_link(&path, &alias).unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("Hard-linked"));
    fs::remove_file(alias).unwrap();
    let old = path.with_extension("old");
    fs::rename(&path, &old).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("differs"));
    fs::remove_file(&path).unwrap();
    fs::rename(old, &path).unwrap();
    let candidate = path.parent().unwrap();
    let moved = candidate.with_extension("old");
    fs::rename(candidate, &moved).unwrap();
    fs::create_dir(candidate).unwrap();
    fs::rename(moved.join("candidate-report.json"), &path).unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("differs"));
    fs::rename(&path, moved.join("candidate-report.json")).unwrap();
    fs::remove_dir(candidate).unwrap();
    fs::rename(moved, candidate).unwrap();
    let update = candidate.parent().unwrap();
    let moved = update.with_extension("old");
    fs::rename(update, &moved).unwrap();
    fs::create_dir(update).unwrap();
    fs::rename(moved.join("candidate"), candidate).unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("roots/attempt"));
}
#[test]
fn compiler_report_missing_empty_or_oversized_never_becomes_success_or_replays() {
    for mode in ["success", "empty-report", "oversized-report"] {
        let tmp = temporary();
        let mut f = ready(tmp.path());
        let (cmd, _) = report_command(&mut f, mode);
        let done = run(&mut f, &cmd).unwrap();
        assert_eq!(done.phase, UpdateBuildPhase::Indeterminate);
        assert!(done.outcome.is_none());
        assert!(snapshot(&f).evidence.is_none());
        assert!(read_verified(&f.config, &f.id).is_err());
        assert_eq!(run(&mut f, &cmd).unwrap(), done);
        assert_eq!(
            fs::read_to_string(f.config.source_directory.join("target/compiler-runs.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
}
#[test]
fn compiler_report_ignored_receipt_rolls_back_success_and_cannot_be_adopted() {
    for sql in [
        "CREATE TRIGGER ignore_report BEFORE INSERT ON update_build_reports BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER ignore_report BEFORE UPDATE ON update_builds WHEN NEW.phase='\"completed\"' BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let tmp = temporary();
        let mut f = ready(tmp.path());
        let (cmd, _) = report_command(&mut f, "report");
        f.db().execute_batch(sql).unwrap();
        assert!(run(&mut f, &cmd).is_err());
        let view = snapshot(&f);
        let (saved, proof) = (view.build, view.evidence);
        assert_eq!(saved.phase, UpdateBuildPhase::Running);
        assert!(proof.is_none());
        assert!(read_verified(&f.config, &f.id).is_err());
        f.db().execute_batch("DROP TRIGGER ignore_report;").unwrap();
        assert_eq!(run(&mut f, &cmd).unwrap(), saved);
        assert!(snapshot(&f).evidence.is_none());
    }
}
#[test]
fn compiler_report_existing_staging_is_rejected_before_intent_or_process() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let (cmd, path) = report_command(&mut f, "report");
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::write(&path, b"existing candidate").unwrap();
    assert!(run(&mut f, &cmd).unwrap_err().contains("already exists"));
    assert!(
        UpdateJournal::inspect_build(&f.config.runtime_directory.join("update-state"), &f.id)
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"existing candidate");
    assert!(!f
        .config
        .source_directory
        .join("target/compiler-runs.log")
        .exists());
}
#[test]
fn compiler_report_generic_and_schema_five_history_cannot_adopt_later_report() {
    let tmp = temporary();
    let mut f = ready(tmp.path());
    let cmd = command(&f, "success");
    let done = run(&mut f, &cmd).unwrap();
    let directory = f.config.runtime_directory.join("update-state");
    f.db().execute_batch("DROP TABLE update_build_reports; DROP TABLE update_build_report_intents; PRAGMA user_version=5; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let old = UpdateJournal::inspect(&directory).unwrap();
    assert!(UpdateJournal::open(&directory, &old.scope_sha256).is_err());
    drop(f.owner);
    drop(f.journal);
    let mut journal = UpdateJournal::open(&directory, &old.scope_sha256).unwrap();
    assert_eq!(
        journal.info().unwrap().schema_version,
        winbridge_update::journal::UPDATE_SCHEMA_VERSION
    );
    assert_eq!(journal.info().unwrap().owner_epoch, old.owner_epoch + 1);
    let view = UpdateJournal::inspect_build_report(&directory, &f.id).unwrap();
    let (saved, intent, report) = (view.build, view.intent, view.evidence);
    assert_eq!(saved, done);
    assert!(intent.is_none());
    assert!(report.is_none());
    let candidate = f
        .config
        .runtime_directory
        .join("update")
        .join(&f.id)
        .join("candidate");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("candidate-report.json"), b"unowned report").unwrap();
    assert!(read_verified(&f.config, &f.id)
        .unwrap_err()
        .contains("did not admit"));
    let backup = fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v5-")
        })
        .unwrap();
    let db =
        Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
