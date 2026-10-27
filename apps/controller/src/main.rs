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
    if args == ["--contract-schemas"] {
        println!(
            "{}",
            serde_json::to_string(&winbridge_protocol::implemented_contract_schemas(
                winbridge_protocol::build_identity()
            ))
            .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge controller — Debian dispatch to the configured worker\n  --describe\n  --contract-schemas (actual local DTO fingerprints; read-only)\n  --check-snapshot-manifest PATH (metadata only; does not import or verify payload bytes)\n  --config PATH publish PROJECT REVISION\n  --config PATH measure-publish PROJECT REVISION (monotonic stage timings)\n  --config PATH publish-delta PROJECT REVISION BASE_SNAPSHOT (pinned base, measured)\n  --config PATH publish-resumable PROJECT REVISION [BASE_SNAPSHOT] (manifest-pinned 1 MiB blocks)\n  --config PATH import-delta REQUEST.json\n  --config PATH validate | serve | mcp | status | info | list\n  --config PATH readiness QUERY.json | verify-pair REQUEST.json | verification KEY\n  --config PATH operations REQUEST.json (readonly; pinned journal scope)\n  --config PATH controller-connections REQUEST.json (readonly links/counters; pinned context)\n  --config PATH operation-attribution REQUEST.json (readonly original creator)\n  --config PATH operation-cancellation REQUEST.json (readonly first cancellation intent; never cancels)\n  --config PATH invoke-v2 ENVELOPE.json (caller-pinned context; no fallback)\n  --config PATH submit REQUEST.json\n  --config PATH attention REQUEST.json | attention-list [AFTER_SEQUENCE] | attention-history [AFTER_SEQUENCE] | ack-attention ID\n  --config PATH attention-events [AFTER_SEQUENCE] | attention-events-history [AFTER_SEQUENCE] | ack-attention-event ID\n  --config PATH history QUERY.json | diagnostic REQUEST.json | storage-usage\n  --config PATH import REQUEST.json | imports | get-import ID | cancel-import ID\n  --config PATH preview PROJECT SNAPSHOT\n  --config PATH snapshots PROJECT [OFFSET]\n  --config PATH workflow REQUEST.json | workflows | get-workflow ID | cancel-workflow ID\n  --config PATH applications | get-application ID | stop-application ID | log ID [CURSOR.json]\n  --config PATH get TASK_ID | cancel TASK_ID\n  --config PATH output TASK_ID [AFTER_SEQUENCE]\n  --config PATH watch TASK_ID [AFTER_EVENT_SEQUENCE] [AFTER_OUTPUT_SEQUENCE]\nWatch prints JSON updates, resumes the same task after reconnect, and never submits commands.");
        return Ok(());
    }
    if args == ["--describe"] {
        let mut descriptor = winbridge_core::descriptor(winbridge_protocol::Component::Controller);
        descriptor.scaffold = false;
        descriptor.implemented_capabilities = vec![
            winbridge_protocol::Capability::TaskSubmit,
            winbridge_protocol::Capability::TaskCancel,
            winbridge_protocol::Capability::WorkspaceImport,
            winbridge_protocol::Capability::WorkspaceDelta,
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
        #[cfg(unix)]
        [name, path]
            if matches!(name.as_str(), "controller-connections" | "operation-attribution" | "operation-cancellation") =>
        {
            let bytes = winbridge_workspace_sync::read_bounded(
                Path::new(path), winbridge_transport::MAX_FRAME_BYTES as u64,
            )?;
            let payload = serde_json::from_slice(&bytes)
                .map_err(|_| "Invalid diagnostic request JSON; no dispatch")?;
            let method = match name.as_str() {
                "controller-connections" => "controller_connections",
                "operation-attribution" => "operation_attribution",
                _ => "operation_cancellation",
            };
            let request = winbridge_controller::diagnostics::request(method, payload)?;
            let response = winbridge_controller::diagnostics::call(&address, &request).await?;
            println!("{}", serde_json::to_string_pretty(&response).map_err(|e| e.to_string())?);
            if let winbridge_protocol::ControllerDiagnosticResult::Error { message } = response.result {
                return Err(message);
            }
            return Ok(());
        }
        #[cfg(unix)]
        [name,path] if name=="operations"=> {
            let request:winbridge_protocol::OperationPageRequest=serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),winbridge_transport::MAX_FRAME_BYTES as u64)?).map_err(|e|e.to_string())?;
            let response=winbridge_controller::readiness::call(&address,&winbridge_protocol::ControllerApiRequest {controller_contract_version:1,adapter:winbridge_protocol::build_identity(),adapter_kind:winbridge_protocol::ControllerAdapter::Cli,request:winbridge_protocol::ControllerOperation::OperationPage {request:Box::new(request)}}).await?;
            println!("{}",serde_json::to_string_pretty(&response).map_err(|e|e.to_string())?);
            if let winbridge_protocol::ControllerApiResult::Error {message}=response.result {return Err(message);}
            return Ok(())
        }
        #[cfg(unix)]
        [name,path] if name == "verify-pair" => {
            let request:winbridge_protocol::VerifyPairRequest=serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),winbridge_transport::MAX_FRAME_BYTES as u64)?).map_err(|e|e.to_string())?;
            let response=winbridge_controller::readiness::call(&address,&winbridge_protocol::ControllerApiRequest {controller_contract_version:1,adapter:winbridge_protocol::build_identity(),adapter_kind:winbridge_protocol::ControllerAdapter::Cli,request:winbridge_protocol::ControllerOperation::VerifyPair{request:Box::new(request)}}).await?;
            println!("{}",serde_json::to_string_pretty(&response).map_err(|e|e.to_string())?);
            if let winbridge_protocol::ControllerApiResult::Error{message}=response.result{return Err(message);}
        }
        #[cfg(unix)]
        [name,key] if name == "verification" => {
            let response=winbridge_controller::readiness::call(&address,&winbridge_protocol::ControllerApiRequest {controller_contract_version:1,adapter:winbridge_protocol::build_identity(),adapter_kind:winbridge_protocol::ControllerAdapter::Cli,request:winbridge_protocol::ControllerOperation::GetVerification{verification_key:key.clone()}}).await?;
            println!("{}",serde_json::to_string_pretty(&response).map_err(|e|e.to_string())?);
            if let winbridge_protocol::ControllerApiResult::Error{message}=response.result{return Err(message);}
        }

        #[cfg(unix)]
        [name,path] if name == "readiness" => {
            let query:winbridge_protocol::ContractReadinessQuery=serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),winbridge_transport::MAX_FRAME_BYTES as u64)?).map_err(|e|e.to_string())?;
            let response=winbridge_controller::readiness::call(&address,&winbridge_protocol::ControllerApiRequest {controller_contract_version:winbridge_protocol::CONTROLLER_API_CONTRACT,
                adapter:winbridge_protocol::build_identity(),adapter_kind:winbridge_protocol::ControllerAdapter::Cli,request:winbridge_protocol::ControllerOperation::ContractReadiness{query}}).await?;
            println!("{}",serde_json::to_string_pretty(&response).map_err(|e|e.to_string())?);
            if let winbridge_protocol::ControllerApiResult::Error {message}=response.result {return Err(message);}
        }
        [name, path] if name == "invoke-v2" => {
            let request: winbridge_protocol::FencedRequest = serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path), winbridge_transport::MAX_FRAME_BYTES as u64)?).map_err(|_| "Invalid contract-2 request JSON")?;
            let response = winbridge_transport::call_fenced(&address, &request).await.map_err(|e|format!("Contract-2 request unconfirmed: {e}; retain the same request context/key"))?;
            println!("{}",serde_json::to_string(&response).map_err(|e|e.to_string())?);
            if let LocalResponse::Error { message } = response.response { return Err(message); }
        }
        [name, project, revision, base @ ..] if name == "publish-resumable" && base.len() <= 1 => {
            println!("{}", serde_json::to_string_pretty(&winbridge_controller::publish_resumable_measured(&config, project, revision, base.first().map(String::as_str))?).map_err(|e| e.to_string())?);
        }
        [name] if name == "validate" => println!(
            "Valid local pair {}: ws://{}; worker {}; snapshot import available when delivery is configured",
            config.instance_id, config.controller.listen_address, config.controller.worker_node_id
        ),
        [name,project,revision,base] if name=="publish-delta" => {
            println!("{}",serde_json::to_string_pretty(&winbridge_controller::publish_delta_measured(&config,project,revision,base)?).map_err(|e|e.to_string())?);
        }
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
                [name, path] if name == "import-delta" => LocalRequest::ImportDeltaSnapshot {request:serde_json::from_slice(&winbridge_workspace_sync::read_bounded(Path::new(path),65536)?).map_err(|_|"Invalid import request JSON")?},
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
