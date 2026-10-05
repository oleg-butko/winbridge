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
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge worker — local command service\n  --describe\n  --config PATH validate\n  --config PATH serve\n  --config PATH info | list\n  --config PATH submit REQUEST.json\n  --config PATH get TASK_ID\n  --config PATH output TASK_ID [AFTER_SEQUENCE]\n  --config PATH cancel TASK_ID\nRuntime data stays in stateDirectory; UI/client exit does not cancel tasks.");
        return Ok(());
    }
    if args == ["--describe"] {
        let mut descriptor = winbridge_core::descriptor(winbridge_protocol::Component::Worker);
        descriptor.scaffold = false;
        descriptor.implemented_capabilities = vec![
            winbridge_protocol::Capability::TaskSubmit,
            winbridge_protocol::Capability::TaskCancel,
        ];
        descriptor.planned_capabilities = vec![
            winbridge_protocol::Capability::WorkspaceImport,
            winbridge_protocol::Capability::LogRead,
            winbridge_protocol::Capability::AttentionRequest,
        ];
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
        [name] if name == "validate" => {
            println!("Valid config: instance {}, {} project(s); controller/delivery settings are inactive until later stages", config.instance_id, config.projects.len());
            Ok(())
        }
        [name] if name == "serve" => {
            winbridge_worker::serve(winbridge_worker::Worker::open(config)?).await
        }
        _ => {
            let request = match operation {
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
