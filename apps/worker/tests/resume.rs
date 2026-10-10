//! Actual owned worker crash/restart, same-key reconciliation and explicit resumed import.
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::{DeliverySettings, WorkerConfig};
use winbridge_protocol::*;
use winbridge_workspace_sync as sync;

struct Server {
    child: Child,
    endpoint: String,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
async fn start(root: &Path, delivery: &DeliverySettings, instance: &str) -> Server {
    fs::create_dir_all(root).unwrap();
    let config:WorkerConfig=serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":instance,"nodeId":"resume-test","stateDirectory":root.join("state"),
        "maxOutputBytesPerTask":1048576,"delivery":delivery,"worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]
    })).unwrap();
    let config_path = root.join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let endpoint = winbridge_transport::endpoint(&config.state_directory, instance).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_winbridge-worker"))
        .arg("--config")
        .arg(config_path)
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let server = Server { child, endpoint };
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if matches!(
            winbridge_transport::call(&server.endpoint, &LocalRequest::Info).await,
            Ok(LocalResponse::Info { .. })
        ) {
            return server;
        }
        assert!(Instant::now() < deadline, "isolated resume worker startup");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn rpc(server: &Server, request: LocalRequest) -> LocalResponse {
    let response = winbridge_transport::call(&server.endpoint, &request)
        .await
        .unwrap();
    assert!(
        !matches!(response, LocalResponse::Error { .. }),
        "{response:?}"
    );
    response
}
fn imported(response: LocalResponse) -> ImportRecord {
    let LocalResponse::Import { import } = response else {
        panic!("{response:?}")
    };
    *import
}
async fn get(server: &Server, id: &str) -> ImportRecord {
    imported(
        rpc(
            server,
            LocalRequest::GetImport {
                import_id: id.into(),
            },
        )
        .await,
    )
}
async fn wait(server: &Server, id: &str) -> ImportRecord {
    wait_measured(server, id, None).await.0
}
async fn wait_measured(
    server: &Server,
    id: &str,
    attention_task: Option<&str>,
) -> (ImportRecord, Vec<f64>, Option<f64>) {
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut info_ms = Vec::new();
    let mut attention_ms = None;
    loop {
        let record = get(server, id).await;
        if record.status.is_terminal() {
            return (record, info_ms, attention_ms);
        }
        if record.status == TaskStatus::Running && info_ms.len() < 256 {
            let started = Instant::now();
            rpc(server, LocalRequest::Info).await;
            info_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            if attention_ms.is_none() {
                if let Some(task_id) = attention_task {
                    let started = Instant::now();
                    rpc(
                        server,
                        LocalRequest::RequestAttention {
                            request: AttentionSubmission {
                                request_id: format!("attention-{id}"),
                                idempotency_key: format!("attention-{id}"),
                                task_id: task_id.into(),
                                severity: AttentionSeverity::Info,
                                message: "Resume gate: control request during payload transfer"
                                    .into(),
                            },
                        },
                    )
                    .await;
                    attention_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        assert!(Instant::now() < deadline, "{record:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn seed_attention_task(server: &Server) -> String {
    let LocalResponse::Task { task } = rpc(
        server,
        LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "control-fixture".into(),
                project_id: "fixture".into(),
                idempotency_key: "control-fixture".into(),
                snapshot_id: None,
                timeout_ms: 30000,
                command: CommandSource::Direct {
                    executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                    arguments: vec!["success".into()],
                    working_directory: ".".into(),
                },
            },
        },
    )
    .await
    else {
        panic!("control fixture ACK")
    };
    let id = task.task_id;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let LocalResponse::Task { task } = rpc(
            server,
            LocalRequest::GetTask {
                task_id: id.clone(),
            },
        )
        .await
        else {
            panic!("control fixture state")
        };
        if task.status.is_terminal() {
            assert_eq!(task.status, TaskStatus::Succeeded);
            return id;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn git(root: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success());
}

async fn payload_checkpoint(server: &Server, id: &str, minimum: u64) -> ImportRecord {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let current = get(server, id).await;
        if current
            .transfer
            .as_ref()
            .is_some_and(|s| s.shared_bytes >= minimum && s.shared_bytes < s.payload_bytes)
        {
            return current;
        }
        assert!(
            !current.status.is_terminal(),
            "Import finished before the planned checkpoint: {current:?}"
        );
        assert!(
            Instant::now() < deadline,
            "planned payload checkpoint deadline"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test]
async fn native_crash_explicit_resume_dedup_pin_and_offline_build() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let baseline_instance = format!("resume-baseline-{}", &suffix[..8]);
    let resumed_instance = format!("resume-crash-{}", &suffix[..8]);
    let repo = root.join("source");
    fs::create_dir(&repo).unwrap();
    fs::create_dir(repo.join("src")).unwrap();
    let external_delivery = std::env::var_os("WINBRIDGE_RESUME_GATE_DELIVERY");
    let shared = external_delivery
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("WINBRIDGE_RESUME_GATE_SHARED")
                .map(|p| Path::new(&p).join(format!("worker-resume-{}", uuid::Uuid::new_v4())))
                .unwrap_or_else(|| root.join("shared"))
        });
    if external_delivery.is_none() {
        fs::create_dir(&shared).unwrap();
    }
    let blob = vec![0xB6; 32 * 1024 * 1024 + 31];
    fs::write(repo.join("blob.dat"), &blob).unwrap();
    fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"resume-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("Cargo.lock"),
        "version = 3\n[[package]]\nname = \"resume-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/main.rs"),
        "fn main(){println!(\"Проверенный resume → Windows build ✓\");}\n",
    )
    .unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "resume",
        ],
    );
    let delivery = DeliverySettings {
        adapter: "shared-folder".into(),
        root: shared,
        namespace: "WinBridge".into(),
        peer_root: None,
    };
    let publication: sync::PublicationMeasurement =
        if let Some(path) = std::env::var_os("WINBRIDGE_RESUME_GATE_PUBLICATION") {
            assert!(external_delivery.is_some());
            serde_json::from_slice(&sync::read_bounded(Path::new(&path), 65536).unwrap()).unwrap()
        } else {
            sync::publish_resumable_measured(
                &delivery,
                &repo,
                &root.join("capture"),
                "fixture",
                "HEAD",
                None,
            )
            .unwrap()
        };
    assert_eq!(publication.ready.schema_version, 3);
    let request = ImportSubmission {
        protocol_version: 1,
        request_id: "baseline".into(),
        project_id: "fixture".into(),
        snapshot_id: publication.ready.snapshot_id.clone(),
        manifest_sha256: publication.ready.manifest_sha256.clone(),
        idempotency_key: "baseline".into(),
        timeout_ms: 180000,
    };
    let baseline = start(&root.join("baseline"), &delivery, &baseline_instance).await;
    let baseline_attention = seed_attention_task(&baseline).await;
    let started = Instant::now();
    let full = imported(
        rpc(
            &baseline,
            LocalRequest::ImportSnapshot {
                request: request.clone(),
            },
        )
        .await,
    );
    let (full, full_info_ms, full_attention_ms) =
        wait_measured(&baseline, &full.import_id, Some(&baseline_attention)).await;
    let full_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(full.status, TaskStatus::Succeeded, "{full:?}");
    assert_eq!(full.transfer.as_ref().unwrap().reused_bytes, 0);
    assert_eq!(
        full.transfer.as_ref().unwrap().shared_bytes,
        publication.ready.payload_bytes
    );
    drop(baseline);

    let local = root.join("resumed");
    let mut server = start(&local, &delivery, &resumed_instance).await;
    let resumed_attention = seed_attention_task(&server).await;
    let mut cancellation = request.clone();
    cancellation.request_id = "cancel".into();
    cancellation.idempotency_key = "cancel".into();
    let cancelling = imported(
        rpc(
            &server,
            LocalRequest::ImportSnapshot {
                request: cancellation.clone(),
            },
        )
        .await,
    );
    payload_checkpoint(&server, &cancelling.import_id, 4 * 1024 * 1024).await;
    let cancel_started = Instant::now();
    rpc(
        &server,
        LocalRequest::CancelImport {
            import_id: cancelling.import_id.clone(),
        },
    )
    .await;
    let cancel_ack_ms = cancel_started.elapsed().as_secs_f64() * 1000.0;
    let cancelled = wait(&server, &cancelling.import_id).await;
    let cancel_terminal_ms = cancel_started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cancelled.status, TaskStatus::Cancelled, "{cancelled:?}");
    let duplicate = imported(
        rpc(
            &server,
            LocalRequest::ImportSnapshot {
                request: cancellation,
            },
        )
        .await,
    );
    assert_eq!(duplicate.import_id, cancelling.import_id);
    assert_eq!(duplicate.status, TaskStatus::Cancelled);
    let mut first = request.clone();
    first.request_id = "crash".into();
    first.idempotency_key = "crash".into();
    let accepted = imported(
        rpc(
            &server,
            LocalRequest::ImportSnapshot {
                request: first.clone(),
            },
        )
        .await,
    );
    let checkpoint = payload_checkpoint(&server, &accepted.import_id, 20 * 1024 * 1024).await;
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    drop(server);
    assert!(!sync::generation_path(&local, &request.snapshot_id)
        .unwrap()
        .exists());
    let server = start(&local, &delivery, &resumed_instance).await;
    let recovered = get(&server, &accepted.import_id).await;
    assert_eq!(recovered.status, TaskStatus::Interrupted, "{recovered:?}");
    let duplicate = imported(
        rpc(
            &server,
            LocalRequest::ImportSnapshot {
                request: first.clone(),
            },
        )
        .await,
    );
    assert_eq!(duplicate.import_id, accepted.import_id);
    assert_eq!(duplicate.status, TaskStatus::Interrupted);
    let mut conflict = first;
    conflict.manifest_sha256 = "a".repeat(64);
    assert!(matches!(
        winbridge_transport::call(
            &server.endpoint,
            &LocalRequest::ImportSnapshot { request: conflict }
        )
        .await
        .unwrap(),
        LocalResponse::Error { .. }
    ));
    let mut explicit = request.clone();
    explicit.idempotency_key = "explicit-resume".into();
    explicit.request_id = "explicit-resume".into();
    let started = Instant::now();
    let resumed = imported(
        rpc(
            &server,
            LocalRequest::ImportSnapshot {
                request: explicit.clone(),
            },
        )
        .await,
    );
    assert_ne!(resumed.import_id, accepted.import_id);
    let dedup = imported(rpc(&server, LocalRequest::ImportSnapshot { request: explicit }).await);
    assert_eq!(resumed.import_id, dedup.import_id);
    let (resumed, resumed_info_ms, resumed_attention_ms) =
        wait_measured(&server, &resumed.import_id, Some(&resumed_attention)).await;
    let resumed_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(resumed.status, TaskStatus::Succeeded, "{resumed:?}");
    let summary = resumed.transfer.as_ref().unwrap();
    assert!(summary.reused_bytes >= 4 * 1024 * 1024);
    assert!(summary.shared_bytes < summary.payload_bytes);
    assert_eq!(
        summary.shared_bytes + summary.reused_bytes,
        summary.payload_bytes
    );
    assert!(resumed.events.len() < 20);
    let generation = sync::generation_path(&local, &request.snapshot_id).unwrap();
    let actual = fs::read(generation.join("blob.dat")).unwrap();
    assert_eq!(actual, blob);
    let manifest = sync::verified_generation(
        &local,
        "fixture",
        &request.snapshot_id,
        Some(&request.manifest_sha256),
        |_, _, _| Ok(()),
    )
    .unwrap();
    let build = TaskSubmission {
        protocol_version: 1,
        request_id: "build".into(),
        project_id: "fixture".into(),
        idempotency_key: "build".into(),
        snapshot_id: Some(request.snapshot_id.clone()),
        timeout_ms: 180000,
        command: CommandSource::Direct {
            executable: "cargo".into(),
            arguments: vec![
                "build".into(),
                "--locked".into(),
                "--offline".into(),
                "--target-dir".into(),
                local.join("build-cache").to_str().unwrap().into(),
            ],
            working_directory: ".".into(),
        },
    };
    let LocalResponse::Task { task } = rpc(
        &server,
        LocalRequest::Submit {
            request: build.clone(),
        },
    )
    .await
    else {
        panic!("build ACK")
    };
    let build_id = task.task_id;
    let deadline = Instant::now() + Duration::from_secs(180);
    let task = loop {
        let LocalResponse::Task { task } = rpc(
            &server,
            LocalRequest::GetTask {
                task_id: build_id.clone(),
            },
        )
        .await
        else {
            panic!("build state")
        };
        if task.status.is_terminal() {
            break task;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(task.status, TaskStatus::Succeeded, "{task:?}");
    assert_eq!(task.exit_code, Some(0));
    assert_eq!(
        task.snapshot.as_ref().unwrap().manifest_sha256,
        request.manifest_sha256
    );
    let LocalResponse::Task { task: again } =
        rpc(&server, LocalRequest::Submit { request: build }).await
    else {
        panic!("build dedup")
    };
    assert_eq!(again.task_id, build_id);
    let executable = local.join("build-cache/debug").join(if cfg!(windows) {
        "resume-fixture.exe"
    } else {
        "resume-fixture"
    });
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Проверенный resume → Windows build ✓"));
    let LocalResponse::Info { info } = rpc(&server, LocalRequest::Info).await else {
        panic!("Info")
    };
    assert!(info.snapshot_resume_available);
    assert_eq!(info.active_task_count, Some(0));
    assert!(info.journal_recovery_required.is_none());
    assert!(info.maintenance_error.is_none());
    let proof = serde_json::json!({"schemaVersion":1,"platform":if cfg!(windows){"windows"}else{"linux"},"passed":true,"build":info.build,"ready":publication.ready,"publicationTimings":publication.timings,"blobSha256":sync::sha256(&actual),"files":manifest.files,"baseline":{"import":full,"observedMs":full_ms,"infoMs":full_info_ms,"attentionMs":full_attention_ms},"cancelled":cancelled,"cancelAckMs":cancel_ack_ms,"cancelTerminalMs":cancel_terminal_ms,"checkpoint":checkpoint,"recovered":recovered,"resumed":{"import":resumed,"observedMs":resumed_ms,"infoMs":resumed_info_ms,"attentionMs":resumed_attention_ms},"buildTask":task});
    if let Some(report) = std::env::var_os("WINBRIDGE_RESUME_GATE_SHARED") {
        let path = Path::new(&report).join("resume-proof.json");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut file, &serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
        file.sync_all().unwrap();
    }
    println!(
        "resume proof: full {} bytes, reused {} bytes, shared {} bytes",
        summary.payload_bytes, summary.reused_bytes, summary.shared_bytes
    );
}
