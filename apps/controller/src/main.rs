use std::{io::Read, path::Path, process::ExitCode};
use winbridge_core::controller_config::ControllerConfig;
use winbridge_protocol::{LocalRequest, LocalResponse};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("WinBridge controller: {error}");
            ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [operation, path] = args.as_slice() {
        if operation == "--check-snapshot-manifest" {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|e| format!("manifest {path}: {e}"))?
                .take(winbridge_core::snapshot::MAX_MANIFEST_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("manifest {path}: {e}"))?;
            let manifest = winbridge_core::snapshot::parse_manifest(&bytes)?;
            println!(
                "{}",
                serde_json::json!({"validation":"metadata_only", "projectId":manifest.project_id, "snapshotId":manifest.snapshot_id, "files":manifest.files.len(), "totalBytes":manifest.total_bytes, "payloadVerified":false, "imported":false})
            );
            return Ok(());
        }
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge controller — Debian dispatch to the configured worker\n  --describe\n  --check-snapshot-manifest PATH (metadata only; does not import or verify payload bytes)\n  --config PATH publish PROJECT REVISION\n  --config PATH measure-publish PROJECT REVISION (monotonic stage timings)\n  --config PATH validate | serve | mcp | status | info | list\n  --config PATH submit REQUEST.json\n  --config PATH attention REQUEST.json | attention-list [AFTER_SEQUENCE] | attention-history [AFTER_SEQUENCE] | ack-attention ID\n  --config PATH attention-events [AFTER_SEQUENCE] | attention-events-history [AFTER_SEQUENCE] | ack-attention-event ID\n  --config PATH history QUERY.json | diagnostic REQUEST.json | storage-usage\n  --config PATH import REQUEST.json | imports | get-import ID | cancel-import ID\n  --config PATH preview PROJECT SNAPSHOT\n  --config PATH snapshots PROJECT [OFFSET]\n  --config PATH workflow REQUEST.json | workflows | get-workflow ID | cancel-workflow ID\n  --config PATH applications | get-application ID | stop-application ID | log ID [CURSOR.json]\n  --config PATH get TASK_ID | cancel TASK_ID\n  --config PATH output TASK_ID [AFTER_SEQUENCE]\n  --config PATH watch TASK_ID [AFTER_EVENT_SEQUENCE] [AFTER_OUTPUT_SEQUENCE]\nWatch prints JSON updates, resumes the same task after reconnect, and never submits commands.");
        return Ok(());
    }
    if args == ["--describe"] {
        let mut descriptor = winbridge_core::descriptor(winbridge_protocol::Component::Controller);
        descriptor.scaffold = false;
        descriptor.implemented_capabilities = vec![
            winbridge_protocol::Capability::TaskSubmit,
            winbridge_protocol::Capability::TaskCancel,
            winbridge_protocol::Capability::WorkspaceImport,
            winbridge_protocol::Capability::LogRead,
            winbridge_protocol::Capability::AttentionRequest,
            winbridge_protocol::Capability::AttentionTargets,
            winbridge_protocol::Capability::HistoryRead,
        ];
        descriptor
            .planned_capabilities
            .retain(|c| !descriptor.implemented_capabilities.contains(c));
        println!(
            "{}",
            serde_json::to_string_pretty(&descriptor).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.len() < 3 || args[0] != "--config" {
        return Err("Use --config PATH followed by an operation; see --help".into());
    }
    let config = ControllerConfig::load(Path::new(&args[1]))?;
    let operation = &args[2..];
    let address = winbridge_controller::endpoint(&config);
    match operation {
        [name] if name == "validate" => println!(
            "Valid local pair {}: ws://{}; worker {}; snapshot import available when delivery is configured",
            config.instance_id, config.controller.listen_address, config.controller.worker_node_id
        ),
        [name,project,revision] if name=="publish" => {
            let ready=winbridge_controller::publish(&config,project,revision)?;
            println!("{}",serde_json::to_string(&ready).map_err(|e|e.to_string())?);
        }
        [name,project,revision] if name=="measure-publish" => {
            let measurement=winbridge_controller::publish_measured(&config,project,revision)?;
            println!("{}",serde_json::to_string(&measurement).map_err(|e|e.to_string())?);
        }
        [name] if name == "mcp" => { return winbridge_controller::mcp::run(config,tokio::io::stdin(),tokio::io::stdout()).await; }
        [name] if name == "serve" => {
            return winbridge_controller::serve(winbridge_controller::Controller::new(config)?)
                .await
        }
        [name, id, after @ ..] if name == "watch" && after.len() <= 2 => {
            return watch(&address, id, cursor(after.first())?, cursor(after.get(1))?).await;
        }
        _ => {
            let request = match operation {
                [name, path] if name == "attention" => LocalRequest::RequestAttention {request:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),8192)?).map_err(|_|"Invalid attention request JSON")?},
                [name] if name == "attention-list" || name == "attention-history" => LocalRequest::ListAttention {after_sequence:0,pending_only:name=="attention-list"},
                [name, after] if name == "attention-list" || name == "attention-history" => LocalRequest::ListAttention {after_sequence:after.parse().map_err(|_|"Attention cursor must be an unsigned integer")?,pending_only:name=="attention-list"},
                [name, id] if name == "ack-attention" => LocalRequest::AcknowledgeAttention {attention_id:id.clone()},
                [name, path] if name == "history" => LocalRequest::History {query:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),65536)?).map_err(|_|"Invalid history query JSON")?},
                [name, path] if name == "diagnostic" => LocalRequest::PrepareTaskDiagnostic {request:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),262144)?).map_err(|_|"Invalid diagnostic request JSON")?},
                [name] if name == "storage-usage" => LocalRequest::StorageUsage,
                [name] if name == "attention-events" || name == "attention-events-history" => LocalRequest::ListAttentionEvents {after_sequence:0,pending_only:name=="attention-events"},
                [name, after] if name == "attention-events" || name == "attention-events-history" => LocalRequest::ListAttentionEvents {after_sequence:after.parse().map_err(|_|"Attention cursor must be an unsigned integer")?,pending_only:name=="attention-events"},
                [name, id] if name == "ack-attention-event" => LocalRequest::AcknowledgeAttentionEvent {attention_id:id.clone()},
                [name, path] if name == "workflow" => LocalRequest::SubmitWorkflow {request:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),65536)?).map_err(|_|"Invalid workflow JSON")?},
                [name] if name == "workflows" => LocalRequest::ListWorkflows,
                [name,id] if name == "get-workflow" => LocalRequest::GetWorkflow {workflow_id:id.clone()},
                [name,id] if name == "cancel-workflow" => LocalRequest::CancelWorkflow {workflow_id:id.clone()},
                [name] if name == "applications" => LocalRequest::ListApplications,
                [name,id] if name == "get-application" => LocalRequest::GetApplication {application_id:id.clone()},
                [name,id] if name == "stop-application" => LocalRequest::StopApplication {application_id:id.clone()},
                [name,id] if name == "log" => LocalRequest::ReadApplicationLog {application_id:id.clone(),cursor:None,max_bytes:65536},
                [name,id,path] if name == "log" => LocalRequest::ReadApplicationLog {application_id:id.clone(),cursor:Some(serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),8192)?).map_err(|_|"Invalid log cursor JSON")?),max_bytes:65536},
                [name] if name == "imports" => LocalRequest::ListImports,
                [name, project] if name == "snapshots" => LocalRequest::ListSnapshots {project_id:project.clone(),offset:0},
                [name, project, offset] if name == "snapshots" => LocalRequest::ListSnapshots {project_id:project.clone(),offset:offset.parse().map_err(|_|"Snapshot offset must be an integer")?},
                [name, id] if name == "get-import" => LocalRequest::GetImport {import_id:id.clone()},
                [name, id] if name == "cancel-import" => LocalRequest::CancelImport {import_id:id.clone()},
                [name, project, snapshot] if name == "preview" => LocalRequest::PreviewSnapshot {project_id:project.clone(),snapshot_id:snapshot.clone(),offset:0},
                [name, path] if name == "import" => LocalRequest::ImportSnapshot {request:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),65536)?).map_err(|_|"Invalid import request JSON")?},
                [name] if name == "status" => LocalRequest::ControllerStatus,
                [name] if name == "info" => LocalRequest::Info,
                [name] if name == "list" => LocalRequest::ListTasks,
                [name, id] if name == "get" => LocalRequest::GetTask {
                    task_id: id.clone(),
                },
                [name, id] if name == "cancel" => LocalRequest::Cancel {
                    task_id: id.clone(),
                },
                [name, id] if name == "output" => LocalRequest::ReadOutput {
                    task_id: id.clone(),
                    after_sequence: 0,
                },
                [name, id, after] if name == "output" => LocalRequest::ReadOutput {
                    task_id: id.clone(),
                    after_sequence: cursor(Some(after))?,
                },
                [name, path] if name == "submit" => {
                    let bytes = std::fs::read(path).map_err(|e| format!("request {path}: {e}"))?;
                    if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
                        return Err("Request file exceeds 1 MiB".into());
                    }
                    LocalRequest::Submit {
                        request: serde_json::from_slice(&bytes)
                            .map_err(|e| format!("request {path}: {e}"))?,
                    }
                }
                _ => return Err("Unsupported operation; see --help".into()),
            };
            let response = call(&address, request).await?;
            println!(
                "{}",
                serde_json::to_string(&response).map_err(|e| e.to_string())?
            );
        }
    }
    Ok(())
}
fn cursor(value: Option<&String>) -> Result<u64, String> {
    value
        .map(|value| {
            value
                .parse()
                .map_err(|_| "Sequence cursor must be an unsigned integer".into())
        })
        .unwrap_or(Ok(0))
}
async fn call(address: &str, request: LocalRequest) -> Result<LocalResponse, String> {
    let response = winbridge_transport::call(address, &request)
        .await
        .map_err(|e| format!("Controller IPC unavailable: {e}; reconcile the same task/key"))?;
    if let LocalResponse::Error { message } = response {
        return Err(message);
    }
    Ok(response)
}
async fn watch(
    address: &str,
    id: &str,
    mut event_cursor: u64,
    mut output_cursor: u64,
) -> Result<(), String> {
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut first = true;
    let mut offline = false;
    loop {
        let poll = async {
            let LocalResponse::Task { task } =
                call(address, LocalRequest::GetTask { task_id: id.into() }).await?
            else {
                return Err("Unexpected task response".into());
            };
            let LocalResponse::Output { chunks } = call(
                address,
                LocalRequest::ReadOutput {
                    task_id: id.into(),
                    after_sequence: output_cursor,
                },
            )
            .await?
            else {
                return Err("Unexpected output response".into());
            };
            Ok((task, chunks))
        };
        let result: Result<_, String> =
            tokio::select! { _ = &mut shutdown => return Ok(()), result = poll => result };
        match result {
            Ok((task, chunks)) => {
                let events: Vec<_> = task
                    .events
                    .iter()
                    .filter(|event| event.sequence > event_cursor)
                    .collect();
                let complete = task.status.is_terminal() && chunks.is_empty();
                if first || offline || !events.is_empty() || !chunks.is_empty() || complete {
                    println!(
                        "{}",
                        serde_json::json!({"kind":"task_update","task":task,"events":events,"output":chunks,"outputComplete":complete})
                    );
                }
                if let Some(event) = events.last() {
                    event_cursor = event.sequence;
                }
                if let Some(chunk) = chunks.last() {
                    output_cursor = chunk.sequence;
                }
                first = false;
                offline = false;
                if complete {
                    return Ok(());
                }
            }
            Err(error) => {
                if error.starts_with("taskId ") && error.ends_with(" was not found") {
                    return Err(error);
                }
                if !offline {
                    println!(
                        "{}",
                        serde_json::json!({"kind":"connection_unavailable","taskId":id,"message":error,"afterEventSequence":event_cursor,"afterOutputSequence":output_cursor})
                    );
                }
                offline = true;
            }
        }
        tokio::select! { _ = &mut shutdown => return Ok(()), _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {} }
    }
}
