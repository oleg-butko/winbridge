//! Project/config policy and the controller scaffold entry point. No OS execution here.

pub mod attention;
pub mod commands;
pub mod config;
pub mod controller_config;
pub mod desktop_state;
pub mod diagnostics;
pub mod notifications;
pub mod profiles;
pub mod readiness;
pub mod snapshot;
pub mod task;
pub mod work_scheduling;
pub mod workflow;

use std::io::{self, Write};
use winbridge_protocol::{Capability, Component, RuntimeDescriptor, PROTOCOL_VERSION};

pub fn descriptor(component: Component) -> RuntimeDescriptor {
    RuntimeDescriptor {
        build: Some(winbridge_protocol::build_identity()),
        application: "WinBridge".into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        protocol_version: PROTOCOL_VERSION,
        component,
        scaffold: true,
        implemented_capabilities: Vec::new(),
        planned_capabilities: vec![
            Capability::TaskSubmit,
            Capability::TaskCancel,
            Capability::WorkspaceImport,
            Capability::LogRead,
            Capability::AttentionRequest,
            Capability::AttentionTargets,
            Capability::HistoryRead,
        ],
    }
}

pub fn run_scaffold_cli(component: Component) -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [argument] if argument == "--describe" => {
            let mut stdout = io::stdout().lock();
            serde_json::to_writer_pretty(&mut stdout, &descriptor(component))?;
            writeln!(stdout)?;
        }
        [] => print_help(component),
        [argument] if argument == "--help" || argument == "-h" => print_help(component),
        _ => return Err("unsupported arguments; use --help or --describe".into()),
    }
    Ok(())
}

fn print_help(component: Component) {
    println!("WinBridge {component:?} v{}", env!("CARGO_PKG_VERSION"));
    println!("Project scaffold: execution and transport are not implemented yet.");
    println!("  --describe  Print application/protocol metadata and planned capabilities as JSON");
    println!("  --help      Show this message");
}
pub mod operation_query;
