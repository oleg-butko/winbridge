//! Explicit, isolated native validation. No debugger port or external control endpoint.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::Manager;
use winbridge_core::desktop_state::DesktopDraftStore;
use winbridge_protocol::{
    AttentionSeverity, AttentionSubmission, CommandSource, LocalRequest, LocalResponse,
    TaskSubmission,
};

#[derive(Default)]
pub struct Validation {
    pub enabled: bool,
    probes: Mutex<HashMap<String, serde_json::Value>>,
    signals: Mutex<Vec<String>>,
    pub loaded_windows: Mutex<HashSet<String>>,
}
impl Validation {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }
}

#[tauri::command]
pub fn companion_probe(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Validation>,
    probe: String,
    value: serde_json::Value,
) -> Result<(), String> {
    if !state.enabled || !matches!(window.label(), "main" | "attention-popup") || probe.len() > 100
    {
        return Err("Native validation probe is disabled".into());
    }
    state
        .probes
        .lock()
        .map_err(|_| "Probe lock poisoned")?
        .insert(probe, value);
    Ok(())
}

pub fn presented(app: &tauri::AppHandle, attention_id: &str) {
    let state = app.state::<Validation>();
    if state.enabled {
        state.signals.lock().unwrap().push(attention_id.into());
    }
}

async fn pause() {
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
}
async fn wait<F: FnMut() -> Result<bool, String>>(
    description: &str,
    mut ready: F,
) -> Result<(), String> {
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    while Instant::now() < deadline {
        if ready()? {
            return Ok(());
        }
        pause().await;
    }
    Err(format!("Native validation deadline: {description}"))
}

async fn js(
    app: &tauri::AppHandle,
    label: &str,
    probe: &str,
    expression: &str,
) -> Result<serde_json::Value, String> {
    wait("WebView page loaded", || {
        Ok(app
            .state::<Validation>()
            .loaded_windows
            .lock()
            .unwrap()
            .contains(label))
    })
    .await?;
    let window = app
        .get_webview_window(label)
        .ok_or("Validation window not created")?;
    let key = serde_json::to_string(probe).map_err(|e| e.to_string())?;
    // Expressions are fixed source literals below; task values use JSON serialization.
    let script = format!("Promise.resolve((() => {{ {expression} }})()).then(value => window.__TAURI_INTERNALS__.invoke('companion_probe', {{ probe:{key}, value }}));");
    window.eval(&script).map_err(|e| e.to_string())?;
    wait(probe, || {
        Ok(app
            .state::<Validation>()
            .probes
            .lock()
            .unwrap()
            .contains_key(probe))
    })
    .await?;
    Ok(app
        .state::<Validation>()
        .probes
        .lock()
        .unwrap()
        .remove(probe)
        .unwrap())
}

async fn click(app: &tauri::AppHandle, label: &str, test_id: &str) -> Result<(), String> {
    let expression = format!("const b = document.querySelector('[data-testid=\"{test_id}\"]'); if (!b || b.disabled) return false; b.click(); return true;");
    // A window may exist before its React component is ready.
    for _ in 0..80 {
        if js(app, label, test_id, &expression).await? == true {
            return Ok(());
        }
        pause().await;
    }
    Err(format!("Native UI button unavailable: {test_id}"))
}

async fn call(address: &str, request: LocalRequest) -> Result<LocalResponse, String> {
    let r = winbridge_transport::call(address, &request)
        .await
        .map_err(|e| e.to_string())?;
    if let LocalResponse::Error { message } = r {
        Err(message)
    } else {
        Ok(r)
    }
}

async fn attention(address: &str, task: &str, key: &str) -> Result<String, String> {
    match call(
        address,
        LocalRequest::RequestAttention {
            request: AttentionSubmission {
                request_id: key.into(),
                idempotency_key: key.into(),
                task_id: task.into(),
                severity: AttentionSeverity::Warning,
                message: format!("Native tray check · {key} · Привет ✓"),
            },
        },
    )
    .await?
    {
        LocalResponse::Attention { attention } => Ok(attention.attention_id),
        _ => Err("Unexpected attention response".into()),
    }
}

