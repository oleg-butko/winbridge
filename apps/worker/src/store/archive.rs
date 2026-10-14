//! SQLite's backup API preserves a consistent read snapshot, including WAL data.
use super::{sql, Result, Store};
use std::{fs::OpenOptions, path::Path, time::Duration};

impl Store {
    pub fn archive_read_only(&self, destination: &Path) -> Result<()> {
        if !self.read_only {
            return Err("Archive requires a read-only source journal".into());
        }
        self.check_authority()?;
        winbridge_platform::require_safe_path(destination).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(destination).map_err(|e| e.to_string())?;
        // An incomplete destination is retained on error, never overwritten.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(destination).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        {
            let mut target = rusqlite::Connection::open(destination).map_err(sql)?;
            rusqlite::backup::Backup::new(&self.connection, &mut target)
                .map_err(sql)?
                .run_to_completion(64, Duration::from_millis(10), None)
                .map_err(sql)?;
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(destination)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
        let archived = Store::open_read_only(destination)?;
        if archived.authority != self.authority {
            return Err("Archived journal authority mismatch; no activation allowed".into());
        }
        let integrity: String = archived
            .connection
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(sql)?;
        if integrity != "ok" {
            return Err("Archive integrity check failed; no activation allowed".into());
        }
        self.check_authority()
    }
}
