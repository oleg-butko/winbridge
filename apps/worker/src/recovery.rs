//! A latch outside SQLite survives a database-only restore and ordinary restart.
//! It is not an automatic rollback detector or permission to clear uncertainty.
use crate::{store::now_ms, Result};
use std::{fs::OpenOptions, io::Write, path::Path};

pub const RECOVERY_MARKER: &str = "journal-recovery.required";
pub(crate) const INSPECTION_REASON: &str = "Журнал открыт только для диагностики восстановления. Сохранённые статусы не подтверждают состояние процессов. Запуск, отмена, очистка и изменение настроек запрещены; обычный перезапуск не снимает блокировку.";

pub(crate) fn latch(state: &Path) -> Result<()> {
    let path = state.join(RECOVERY_MARKER);
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

pub(crate) fn reject_normal_start(state: &Path) -> Result<()> {
    let path = state.join(RECOVERY_MARKER);
    crate::reject_symlink(&path)?;
    if path.try_exists().map_err(|e| e.to_string())? {
        return Err(format!("Recovery marker {} blocks normal execution. Use --config PATH serve-recovery for read-only inspection; do not delete the marker or automatically replay work", path.display()));
    }
    Ok(())
}
