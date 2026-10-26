use std::{path::Path, process::ExitCode};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::{LocalRequest, LocalResponse};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("WinBridge: {error}");
            ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] || args == ["-V"] {
        let build = winbridge_protocol::build_identity();
        println!(
            "WinBridge worker {} (source {}; dirty {}; build {})",
            env!("CARGO_PKG_VERSION"),
            build.source_commit.as_deref().unwrap_or("unknown"),
            build.source_dirty,
            build.build_id
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "--activate-update") {
        if let [_, config, update, attempt] = args.as_slice() {
            return winbridge_worker::activation::run(Path::new(config), update, attempt);
        }
        return Err("Use --activate-update ABSOLUTE_CONFIG UPDATE_ID ATTEMPT_ID; private parent handshake required, no normal startup".into());
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--export-update-state")
    {
        if let [_, path, sha] = args.as_slice() {
            let report = winbridge_worker::state_backup_export::export(Path::new(path), sha)?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        return Err("Use --export-update-state ABSOLUTE_INPUT_JSON SHA256; exact saved attempt required, no normal worker startup".into());
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--inspect-update-shutdown")
    {
        if let [_, path, sha] = args.as_slice() {
            let report = winbridge_worker::shutdown_inspection::inspect(Path::new(path), sha)?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        return Err(
            "Use --inspect-update-shutdown ABSOLUTE_INPUT_JSON SHA256; readonly, no worker startup"
                .into(),
        );
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--check-update-config")
    {
        if let [_, path, sha] = args.as_slice() {
            let report = winbridge_worker::compatibility::check_config(Path::new(path), sha)?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        return Err("Use --check-update-config ABSOLUTE_CONFIG SHA256; no journal opened".into());
    }
    if args.first().is_some_and(|arg| arg == "--rehearsal") {
        if let [_, directory] = args.as_slice() {
            let report = winbridge_worker::rehearsal::run(Path::new(directory)).await?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        return Err(
            "Use --rehearsal NEW_ABSOLUTE_DIRECTORY; production config is never accepted".into(),
        );
    }
    if args.first().is_some_and(|arg| arg == "--artifact-info") {
        if args.len() != 1 {
            return Err(
                "Unsupported artifact query arguments; no configuration or work executed".into(),
            );
        }
        println!(
            "{}",
            serde_json::to_string(&winbridge_protocol::artifact_descriptor(
                winbridge_protocol::ArtifactRole::Worker
            ))
            .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge worker — local command service\n  --version | -V (no configuration; build identity)\n  --artifact-info (JSON artifact identity; no configuration)\n  --check-update-config ABSOLUTE_CONFIG SHA256 (readonly; no journal)\n  --rehearsal NEW_ABSOLUTE_DIRECTORY (fresh isolated diagnostic, no production config)\n  --describe\n  --config PATH validate\n  --config PATH verify-runtime (read-only; no journal/scheduler)\n  --config PATH serve\n  --config PATH serve-recovery (read-only; persistent execution block)\n  --config PATH prepare-recovery NEW_DIRECTORY (archive/new pair; stays blocked)\n  --config PATH activate-recovery (prepared target config; physical check required)\n  --config PATH info | list\n  --config PATH operations REQUEST.json (readonly; pinned journal scope)\n  --config PATH invoke-v2 ENVELOPE.json (caller-pinned context; no fallback)\n  --config PATH submit REQUEST.json\n  --config PATH attention REQUEST.json | attention-list [AFTER_SEQUENCE] | attention-history [AFTER_SEQUENCE] | ack-attention ID\n  --config PATH attention-events [AFTER_SEQUENCE] | attention-events-history [AFTER_SEQUENCE] | ack-attention-event ID\n  --config PATH history QUERY.json | diagnostic REQUEST.json | storage-usage\n  --config PATH import-delta REQUEST.json\n  --config PATH import REQUEST.json | imports | get-import ID | cancel-import ID\n  --config PATH preview PROJECT SNAPSHOT\n  --config PATH snapshots PROJECT [OFFSET]\n  --config PATH workflow REQUEST.json | workflows | get-workflow ID | cancel-workflow ID\n  --config PATH applications | get-application ID | stop-application ID | log ID [CURSOR.json]\n  --config PATH get TASK_ID\n  --config PATH output TASK_ID [AFTER_SEQUENCE]\n  --config PATH cancel TASK_ID\nRuntime data stays in stateDirectory; UI/client exit does not cancel tasks.");
        return Ok(());
    }
    if args == ["--describe"] {
        let mut descriptor = winbridge_core::descriptor(winbridge_protocol::Component::Worker);
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
        descriptor.planned_capabilities = vec![];
        println!(
            "{}",
            serde_json::to_string_pretty(&descriptor).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.len() < 3 || args[0] != "--config" {
        return Err("use --config PATH followed by an operation; see --help".into());
    }
    let config = WorkerConfig::load(Path::new(&args[1]))?;
    let operation = &args[2..];
    match operation {
        [name, path] if name == "operations" => {
            let request: winbridge_protocol::OperationPageRequest =
                serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                    Path::new(path),
                    winbridge_transport::MAX_FRAME_BYTES as u64,
                )?)
                .map_err(|e| e.to_string())?;
            let endpoint =
                winbridge_transport::endpoint(&config.state_directory, &config.instance_id)
                    .map_err(|e| e.to_string())?;
            let page = winbridge_transport::operation_page(&endpoint, &request).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&page).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [name, path] if name == "prepare-recovery" => {
            let prepared = winbridge_worker::prepare_recovery(&config, Path::new(path))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&prepared).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [name] if name == "activate-recovery" => {
            let receipt = winbridge_worker::activate_recovery(&config)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [name, path] if name == "invoke-v2" => {
            let request: winbridge_protocol::FencedRequest =
                serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                    Path::new(path),
                    winbridge_transport::MAX_FRAME_BYTES as u64,
                )?)
                .map_err(|_| "Invalid contract-2 request JSON")?;
            let endpoint =
                winbridge_transport::endpoint(&config.state_directory, &config.instance_id)
                    .map_err(|e| e.to_string())?;
            let response = winbridge_transport::call_fenced(&endpoint, &request)
                .await
                .map_err(|e| {
                    format!(
                        "Contract-2 request unconfirmed: {e}; retain the same request context/key"
                    )
                })?;
            println!(
                "{}",
                serde_json::to_string(&response).map_err(|e| e.to_string())?
            );
            if let LocalResponse::Error { message } = response.response {
                return Err(message);
            }
            Ok(())
        }
        [name] if name == "validate" => {
            println!("Valid config: instance {}, {} project(s); controller connection {}, snapshot import available when delivery is configured", config.instance_id, config.projects.len(), config.worker.controller_url.as_deref().unwrap_or("disabled"));
            Ok(())
        }
        [name] if name == "serve" => {
            // Known-runtime validation precedes opening/migrating the worker journal.
            let guard =
                winbridge_worker::runtime_guard::prepare_runtime_guard(Path::new(&args[1]))?;
            let worker = winbridge_worker::Worker::open_file(Path::new(&args[1]))?;
            if let Some(guard) = guard {
                worker.attach_runtime_guard(guard)?;
            }
            #[cfg(feature = "update-validation")]
            let validation = winbridge_worker::update_validation::attach(&worker)?;
            let outcome = winbridge_worker::serve(worker).await;
            #[cfg(feature = "update-validation")]
            if let Some(validation) = validation {
                validation.await.map_err(|e| e.to_string())??;
            }
            outcome
        }
        [name] if name == "verify-runtime" => {
            let guard =
                winbridge_worker::runtime_guard::prepare_runtime_guard(Path::new(&args[1]))?
                    .ok_or(
                        "Bootstrap required: no known installed-set manifest; no journal opened",
                    )?;
            guard.revalidate()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schemaVersion": 1, "kind": "worker_runtime_verification", "readOnly": true,
                    "identity": guard.identity(), "workerJournalOpened": false,
                    "schedulerStarted": false, "updateAuthorityEnabled": false
                }))
                .map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [name] if name == "serve-recovery" => {
            eprintln!("Read-only journal recovery: saved statuses are historical; execution and all changes are blocked. A persistent marker also blocks normal serve after restart.");
            winbridge_worker::serve(winbridge_worker::Worker::open_recovery_file(Path::new(
                &args[1],
            ))?)
            .await
        }
        _ => {
            let request = match operation {
                [name, path] if name == "attention" => LocalRequest::RequestAttention {
                    request: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        8192,
                    )?)
                    .map_err(|_| "Invalid attention request JSON")?,
                },
                [name] if name == "attention-list" || name == "attention-history" => {
                    LocalRequest::ListAttention {
                        after_sequence: 0,
                        pending_only: name == "attention-list",
                    }
                }
                [name, after] if name == "attention-list" || name == "attention-history" => {
                    LocalRequest::ListAttention {
                        after_sequence: after
                            .parse()
                            .map_err(|_| "Attention cursor must be an unsigned integer")?,
                        pending_only: name == "attention-list",
                    }
                }
                [name, id] if name == "ack-attention" => LocalRequest::AcknowledgeAttention {
                    attention_id: id.clone(),
                },
                [name, path] if name == "history" => LocalRequest::History {
                    query: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        65536,
                    )?)
                    .map_err(|_| "Invalid history query JSON")?,
                },
                [name, path] if name == "diagnostic" => LocalRequest::PrepareTaskDiagnostic {
                    request: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        262144,
                    )?)
                    .map_err(|_| "Invalid diagnostic request JSON")?,
                },
                [name] if name == "storage-usage" => LocalRequest::StorageUsage,
                [name] if name == "attention-events" || name == "attention-events-history" => {
                    LocalRequest::ListAttentionEvents {
                        after_sequence: 0,
                        pending_only: name == "attention-events",
                    }
                }
                [name, after]
                    if name == "attention-events" || name == "attention-events-history" =>
                {
                    LocalRequest::ListAttentionEvents {
                        after_sequence: after
                            .parse()
                            .map_err(|_| "Attention cursor must be an unsigned integer")?,
                        pending_only: name == "attention-events",
                    }
                }
                [name, id] if name == "ack-attention-event" => {
                    LocalRequest::AcknowledgeAttentionEvent {
                        attention_id: id.clone(),
                    }
                }
                [name, path] if name == "workflow" => LocalRequest::SubmitWorkflow {
                    request: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        65536,
                    )?)
                    .map_err(|_| "Invalid workflow JSON")?,
                },
                [name] if name == "workflows" => LocalRequest::ListWorkflows,
                [name, id] if name == "get-workflow" => LocalRequest::GetWorkflow {
                    workflow_id: id.clone(),
                },
                [name, id] if name == "cancel-workflow" => LocalRequest::CancelWorkflow {
                    workflow_id: id.clone(),
                },
                [name] if name == "applications" => LocalRequest::ListApplications,
                [name, id] if name == "get-application" => LocalRequest::GetApplication {
                    application_id: id.clone(),
                },
                [name, id] if name == "stop-application" => LocalRequest::StopApplication {
                    application_id: id.clone(),
                },
                [name, id] if name == "log" => LocalRequest::ReadApplicationLog {
                    application_id: id.clone(),
                    cursor: None,
                    max_bytes: 65536,
                },
                [name, id, path] if name == "log" => LocalRequest::ReadApplicationLog {
                    application_id: id.clone(),
                    cursor: Some(
                        serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                            Path::new(path),
                            8192,
                        )?)
                        .map_err(|_| "Invalid log cursor JSON")?,
                    ),
                    max_bytes: 65536,
                },
                [name] if name == "imports" => LocalRequest::ListImports,
                [name, project] if name == "snapshots" => LocalRequest::ListSnapshots {
                    project_id: project.clone(),
                    offset: 0,
                },
                [name, project, offset] if name == "snapshots" => LocalRequest::ListSnapshots {
                    project_id: project.clone(),
                    offset: offset
                        .parse()
                        .map_err(|_| "Snapshot offset must be an integer")?,
                },
                [name, id] if name == "get-import" => LocalRequest::GetImport {
                    import_id: id.clone(),
                },
                [name, id] if name == "cancel-import" => LocalRequest::CancelImport {
                    import_id: id.clone(),
                },
                [name, project, snapshot] if name == "preview" => LocalRequest::PreviewSnapshot {
                    project_id: project.clone(),
                    snapshot_id: snapshot.clone(),
                    offset: 0,
                },
                [name, path] if name == "import-delta" => LocalRequest::ImportDeltaSnapshot {
                    request: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        65536,
                    )?)
                    .map_err(|_| "Invalid import request JSON")?,
                },
                [name, path] if name == "import" => LocalRequest::ImportSnapshot {
                    request: serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
                        Path::new(path),
                        65536,
                    )?)
                    .map_err(|_| "Invalid import request JSON")?,
                },
                [name] if name == "info" => LocalRequest::Info,
                [name] if name == "list" => LocalRequest::ListTasks,
                [name, path] if name == "submit" => {
                    let bytes = std::fs::read(path).map_err(|e| format!("request {path}: {e}"))?;
                    if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
                        return Err("request file exceeds 1 MiB".into());
                    }
                    LocalRequest::Submit {
                        request: serde_json::from_slice(&bytes)
                            .map_err(|e| format!("request {path}: {e}"))?,
                    }
                }
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
                    after_sequence: after
                        .parse()
                        .map_err(|_| "AFTER_SEQUENCE must be an unsigned integer")?,
                },
                _ => return Err("unsupported operation; see --help".into()),
            };
            let endpoint =
                winbridge_transport::endpoint(&config.state_directory, &config.instance_id)
                    .map_err(|e| e.to_string())?;
            let response = winbridge_transport::call(&endpoint, &request)
                .await
                .map_err(|e| format!("worker unavailable at {endpoint}: {e}"))?;
            println!(
                "{}",
                serde_json::to_string(&response).map_err(|e| e.to_string())?
            );
            if let LocalResponse::Error { message } = response {
                return Err(message);
            }
            Ok(())
        }
    }
}
