use chrono::Timelike;
use std::{
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use winbridge_core::{desktop_state::DesktopDraftStore, notifications};
use winbridge_protocol::{
    CompanionStatus, LocalRequest, LocalResponse, NotificationCursor, NotificationPreferences,
};

pub struct Companion {
    pub status: Mutex<CompanionStatus>,
    tray_error: Mutex<Option<String>>,
}

impl Companion {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(CompanionStatus::default()),
            tray_error: Mutex::new(None),
        }
    }
    pub fn snapshot(&self) -> Result<CompanionStatus, String> {
        self.status
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "Companion lock poisoned".into())
    }
}

pub fn show_main(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window unavailable")?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

pub fn setup(
    app: &tauri::AppHandle,
    endpoint: Result<String, String>,
    store: Result<Arc<DesktopDraftStore>, String>,
) {
    let result = (|| -> tauri::Result<()> {
        let open =
            tauri::menu::MenuItem::with_id(app, "open", "Открыть журнал", true, None::<&str>)?;
        let quit = tauri::menu::MenuItem::with_id(
            app,
            "quit",
            "Завершить интерфейс…",
            true,
            None::<&str>,
        )?;
        let menu = tauri::menu::Menu::with_items(app, &[&open, &quit])?;
        TrayIconBuilder::with_id("companion")
            .icon(
                app.default_window_icon()
                    .ok_or(tauri::Error::InvalidIcon(std::io::Error::other(
                        "App icon unavailable",
                    )))?
                    .clone(),
            )
            .tooltip("WinBridge · ожидается worker")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| {
                if event.id.as_ref() == "quit" {
                    if let Ok(mut s) = app.state::<Companion>().status.lock() {
                        s.quit_request_sequence += 1;
                    }
                }
                if let Err(e) = show_main(app) {
                    report_error(app, e);
                }
            })
            .on_tray_icon_event(|tray, event| {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    if let Err(e) = show_main(tray.app_handle()) {
                        report_error(tray.app_handle(), e);
                    }
                }
            })
            .build(app)?;
        Ok(())
    })();
    match result {
        Ok(()) => app.state::<Companion>().status.lock().unwrap().tray_ready = true,
        Err(e) => {
            let message = format!("Tray unavailable; window cannot be hidden: {e}");
            *app.state::<Companion>().tray_error.lock().unwrap() = Some(message.clone());
            report_error(app, message);
        }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_signal: Option<Instant> = None;
        loop {
            if let Err(e) = poll(&app, &endpoint, &store, &mut last_signal).await {
                let state = app.state::<Companion>();
                if let Ok(mut s) = state.status.lock() {
                    s.worker_connected = false;
                    s.pending_count = None;
                    s.active_task_count = None;
                    s.error = Some(e);
                };
            }
            if let Err(e) = update_tray(&app) {
                *app.state::<Companion>().tray_error.lock().unwrap() = Some(e.clone());
                app.state::<Companion>().status.lock().unwrap().tray_ready = false;
                report_error(&app, e);
                // Ensure an unavailable tray never strands a hidden main window.
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
}

fn report_error(app: &tauri::AppHandle, error: String) {
    if let Ok(mut s) = app.state::<Companion>().status.lock() {
        s.error = Some(error);
    }
}

async fn rpc(address: &str, request: LocalRequest) -> Result<LocalResponse, String> {
    let response = winbridge_transport::call(address, &request)
        .await
        .map_err(|e| format!("Worker IPC: {e}"))?;
    match response {
        LocalResponse::Error { message } => Err(message),
        response => Ok(response),
    }
}

async fn poll(
    app: &tauri::AppHandle,
    endpoint: &Result<String, String>,
    store: &Result<Arc<DesktopDraftStore>, String>,
    last_signal: &mut Option<Instant>,
) -> Result<(), String> {
    let address = endpoint.as_ref().map_err(Clone::clone)?;
    let info = match rpc(address, LocalRequest::Info).await? {
        LocalResponse::Info { info } => info,
        _ => return Err("Unexpected worker info response".into()),
    };
    {
        let state = app.state::<Companion>();
        let mut s = state.status.lock().map_err(|_| "Companion lock poisoned")?;
        s.worker_connected = true;
        s.last_confirmed_at_ms = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64,
        );
        s.active_task_count = info.active_task_count;
        s.error = state
            .tray_error
            .lock()
            .map_err(|_| "Tray lock poisoned")?
            .clone();
    }
    // Capability check prevents unknown RPCs against an older installed worker.
    if !info.attention_available {
        app.state::<Companion>()
            .status
            .lock()
            .map_err(|_| "Companion lock poisoned")?
            .pending_count = None;
        report_error(
            app,
            "This worker has no attention support; update worker and desktop together".into(),
        );
        return Ok(());
    }
    let store = match store.as_ref() {
        Ok(store) => store,
        Err(e) => {
            report_error(app, e.clone());
            return Ok(());
        }
    };
    let state = match store.load() {
        Ok(state) => state,
        Err(e) => {
            report_error(app, e);
            return Ok(());
        }
    };
    let after = state
        .notification_cursors
        .iter()
        .find(|c| c.worker_instance_id == info.instance_id && c.node_id == info.node_id)
        .map_or(0, |c| c.after_sequence);
    let mut scanned = after;
    let mut records = Vec::new();
    let mut count = 0;
    // Bounded batches prevent long histories from starving freshness checks.
    for _ in 0..4 {
        let page = match rpc(
            address,
            LocalRequest::ListAttention {
                after_sequence: scanned,
                pending_only: true,
            },
        )
        .await?
        {
            LocalResponse::AttentionList { page } => page,
            _ => return Err("Unexpected worker attention response".into()),
        };
        count = page.pending_count;
        scanned = page.next_sequence;
        records.extend(page.records);
        if !page.has_more {
            break;
        }
    }
    // Honor a preference save that completed while IPC pages were in flight.
    let prefs = match store.load() {
        Ok(state) => state.notification_preferences,
        Err(e) => {
            report_error(app, e);
            return Ok(());
        }
    };
    {
        let state = app.state::<Companion>();
        let mut s = state.status.lock().map_err(|_| "Companion lock poisoned")?;
        s.pending_count = Some(count);
        s.preferences = prefs.clone();
    }
    let local = chrono::Local::now();
    let (cursor, signal) = notifications::plan(
        &prefs,
        (local.hour() * 60 + local.minute()) as u16,
        last_signal.map(|time| time.elapsed().as_secs()),
        after,
        scanned,
        &records,
    );
    if let Some(record) = signal {
        let attention_id = record.attention_id.clone();
        app.state::<Companion>()
            .status
            .lock()
            .map_err(|_| "Companion lock poisoned")?
            .latest_attention = Some(record);
        // Presentation failures remain visible and do not consume the durable cursor.
        let presentation = (|| {
            if prefs.popup_enabled {
                show_popup(app)?;
            }
            if prefs.sound_enabled && prefs.volume_percent > 0 {
                play_sound(prefs.volume_percent)?;
            }
            Ok::<_, String>(())
        })();
        // Also throttle partial failures, e.g. popup succeeded but sound failed.
        *last_signal = Some(Instant::now());
        if let Err(e) = presentation {
            report_error(app, format!("Notification presentation failed: {e}"));
            return Ok(());
        }
        super::validation::presented(app, &attention_id);
    }
    if cursor > after {
        if let Err(e) = store.save_cursor(NotificationCursor {
            worker_instance_id: info.instance_id.clone(),
            node_id: info.node_id.clone(),
            after_sequence: cursor,
        }) {
            report_error(
                app,
                format!("Notification cursor not saved; event may be presented again: {e}"),
            );
        }
    }
    Ok(())
}

fn update_tray(app: &tauri::AppHandle) -> Result<(), String> {
    let s = app.state::<Companion>().snapshot()?;
    if !s.tray_ready {
        return Ok(());
    }
    let tray = app.tray_by_id("companion").ok_or("Tray disappeared")?;
    let count = s
        .pending_count
        .map_or_else(|| "?".into(), |n| n.to_string());
    let tasks = s
        .active_task_count
        .map_or_else(|| "?".into(), |n| n.to_string());
    tray.set_tooltip(Some(format!(
        "WinBridge · {} · внимание {count} · задачи {tasks}",
        if s.worker_connected {
            "worker подключён"
        } else {
            "worker недоступен"
        }
    )))
    .map_err(|e| e.to_string())?;
    if let Some(base) = app.default_window_icon() {
        let mut rgba = base.rgba().to_vec();
        if !s.worker_connected || s.pending_count.is_some_and(|n| n > 0) {
            let (w, h) = (base.width() as usize, base.height() as usize);
            let radius = w.min(h) / 5;
            for y in h.saturating_sub(2 * radius)..h {
                for x in w.saturating_sub(2 * radius)..w {
                    let dx = x as isize - (w - radius) as isize;
                    let dy = y as isize - (h - radius) as isize;
                    if dx * dx + dy * dy <= (radius * radius) as isize {
                        rgba[4 * (y * w + x)..4 * (y * w + x) + 4].copy_from_slice(
                            if s.worker_connected {
                                &[255, 140, 40, 255]
                            } else {
                                &[140, 140, 140, 255]
                            },
                        );
                    }
                }
            }
        }
        tray.set_icon(Some(tauri::image::Image::new_owned(
            rgba,
            base.width(),
            base.height(),
        )))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn show_popup(app: &tauri::AppHandle) -> Result<(), String> {
    let popup = if let Some(window) = app.get_webview_window("attention-popup") {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            "attention-popup",
            WebviewUrl::App("index.html?attention-popup".into()),
        )
        .title("WinBridge · требуется внимание")
        .inner_size(440.0, 270.0)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?
    };
    if let Some(monitor) = popup.current_monitor().map_err(|e| e.to_string())? {
        let area = monitor.work_area();
        let size = popup.outer_size().map_err(|e| e.to_string())?;
        popup
            .set_position(tauri::PhysicalPosition::new(
                area.position.x + area.size.width.saturating_sub(size.width + 16) as i32,
                area.position.y + area.size.height.saturating_sub(size.height + 16) as i32,
            ))
            .map_err(|e| e.to_string())?;
    }
    // Win32's show operation can activate a window; use SW_SHOWNOACTIVATE explicitly.
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
            popup.hwnd().map_err(|e| e.to_string())?.0 as _,
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
        );
    }
    #[cfg(not(windows))]
    popup.show().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn play_sound(volume: u8) -> Result<(), String> {
    #[cfg(windows)]
    {
        let rate = 22050u32;
        let samples = rate / 6;
        let mut wav = Vec::with_capacity(44 + samples as usize * 2);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        for i in 0..samples {
            let phase = i as f64 / samples as f64;
            let envelope = (phase * std::f64::consts::PI).sin();
            let sample = ((i as f64 * 880.0 * std::f64::consts::TAU / rate as f64).sin()
                * envelope
                * 20000.0
                * f64::from(volume)
                / 100.0) as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        // Synchronous playback on this background task retains the memory until completion.
        let ok = unsafe {
            windows_sys::Win32::Media::Audio::PlaySoundW(
                wav.as_ptr() as _,
                std::ptr::null_mut(),
                windows_sys::Win32::Media::Audio::SND_MEMORY
                    | windows_sys::Win32::Media::Audio::SND_NODEFAULT,
            )
        };
        if ok == 0 {
            return Err("Windows PlaySoundW failed".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = volume;
        Err("Built-in sound is available on Windows".into())
    }
}

#[tauri::command]
pub fn companion_status(state: tauri::State<'_, Companion>) -> Result<CompanionStatus, String> {
    state.snapshot()
}

#[tauri::command]
pub fn companion_close(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    quit: bool,
) -> Result<(), String> {
    super::main_only(&window)?;
    if quit {
        app.exit(0);
        return Ok(());
    }
    if !app.state::<Companion>().snapshot()?.tray_ready {
        return Err(
            "Tray unavailable: window stays open. Use ‘Завершить интерфейс’ to exit".into(),
        );
    }
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn companion_open_task(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Companion>();
    {
        let mut s = state.status.lock().map_err(|_| "Companion lock poisoned")?;
        s.open_task_id = Some(
            s.latest_attention
                .as_ref()
                .ok_or("No attention event")?
                .task_id
                .clone(),
        );
        s.open_task_sequence += 1;
    }
    show_main(&app)?;
    companion_dismiss(app)
}

#[tauri::command]
pub fn companion_dismiss(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(popup) = app.get_webview_window("attention-popup") {
        popup.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn save_notification_preferences(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, super::DesktopState>,
    preferences: NotificationPreferences,
) -> Result<(), String> {
    super::main_only(&window)?;
    state
        .0
        .as_ref()
        .map_err(Clone::clone)?
        .save_preferences(preferences)
}
