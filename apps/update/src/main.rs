use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

fn run() -> Result<bool, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let [op, image, sha, directory, worker_schema, updater_schema] = args.as_slice() {
        if op == "rehearse-worker" {
            let proof = winbridge_update::worker_rehearsal::inspect(
                Path::new(image),
                sha,
                Path::new(directory),
                worker_schema.parse().map_err(|_| "Invalid worker schema")?,
                updater_schema
                    .parse()
                    .map_err(|_| "Invalid updater schema")?,
            )?;
            println!(
                "{}",
                serde_json::to_string(&proof).map_err(|e| e.to_string())?
            );
            return Ok(true);
        }
    }
    if let [op, source, manifest, stage, report_sha256] = args.as_slice() {
        if op == "inspect-candidate-files" {
            let inspection = winbridge_update::candidate_artifacts::inspect_candidate_files(
                Path::new(source),
                manifest,
                Path::new(stage),
                report_sha256,
            )?;
            println!(
                "{}",
                serde_json::to_string(&inspection).map_err(|e| e.to_string())?
            );
            return Ok(true);
        }
    }
    if let [op, image, role] = args.as_slice() {
        if op == "artifact-query" {
            let role = match role.as_str() {
                "worker" => winbridge_protocol::ArtifactRole::Worker,
                "fixture" => winbridge_protocol::ArtifactRole::Fixture,
                "desktop" => winbridge_protocol::ArtifactRole::Desktop,
                "updater" => winbridge_protocol::ArtifactRole::Updater,
                _ => return Err("Unknown artifact role".into()),
            };
            let report = winbridge_update::artifact::query_image(Path::new(image), role)?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|e| e.to_string())?
            );
            return Ok(true);
        }
    }
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::to_string(&winbridge_protocol::build_identity())
                .map_err(|e| e.to_string())?
        );
        return Ok(true);
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
                winbridge_protocol::ArtifactRole::Updater
            ))
            .map_err(|e| e.to_string())?
        );
        return Ok(true);
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge updater — preparation and update journal\n  rehearse-worker IMAGE SHA256 NEW_ABSOLUTE_DIRECTORY WORKER_SCHEMA UPDATER_SCHEMA (isolated diagnostic; creates fresh state, no install authority)\n  --build-info (readonly identity; no configuration required)\n  inspect-candidate-files ABSOLUTE_SOURCE MANIFEST_SHA256 ABSOLUTE_STAGING REPORT_SHA256 (readonly files; no journal/install authority)\n  verify-source ABSOLUTE_DIRECTORY EXPECTED_MANIFEST_SHA256\n  verify-archive ABSOLUTE_ZIP ARCHIVE_SHA256 MANIFEST_SHA256\n  [--config PATH] plan ABSOLUTE_INCOMING_DIRECTORY EXPECTED_MANIFEST_SHA256\n  [--config PATH] journal init\n  [--config PATH] journal status [UPDATE_ID]\n  [--config PATH] journal build-status UPDATE_ID_OR_ATTEMPT_ID\n  [--config PATH] journal supervisor-status UPDATE_ID_OR_LAUNCH_ID\n  [--config PATH] journal source-status UPDATE_ID [OFFSET] [LIMIT]\n  [--config PATH] journal source-inspect UPDATE_ID [OFFSET] [LIMIT]\nDefault config: winbridge-update.local.json beside this executable.\nSupervisor internal entry is gated by a durable launch and is not a user update command. Only journal init is a public metadata writer. Source apply is library-only for the trusted driver. No source-write CLI, runtime replacement, shutdown or automatic update is implemented.");
        return Ok(true);
    }
    let config_path = if args.first().is_some_and(|a| a == "--config") {
        if args.len() < 3 {
            return Err("--config requires a file and operation".into());
        }
        let path = PathBuf::from(args.remove(1));
        args.remove(0);
        path
    } else {
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("winbridge-update.local.json")
    };
    match args.as_slice() {
        [op, action, id] if op == "journal" && action == "build-status" => {
            let config = winbridge_update::load_config(&config_path)?;
            let dir = config.runtime_directory.join("update-state");
            let info = winbridge_update::journal::UpdateJournal::inspect(&dir)
                .map_err(|e| e.to_string())?;
            if info.scope_sha256 != winbridge_update::installation_scope(&config)? {
                return Err("Build journal belongs to another installation".into());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&winbridge_update::build::inspect_build(&dir, id)?)
                    .map_err(|e| e.to_string())?
            );
            Ok(true)
        }
        [op, action, id] if op == "journal" && action == "supervisor-status" => {
            let config = winbridge_update::load_config(&config_path)?;
            let dir = config.runtime_directory.join("update-state");
            let info = winbridge_update::journal::UpdateJournal::inspect(&dir)
                .map_err(|e| e.to_string())?;
            if info.scope_sha256 != winbridge_update::installation_scope(&config)? {
                return Err("Supervisor journal belongs to another installation".into());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&winbridge_update::supervisor::inspect(&dir, id)?)
                    .map_err(|e| e.to_string())?
            );
            Ok(true)
        }
        [op, id, token] if op == "supervisor-internal" => {
            tokio::runtime::Runtime::new()
                .map_err(|e| e.to_string())?
                .block_on(winbridge_update::supervisor::run(&config_path, id, token))?;
            Ok(true)
        }
        [op, action, id, paging @ ..]
            if op == "journal"
                && matches!(action.as_str(), "source-status" | "source-inspect")
                && paging.len() <= 2 =>
        {
            let config = winbridge_update::load_config(&config_path)?;
            let directory = config.runtime_directory.join("update-state");
            let info = winbridge_update::journal::UpdateJournal::inspect(&directory)
                .map_err(|e| e.to_string())?;
            if info.scope_sha256 != winbridge_update::installation_scope(&config)? {
                return Err("Source journal belongs to another installation".into());
            }
            let offset = paging
                .first()
                .map(|s| s.parse::<u32>())
                .transpose()
                .map_err(|_| "Invalid source offset")?
                .unwrap_or(0);
            let limit = paging
                .get(1)
                .map(|s| s.parse::<u16>())
                .transpose()
                .map_err(|_| "Invalid source limit")?
                .unwrap_or(100);
            let value = if action == "source-status" {
                serde_json::to_value(
                    winbridge_update::journal::UpdateJournal::read_source_status(
                        &directory, id, offset, limit,
                    )
                    .map_err(|e| e.to_string())?,
                )
            } else {
                serde_json::to_value(winbridge_update::source::inspect_source(
                    &directory, id, offset, limit,
                )?)
            }
            .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
            );
            Ok(true)
        }
        [op, path, archive_hash, manifest_hash] if op == "verify-archive" => {
            let result = winbridge_update::archive::verify_archive(
                Path::new(path),
                archive_hash,
                manifest_hash,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
            Ok(true)
        }
        [op, directory, hash] if op == "verify-source" => {
            let result = winbridge_update::verify_source(Path::new(directory), hash)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
            Ok(true)
        }
        [op, directory, hash] if op == "plan" => {
            let config = winbridge_update::load_config(&config_path)?;
            let result = winbridge_update::source_plan(&config, Path::new(directory), hash)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
            Ok(result.plan.can_apply)
        }
        [op, action, tail @ ..]
            if op == "journal"
                && ((action == "init" && tail.is_empty())
                    || (action == "status" && tail.len() <= 1)) =>
        {
            let config = winbridge_update::load_config(&config_path)?;
            let scope = winbridge_update::installation_scope(&config)?;
            let directory = config.runtime_directory.join("update-state");
            let (info, record) = if action == "init" {
                let mut store = winbridge_update::journal::UpdateJournal::open(&directory, &scope)
                    .map_err(|e| e.to_string())?;
                (store.info().map_err(|e| e.to_string())?, None)
            } else {
                winbridge_update::journal::UpdateJournal::read_snapshot(
                    &directory,
                    tail.first().map(String::as_str),
                )
                .map_err(|e| e.to_string())?
            };
            if info.scope_sha256 != scope {
                return Err("Update journal belongs to different installation paths".into());
            }
            println!("{}",serde_json::to_string_pretty(&serde_json::json!({"schemaVersion":1,"kind":"update_journal","readOnly":action=="status","runtimeApplied":false,"info":info,"update":record})).map_err(|e|e.to_string())?);
            Ok(true)
        }
        _ => Err("Unsupported operation; see --help. Runtime apply is not implemented.".into()),
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(message) => {
            eprintln!(
                "{}",
                serde_json::json!({"schemaVersion":1,"kind":"error","message":message})
            );
            ExitCode::FAILURE
        }
    }
}
