#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use winbridge_core::config::{DesktopConfig, WorkerConfig, DESKTOP_CONFIG_FILE};
use winbridge_core::desktop_state::{DesktopDraftStore, DESKTOP_STATE_FILE};
use winbridge_protocol::{CommandDraft, DeliveryDraft, DesktopDraftState, LocalRequest, LocalResponse};

struct WorkerEndpoint(Result<String, String>);
struct DesktopState(Result<DesktopDraftStore, String>);

#[tauri::command]
fn load_command_drafts(state: tauri::State<'_, DesktopState>) -> Result<DesktopDraftState, String> {
    state.0.as_ref().map_err(Clone::clone)?.load()
}

#[tauri::command]
fn save_command_draft(
    state: tauri::State<'_, DesktopState>,
    draft: CommandDraft,
) -> Result<(), String> {
    state.0.as_ref().map_err(Clone::clone)?.save(draft)
}

#[tauri::command]
fn save_delivery_draft(
    state: tauri::State<'_, DesktopState>,
    draft: DeliveryDraft,
) -> Result<(), String> {
    state.0.as_ref().map_err(Clone::clone)?.save_delivery(draft)
}

#[tauri::command]
async fn worker_rpc(
    endpoint: tauri::State<'_, WorkerEndpoint>,
    request: LocalRequest,
) -> Result<LocalResponse, String> {
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
            Ok(DesktopDraftStore::new(directory.join(DESKTOP_STATE_FILE)))
        });
    tauri::Builder::default()
        .manage(WorkerEndpoint(endpoint))
        .manage(DesktopState(drafts))
        .invoke_handler(tauri::generate_handler![
            worker_rpc,
            load_command_drafts,
            save_command_draft,
            save_delivery_draft
        ])
        .run(tauri::generate_context!())
        .expect("WinBridge desktop runtime");
}
