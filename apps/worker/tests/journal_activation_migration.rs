//! Actual old16 CLI history ->17 and active16 refusal. No synthetic schema rewrite.
#![cfg(windows)]
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winbridge_platform::{ManagedChild, OutputReader};
use winbridge_protocol::{
    update::*, ArtifactDescriptor, ArtifactRole, ResolvedCommand, RuntimeImageEvidence,
};
use winbridge_update::{artifact, sha256};
use winbridge_worker::store::Store;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reference {
    schema_version: u16,
    kind: String,
    reference_run: String,
    report_sha256: String,
    proof_sha256: String,
    source: UpdateSourcePin,
    image: RuntimeImageEvidence,
    descriptor: ArtifactDescriptor,
    fixture_image: RuntimeImageEvidence,
    fixture_descriptor: ArtifactDescriptor,
    manifest_file: RuntimeImageEvidence,
    state: WorkerShutdownState,
    authority: StateBackupAuthority,
    accepted_shutdown: WorkerShutdownReceipt,
    worker_bytes: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    root: PathBuf,
    reference: Reference,
    proof: PathBuf,
}
fn write(path: &Path, value: &impl serde::Serialize) {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    f.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    f.sync_all().unwrap();
}
fn connect(path: &Path) -> Connection {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    c
}
fn version(c: &Connection) -> u32 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
fn rows(c: &Connection) -> String {
    let names:Vec<String>=c.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name!='worker_journal_activations' ORDER BY name").unwrap()
        .query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    let mut values = Vec::new();
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut q = c.prepare(&format!("SELECT * FROM {name}")).unwrap();
        let columns = q.column_count();
        let mut data = q
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .map(|r| format!("{:?}", r.unwrap()))
            .collect::<Vec<_>>();
        data.sort();
        values.push((name, data));
    }
    sha256(&serde_json::to_vec(&values).unwrap())
}
fn authority(c: &Connection) -> StateBackupAuthority {
    let (incarnation, epoch, id): (String, i64, String) = c
        .query_row(
            "SELECT incarnation,epoch,authority FROM journal_authority WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let epoch: u64 = epoch.try_into().unwrap();
    let mut raw = b"winbridge.worker-state-authority.v1\n".to_vec();
    raw.extend(serde_json::to_vec(&(&incarnation, epoch, &id)).unwrap());
    StateBackupAuthority {
        incarnation,
        epoch,
        fingerprint_sha256: sha256(&raw),
    }
}
fn bytes(directory: &Path) -> serde_json::Value {
    json!(["journal.sqlite", "journal.sqlite-wal"].map(|name| {
        let path = directory.join(name);
        let data = path.is_file().then(|| fs::read(path).unwrap());
        (
            name,
            data.and_then(|b| {
                if name.ends_with("-wal") && b.is_empty() {
                    None
                } else {
                    Some(sha256(&b))
                }
            }),
        )
    }))
}
fn backup(source: &Connection, directory: &Path) -> PathBuf {
    fs::create_dir(directory).unwrap();
    let path = directory.join("journal.sqlite");
    let f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    f.sync_all().unwrap();
    drop(f);
    let mut dest = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    rusqlite::backup::Backup::new(source, &mut dest)
        .unwrap()
        .run_to_completion(64, Duration::from_millis(10), None)
        .unwrap();
    drop(dest);
    path
}
fn drain(mut reader: OutputReader) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buf = [0; 16384];
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            let keep = n.min((2 * 1024 * 1024usize).saturating_sub(out.len()));
            out.extend_from_slice(&buf[..keep]);
        }
        out
    })
}
struct OwnedProcess {
    child: ManagedChild,
    out: std::thread::JoinHandle<Vec<u8>>,
    err: std::thread::JoinHandle<Vec<u8>>,
}
impl OwnedProcess {
    fn start(image: &Path, config: &Path, args: &[&str], root: &Path) -> Self {
        let command = ResolvedCommand {
            executable: image.to_str().unwrap().into(),
            arguments: [
                vec!["--config".into(), config.to_str().unwrap().into()],
                args.iter().map(|s| (*s).into()).collect(),
            ]
            .concat(),
            working_directory: root.to_str().unwrap().into(),
            timeout_ms: 30000,
        };
        let (mut child, out, err) = ManagedChild::prepare_atomic_executable(
            &command,
            Some(&format!(
                "WinBridge-worker-migration-{}",
                uuid::Uuid::new_v4()
            )),
            None,
        )
        .unwrap();
        let (out, err) = (drain(out), drain(err));
        child.resume().unwrap();
        Self { child, out, err }
    }
    fn finish(mut self, expect_success: bool) -> (Vec<u8>, Vec<u8>) {
        let until = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                break s;
            }
            assert!(
                Instant::now() < until,
                "Owned migration CLI timed out; no retry of an effect"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        self.child.terminate_tree().unwrap();
        assert!(self.child.tree_is_empty().unwrap());
        let result = (self.out.join().unwrap(), self.err.join().unwrap());
        assert_eq!(
            status.success(),
            expect_success,
            "{}",
            String::from_utf8_lossy(&result.1)
        );
        result
    }
    fn stop(mut self, root: &Path) {
        self.child.terminate_tree().unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !self.child.tree_is_empty().unwrap() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(self.child.tree_is_empty().unwrap());
        fs::write(
            root.join("old16-worker-stdout.log"),
            self.out.join().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("old16-worker-stderr.log"),
            self.err.join().unwrap(),
        )
        .unwrap();
    }
}
fn cli(image: &Path, config: &Path, args: &[&str], root: &Path) -> serde_json::Value {
    let (out, _) = OwnedProcess::start(image, config, args, root).finish(true);
    serde_json::from_slice(&out).unwrap()
}
fn copy_image(
    e: &RuntimeImageEvidence,
    d: &ArtifactDescriptor,
    role: ArtifactRole,
    target: &Path,
    source: &UpdateSourcePin,
) {
    let query = artifact::query_image(Path::new(&e.path), role).unwrap();
    assert_eq!(&query.image, e);
    assert_eq!(&query.descriptor, d);
    assert_eq!(d.build.source_commit.as_ref(), Some(&source.source_commit));
    assert_eq!(
        d.build.source_manifest_sha256.as_ref(),
        Some(&source.manifest_sha256)
    );
    assert!(!d.build.source_dirty);
    fs::copy(&e.path, target).unwrap();
    let copy = artifact::query_image(target, role).unwrap();
    assert_eq!(copy.image.sha256, e.sha256);
    assert_eq!(&copy.descriptor, d);
}
fn refused_open(path: &Path) -> String {
    match Store::open(path) {
        Ok(_) => panic!("Active old16 must not migrate or acquire authority"),
        Err(reason) => {
            assert!(
                reason.contains("unfinished") || reason.contains("Unfinished"),
                "{reason}"
            );
            reason
        }
    }
}
#[test]
#[ignore = "Pinned actual16 executables and accepted-stop history; isolated migration/refusal only"]
fn actual_sixteen_cli_history_migrates_to_seventeen_and_active_sixteen_stays_blocked() {
    assert_eq!(winbridge_worker::store::JOURNAL_SCHEMA_VERSION, 17);
    let input: Input = serde_json::from_slice(
        &fs::read(std::env::var_os("WINBRIDGE_WRITER_WORKER_MIGRATION_INPUT").unwrap()).unwrap(),
    )
    .unwrap();
    let r = &input.reference;
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.kind, "journal_schema16_reference");
    for sha in [&r.report_sha256, &r.proof_sha256] {
        assert_eq!(sha.len(), 64)
    }
    assert!(!r.reference_run.is_empty());
    winbridge_platform::require_safe_path(&input.root).unwrap();
    winbridge_platform::require_local_disk(&input.root).unwrap();
    fs::create_dir(&input.root).unwrap();
    let source_dir = Path::new(&r.state.state_directory);
    let source_file = source_dir.join("journal.sqlite");
    assert_eq!(bytes(source_dir), r.worker_bytes);
    let source = connect(&source_file);
    assert_eq!(version(&source), 16);
    let stop: String = source
        .query_row(
            "SELECT receipt FROM worker_shutdown_receipts WHERE update_id=?1",
            [&r.accepted_shutdown.update_id],
            |q| q.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<WorkerShutdownReceipt>(&stop).unwrap(),
        r.accepted_shutdown
    );
    assert_eq!(r.accepted_shutdown.state, r.state);
    assert_eq!(r.accepted_shutdown.lease.phase, WorkerUpdatePhase::Quiesced);
    let active_rows = rows(&source);
    let active_authority = authority(&source);
    assert_eq!(active_authority, r.authority);
    let active_dir = input.root.join("active16-copy");
    let active_file = backup(&source, &active_dir);
    let active_before = bytes(&active_dir);
    let listing = |directory: &Path| {
        let mut names = fs::read_dir(directory)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != "journal.sqlite-wal" && n != "journal.sqlite-shm")
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let active_listing = listing(&active_dir);
    let active_refusal = refused_open(&active_file);
    assert_eq!(bytes(&active_dir), active_before);
    let active_check = connect(&active_file);
    assert_eq!(version(&active_check), 16);
    assert_eq!(rows(&active_check), active_rows);
    assert_eq!(authority(&active_check), active_authority);
    let copied_stop: String = active_check
        .query_row(
            "SELECT receipt FROM worker_shutdown_receipts WHERE update_id=?1",
            [&r.accepted_shutdown.update_id],
            |q| q.get(0),
        )
        .unwrap();
    let copied_stop: WorkerShutdownReceipt = serde_json::from_str(&copied_stop).unwrap();
    assert_eq!(copied_stop, r.accepted_shutdown);
    // Readonly SQLite coordination files may appear. No frames, main DB
    // changes or new migration/backup artifacts are permitted.
    assert_eq!(listing(&active_dir), active_listing);
    drop(active_check);
    let bin = input.root.join("old16-bin");
    fs::create_dir(&bin).unwrap();
    let old = bin.join("winbridge-worker.exe");
    let fixture = bin.join("winbridge-fixture.exe");
    copy_image(
        &r.image,
        &r.descriptor,
        ArtifactRole::Worker,
        &old,
        &r.source,
    );
    copy_image(
        &r.fixture_image,
        &r.fixture_descriptor,
        ArtifactRole::Fixture,
        &fixture,
        &r.source,
    );
    // Keep the exact copied executable bytes bound through startup, task
    // execution and old-version refusal. These read handles grant no custody.
    let old_pin = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&old)
        .unwrap();
    let fixture_pin = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&fixture)
        .unwrap();
    let copied_old_image = artifact::inspect_image(&old).unwrap();
    let copied_fixture_image = artifact::inspect_image(&fixture).unwrap();
    assert_eq!(copied_old_image.sha256, r.image.sha256);
    assert_eq!(copied_fixture_image.sha256, r.fixture_image.sha256);
    assert_eq!(
        artifact::inspect_image(Path::new(&r.manifest_file.path)).unwrap(),
        r.manifest_file
    );
    let project = input.root.join("project");
    fs::create_dir(&project).unwrap();
    let inactive = input.root.join("inactive16-source");
    let config = input.root.join("worker.local.json");
    let mut cfg = json!({"schemaVersion":1,"instanceId":format!("migration-{}",uuid::Uuid::new_v4()),"nodeId":"old16-migration-fixture","stateDirectory":inactive,"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"source","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]});
    write(&config, &cfg);
    let mut server = OwnedProcess::start(&old, &config, &["serve"], &input.root);
    let old_process = server.child.identity().unwrap().clone();
    // Readiness only: failed Info is never a retry of task submission.
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "Old16 normal service stopped before readiness"
        );
        let endpoint =
            winbridge_transport::endpoint(&inactive, cfg["instanceId"].as_str().unwrap()).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        if matches!(rt.block_on(winbridge_transport::call(&endpoint,&winbridge_protocol::LocalRequest::Info)),
            Ok(winbridge_protocol::LocalResponse::Info { info }) if info.build.as_ref()==Some(&r.descriptor.build))
        {
            break;
        }
        assert!(Instant::now() < until, "Old16 IPC readiness expired");
        std::thread::sleep(Duration::from_millis(25));
    }
    let submission = json!({"protocolVersion":1,"requestId":uuid::Uuid::new_v4().to_string(),"projectId":"source","idempotencyKey":"old16-history","timeoutMs":10000,"command":{"kind":"direct","executable":fixture,"arguments":["success"],"workingDirectory":"."}});
    let request = input.root.join("submit.json");
    write(&request, &submission);
    let accepted = cli(
        &old,
        &config,
        &["submit", request.to_str().unwrap()],
        &input.root,
    );
    let task_id = accepted["task"]["taskId"].as_str().unwrap().to_owned();
    let until = Instant::now() + Duration::from_secs(30);
    let completed = loop {
        let t = cli(&old, &config, &["get", &task_id], &input.root);
        if t["task"]["status"] == "succeeded" {
            assert_eq!(t["task"]["exitCode"], 0);
            break t;
        }
        assert!(
            Instant::now() < until,
            "Observe the same old16 task; never resubmit a failed/unknown command"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let duplicate = cli(
        &old,
        &config,
        &["submit", request.to_str().unwrap()],
        &input.root,
    );
    assert_eq!(duplicate["task"]["taskId"], task_id);
    let output = cli(&old, &config, &["output", &task_id], &input.root);
    assert!(!output["chunks"].as_array().unwrap().is_empty());
    server.stop(&input.root);
    let old_file = inactive.join("journal.sqlite");
    let old_db = connect(&old_file);
    assert_eq!(version(&old_db), 16);
    let stored_task: String = old_db
        .query_row("SELECT record FROM tasks WHERE id=?1", [&task_id], |q| {
            q.get(0)
        })
        .unwrap();
    let stored_task: serde_json::Value = serde_json::from_str(&stored_task).unwrap();
    assert_eq!(stored_task["taskId"], task_id);
    assert_eq!(stored_task["status"], "succeeded");
    let stored_key: String = old_db
        .query_row("SELECT key FROM tasks WHERE id=?1", [&task_id], |q| {
            q.get(0)
        })
        .unwrap();
    assert_eq!(stored_key, "old16-history");
    let stored_chunks = old_db
        .prepare("SELECT sequence,stream,offset,bytes FROM output WHERE task=?1 ORDER BY sequence")
        .unwrap()
        .query_map([&task_id], |q| {
            Ok(
                json!({"sequence":q.get::<_,i64>(0)?, "stream":q.get::<_,String>(1)?,
            "offset":q.get::<_,i64>(2)?, "bytes":q.get::<_,Vec<u8>>(3)?}),
            )
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(json!(stored_chunks), output["chunks"]);
    let before_rows = rows(&old_db);
    let before_authority = authority(&old_db);
    let old_bytes = bytes(&inactive);
    let copied_dir = input.root.join("inactive17-copy");
    let copied = backup(&old_db, &copied_dir);
    let store = Store::open(&copied).unwrap();
    assert_eq!(store.state_backup_authority().unwrap(), before_authority);
    drop(store);
    let after = connect(&copied);
    assert_eq!(version(&after), 17);
    assert_eq!(rows(&after), before_rows);
    assert_eq!(authority(&after), before_authority);
    assert_eq!(
        after
            .query_row("SELECT count(*) FROM worker_journal_activations", [], |q| q
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let archives = fs::read_dir(&copied_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v16-")
                && p.extension().is_some_and(|e| e == "sqlite")
        })
        .collect::<Vec<_>>();
    assert_eq!(archives.len(), 1);
    let archived = connect(&archives[0]);
    assert_eq!(version(&archived), 16);
    assert_eq!(rows(&archived), before_rows);
    assert_eq!(authority(&archived), before_authority);
    assert_eq!(
        archived
            .query_row("PRAGMA integrity_check", [], |q| q.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(archived);
    drop(after);
    cfg["stateDirectory"] = json!(copied_dir);
    let next_config = input.root.join("migrated.local.json");
    write(&next_config, &cfg);
    let before_refusal = bytes(&copied_dir);
    let (_, old_error) =
        OwnedProcess::start(&old, &next_config, &["serve"], &input.root).finish(false);
    let old_error = String::from_utf8_lossy(&old_error);
    assert!(
        old_error.contains("schema") && old_error.contains("17"),
        "{old_error}"
    );
    assert_eq!(bytes(&copied_dir), before_refusal);
    assert_eq!(bytes(&inactive), old_bytes);
    assert_eq!(rows(&old_db), before_rows);
    drop(old_db);
    assert_eq!(bytes(source_dir), r.worker_bytes);
    assert_eq!(rows(&source), active_rows);
    drop(source);
    assert_eq!(
        artifact::inspect_image(Path::new(&r.image.path)).unwrap(),
        r.image
    );
    assert_eq!(artifact::inspect_image(&old).unwrap(), copied_old_image);
    assert_eq!(
        artifact::inspect_image(&fixture).unwrap(),
        copied_fixture_image
    );
    assert_eq!(
        winbridge_platform::file_identity(&old_pin).unwrap(),
        copied_old_image.file_identity
    );
    assert_eq!(
        winbridge_platform::file_identity(&fixture_pin).unwrap(),
        copied_fixture_image.file_identity
    );
    let proof = json!({"schemaVersion":1,"kind":"worker_journal_activation_migration_proof","platform":"windows","passed":true,"build":winbridge_protocol::build_identity(),
        "referenceRun":r.reference_run,"referenceProofSha256":r.proof_sha256,"referenceReportSha256":r.report_sha256,"referenceImage":r.image,"referenceDescriptor":r.descriptor,
        "oldProcess":old_process,"copiedOldImage":copied_old_image,"copiedFixtureImage":copied_fixture_image,
        "task":completed["task"],"duplicateTaskId":duplicate["task"]["taskId"],"outputChunks":output["chunks"],
        "fromSchema":16,"toSchema":17,"authorityBefore":before_authority,"authorityAfter":before_authority,"rowsBefore":before_rows,"rowsAfter":before_rows,"backupRows":before_rows,
        "activeAuthority":active_authority,"activeAcceptedShutdown":copied_stop,"activeRowsBefore":active_rows,"activeRowsAfter":active_rows,"activeSchemaAfter":16,"activeRefusal":active_refusal,
        "oldSixteenRefusedSeventeen":true,"activeSixteenRefusedBeforeWrites":true,"migrationBackupVerified":true,"oldWorkerJobEmpty":true,"sourceBytesUnchanged":true,
        "noActivationInvented":true,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false});
    write(&input.proof, &proof);
}
