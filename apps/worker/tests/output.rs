use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::store::Store;

fn config(root: &Path, instance: &str) -> WorkerConfig {
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":instance,"nodeId":"output-test",
        "stateDirectory":root.join("state"),"maxOutputBytesPerTask":8*1024*1024,
        "worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"fixture","workspaceRoot":root,"allowDirectCommands":true,
            "maxConcurrentMutations":1,"recipes":[]}]
    }))
    .unwrap()
}
fn request(key: &str, mode: &str) -> TaskSubmission {
    TaskSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "fixture".into(),
        idempotency_key: key.into(),
        snapshot_id: None,
        timeout_ms: 30000,
        command: CommandSource::Direct {
            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
        },
    }
}
fn seed(root: &Path) -> (Store, String) {
    let config = config(root, "output-store");
    let mut store = Store::open(&root.join("journal.sqlite")).unwrap();
    let request = request("seed", "success");
    let task = store
        .accept(
            request.clone(),
            winbridge_runner::resolve(&config, &request).unwrap(),
            None,
        )
        .unwrap();
    (store, task.task_id)
}

#[test]
fn byte_bounded_pages_preserve_rows_offsets_and_worst_case_json() {
    let temp = TempDir::new().unwrap();
    let (mut store, id) = seed(temp.path());
    for index in 0..300 {
        store
            .append_output(
                &id,
                if index % 2 == 0 {
                    OutputStream::Stdout
                } else {
                    OutputStream::Stderr
                },
                index / 2 * 8192,
                &[255; 8192],
                8 * 1024 * 1024,
            )
            .unwrap();
    }
    let mut cursor = 0;
    let mut offsets = [0, 0];
    let mut pages = 0;
    loop {
        let page = store.output(&id, cursor).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.iter().map(|c| c.bytes.len()).sum::<usize>() <= 192 * 1024);
        let response = LocalResponse::Output {
            chunks: page.clone(),
        };
        assert!(
            serde_json::to_vec(&response).unwrap().len() < winbridge_transport::MAX_FRAME_BYTES
        );
        for chunk in page {
            assert_eq!(chunk.sequence, cursor + 1);
            let stream = match chunk.stream {
                OutputStream::Stdout => 0,
                OutputStream::Stderr => 1,
            };
            assert_eq!(chunk.offset, offsets[stream]);
            assert_eq!(chunk.bytes, [255; 8192]);
            offsets[stream] += chunk.bytes.len() as u64;
            cursor = chunk.sequence;
        }
        pages += 1;
    }
    assert_eq!(cursor, 300);
    assert_eq!(offsets, [150 * 8192; 2]);
    assert_eq!(pages, 13);
    assert!(store.output(&id, i64::MAX as u64 + 1).is_err());
}

#[test]
fn tiny_rows_batch_and_partial_cursor_survive_reopen() {
    let temp = TempDir::new().unwrap();
    let (mut store, id) = seed(temp.path());
    for offset in 0..1500 {
        store
            .append_output(&id, OutputStream::Stdout, offset, &[255], 4096)
            .unwrap();
    }
    let first = store.output(&id, 0).unwrap();
    assert_eq!(first.len(), 1024);
    drop(store);
    let store = Store::open(&temp.path().join("journal.sqlite")).unwrap();
    let remainder = store.output(&id, first.last().unwrap().sequence).unwrap();
    assert_eq!(remainder.len(), 476);
    assert_eq!(remainder[0].offset, 1024);
    assert_eq!(store.output(&id, 333).unwrap()[0].sequence, 334);
    assert!(store.output(&id, 1500).unwrap().is_empty());
}

