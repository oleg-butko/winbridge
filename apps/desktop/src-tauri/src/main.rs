#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod companion;
mod validation;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tauri::Manager;

use winbridge_core::config::{DesktopConfig, WorkerConfig, DESKTOP_CONFIG_FILE};
use winbridge_core::desktop_state::{DesktopDraftStore, DESKTOP_STATE_FILE};
use winbridge_protocol::{
    CommandDraft, DeliveryDraft, DesktopDraftState, LocalRequest, LocalResponse, WorkflowProfile,
};

struct WorkerEndpoint(Result<String, String>);
struct DesktopState(Result<Arc<DesktopDraftStore>, String>);

fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("This command is available in the main window only".into())
    }
}

#[tauri::command]
fn desktop_build() -> winbridge_protocol::BuildIdentity {
    winbridge_protocol::build_identity()
}

#[tauri::command]
fn load_command_drafts(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopState>,
) -> Result<DesktopDraftState, String> {
    main_only(&window)?;
    state.0.as_ref().map_err(Clone::clone)?.load()
}

#[tauri::command]
fn save_command_draft(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopState>,
    draft: CommandDraft,
) -> Result<(), String> {
    main_only(&window)?;
    state.0.as_ref().map_err(Clone::clone)?.save(draft)
}

#[tauri::command]
fn save_delivery_draft(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopState>,
    draft: DeliveryDraft,
) -> Result<(), String> {
    main_only(&window)?;
    state.0.as_ref().map_err(Clone::clone)?.save_delivery(draft)
}

#[tauri::command]
fn save_workflow_profile(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopState>,
    profile: WorkflowProfile,
) -> Result<(), String> {
    main_only(&window)?;
    state
        .0
        .as_ref()
        .map_err(Clone::clone)?
        .save_workflow(profile)
}

#[tauri::command]
async fn worker_rpc(
    window: tauri::WebviewWindow,
    endpoint: tauri::State<'_, WorkerEndpoint>,
    request: LocalRequest,
) -> Result<LocalResponse, String> {
    main_only(&window)?;
    let address = endpoint.0.as_ref().map_err(Clone::clone)?;
    winbridge_transport::call(address, &request).await.map_err(|e| format!("Windows worker is unavailable: {e}. Start worker with the same config; accepted tasks continue independently of this window"))
}

fn configured_endpoint() -> Result<String, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("desktop executable path: {e}"))?;
    let directory = executable
        .parent()
        .ok_or("desktop executable has no directory")?;
    let desktop_config = directory.join(DESKTOP_CONFIG_FILE);
    let worker_config = DesktopConfig::load(&desktop_config).map_err(|e| {
        format!(
            "{e}. Set workerConfigPath in {} and restart desktop",
            desktop_config.display()
        )
    })?;
    let config = WorkerConfig::load(&worker_config)?;
    winbridge_transport::endpoint(&config.state_directory, &config.instance_id)
        .map_err(|e| e.to_string())
}

fn main() {
    // Keep the window available so configuration errors are visible in the UI.
    let endpoint = configured_endpoint();
    let drafts = std::env::current_exe()
        .map_err(|e| format!("desktop executable path: {e}"))
        .and_then(|path| {
            let directory = path.parent().ok_or("desktop executable has no directory")?;
            Ok(Arc::new(DesktopDraftStore::new(
                directory.join(DESKTOP_STATE_FILE),
            )))
        });
    let mut context = tauri::generate_context!();
    let directory = std::env::current_exe()
        .expect("Executable path")
        .parent()
        .expect("Executable directory")
        .canonicalize()
        .expect("Runtime directory");
    let identity = format!(
        "{:x}",
        Sha256::digest(directory.to_string_lossy().as_bytes())
    );
    context.config_mut().identifier = format!("local.winbridge.desktop.{}", &identity[..16]);
    let background_endpoint = endpoint.clone();
    let background_drafts = drafts.clone();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let validation_report = match args.as_slice() {
        [mode, report]
            if matches!(
                mode.as_str(),
                "--validate-companion" | "--validate-companion-restart"
            ) =>
        {
            Some((std::path::PathBuf::from(report), mode.ends_with("-restart")))
        }
        _ => None,
    };
    if validation_report.is_some() {
        context.config_mut().app.windows[0].title =
            "WinBridge — АВТОМАТИЧЕСКАЯ ПРОВЕРКА (отдельный runtime)".into();
    }
    let validation = validation::Validation::new(
        validation_report.as_ref().map(|(path, _)| path),
        validation_report
            .as_ref()
            .is_some_and(|(_, restart)| *restart),
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Err(e) = companion::show_main(app) {
                eprintln!("{e}");
            }
        }))
        .manage(WorkerEndpoint(endpoint))
        .manage(DesktopState(drafts))
        .manage(companion::Companion::new())
        .manage(validation)
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                webview
                    .app_handle()
                    .state::<validation::Validation>()
                    .loaded_windows
                    .lock()
                    .unwrap()
                    .insert(webview.label().into());
            }
        })
        .setup(move |app| {
            companion::setup(
                app.handle(),
                background_endpoint.clone(),
                background_drafts.clone(),
            );
            if let Some((report, restart)) = validation_report {
                validation::start(
                    app.handle().clone(),
                    background_endpoint,
                    background_drafts,
                    report,
                    restart,
                );
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let state = window.app_handle().state::<validation::Validation>();
                if state.enabled {
                    eprintln!(
                        "Native validation window close requested: {}",
                        window.label()
                    );
                }
                if window.label() == "attention-popup" {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_build,
            worker_rpc,
            load_command_drafts,
            save_command_draft,
            save_delivery_draft,
            save_workflow_profile,
            companion::companion_status,
            companion::companion_close,
            companion::companion_open_task,
            companion::companion_dismiss,
            companion::save_notification_preferences,
            validation::companion_probe,
            validation::desktop_validation_status,
            validation::abort_desktop_validation
        ])
        .run(context)
        .expect("WinBridge desktop runtime");
}
