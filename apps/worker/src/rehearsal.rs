//! Fresh-state worker/IPC smoke. Never accepts production config or copies its journal.
use crate::{Result, Worker};
use std::{fs, path::Path, sync::Arc, time::Duration};
use winbridge_protocol::{LocalRequest, LocalResponse, WorkerRehearsalReport};

pub async fn run(directory: &Path) -> Result<WorkerRehearsalReport> {
    std::hint::black_box(winbridge_protocol::WORKER_REHEARSAL_MARKER);
    if !directory.is_absolute()
        || directory
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Rehearsal requires an absolute new local directory without traversal".into());
    }
    for check in [
        winbridge_platform::require_local_path_prefix,
        winbridge_platform::require_safe_path,
        winbridge_platform::require_local_disk,
    ] {
        check(directory).map_err(|e| e.to_string())?;
    }
    // create_dir, never create_dir_all: existing directories (including empty
    // directories and live/copy journals) cannot be adopted or opened.
    fs::create_dir(directory).map_err(|e| format!("Create-only rehearsal directory: {e}"))?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let directory_identity = winbridge_platform::source_files::directory_identity(&directory)
        .map_err(|e| e.to_string())?;
    let state = directory.join("state");
    let project = directory.join("project");
    fs::create_dir(&project).map_err(|e| e.to_string())?;
    let instance = format!("rehearsal-{}", uuid::Uuid::new_v4());
    let cfg = serde_json::from_value(serde_json::json!({
        "schemaVersion":1, "instanceId":instance, "nodeId":"candidate-rehearsal",
        "stateDirectory":state, "maxOutputBytesPerTask":4096,
        "worker":{"keepRunningOnUiExit":true},
        "projects":[{"id":"rehearsal", "workspaceRoot":project,
            "allowDirectCommands":false, "maxConcurrentMutations":1, "recipes":[]}]
    }))
    .map_err(|e| e.to_string())?;
    let worker = Worker::open(cfg)?;
    let address = winbridge_transport::endpoint(&state, &instance).map_err(|e| e.to_string())?;
    let mut server = tokio::spawn(crate::serve(Arc::clone(&worker)));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let observed = loop {
        if server.is_finished() {
            break Err("Rehearsal server exited before IPC readiness".to_string());
        }
        match tokio::time::timeout(
            Duration::from_millis(500),
            winbridge_transport::call(&address, &LocalRequest::Info),
        )
        .await
        {
            Ok(Ok(LocalResponse::Info { info })) => break Ok(info),
            _ if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            _ => break Err("Rehearsal IPC readiness timeout".into()),
        }
    };
    // Only this freshly created instance; it has no admitted work. Await normal
    // listener/control teardown rather than declaring abort() to be shutdown.
    worker.stop();
    match tokio::time::timeout(Duration::from_secs(5), &mut server).await {
        Ok(joined) => joined.map_err(|e| e.to_string())??,
        Err(_) => {
            server.abort();
            let _ = server.await;
            return Err("Rehearsal server cleanup unconfirmed".into());
        }
    }
    let info = observed?;
    let context = &info
        .fenced_requests
        .as_ref()
        .ok_or("Rehearsal context missing")?
        .context;
    if info.instance_id != instance
        || info.generation != worker.generation
        || info.controller_connection.is_some()
        || info.active_task_count != Some(0)
        || info.journal_recovery_required.is_some()
        || info.maintenance_error.is_some()
        || info.projects.len() != 1
        || info.projects[0].allow_direct_commands
        || !info.projects[0].recipes.is_empty()
        || info.snapshot_import_available
        || context.instance_id != instance
        || context.journal_incarnation.is_empty()
    {
        return Err("Fresh rehearsal IPC state differs from restrictive configuration".into());
    }
    Ok(WorkerRehearsalReport {
        schema_version: 1,
        kind: "worker_rehearsal".into(),
        contract_version: 1,
        descriptor: winbridge_protocol::artifact_descriptor(
            winbridge_protocol::ArtifactRole::Worker,
        ),
        process: winbridge_platform::infrastructure::stamp(std::process::id())
            .map_err(|e| e.to_string())?,
        directory: directory
            .to_str()
            .ok_or("Rehearsal path must be UTF-8")?
            .into(),
        directory_identity,
        instance_id: instance,
        generation: info.generation,
        journal_incarnation: context.journal_incarnation.clone(),
        worker_journal_schema: crate::store::JOURNAL_SCHEMA_VERSION,
        updater_journal_schema: winbridge_update::journal::UPDATE_SCHEMA_VERSION,
        worker_config_schema: 1,
        protocol_version: winbridge_protocol::PROTOCOL_VERSION,
        fresh_journal: true,
        ipc_ready: true,
        server_stopped: true,
        controller_enabled: false,
        direct_commands_enabled: false,
        production_state_opened: false,
        candidate_verified: false,
        runtime_applied: false,
    })
}