async fn fixture(
    address: &str,
    runtime: &std::path::Path,
    project: &str,
    key: &str,
) -> Result<String, String> {
    match call(
        address,
        LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: key.into(),
                project_id: project.into(),
                idempotency_key: key.into(),
                timeout_ms: 60000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: runtime
                        .join("project/winbridge-fixture.exe")
                        .to_string_lossy()
                        .into_owned(),
                    arguments: vec!["success".into()],
                    working_directory: ".".into(),
                },
            },
        },
    )
    .await?
    {
        LocalResponse::Task { task } => Ok(task.task_id),
        _ => Err("Task response".into()),
    }
}

async fn task_page(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    for _ in 0..100 {
        if js(app, "main", "task-page", "return document.querySelector('[data-confirmed-task-id]')?.getAttribute('data-confirmed-task-id') ?? null;").await? == id { return Ok(()); }
        pause().await;
    }
    Err("Confirmed worker task page was not opened".into())
}

pub fn start(
    app: tauri::AppHandle,
    address: Result<String, String>,
    store: Result<Arc<DesktopDraftStore>, String>,
    report: PathBuf,
    restart: bool,
) {
    tauri::async_runtime::spawn(async move {
        let result = run(&app, address, store, restart).await;
        let passed = result.is_ok();
        let report_value = serde_json::json!({"platform":"windows", "passed":passed, "restart":restart, "observations":result.as_ref().ok(), "error":result.err()});
        if let Err(e) = std::fs::write(&report, serde_json::to_vec_pretty(&report_value).unwrap()) {
            eprintln!("Native validation report: {e}");
            app.exit(1);
            return;
        }
        if passed {
            // Exercise the actual UI save/quit route. PS checks exit and worker survival.
            if let Err(e) = click(&app, "main", "companion-quit").await {
                eprintln!("Native quit: {e}");
                app.exit(1);
            }
        } else {
            app.exit(1);
        }
    });
}

