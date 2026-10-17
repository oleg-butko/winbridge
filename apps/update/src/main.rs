use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

fn run() -> Result<bool, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("WinBridge updater — readonly preparation (U0)\n  verify-source ABSOLUTE_DIRECTORY EXPECTED_MANIFEST_SHA256\n  [--config PATH] plan ABSOLUTE_INCOMING_DIRECTORY EXPECTED_MANIFEST_SHA256\nDefault config: winbridge-update.local.json beside this executable.\nNo source writes, runtime replacement, shutdown or automatic update are implemented.");
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
