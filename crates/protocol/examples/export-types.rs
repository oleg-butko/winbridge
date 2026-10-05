use std::{fs, path::Path};
use ts_rs::{Config, TS};
use winbridge_protocol::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // serde_json encodes these bounded counters/timestamps as JSON numbers.
    // Only this generator process sets the binding; no global configuration changes.
    std::env::set_var("TS_RS_LARGE_INT", "number");
    let config = Config::from_env();
    let declarations = [
        TaskStatus::decl(&config),
        CommandSource::decl(&config),
        TaskSubmission::decl(&config),
        ResolvedCommand::decl(&config),
        JournalEvent::decl(&config),
        TaskRecord::decl(&config),
        TaskSummary::decl(&config),
        OutputStream::decl(&config),
        OutputChunk::decl(&config),
        RecipeInfo::decl(&config),
        ProjectInfo::decl(&config),
        ConnectionState::decl(&config),
        ControllerConnection::decl(&config),
        ControllerStatus::decl(&config),
        WorkerInfo::decl(&config),
        LocalRequest::decl(&config),
        LocalResponse::decl(&config),
        TaskReport::decl(&config),
        CommandDraft::decl(&config),
        DesktopDraftState::decl(&config),
    ];
    let text = format!(
        "// Generated from crates/protocol by npm run generate:contracts. Do not edit.\n{}\n",
        declarations
            .iter()
            .map(|d| format!("export {d}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if text.contains("bigint") {
        return Err("JSON bindings must use number for bounded counters/timestamps".into());
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/runtime/contracts.ts");
    if std::env::args().any(|arg| arg == "--check") {
        if fs::read_to_string(&path)? != text {
            return Err(
                "UI contracts differ from Rust DTOs; run npm run generate:contracts".into(),
            );
        }
        println!("UI contracts match Rust DTOs");
    } else {
        fs::write(path, text)?;
        println!("UI contracts generated from Rust DTOs");
    }
    Ok(())
}
