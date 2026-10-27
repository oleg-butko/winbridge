//! Actual released worker17 history -> current19, local IPC only; no installed state.
#![cfg(windows)]
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use winbridge_platform::{ManagedChild, OutputReader};
use winbridge_protocol::*;
use winbridge_update::{artifact, sha256};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    root: PathBuf,
    reference: PathBuf,
    reference_sha256: String,
    powershell: PathBuf,
    proof: PathBuf,
}
fn drain(mut r: OutputReader) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut v = vec![];
        r.read_to_end(&mut v).unwrap();
        v
    })
}
struct Server {
    child: ManagedChild,
    out: std::thread::JoinHandle<Vec<u8>>,
    err: std::thread::JoinHandle<Vec<u8>>,
}
impl Server {
    fn start(image: &Path, config: &Path, root: &Path) -> Self {
        let command = ResolvedCommand {
            executable: image.to_str().unwrap().into(),
            arguments: vec![
                "--config".into(),
                config.to_str().unwrap().into(),
                "serve".into(),
            ],
            working_directory: root.to_str().unwrap().into(),
            timeout_ms: 60000,
        };
        let (mut child, out, err) = ManagedChild::prepare_atomic_executable(
            &command,
            Some(&format!(
                "WinBridge-attribution-migration-{}",
                uuid::Uuid::new_v4()
            )),
            None,
        )
        .unwrap();
        let out = drain(out);
        let err = drain(err);
        child.resume().unwrap();
        Self { child, out, err }
    }
    fn stop(mut self) -> (Vec<u8>, Vec<u8>) {
        self.child.terminate_tree().unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !self.child.tree_is_empty().unwrap() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        (self.out.join().unwrap(), self.err.join().unwrap())
    }
}
fn info(rt: &tokio::runtime::Runtime, endpoint: &str, server: &mut Server) -> WorkerInfo {
    let until = Instant::now() + Duration::from_secs(25);
    loop {
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "Owned migration worker exited"
        );
        if let Ok(LocalResponse::Info { info }) =
            rt.block_on(winbridge_transport::call(endpoint, &LocalRequest::Info))
        {
            return *info;
        }
        assert!(
            Instant::now() < until,
            "Readiness timeout; do not replay any command"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn call(rt: &tokio::runtime::Runtime, endpoint: &str, q: LocalRequest) -> LocalResponse {
    rt.block_on(winbridge_transport::call(endpoint, &q))
        .unwrap()
}
fn task(rt: &tokio::runtime::Runtime, endpoint: &str, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } =
        call(rt, endpoint, LocalRequest::GetTask { task_id: id.into() })
    else {
        panic!("task")
    };
    *task
}
fn finish(rt: &tokio::runtime::Runtime, endpoint: &str, id: &str, status: TaskStatus) {
    let until = Instant::now() + Duration::from_secs(25);
    loop {
        let t = task(rt, endpoint, id);
        if t.status.is_terminal() {
            assert_eq!(t.status, status);
            return;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn submit(rt: &tokio::runtime::Runtime, endpoint: &str, q: &TaskSubmission) -> TaskRecord {
    let LocalResponse::Task { task } =
        call(rt, endpoint, LocalRequest::Submit { request: q.clone() })
    else {
        panic!("submit ACK unavailable; do not retry")
    };
    *task
}
fn db(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn version(c: &Connection) -> u32 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
fn rows(c: &Connection) -> Vec<(String, String, String)> {
    c.prepare("SELECT id,payload,record FROM tasks ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}
fn fingerprint(path: &Path) -> serde_json::Value {
    let sidecar = |suffix: &str| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        match fs::read(PathBuf::from(name)) {
            Ok(bytes) if bytes.len() > 32 => Some(sha256(&bytes)),
            Ok(_) => None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => panic!("{e}"),
        }
    };
    json!({"main":sha256(&fs::read(path).unwrap()),"wal":sidecar("-wal"),"rollback":sidecar("-journal")})
}
fn write(path: &Path, v: &impl serde::Serialize) {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    f.write_all(&serde_json::to_vec_pretty(v).unwrap()).unwrap();
    f.sync_all().unwrap();
}

#[test]
#[ignore = "Owned Windows migration gate requires an explicit pinned released worker17 and fresh local root"]
fn actual_seventeen_history_migrates_to_nineteen_without_replay_or_old_worker_write() {
    assert_eq!(winbridge_worker::store::JOURNAL_SCHEMA_VERSION, 19);
    let input: Input = serde_json::from_slice(
        &fs::read(std::env::var_os("WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT").unwrap()).unwrap(),
    )
    .unwrap();
    winbridge_platform::require_safe_path(&input.root).unwrap();
    winbridge_platform::require_local_disk(&input.root).unwrap();
    fs::create_dir(&input.root).unwrap();
    assert_eq!(
        sha256(&fs::read(&input.reference).unwrap()),
        input.reference_sha256
    );
    let old = input.root.join("old17-worker.exe");
    let new = input.root.join("new19-worker.exe");
    fs::copy(&input.reference, &old).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_winbridge-worker"), &new).unwrap();
    let old_identity = artifact::query_image(&old, ArtifactRole::Worker).unwrap();
    let new_identity = artifact::query_image(&new, ArtifactRole::Worker).unwrap();
    assert_eq!(old_identity.image.sha256, input.reference_sha256);
    assert!(!old_identity.descriptor.build.source_dirty);
    assert!(!new_identity.descriptor.build.source_dirty);
    assert_eq!(new_identity.descriptor.build, build_identity());
    let project = input.root.join("project");
    fs::create_dir(&project).unwrap();
    let state = input.root.join("state");
    let config = input.root.join("worker.json");
    let instance = format!("attr-{}", uuid::Uuid::new_v4().simple());
    write(
        &config,
        &json!({"schemaVersion":1,"instanceId":instance,"nodeId":"migration","stateDirectory":state,"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]}),
    );
    let endpoint = winbridge_transport::endpoint(&state, &instance).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut server = Server::start(&old, &config, &input.root);
    let old_info = info(&rt, &endpoint, &mut server);
    let request=TaskSubmission {protocol_version:1,request_id:"original".into(),project_id:"fixture".into(),idempotency_key:"old-key".into(),timeout_ms:10000,snapshot_id:None,command:CommandSource::Direct {executable:input.powershell.to_str().unwrap().into(),arguments:vec!["-NoProfile".into(),"-NonInteractive".into(),"-Command".into(),"[IO.File]::AppendAllText((Join-Path (Get-Location) 'runs.log'),\"old`n\");[Console]::WriteLine('old output ✓')".into()],working_directory:".".into()}};
    let accepted = submit(&rt, &endpoint, &request);
    finish(&rt, &endpoint, &accepted.task_id, TaskStatus::Succeeded);
    let old_output = serde_json::to_value(call(
        &rt,
        &endpoint,
        LocalRequest::ReadOutput {
            task_id: accepted.task_id.clone(),
            after_sequence: 0,
        },
    ))
    .unwrap();
    server.stop();
    let file = state.join("journal.sqlite");
    let source = db(&file);
    assert_eq!(version(&source), 17);
    let old_rows = rows(&source);
    drop(source);
    let mut server = Server::start(&new, &config, &input.root);
    let new_info = info(&rt, &endpoint, &mut server);
    assert_ne!(old_info.generation, new_info.generation);
    let context = new_info.fenced_requests.as_ref().unwrap().context.clone();
    assert_eq!(old_info.fenced_requests.unwrap().context, context);
    let current = db(&file);
    assert_eq!(version(&current), 19);
    assert_eq!(rows(&current), old_rows);
    drop(current);
    let backups: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("journal-v17-")
                && p.extension().is_some_and(|e| e == "sqlite")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup = db(&backups[0]);
    assert_eq!(version(&backup), 17);
    assert_eq!(rows(&backup), old_rows);
    assert_eq!(
        backup
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(backup);
    assert_eq!(submit(&rt, &endpoint, &request).task_id, accepted.task_id);
    assert_eq!(
        serde_json::to_value(call(
            &rt,
            &endpoint,
            LocalRequest::ReadOutput {
                task_id: accepted.task_id.clone(),
                after_sequence: 0
            }
        ))
        .unwrap(),
        old_output
    );
    let query = |id: &str| OperationAttributionRequest {
        context: context.clone(),
        query: OperationAttributionQuery {
            kind: HistoryKind::Task,
            operation_id: id.into(),
        },
    };
    assert!(rt
        .block_on(winbridge_transport::operation_attribution(
            &endpoint,
            &query(&accepted.task_id)
        ))
        .unwrap()
        .creator
        .is_none());
    let mut fresh = request.clone();
    fresh.request_id = "new".into();
    fresh.idempotency_key = "new-key".into();
    if let CommandSource::Direct { arguments, .. } = &mut fresh.command {
        arguments[3] =
            "[IO.File]::AppendAllText((Join-Path (Get-Location) 'runs.log'),\"new`n\")".into();
    }
    let next = submit(&rt, &endpoint, &fresh);
    finish(&rt, &endpoint, &next.task_id, TaskStatus::Succeeded);
    let creator = rt
        .block_on(winbridge_transport::operation_attribution(
            &endpoint,
            &query(&next.task_id),
        ))
        .unwrap()
        .creator
        .unwrap();
    assert_eq!(
        creator,
        OperationCreator::Local {
            worker_generation: new_info.generation.clone()
        }
    );
    let mut long = fresh.clone();
    long.idempotency_key = "cancel-key".into();
    long.timeout_ms = 60000;
    if let CommandSource::Direct { arguments, .. } = &mut long.command {
        arguments[3] = "Start-Sleep -Seconds 60".into();
    }
    let active = submit(&rt, &endpoint, &long);
    let until = Instant::now() + Duration::from_secs(20);
    while task(&rt, &endpoint, &active.task_id).process_id.is_none() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    call(
        &rt,
        &endpoint,
        LocalRequest::Cancel {
            task_id: active.task_id.clone(),
        },
    );
    finish(&rt, &endpoint, &active.task_id, TaskStatus::Cancelled);
    let cancellation = rt
        .block_on(winbridge_transport::operation_cancellation(
            &endpoint,
            &query(&active.task_id),
        ))
        .unwrap()
        .intent
        .unwrap();
    assert_eq!(cancellation.actor, Some(creator.clone()));
    server.stop();
    let mut server = Server::start(&new, &config, &input.root);
    let reopened = info(&rt, &endpoint, &mut server);
    assert_ne!(reopened.generation, new_info.generation);
    assert_eq!(submit(&rt, &endpoint, &fresh).task_id, next.task_id);
    assert_eq!(
        rt.block_on(winbridge_transport::operation_attribution(
            &endpoint,
            &query(&next.task_id)
        ))
        .unwrap()
        .creator,
        Some(creator)
    );
    let retained = rt
        .block_on(winbridge_transport::operation_cancellation(
            &endpoint,
            &query(&active.task_id),
        ))
        .unwrap()
        .intent
        .unwrap();
    assert_eq!(retained.actor, cancellation.actor);
    assert_eq!(retained.requested_at_ms, cancellation.requested_at_ms);
    server.stop();
    assert_eq!(
        fs::read_to_string(project.join("runs.log"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        vec!["old", "new"]
    );
    let before = fingerprint(&file);
    let mut incompatible = Server::start(&old, &config, &input.root);
    let until = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(s) = incompatible.child.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success());
    let (_, err) = incompatible.stop();
    assert!(String::from_utf8_lossy(&err).contains("schema 19"));
    assert_eq!(fingerprint(&file), before);
    assert_eq!(
        sha256(&fs::read(&input.reference).unwrap()),
        input.reference_sha256
    );
    write(
        &input.proof,
        &json!({"schemaVersion":1,"kind":"worker_attribution_migration_native_proof","passed":true,"platform":"windows","build":build_identity(),"fromSchema":17,"toSchema":19,"referenceSha256":input.reference_sha256,"referenceBuild":old_identity.descriptor.build,"oldHistoryPreserved":true,"verifiedMigrationBackup":true,"outputPreserved":true,"dedupPreserved":true,"legacyCreatorUnknown":true,"creatorSurvivesRestart":true,"cancellationSurvivesRestart":true,"oldWorkerRefusedWithoutJournalWrite":true,"completedExternalExecutions":2,"startedAndCancelledProcesses":1,"installedStateOpened":false,"activeUpdaterMigrationChecked":false}),
    );
}
