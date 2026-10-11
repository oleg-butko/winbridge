use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::Manager;
use winbridge_platform::event_log::EventLog;

pub struct Diagnostics {
    pub log: Result<Arc<EventLog>, String>,
}
impl Diagnostics {
    pub fn new(directory: &Path) -> Self {
        Self {
            log: EventLog::open(&directory.join("logs"), 1024 * 1024, 5)
                .map(Arc::new)
                .map_err(|e| e.to_string()),
        }
    }
    pub fn record(&self, name: &str, details: Value) {
        if let Ok(log) = &self.log {
            log.record(name, details);
        }
    }
}
pub fn event(app: &tauri::AppHandle, name: &str, details: Value) {
    app.state::<Diagnostics>().record(name, details);
}
#[tauri::command]
pub fn desktop_log_status(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Diagnostics>,
) -> Result<Value, String> {
    super::main_only(&window)?;
    Ok(match &state.log {
        Ok(log) => log.status(),
        Err(e) => json!({"path":null,"error":e,"droppedRecords":0}),
    })
}
#[tauri::command]
// Tauri injects window/state; the remaining fields are a fixed metadata schema.
#[allow(clippy::too_many_arguments)]
pub fn desktop_ui_event(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Diagnostics>,
    event: String,
    source: String,
    quit: bool,
    phase: String,
    pending_saves: usize,
    elapsed_ms: u64,
) -> Result<(), String> {
    if window.label() != "main" && window.label() != "attention-popup" {
        return Err("Unknown diagnostic window".into());
    }
    if ![
        "heartbeat",
        "error",
        "close.request",
        "close.busy",
        "close.wait",
        "close.saved",
        "close.invoke",
        "close.done",
        "close.failed",
        "close.timeout",
        "close.force",
        "popup.hide",
        "popup.open",
        "popup.done",
        "popup.failed",
    ]
    .contains(&event.as_str())
        || !["runtime", "titlebar", "panel", "tray", "recovery", "popup"].contains(&source.as_str())
        || !["none", "settings", "saves", "native"].contains(&phase.as_str())
        || pending_saves > 128
    {
        return Err("Unknown desktop diagnostic event".into());
    }
    state.record(&format!("ui.{event}"),json!({"window":window.label(),"source":source,"quit":quit,"phase":phase,"pendingSaves":pending_saves,"elapsedMs":elapsed_ms}));
    Ok(())
}
/// An independent thread still records stalled dispatch when async IPC or the GUI stalls.
pub fn watchdog(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let pending = Arc::new(AtomicBool::new(false));
        loop {
            if pending.swap(true, Ordering::SeqCst) {
                event(
                    &app,
                    "native.dispatch.stalled",
                    json!({"minimumWaitSeconds":10}),
                );
            } else {
                let started = Instant::now();
                let main = app.clone();
                let confirmed = pending.clone();
                event(&app, "native.dispatch.queued", json!({}));
                if app
                    .run_on_main_thread(move || {
                        event(
                            &main,
                            "native.dispatch.confirmed",
                            json!({"elapsedMs":started.elapsed().as_millis()}),
                        );
                        confirmed.store(false, Ordering::SeqCst);
                    })
                    .is_err()
                {
                    event(&app, "native.dispatch.failed", json!({}));
                    pending.store(false, Ordering::SeqCst);
                }
            }
            std::thread::sleep(Duration::from_secs(10));
        }
    });
}
