//! Two projects use one profile through actual IPC and the existing task service.
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;

struct Server {
    child: Child,
    endpoint: String,
    crash_group: Option<u32>,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // The Unix development adapter has no Windows Job Object. Terminate
        // only this test's recorded process group after the intentional crash.
        #[cfg(unix)]
        if let Some(pid) = self.crash_group {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .status();
        }
    }
}
async fn start(path: &Path) -> Server {
    let config = WorkerConfig::load(path).unwrap();
    let endpoint =
        winbridge_transport::endpoint(&config.state_directory, &config.instance_id).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_winbridge-worker"))
        .args(["--config", path.to_str().unwrap(), "serve"])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let server = Server {
        child,
        endpoint,
        crash_group: None,
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if matches!(
            winbridge_transport::call(&server.endpoint, &LocalRequest::Info).await,
            Ok(LocalResponse::Info { .. })
        ) {
            return server;
        }
        assert!(Instant::now() < deadline, "test worker startup");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn rpc(server: &Server, request: LocalRequest) -> LocalResponse {
    winbridge_transport::call(&server.endpoint, &request)
        .await
        .unwrap()
}
fn task(response: LocalResponse) -> TaskRecord {
    let LocalResponse::Task { task } = response else {
        panic!("{response:?}")
    };
    *task
}
async fn get(server: &Server, id: &str) -> TaskRecord {
    task(rpc(server, LocalRequest::GetTask { task_id: id.into() }).await)
}
async fn wait(server: &Server, id: &str, terminal: bool) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let record = get(server, id).await;
        if if terminal {
            record.status.is_terminal()
        } else {
            record.status == TaskStatus::Running && record.process_id.is_some()
        } {
            return record;
        }
        assert!(Instant::now() < deadline, "{record:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn request(project: &str, key: &str, recipe: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: format!("request-{key}"),
        project_id: project.into(),
        idempotency_key: key.into(),
        timeout_ms: 60000,
        snapshot_id: None,
        command: CommandSource::Recipe {
            recipe_id: recipe.into(),
            arguments: Vec::new(),
        },
    }
}
async fn submit(server: &Server, request: TaskSubmission) -> TaskRecord {
    task(rpc(server, LocalRequest::Submit { request }).await)
}
async fn output(server: &Server, id: &str) -> String {
    let mut after = 0;
    let mut bytes = Vec::new();
    loop {
        let LocalResponse::Output { chunks } = rpc(
            server,
            LocalRequest::ReadOutput {
                task_id: id.into(),
                after_sequence: after,
            },
        )
        .await
        else {
            panic!("output")
        };
        if chunks.is_empty() {
            return String::from_utf8(bytes).unwrap();
        }
        after = chunks.last().unwrap().sequence;
        bytes.extend(
            chunks
                .iter()
                .filter(|chunk| matches!(chunk.stream, OutputStream::Stdout))
                .flat_map(|chunk| chunk.bytes.iter())
                .copied(),
        );
    }
}
#[tokio::test]
async fn profile_projects_preserve_command_origin_dedup_cancel_and_crash_recovery() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    for name in ["first project ✓", "second project"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    let mut json = serde_json::json!({"schemaVersion":1,"instanceId":format!("profiles-{}",&uuid::Uuid::new_v4().to_string()[..8]),"nodeId":"profile-test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "recipeProfiles":[{"id":"common","label":"Общие команды ✓","description":"Один профиль для двух проектов","recipes":[
            {"id":"inspect","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":["arguments","${projectId}","${workspaceRoot}","Привет ✓","$(literal); & quote\""],"workingDirectory":".","timeoutMs":10000},
            {"id":"tree","executable":env!("CARGO_BIN_EXE_winbridge-fixture"),"arguments":["tree"],"workingDirectory":".","timeoutMs":60000}]}],
        "projects":[{"id":"first","workspaceRoot":root.join("first project ✓"),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipeProfile":"common","recipes":[]},{"id":"second","workspaceRoot":root.join("second project"),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipeProfile":"common","recipes":[]}]
    });
    let path = root.join("worker.local.json");
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let mut server = start(&path).await;
    let LocalResponse::Info { info } = rpc(&server, LocalRequest::Info).await else {
        panic!("info")
    };
    assert_eq!(info.command_handlers.as_ref().unwrap().len(), 2);
    assert_eq!(
        info.projects[0].recipes[0]
            .origin
            .as_ref()
            .unwrap()
            .profile_id
            .as_deref(),
        Some("common")
    );
    let mut completed = Vec::new();
    for (project, folder) in [("first", "first project ✓"), ("second", "second project")] {
        let request = request(project, "same-project-scoped-key", "inspect");
        let accepted = submit(&server, request.clone()).await;
        assert_eq!(
            submit(&server, request.clone()).await.task_id,
            accepted.task_id
        );
        let done = wait(&server, &accepted.task_id, true).await;
        assert_eq!(done.status, TaskStatus::Succeeded);
        assert_eq!(done.exit_code, Some(0));
        let body = output(&server, &done.task_id).await;
        let expected = serde_json::to_string(&done.resolved_command.arguments[1..]).unwrap();
        assert!(body.contains(&expected), "{body}");
        assert_eq!(done.resolved_command.arguments[1], project);
        assert_eq!(
            done.resolved_command.arguments[2],
            root.join(folder).to_str().unwrap()
        );
        assert_eq!(
            fs::read_to_string(root.join(folder).join("runs.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let mut changed = request;
        changed.timeout_ms = 5000;
        assert!(matches!(
            rpc(&server, LocalRequest::Submit { request: changed }).await,
            LocalResponse::Error { .. }
        ));
        completed.push(done);
    }
    let mut direct = request("first", "denied", "inspect");
    direct.command = CommandSource::Direct {
        executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
        arguments: vec!["success".into()],
        working_directory: ".".into(),
    };
    assert!(matches!(
        rpc(&server, LocalRequest::Submit { request: direct }).await,
        LocalResponse::Error { .. }
    ));
    let cancellable = submit(&server, request("first", "cancel-tree", "tree")).await;
    wait(&server, &cancellable.task_id, false).await;
    task(
        rpc(
            &server,
            LocalRequest::Cancel {
                task_id: cancellable.task_id.clone(),
            },
        )
        .await,
    );
    let cancelled = wait(&server, &cancellable.task_id, true).await;
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    let crash_request = request("second", "crash-owned-tree", "tree");
    let crashed = submit(&server, crash_request.clone()).await;
    let running = wait(&server, &crashed.task_id, false).await;
    server.crash_group = running.process_id;
    let queued_request = request("second", "queued-old-profile", "inspect");
    let queued = submit(&server, queued_request.clone()).await;
    assert_eq!(queued.status, TaskStatus::Queued);
    drop(server);
    json["recipeProfiles"][0]["label"] = serde_json::json!("Изменённый профиль");
    json["recipeProfiles"][0]["recipes"][0]["arguments"][3] =
        serde_json::json!("changed after acceptance");
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let server = start(&path).await;
    let recovered = get(&server, &crashed.task_id).await;
    assert_eq!(recovered.status, TaskStatus::Indeterminate);
    assert_eq!(
        submit(&server, crash_request).await.task_id,
        crashed.task_id
    );
    let blocked = wait(&server, &queued.task_id, true).await;
    assert_eq!(blocked.status, TaskStatus::Failed);
    assert!(blocked
        .reason
        .as_ref()
        .unwrap()
        .contains("changed since acceptance"));
    assert_eq!(
        submit(&server, queued_request).await.task_id,
        queued.task_id
    );
    for previous in &completed {
        let restored = get(&server, &previous.task_id).await;
        assert_eq!(restored.recipe_origin, previous.recipe_origin);
        assert_eq!(restored.resolved_command, previous.resolved_command);
        assert_eq!(
            restored
                .recipe_origin
                .as_ref()
                .unwrap()
                .profile_label
                .as_deref(),
            Some("Общие команды ✓")
        );
    }
    let uncertain = submit(
        &server,
        request("second", "explicit-new-profile", "inspect"),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let uncertain = get(&server, &uncertain.task_id).await;
    assert_eq!(uncertain.status, TaskStatus::Queued);
    assert!(uncertain
        .reason
        .as_deref()
        .unwrap()
        .contains(&crashed.task_id));
    assert!(uncertain.process_id.is_none());
    let cancelled_wait = task(
        rpc(
            &server,
            LocalRequest::Cancel {
                task_id: uncertain.task_id,
            },
        )
        .await,
    );
    assert_eq!(cancelled_wait.status, TaskStatus::Cancelled);
    // The crash blocks only its project. A healthy independent project can
    // execute the changed profile without treating a new epoch as cleanup proof.
    let next = submit(&server, request("first", "explicit-new-profile", "inspect")).await;
    let next = wait(&server, &next.task_id, true).await;
    assert_eq!(next.status, TaskStatus::Succeeded);
    assert_eq!(
        next.recipe_origin
            .as_ref()
            .unwrap()
            .profile_label
            .as_deref(),
        Some("Изменённый профиль")
    );
    // Remove the additive field to prove legacy journal records still deserialize.
    let mut legacy = serde_json::to_value(&completed[0]).unwrap();
    legacy.as_object_mut().unwrap().remove("recipeOrigin");
    assert!(serde_json::from_value::<TaskRecord>(legacy)
        .unwrap()
        .recipe_origin
        .is_none());
    if let Some(directory) = std::env::var_os("WINBRIDGE_PROFILE_GATE_REPORT") {
        let proof = serde_json::json!({"passed":true,"platform":info.platform,"build":info.build,"projects":info.projects,"completed":completed,"cancelled":cancelled,"recovered":recovered,"blockedQueued":blocked,"explicitNewProfile":next});
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(&directory).join("profiles-proof.json"))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
}
