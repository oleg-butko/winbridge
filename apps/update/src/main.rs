use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

fn run() -> Result<bool, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge updater — preparation and update journal\n  verify-source ABSOLUTE_DIRECTORY EXPECTED_MANIFEST_SHA256\n  verify-archive ABSOLUTE_ZIP ARCHIVE_SHA256 MANIFEST_SHA256\n  [--config PATH] plan ABSOLUTE_INCOMING_DIRECTORY EXPECTED_MANIFEST_SHA256\n  [--config PATH] journal init\n  [--config PATH] journal status [UPDATE_ID]\nDefault config: winbridge-update.local.json beside this executable.\nOnly journal init writes update-state metadata. No source writes, runtime replacement, shutdown or automatic update are implemented.");
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