#[test]
fn historical_large_row_progresses_without_exceeding_json_budget() {
    let temp = TempDir::new().unwrap();
    let (mut store, id) = seed(temp.path());
    store
        .append_output(
            &id,
            OutputStream::Stdout,
            0,
            &vec![255; 200 * 1024],
            1024 * 1024,
        )
        .unwrap();
    store
        .append_output(&id, OutputStream::Stdout, 200 * 1024, b"tail", 1024 * 1024)
        .unwrap();
    let first = store.output(&id, 0).unwrap();
    assert_eq!(first.len(), 1);
    assert!(
        serde_json::to_vec(&LocalResponse::Output {
            chunks: first.clone()
        })
        .unwrap()
        .len()
            < winbridge_transport::MAX_FRAME_BYTES
    );
    assert_eq!(
        store.output(&id, first[0].sequence).unwrap()[0].bytes,
        b"tail"
    );
}

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
async fn start(executable: &Path, root: &Path, instance: &str) -> Server {
    let config = config(root, instance);
    let path = root.join("worker.local.json");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let endpoint = winbridge_transport::endpoint(&config.state_directory, instance).unwrap();
    let child = Command::new(executable)
        .args(["--config"])
        .arg(&path)
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let server = Server { child, endpoint };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if winbridge_transport::call(&server.endpoint, &LocalRequest::Info)
            .await
            .is_ok()
        {
            return server;
        }
        assert!(
            Instant::now() < deadline,
            "isolated worker startup deadline"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
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
fn task(response: LocalResponse) -> TaskRecord {
    let LocalResponse::Task { task } = response else {
        panic!("{response:?}")
    };
    *task
}
async fn wait(server: &Server, id: &str) -> TaskRecord {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let task = task(rpc(server, LocalRequest::GetTask { task_id: id.into() }).await);
        if task.status.is_terminal() {
            return task;
        }
        assert!(Instant::now() < deadline, "task completion deadline: {id}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn read(
    server: &Server,
    id: &str,
    cursor: &mut u64,
    output: &mut [Vec<u8>; 2],
) -> serde_json::Value {
    let started = Instant::now();
    let mut rpc_ms = 0.0;
    let mut pages = 0;
    let mut info_ms = Vec::new();
    loop {
        let call_started = Instant::now();
        let response = rpc(
            server,
            LocalRequest::ReadOutput {
                task_id: id.into(),
                after_sequence: *cursor,
            },
        )
        .await;
        rpc_ms += call_started.elapsed().as_secs_f64() * 1000.0;
        assert!(
            serde_json::to_vec(&response).unwrap().len() < winbridge_transport::MAX_FRAME_BYTES
        );
        let LocalResponse::Output { chunks } = response else {
            panic!()
        };
        if chunks.is_empty() {
            break;
        }
        for chunk in chunks {
            assert_eq!(chunk.sequence, *cursor + 1);
            let stream = match chunk.stream {
                OutputStream::Stdout => 0,
                OutputStream::Stderr => 1,
            };
            assert_eq!(chunk.offset, output[stream].len() as u64);
            output[stream].extend(chunk.bytes);
            *cursor = chunk.sequence;
        }
        pages += 1;
        let now = Instant::now();
        let LocalResponse::Info { info } = rpc(server, LocalRequest::Info).await else {
            panic!()
        };
        assert!(info.journal_recovery_required.is_none());
        info_ms.push(now.elapsed().as_secs_f64() * 1000.0);
    }
    serde_json::json!({"pages":pages,"readRpcMs":rpc_ms,"observedReadAndInfoMs":started.elapsed().as_secs_f64()*1000.0,"infoMs":info_ms})
}

#[tokio::test]
async fn native_ipc_output_hashes_dedup_cursor_recovery_and_control() {
    let temp = TempDir::new().unwrap();
    let current = Path::new(env!("CARGO_BIN_EXE_winbridge-worker"));
    let mut cases = Vec::new();
    // Reference binary is read-only; both versions use NEW test state/project paths.
    let reference = std::env::var_os("WINBRIDGE_OUTPUT_REFERENCE_WORKER");
    let mut versions = vec![("current", current.to_path_buf())];
    if let Some(path) = reference {
        versions.insert(0, ("reference", path.into()));
    }
    for (version, executable) in versions {
        let root = temp.path().join(version);
        fs::create_dir(&root).unwrap();
        let instance = format!("output-{}", uuid::Uuid::new_v4());
        let server = start(&executable, &root, &instance).await;
        let LocalResponse::Info { info } = rpc(&server, LocalRequest::Info).await else {
            panic!()
        };
        let build = serde_json::to_value(info.build).unwrap();
        for repeat in 0..3 {
            let request = request(&format!("flood-{repeat}"), "flood");
            let accepted = task(
                rpc(
                    &server,
                    LocalRequest::Submit {
                        request: request.clone(),
                    },
                )
                .await,
            );
            assert_eq!(
                accepted.task_id,
                task(
                    rpc(
                        &server,
                        LocalRequest::Submit {
                            request: request.clone()
                        }
                    )
                    .await
                )
                .task_id
            );
            let completed = wait(&server, &accepted.task_id).await;
            assert_eq!(completed.status, TaskStatus::Succeeded);
            let mut output = [Vec::new(), Vec::new()];
            let mut cursor = 0;
            let measurement = read(&server, &accepted.task_id, &mut cursor, &mut output).await;
            assert!(output[0].ends_with(&vec![b'x'; 1024 * 1024]));
            assert_eq!(output[1], vec![b'x'; 1024 * 1024]);
            assert_eq!(
                output.iter().map(Vec::len).sum::<usize>() as u64,
                completed.output_bytes
            );
            cases.push(serde_json::json!({"version":version,"repeat":repeat,"build":build,"taskId":accepted.task_id,"measurement":measurement,"stdoutBytes":output[0].len(),"stderrBytes":output[1].len(),"bodySha256":format!("{:x}",Sha256::digest(&output[1]))}));
        }
        let long = task(
            rpc(
                &server,
                LocalRequest::Submit {
                    request: request("cancel", "tree"),
                },
            )
            .await,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let LocalResponse::Output { chunks } = rpc(
                &server,
                LocalRequest::ReadOutput {
                    task_id: long.task_id.clone(),
                    after_sequence: 0,
                },
            )
            .await
            else {
                panic!()
            };
            if chunks
                .iter()
                .any(|chunk| String::from_utf8_lossy(&chunk.bytes).contains("child pid="))
            {
                break;
            }
            assert!(Instant::now() < deadline, "child not observed");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let cancel_started = Instant::now();
        rpc(
            &server,
            LocalRequest::Cancel {
                task_id: long.task_id.clone(),
            },
        )
        .await;
        assert_eq!(
            wait(&server, &long.task_id).await.status,
            TaskStatus::Cancelled
        );
        let cancel_ms = cancel_started.elapsed().as_secs_f64() * 1000.0;
        let attention_started = Instant::now();
        rpc(
            &server,
            LocalRequest::RequestAttention {
                request: AttentionSubmission {
                    request_id: "attention".into(),
                    idempotency_key: "attention".into(),
                    task_id: long.task_id.clone(),
                    severity: AttentionSeverity::Info,
                    message: "Output gate: explicit cancellation confirmed".into(),
                },
            },
        )
        .await;
        let attention_ms = attention_started.elapsed().as_secs_f64() * 1000.0;
        let saved = task(
            rpc(
                &server,
                LocalRequest::Submit {
                    request: request("cursor-restart", "success"),
                },
            )
            .await,
        );
        wait(&server, &saved.task_id).await;
        // Read one durable row, then replace only the isolated worker process.
        let LocalResponse::Output { chunks } = rpc(
            &server,
            LocalRequest::ReadOutput {
                task_id: saved.task_id.clone(),
                after_sequence: 0,
            },
        )
        .await
        else {
            panic!()
        };
        let first = &chunks[0];
        let mut output = [Vec::new(), Vec::new()];
        let stream = match first.stream {
            OutputStream::Stdout => 0,
            OutputStream::Stderr => 1,
        };
        output[stream].extend_from_slice(&first.bytes);
        let mut cursor = first.sequence;
        drop(server);
        let reopened = start(current, &root, &instance).await;
        read(&reopened, &saved.task_id, &mut cursor, &mut output).await;
        assert!(String::from_utf8(output[0].clone())
            .unwrap()
            .contains("Привет ✓"));
        assert!(String::from_utf8(output[1].clone())
            .unwrap()
            .contains("diagnostic"));
        assert_eq!(
            saved.task_id,
            task(
                rpc(
                    &reopened,
                    LocalRequest::Submit {
                        request: request("cursor-restart", "success")
                    }
                )
                .await
            )
            .task_id
        );
        let LocalResponse::Info { info } = rpc(&reopened, LocalRequest::Info).await else {
            panic!()
        };
        assert_eq!(info.active_task_count, Some(0));
        assert!(info.journal_recovery_required.is_none());
        assert!(info.maintenance_error.is_none());
        assert_eq!(
            fs::read_to_string(root.join("runs.log"))
                .unwrap()
                .lines()
                .filter(|line| line.ends_with(" success"))
                .count(),
            1
        );
        cases.push(serde_json::json!({"version":version,"cancelMs":cancel_ms,"attentionMs":attention_ms,"cursorRestartDedup":true,"healthy":true}));
    }
    let proof = serde_json::json!({"schemaVersion":1,"platform":std::env::consts::OS,"passed":true,"cases":cases});
    if let Some(directory) = std::env::var_os("WINBRIDGE_OUTPUT_GATE_REPORT") {
        let path = Path::new(&directory).join("output-proof.json");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
    println!("OUTPUT_GATE {}", proof);
}
