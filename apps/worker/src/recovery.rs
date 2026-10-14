//! A latch outside SQLite survives a database-only restore and ordinary restart.
//! It is not an automatic rollback detector or permission to clear uncertainty.
use crate::{store::now_ms, Result};
use std::{fs::OpenOptions, io::Write, path::Path};

pub(crate) fn persist_new_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("Cannot create recovery record {}: {e}", path.display()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(unix)]
    std::fs::File::open(path.parent().ok_or("recovery record has no parent")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| e.to_string())?;
    if std::fs::read(path).map_err(|e| e.to_string())? != bytes {
        return Err("Recovery record readback mismatch; admission remains blocked".into());
    }
    Ok(())
}

pub const RECOVERY_MARKER: &str = "journal-recovery.required";
pub(crate) const SOURCE_RETIRED_MARKER: &str = "journal-source-retired.required";
pub(crate) const INSPECTION_REASON: &str = "Журнал открыт только для диагностики восстановления. Сохранённые статусы не подтверждают состояние процессов. Запуск, отмена, очистка и изменение настроек запрещены; обычный перезапуск не снимает блокировку.";

pub(crate) fn latch(state: &Path) -> Result<()> {
    latch_named(state, RECOVERY_MARKER)
}
pub(crate) fn latch_source(state: &Path) -> Result<()> {
    latch(state)?;
    latch_named(state, SOURCE_RETIRED_MARKER)
}
fn latch_named(state: &Path, name: &str) -> Result<()> {
    let path = state.join(name);
    crate::reject_symlink(&path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            writeln!(file, "WinBridge journal recovery requested at {}.\n{INSPECTION_REASON}\nDo not delete this marker to resume a restored journal. Controlled recovery is required.", now_ms())
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("recovery marker was not persisted: {e}"))?;
            #[cfg(unix)]
            std::fs::File::open(state)
                .and_then(|directory| directory.sync_all())
                .map_err(|e| format!("recovery marker directory was not synchronized: {e}"))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("cannot persist recovery marker: {e}")),
    }
    Ok(())
}

pub(crate) fn reject_retired_source(state: &Path) -> Result<()> {
    let retired = state.join(SOURCE_RETIRED_MARKER);
    crate::reject_symlink(&retired)?;
    if retired.try_exists().map_err(|e| e.to_string())? {
        return Err("Recovery marker permanently retires this source; an older activation receipt cannot resume it. Keep its history/markers and use read-only inspection".into());
    }
    Ok(())
}

pub(crate) fn reject_normal_start(config: &winbridge_core::config::WorkerConfig) -> Result<()> {
    let state = &config.state_directory;
    reject_retired_source(state)?;
    let path = state.join(RECOVERY_MARKER);
    crate::reject_symlink(&path)?;
    let paired = state
        .join("recovery-pair.json")
        .try_exists()
        .map_err(|e| e.to_string())?
        || state
            .join("recovery-activated.json")
            .try_exists()
            .map_err(|e| e.to_string())?;
    if paired {
        if !path.is_file() {
            return Err("Prepared recovery pair has lost its execution gate; restore its metadata, no effects allowed".into());
        }
        return crate::recovery_pair::check_active(config);
    }
    if path.try_exists().map_err(|e| e.to_string())? {
        return Err(format!("Recovery marker {} blocks normal execution. Use --config PATH serve-recovery for read-only inspection; do not delete the marker or automatically replay work", path.display()));
    }
    Ok(())
}
