use super::*;
use winbridge_protocol::*;

pub(super) async fn run(
    app: &tauri::AppHandle,
    address: Result<String, String>,
    restart: bool,
) -> Result<serde_json::Value, String> {
    if !cfg!(windows) {
        return Err("Actual Windows required".into());
    }
    let address = address?;
    let LocalResponse::Info { info } = call(&address, LocalRequest::Info).await? else {
        return Err("Info response".into());
    };
    let runtime = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Runtime directory")?
        .to_path_buf();
    let LocalResponse::RuntimeSettings { settings } =
        call(&address, LocalRequest::GetRuntimeSettings).await?
    else {
        return Err("Settings response".into());
    };
    if !info.instance_id.starts_with("tray-gate-")
        || info.node_id != "tray-validation"
        || info.projects.len() != 1
        || std::path::Path::new(&info.projects[0].workspace_root) != runtime.join("project")
        || std::path::Path::new(&settings.path) != runtime.join("winbridge-worker.local.json")
        || !info.maintenance_available
    {
        return Err("Refusing settings validation outside its isolated runtime/config".into());
    }
    ui_value(
        app,
        "settings-loaded",
        "return !!document.querySelector('[data-testid=\"settings-json\"]')?.value;",
    )
    .await?;
    if restart {
        app.state::<Validation>().progress(
            "Настройки: подтверждение active config после ручного перезапуска тестового worker",
            None,
        );
        let active: serde_json::Value =
            serde_json::from_str(&settings.active_json).map_err(|e| e.to_string())?;
        if settings.restart_required
            || active["maxOutputBytesPerTask"] != 4096
            || active["outputRetention"]["automatic"] != true
        {
            return Err("Restart did not activate saved quota/retention policy".into());
        }
        let ui=js(app,"main","settings-restart-ui","return JSON.parse(document.querySelector('[data-testid=\"settings-json\"]').value).outputRetention.automatic===true && !document.querySelector('[data-testid=\"settings-restart-required\"]');").await?;
        if ui != true {
            return Err("Restart UI does not match active config".into());
        }
        click(app, "main", "cleanup-preview").await?;
        ui_value(app,"settings-restart-cleanup","return document.querySelector('[data-testid=\"cleanup-summary\"]')?.textContent.includes('0 задач');").await?;
        return Ok(
            serde_json::json!({"activePolicyAfterWorkerRestart":true,"restartRequired":false,"configPath":settings.path,"generation":info.generation,"emptyCleanupDoesNotExecute":true}),
        );
    }
    app.state::<Validation>().progress(
        "Настройки: running fixture, UI save и защита закрытия несохранённой формы",
        None,
    );
    let LocalResponse::Task { task: accepted } = call(
        &address,
        LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: "maintenance-native-live".into(),
                idempotency_key: "maintenance-native-live".into(),
                project_id: info.projects[0].id.clone(),
                timeout_ms: 120000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: runtime
                        .join("project/winbridge-fixture.exe")
                        .to_string_lossy()
                        .into(),
                    arguments: vec!["long".into()],
                    working_directory: ".".into(),
                },
            },
        },
    )
    .await?
    else {
        return Err("Submit response".into());
    };
    let id = accepted.task_id;
    let mut pid = None;
    for _ in 0..80 {
        if let LocalResponse::Task { task } = call(
            &address,
            LocalRequest::GetTask {
                task_id: id.clone(),
            },
        )
        .await?
        {
            pid = task.process_id;
            if pid.is_some() {
                break;
            }
        }
        pause().await;
    }
    if pid.is_none() {
        return Err("Fixture PID not confirmed".into());
    }
    let original = std::fs::read(&settings.path).map_err(|e| e.to_string())?;
    let mut next: serde_json::Value =
        serde_json::from_str(&settings.saved_json).map_err(|e| e.to_string())?;
    next["maxOutputBytesPerTask"] = serde_json::json!(4096);
    next["outputRetention"] = serde_json::json!({"automatic":true,"olderThanDays":30});
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    let quoted = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    let edit=format!("const input=document.querySelector('[data-testid=\"settings-json\"]');input.scrollIntoView();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(input,{quoted});input.dispatchEvent(new Event('input',{{bubbles:true}}));return true;");
    js(app, "main", "settings-edit", &edit).await?;
    ui_value(app,"settings-dirty","return document.querySelector('#native-settings').textContent.includes('Есть несохранённые изменения');").await?;
    click(app, "main", "companion-quit").await?;
    ui_value(app,"settings-close-blocked","return document.body.textContent.includes('Сохраните или отмените изменения конфигурации');").await?;
    click(app, "main", "settings-save").await?;
    ui_value(app,"settings-saved","return !!document.querySelector('[data-testid=\"settings-saved\"]') && !!document.querySelector('[data-testid=\"settings-restart-required\"]');").await?;
    let LocalResponse::RuntimeSettings { settings: saved } =
        call(&address, LocalRequest::GetRuntimeSettings).await?
    else {
        return Err("Settings response".into());
    };
    let active: serde_json::Value =
        serde_json::from_str(&saved.active_json).map_err(|e| e.to_string())?;
    if !saved.restart_required
        || active["maxOutputBytesPerTask"] == 4096
        || std::fs::read(&saved.path).map_err(|e| e.to_string())? != json.as_bytes()
    {
        return Err("Settings save did not preserve active config or exact preview bytes".into());
    }
    let backups: Vec<_> = std::fs::read_dir(&runtime)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("winbridge-worker.local.json.before-")
        })
        .collect();
    if backups.len() != 1
        || std::fs::read(backups[0].path()).map_err(|e| e.to_string())? != original
    {
        return Err("Previous config backup not confirmed".into());
    }
    let LocalResponse::Task { task } = call(
        &address,
        LocalRequest::GetTask {
            task_id: id.clone(),
        },
    )
    .await?
    else {
        return Err("Task response".into());
    };
    if task.status != TaskStatus::Running || task.process_id != pid {
        return Err("UI settings save altered the running process".into());
    }
    click(app, "main", "cleanup-preview").await?;
    ui_value(app,"settings-cleanup-empty","return document.querySelector('[data-testid=\"cleanup-summary\"]')?.textContent.includes('0 задач') && document.querySelector('[data-testid=\"cleanup-apply\"]')?.disabled;").await?;
    call(
        &address,
        LocalRequest::Cancel {
            task_id: id.clone(),
        },
    )
    .await?;
    task_terminal(&address, &id).await?;
    Ok(
        serde_json::json!({"settingsSavedThroughActualUi":true,"exactPreviewBytes":true,"backupPreserved":true,"unsavedCloseBlocked":true,"activeConfigAndPidUnchanged":true,"taskId":id,"pid":pid,"restartRequired":true,"emptyPreviewDoesNotDelete":true,"savedSha256":saved.saved_sha256}),
    )
}