async fn run(
    app: &tauri::AppHandle,
    address: Result<String, String>,
    store: Result<Arc<DesktopDraftStore>, String>,
    restart: bool,
) -> Result<serde_json::Value, String> {
    if !cfg!(windows) {
        return Err("This scenario requires actual Windows".into());
    }
    let address = address?;
    let store = store?;
    let info = match call(&address, LocalRequest::Info).await? {
        LocalResponse::Info { info } => info,
        _ => return Err("Info response".into()),
    };
    let runtime = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Runtime directory")?
        .to_path_buf();
    if !info.instance_id.starts_with("tray-gate-")
        || info.node_id != "tray-validation"
        || info.projects.len() != 1
        || std::path::Path::new(&info.projects[0].workspace_root) != runtime.join("project")
    {
        return Err("Refusing native validation outside its isolated worker/runtime".into());
    }
    wait("tray ready", || {
        Ok(app
            .state::<super::companion::Companion>()
            .snapshot()?
            .tray_ready)
    })
    .await?;
    let main = app.get_webview_window("main").ok_or("Main window")?;
    if restart {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if !app.state::<Validation>().signals.lock().unwrap().is_empty() {
            return Err("Restart replayed a presented notification".into());
        }
        let prefs = store.load()?.notification_preferences;
        if prefs.volume_percent != 25 || prefs.minimum_interval_seconds != 1 {
            return Err("Preferences did not survive restart".into());
        }
        let s = app.state::<super::companion::Companion>().snapshot()?;
        if s.pending_count != Some(3) {
            return Err(format!(
                "Pending journal count after restart: {:?}",
                s.pending_count
            ));
        }
        return Ok(
            serde_json::json!({"noReplay":true,"savedPreferences":true,"pendingCount":3,"trayReady":true}),
        );
    }
    let defaults = store.load()?.notification_preferences;
    if defaults.volume_percent != 25 || !defaults.sound_enabled || !defaults.popup_enabled {
        return Err("Unexpected notification defaults".into());
    }
    let mut prefs = defaults;
    prefs.minimum_interval_seconds = 1;
    store.save_preferences(prefs.clone())?;
    let baseline = fixture(&address, &runtime, &info.projects[0].id, "baseline-task").await?;
    task_page(app, &baseline).await?;
    // Give the actual React close handler time to install and load forms.
    let mut close_ready = false;
    for _ in 0..80 {
        if js(
            app,
            "main",
            "close-listener",
            "return Boolean(document.querySelector('[data-close-ready=\"true\"]'));",
        )
        .await?
            == true
        {
            close_ready = true;
            break;
        }
        pause().await;
    }
    if !close_ready {
        return Err("Main close listener was not installed".into());
    }
    main.close().map_err(|e| e.to_string())?;
    wait("main hidden", || {
        main.is_visible().map(|v| !v).map_err(|e| e.to_string())
    })
    .await?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let secondary = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let mut child = std::process::Command::new(exe)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Secondary desktop did not exit; owned child stopped".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if !secondary.success() {
        return Err("Secondary launch failed".into());
    }
    wait("single instance reopened main", || {
        main.is_visible().map_err(|e| e.to_string())
    })
    .await?;
    click(app, "main", "companion-hide").await?;
    wait("main hidden again", || {
        main.is_visible().map(|v| !v).map_err(|e| e.to_string())
    })
    .await?;
    let task_id = fixture(&address, &runtime, &info.projects[0].id, "tray-task").await?;
    let first = attention(&address, &task_id, "first").await?;
    wait("notification while hidden", || {
        Ok(app
            .state::<Validation>()
            .signals
            .lock()
            .unwrap()
            .contains(&first))
    })
    .await?;
    let popup = app
        .get_webview_window("attention-popup")
        .ok_or("Popup window missing")?;
    if main.is_visible().map_err(|e| e.to_string())?
        || !popup.is_visible().map_err(|e| e.to_string())?
    {
        return Err("Popup did not preserve the hidden main window".into());
    }
    #[cfg(windows)]
    unsafe {
        if windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
            == popup.hwnd().map_err(|e| e.to_string())?.0 as _
        {
            return Err("Popup stole foreground focus".into());
        }
    }
    click(app, "attention-popup", "popup-open-task").await?;
    wait("popup action reopened main", || {
        main.is_visible().map_err(|e| e.to_string())
    })
    .await?;
    task_page(app, &task_id).await?;
    let before = store.load()?.notification_cursors;
    prefs.minimum_interval_seconds = 3600;
    store.save_preferences(prefs.clone())?;
    let deferred = attention(&address, &task_id, "deferred-then-acked").await?;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    if store.load()?.notification_cursors != before {
        return Err("Deferred event consumed its cursor".into());
    }
    call(
        &address,
        LocalRequest::AcknowledgeAttention {
            attention_id: deferred.clone(),
        },
    )
    .await?;
    let burst_one = attention(&address, &task_id, "burst-one").await?;
    let burst_two = attention(&address, &task_id, "burst-two").await?;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    prefs.minimum_interval_seconds = 1;
    store.save_preferences(prefs)?;
    wait("aggregated burst signal", || {
        Ok(app
            .state::<Validation>()
            .signals
            .lock()
            .unwrap()
            .contains(&burst_two))
    })
    .await?;
    let signals = app.state::<Validation>().signals.lock().unwrap().clone();
    if signals.contains(&deferred) || signals.contains(&burst_one) || signals.len() != 2 {
        return Err(format!(
            "Unexpected notification replay/aggregation: {signals:?}"
        ));
    }
    // Wait until the successful presentation cursor is durably committed.
    wait("presentation cursor saved", || {
        Ok(store.load()?.notification_cursors != before)
    })
    .await?;
    let pending = match call(
        &address,
        LocalRequest::ListAttention {
            after_sequence: 0,
            pending_only: true,
        },
    )
    .await?
    {
        LocalResponse::AttentionList { page } => page.pending_count,
        _ => return Err("Attention list response".into()),
    };
    if pending != 3 {
        return Err("Opening popup acknowledged worker attention".into());
    }
    Ok(
        serde_json::json!({"trayReady":true,"hideKeepsProcess":true,"singleInstanceReopens":true,"backgroundPopup":true,"noFocusSteal":true,"nativeSoundApiSucceeded":true,"taskPage":task_id,"previousTaskPage":baseline,"openingDoesNotAcknowledge":true,"deferredAckSuppressed":true,"burstAggregated":true,"signals":signals,"pendingCount":pending}),
    )
}
