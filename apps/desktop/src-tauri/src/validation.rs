//! Explicit, isolated native validation. No debugger port or external control endpoint.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tauri::Manager;
use winbridge_core::desktop_state::DesktopDraftStore;
use winbridge_protocol::{
    AttentionSeverity, AttentionSubmission, CommandSource, DesktopValidationStatus, LocalRequest,
    LocalResponse, TaskSubmission,
};

#[derive(Default)]
pub struct Validation {
    pub enabled: bool,
    probes: Mutex<HashMap<String, serde_json::Value>>,
    signals: Mutex<Vec<String>>,
    pub loaded_windows: Mutex<HashSet<String>>,
    status: Mutex<DesktopValidationStatus>,
    checkpoint: Option<PathBuf>,
    history: Mutex<Vec<serde_json::Value>>,
    pub allow_close: AtomicBool,
    aborted: AtomicBool,
}
impl Validation {
    pub fn new(report: Option<&PathBuf>, restart: bool) -> Self {
        let state = Self {
            enabled: report.is_some(),
            checkpoint: report.map(|p| p.with_extension("progress.json")),
            status: Mutex::new(DesktopValidationStatus {
                enabled: report.is_some(),
                phase: if restart { "restart" } else { "initial" }.into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        state.progress("Запуск отдельного тестового окна", None);
        state
    }
    pub fn progress(&self, step: &str, error: Option<String>) {
        if !self.enabled {
            return;
        }
        let mut status = self.status.lock().unwrap();
        status.step = step.into();
        status.error = error;
        let mut history = self.history.lock().unwrap();
        history.push(serde_json::json!({"at":chrono::Utc::now().to_rfc3339(), "status":*status}));
        let checkpoint = serde_json::json!({"pid":std::process::id(), "executable":std::env::current_exe().ok(), "arguments":std::env::args().skip(1).collect::<Vec<_>>(), "history":*history});
        if let Some(path) = &self.checkpoint {
            if let Err(e) = std::fs::write(path, serde_json::to_vec_pretty(&checkpoint).unwrap()) {
                eprintln!("Validation checkpoint: {e}");
            }
        }
        eprintln!("Native validation: {step}");
    }
    pub fn guard_close(&self) -> Result<(), String> {
        if self.enabled && !self.allow_close.load(Ordering::SeqCst) {
            return Err("Идёт автоматическая проверка. Для остановки используйте «Прервать проверку» в жёлтом блоке.".into());
        }
        Ok(())
    }
}

#[tauri::command]
pub fn desktop_validation_status(state: tauri::State<'_, Validation>) -> DesktopValidationStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
pub fn abort_desktop_validation(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    super::main_only(&window)?;
    let state = app.state::<Validation>();
    if !state.enabled {
        return Err("Native validation is disabled".into());
    }
    state.aborted.store(true, Ordering::SeqCst);
    state.progress(
        "Проверка прервана пользователем",
        Some("User cancelled native validation".into()),
    );
    app.exit(2);
    Ok(())
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
    let script = format!("Promise.resolve().then(() => {{ {expression} }}).catch(error => ({{__validationError:String(error)}})).then(value => window.__TAURI_INTERNALS__.invoke('companion_probe', {{ probe:{key}, value }}));");
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
    let value = app
        .state::<Validation>()
        .probes
        .lock()
        .unwrap()
        .remove(probe)
        .unwrap();
    if let Some(error) = value.get("__validationError") {
        return Err(format!("WebView probe {probe}: {error}"));
    }
    Ok(value)
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

#[path = "validation_settings.rs"]
mod settings;
#[allow(clippy::too_many_arguments)]
pub fn start(
    app: tauri::AppHandle,
    address: Result<String, String>,
    store: Result<Arc<DesktopDraftStore>, String>,
    report: PathBuf,
    restart: bool,
    targets: bool,
    history: bool,
    maintenance: bool,
) {
    tauri::async_runtime::spawn(async move {
        let result = if maintenance {
            settings::run(&app, address, restart).await
        } else if history {
            run_history(&app, address).await
        } else if targets {
            run_targets(&app, address, store).await
        } else {
            run(&app, address, store, restart).await
        };
        if app.state::<Validation>().aborted.load(Ordering::SeqCst) {
            return;
        }
        let passed = result.is_ok();
        app.state::<Validation>().status.lock().unwrap().completed = true;
        app.state::<Validation>().progress(
            if passed {
                "Проверки выполнены; сохранение и завершение тестового окна"
            } else {
                "Проверка завершилась ошибкой"
            },
            result.as_ref().err().cloned(),
        );
        let report_value = serde_json::json!({"platform":"windows", "passed":passed, "restart":restart, "observations":result.as_ref().ok(), "error":result.err()});
        if let Err(e) = std::fs::write(&report, serde_json::to_vec_pretty(&report_value).unwrap()) {
            eprintln!("Native validation report: {e}");
            app.exit(1);
            return;
        }
        if passed {
            // Exercise the actual UI save/quit route. PS checks exit and worker survival.
            app.state::<Validation>()
                .allow_close
                .store(true, Ordering::SeqCst);
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
    app.state::<Validation>()
        .progress("Проверка связи с отдельным worker и готовности tray", None);
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
        app.state::<Validation>().progress(
            "Повторный запуск: настройки, счётчик и отсутствие повторного сигнала",
            None,
        );
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
    app.state::<Validation>().progress(
        "Создание исходной задачи и проверка загрузки интерфейса",
        None,
    );
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
    app.state::<Validation>().progress(
        "Отмена добавления проекта без ложной блокировки закрытия",
        None,
    );
    ui_value(app,"settings-ready-for-undo","return !!document.querySelector('[data-testid=\"settings-json\"]')?.value && !document.querySelector('[data-testid=\"settings-add-project\"]')?.disabled;").await?;
    let original_settings = js(
        app,
        "main",
        "settings-before-undo",
        "return document.querySelector('[data-testid=\"settings-json\"]').value;",
    )
    .await?;
    click(app, "main", "settings-add-project").await?;
    ui_value(
        app,
        "settings-added",
        "return !document.querySelector('[data-testid=\"settings-remove-project\"]')?.disabled;",
    )
    .await?;
    click(app, "main", "settings-remove-project").await?;
    let restored_settings = js(
        app,
        "main",
        "settings-after-undo",
        "return document.querySelector('[data-testid=\"settings-json\"]').value;",
    )
    .await?;
    if original_settings != restored_settings {
        return Err("Adding/removing project left formatting-only unsaved settings".into());
    }

    app.state::<Validation>().progress(
        "Зависший save ACK: timeout закрытия, доступные кнопки и позднее сохранение",
        None,
    );
    js(app,"main","defer-save",r#"
      window.__wbOriginalInvoke=window.__TAURI_INTERNALS__.invoke;
      window.__wbDeferredSave=null;
      window.__TAURI_INTERNALS__.invoke=function(command,args) {
        if(command==='save_command_draft') return new Promise((resolve,reject)=>{window.__wbDeferredSave=()=>window.__wbOriginalInvoke(command,args).then(resolve,reject);});
        return window.__wbOriginalInvoke(command,args);
      };
      const label=Array.from(document.querySelectorAll('label')).find(e=>e.textContent==='Timeout, секунд');
      const input=document.getElementById(label.getAttribute('for'));
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'121');
      input.dispatchEvent(new Event('input',{bubbles:true}));
      return true;
    "#).await?;
    ui_value(app, "save-deferred", "return !!window.__wbDeferredSave;").await?;
    click(app, "main", "companion-hide").await?;
    ui_value(app,"close-timeout-visible","return document.body.textContent.includes('Сохранение форм не подтвердилось за 10 секунд') && !document.querySelector('[data-testid=\"companion-hide\"]').disabled && !document.querySelector('[data-testid=\"companion-quit\"]').disabled;").await?;
    if !main.is_visible().map_err(|e| e.to_string())? {
        return Err("Unconfirmed save hid native main window".into());
    }
    js(app,"main","release-save","window.__TAURI_INTERNALS__.invoke=window.__wbOriginalInvoke; window.__wbDeferredSave(); return true;").await?;
    ui_value(
        app,
        "late-save-confirmed",
        "return document.body.textContent.includes('Поля сохранены. Восстановление');",
    )
    .await?;
    if !main.is_visible().map_err(|e| e.to_string())? {
        return Err("Late save resumed expired close request".into());
    }
    app.state::<Validation>()
        .progress("Защита от случайного закрытия тестового окна", None);
    main.close().map_err(|e| e.to_string())?;
    let mut blocked = false;
    for _ in 0..40 {
        if js(app, "main", "close-blocked", "return document.body.textContent.includes('Для остановки используйте «Прервать проверку»');").await? == true {
            blocked = true;
            break;
        }
        pause().await;
    }
    if !blocked || !main.is_visible().map_err(|e| e.to_string())? {
        return Err("Accidental close was not blocked with a visible explanation".into());
    }
    app.state::<Validation>().progress(
        "Закрытие тестового окна: сохранение форм и скрытие в tray",
        None,
    );
    app.state::<Validation>()
        .allow_close
        .store(true, Ordering::SeqCst);
    main.close().map_err(|e| e.to_string())?;
    wait("main hidden", || {
        main.is_visible().map(|v| !v).map_err(|e| e.to_string())
    })
    .await?;
    app.state::<Validation>()
        .allow_close
        .store(false, Ordering::SeqCst);
    app.state::<Validation>()
        .progress("Повторный запуск открывает прежнее тестовое окно", None);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let secondary = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let mut command = std::process::Command::new(exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW: debug console must not appear.
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
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
    app.state::<Validation>()
        .allow_close
        .store(true, Ordering::SeqCst);
    click(app, "main", "companion-hide").await?;
    wait("main hidden again", || {
        main.is_visible().map(|v| !v).map_err(|e| e.to_string())
    })
    .await?;
    app.state::<Validation>()
        .allow_close
        .store(false, Ordering::SeqCst);
    app.state::<Validation>().progress(
        "Уведомление при скрытом окне: popup, звук и сохранение фокуса",
        None,
    );
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
    app.state::<Validation>()
        .progress("Открытие точной страницы задачи из popup", None);
    click(app, "attention-popup", "popup-hide").await?;
    wait("popup dismissed", || {
        Ok(!popup.is_visible().map_err(|e| e.to_string())?)
    })
    .await?;
    let LocalResponse::AttentionList { page } = call(
        &address,
        LocalRequest::ListAttention {
            after_sequence: 0,
            pending_only: true,
        },
    )
    .await?
    else {
        return Err("Attention after dismiss".into());
    };
    if !page.records.iter().any(|event| event.attention_id == first) {
        return Err("Dismissing popup acknowledged attention".into());
    }
    // Reopen the same notification to exercise the existing task navigation path.
    popup.show().map_err(|e| e.to_string())?;
    click(app, "attention-popup", "popup-open-task").await?;
    wait("popup action reopened main", || {
        main.is_visible().map_err(|e| e.to_string())
    })
    .await?;
    task_page(app, &task_id).await?;
    app.state::<Validation>().progress(
        "Rate limit, подтверждение отложенного события и объединение сигналов",
        None,
    );
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
        serde_json::json!({"trayReady":true,"configUndoRestoresBytes":true,"closeSaveTimeoutRecovers":true,"lateSaveDoesNotHide":true,"popupDismissWithoutAck":true,"accidentalCloseBlocked":true,"hideKeepsProcess":true,"singleInstanceReopens":true,"backgroundPopup":true,"noFocusSteal":true,"nativeSoundApiSucceeded":true,"taskPage":task_id,"previousTaskPage":baseline,"openingDoesNotAcknowledge":true,"deferredAckSuppressed":true,"burstAggregated":true,"signals":signals,"pendingCount":pending}),
    )
}

async fn run_targets(
    app: &tauri::AppHandle,
    address: Result<String, String>,
    store: Result<Arc<DesktopDraftStore>, String>,
) -> Result<serde_json::Value, String> {
    if !cfg!(windows) {
        return Err("Actual Windows required".into());
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
        || !info.attention_targets_available
    {
        return Err(
            "Refusing validation outside its isolated target-capable worker/runtime".into(),
        );
    }
    wait("tray ready", || {
        Ok(app
            .state::<super::companion::Companion>()
            .snapshot()?
            .tray_ready)
    })
    .await?;
    let mut prefs = store.load()?.notification_preferences;
    prefs.minimum_interval_seconds = 1;
    store.save_preferences(prefs)?;
    let main = app.get_webview_window("main").ok_or("Main window")?;
    let mut workflow_ids = Vec::new();
    let mut event_ids = Vec::new();
    for phase in ["baseline", "target"] {
        app.state::<Validation>().progress(
            if phase == "baseline" {
                "Загрузка исходного workflow и подтверждение его события"
            } else {
                "Ошибка workflow до Task ID при скрытом окне"
            },
            None,
        );
        if phase == "target" {
            app.state::<Validation>()
                .allow_close
                .store(true, Ordering::SeqCst);
            click(app, "main", "companion-hide").await?;
            wait("main hidden", || {
                main.is_visible().map(|v| !v).map_err(|e| e.to_string())
            })
            .await?;
            app.state::<Validation>()
                .allow_close
                .store(false, Ordering::SeqCst);
        }
        let executable = runtime
            .join("project/winbridge-fixture.exe")
            .to_string_lossy()
            .into_owned();
        let command = winbridge_protocol::CommandSource::Direct {
            executable,
            arguments: vec!["success".into()],
            working_directory: ".".into(),
        };
        let request = winbridge_protocol::WorkflowSubmission {
            protocol_version: 1,
            request_id: phase.into(),
            idempotency_key: phase.into(),
            name: format!("Native workflow target · {phase}"),
            project_id: info.projects[0].id.clone(),
            snapshot_id: format!("missing-native-package-{phase}"),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 60000,
            build: command.clone(),
            start: command,
            readiness: winbridge_protocol::ReadinessProbe {
                stdout_contains: "READY".into(),
                timeout_ms: 5000,
                netwatch: None,
            },
            log_path: "logs/app.log".into(),
            artifacts: vec![],
        };
        let id = match call(
            &address,
            LocalRequest::SubmitWorkflow {
                request: Box::new(request),
            },
        )
        .await?
        {
            LocalResponse::Workflow { workflow } => workflow.workflow_id,
            _ => return Err("Workflow response".into()),
        };
        let mut event = None;
        for _ in 0..120 {
            if let LocalResponse::AttentionEvents { page } = call(
                &address,
                LocalRequest::ListAttentionEvents {
                    after_sequence: 0,
                    pending_only: true,
                },
            )
            .await?
            {
                event = page.records.into_iter().find(|r| {
                    r.target
                        == winbridge_protocol::AttentionTarget::Workflow {
                            workflow_id: id.clone(),
                        }
                });
            }
            if event.is_some() {
                break;
            }
            pause().await;
        }
        let event = event.ok_or("Workflow attention missing")?;
        wait("workflow popup signal", || {
            Ok(app
                .state::<Validation>()
                .signals
                .lock()
                .unwrap()
                .contains(&event.attention_id))
        })
        .await?;
        if phase == "target" {
            let popup = app
                .get_webview_window("attention-popup")
                .ok_or("Popup window")?;
            if main.is_visible().map_err(|e| e.to_string())?
                || !popup.is_visible().map_err(|e| e.to_string())?
            {
                return Err("Workflow popup did not preserve hidden main".into());
            }
            #[cfg(windows)]
            unsafe {
                if windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
                    == popup.hwnd().map_err(|e| e.to_string())?.0 as _
                {
                    return Err("Workflow popup stole focus".into());
                }
            }
            app.state::<Validation>().progress(
                "Popup открывает точный workflow вместо выдуманной task",
                None,
            );
            click(app, "attention-popup", "popup-open-task").await?;
        }
        let mut confirmed = false;
        for _ in 0..100 {
            if js(app,"main","workflow-page","return document.querySelector('[data-confirmed-workflow-id]')?.getAttribute('data-confirmed-workflow-id') ?? null;").await? == id { confirmed=true;break; }
            pause().await;
        }
        if !confirmed {
            return Err("Confirmed workflow page did not open".into());
        }
        let LocalResponse::AttentionEvents { page } = call(
            &address,
            LocalRequest::ListAttentionEvents {
                after_sequence: 0,
                pending_only: true,
            },
        )
        .await?
        else {
            return Err("Feed response".into());
        };
        if !page
            .records
            .iter()
            .any(|r| r.attention_id == event.attention_id)
        {
            return Err("Opening workflow implicitly acknowledged its event".into());
        }
        // Actual React ACK action, separately from navigation.
        click(
            app,
            "main",
            &format!("ack-attention-{}", event.attention_id),
        )
        .await?;
        for _ in 0..100 {
            let LocalResponse::AttentionEvents { page } = call(
                &address,
                LocalRequest::ListAttentionEvents {
                    after_sequence: 0,
                    pending_only: true,
                },
            )
            .await?
            else {
                return Err("Feed response".into());
            };
            if !page
                .records
                .iter()
                .any(|r| r.attention_id == event.attention_id)
            {
                break;
            }
            pause().await;
        }
        if let Some(popup) = app.get_webview_window("attention-popup") {
            super::companion::companion_dismiss(app.clone(), popup).await?;
        }
        workflow_ids.push(id);
        event_ids.push(event.attention_id);
    }
    let LocalResponse::AttentionEvents { page } = call(
        &address,
        LocalRequest::ListAttentionEvents {
            after_sequence: 0,
            pending_only: true,
        },
    )
    .await?
    else {
        return Err("Feed response".into());
    };
    if page.pending_count != 0 {
        return Err("Workflow UI ACK was not committed".into());
    }
    Ok(
        serde_json::json!({"genericWorkflowPopup":true,"tasklessFailure":true,"exactWorkflowPage":true,"noFocusSteal":true,"openingDoesNotAcknowledge":true,"explicitUiAck":true,"workflowIds":workflow_ids,"eventIds":event_ids}),
    )
}

async fn task_terminal(address: &str, id: &str) -> Result<(), String> {
    for _ in 0..120 {
        match call(address, LocalRequest::GetTask { task_id: id.into() }).await? {
            LocalResponse::Task { task } if task.status.is_terminal() => return Ok(()),
            LocalResponse::Task { .. } => pause().await,
            _ => return Err("Task response".into()),
        }
    }
    Err(format!("Task did not finish: {id}"))
}
async fn ui_value(
    app: &tauri::AppHandle,
    probe: &str,
    expression: &str,
) -> Result<serde_json::Value, String> {
    for _ in 0..120 {
        let value = js(app, "main", probe, expression).await?;
        if !value.is_null() && value != false {
            return Ok(value);
        }
        pause().await;
    }
    Err(format!("Native UI deadline: {probe}"))
}
async fn set_text(app: &tauri::AppHandle, test_id: &str, value: &str) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let expression = format!("const e=document.querySelector('[data-testid=\"{test_id}\"]'); if(!e)return false; const p=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(p,'value').set.call(e,{encoded});e.dispatchEvent(new Event('input',{{bubbles:true}}));return true;");
    ui_value(app, "set-text", &expression).await?;
    pause().await;
    Ok(())
}
async fn run_history(
    app: &tauri::AppHandle,
    address: Result<String, String>,
) -> Result<serde_json::Value, String> {
    use sha2::{Digest, Sha256};
    if !cfg!(windows) {
        return Err("Actual Windows required".into());
    }
    let address = address?;
    let info = match call(&address, LocalRequest::Info).await? {
        LocalResponse::Info { info } => info,
        _ => return Err("Info response".into()),
    };
    let runtime = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Runtime directory")?
        .to_path_buf();
    let reports = app.state::<super::DiagnosticDirectory>().0.clone()?;
    if !info.instance_id.starts_with("tray-gate-")
        || info.node_id != "tray-validation"
        || info.projects.len() != 1
        || std::path::Path::new(&info.projects[0].workspace_root) != runtime.join("project")
        || !info.history_available
        || reports != runtime.join("reports")
    {
        return Err(
            "Refusing history validation outside its isolated runtime/report directory".into(),
        );
    }
    app.state::<Validation>().progress(
        "История: настоящая task и более 50 последующих записей",
        None,
    );
    let executable = runtime
        .join("project/winbridge-fixture.exe")
        .to_string_lossy()
        .into_owned();
    let old = match call(
        &address,
        LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "history-old".into(),
                idempotency_key: "history-old".into(),
                project_id: info.projects[0].id.clone(),
                timeout_ms: 60000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable,
                    arguments: vec![
                        "arguments".into(),
                        "--token".into(),
                        "history-native-secret".into(),
                        "Привет ✓".into(),
                    ],
                    working_directory: ".".into(),
                },
            },
        },
    )
    .await?
    {
        LocalResponse::Task { task } => task.task_id,
        _ => return Err("Submit response".into()),
    };
    task_terminal(&address, &old).await?;
    for i in 0..55 {
        let id = fixture(
            &address,
            &runtime,
            &info.projects[0].id,
            &format!("history-recent-{i}"),
        )
        .await?;
        call(
            &address,
            LocalRequest::Cancel {
                task_id: id.clone(),
            },
        )
        .await?;
        task_terminal(&address, &id).await?;
    }
    let before = match call(&address, LocalRequest::StorageUsage).await? {
        LocalResponse::StorageUsage { usage } => usage,
        _ => return Err("Usage response".into()),
    };
    ui_value(
        app,
        "history-first-page",
        "return !!document.querySelector('[data-testid=\"history-older\"]:not(:disabled)');",
    )
    .await?;
    app.state::<Validation>().progress(
        "История: следующая страница → точный старый Task ID без выполнения",
        None,
    );
    click(app, "main", "history-older").await?;
    click(app, "main", &format!("history-open-{old}")).await?;
    task_page(app, &old).await?;
    app.state::<Validation>()
        .progress("Отчёт: безопасный preview без command/output", None);
    click(app, "main", "diagnostic-prepare").await?;
    let text = ui_value(
        app,
        "diagnostic-default",
        "return document.querySelector('[data-testid=\"diagnostic-preview\"]')?.value || null;",
    )
    .await?;
    let defaults: serde_json::Value =
        serde_json::from_str(text.as_str().ok_or("Preview text")?).map_err(|e| e.to_string())?;
    if defaults["commandIncluded"] != false
        || defaults["outputIncluded"] != false
        || text.as_str().unwrap().contains("history-native-secret")
    {
        return Err("Default report disclosed omitted command/output".into());
    }
    app.state::<Validation>().progress(
        "Отчёт: Unicode, маскирование и ручное редактирование preview",
        None,
    );
    click(app, "main", "diagnostic-include-command").await?;
    click(app, "main", "diagnostic-include-output").await?;
    set_text(app, "diagnostic-redactions", "history-native-secret").await?;
    click(app, "main", "diagnostic-prepare").await?;
    let text=ui_value(app,"diagnostic-redacted","const e=document.querySelector('[data-testid=\"diagnostic-preview\"]');if(!e)return null;const j=JSON.parse(e.value);return j.commandIncluded && j.outputIncluded ? e.value:null;").await?;
    let text = text.as_str().ok_or("Redacted preview text")?;
    if text.contains("history-native-secret")
        || !text.contains("Привет ✓")
        || !text.contains("[REDACTED]")
    {
        return Err("Report redaction/Unicode mismatch".into());
    }
    let mut reviewed: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    reviewed["reviewNote"] = serde_json::json!("Проверено ✓");
    let reviewed = serde_json::to_string_pretty(&reviewed).map_err(|e| e.to_string())?;
    set_text(app, "diagnostic-preview", &reviewed).await?;
    app.state::<Validation>().progress(
        "Отчёт: сохранение показанного JSON и повтор с тем же ID",
        None,
    );
    click(app, "main", "diagnostic-save").await?;
    let saved=ui_value(app,"diagnostic-file","const e=document.querySelector('[data-testid=\"diagnostic-saved\"]');return e?{path:e.getAttribute('data-diagnostic-path'),sha256:e.getAttribute('data-diagnostic-sha256')}:null;").await?;
    let path = std::path::PathBuf::from(saved["path"].as_str().ok_or("Saved path")?);
    if path.parent() != Some(reports.as_path()) {
        return Err("Report escaped isolated report directory".into());
    }
    let actual = std::fs::read(&path).map_err(|e| e.to_string())?;
    let sha = format!("{:x}", Sha256::digest(&actual));
    if actual != reviewed.as_bytes() || saved["sha256"] != sha {
        return Err("Saved file differs from reviewed preview/SHA-256".into());
    }
    let count = std::fs::read_dir(&reports)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("diagnostic-"))
        .count();
    click(app, "main", "diagnostic-save").await?;
    ui_value(app,"diagnostic-retry-finished","return !!document.querySelector('[data-testid=\"diagnostic-saved\"]') && !!document.querySelector('[data-testid=\"diagnostic-save\"]:not([data-loading])');").await?;
    let count_after = std::fs::read_dir(&reports)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("diagnostic-"))
        .count();
    let after = match call(&address, LocalRequest::StorageUsage).await? {
        LocalResponse::StorageUsage { usage } => usage,
        _ => return Err("Usage response".into()),
    };
    if count != 1
        || count_after != count
        || before.task_count != after.task_count
        || before.pending_attention_count != after.pending_attention_count
    {
        return Err(
            "History/report UI created a task, acknowledged attention or duplicated report".into(),
        );
    }
    Ok(
        serde_json::json!({"historyTaskId":old,"historyOlderPage":true,"exactTaskPage":true,"historyReadHasNoEffects":true,"defaultReportOmittedCommandAndOutput":true,"literalRedactionAndUnicode":true,"editedPreviewSavedExactly":true,"retryKeptOneFile":true,"diagnosticFile":path,"diagnosticBytes":actual.len(),"diagnosticSha256":sha,"taskCount":after.task_count}),
    )
}
