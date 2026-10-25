//! SQLite's backup API preserves a consistent read snapshot, including WAL data.
use super::{sql, Result, Store};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

pub(super) fn require_destination(path: &Path, held: &File, identity: &str) -> Result<()> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    if winbridge_platform::file_identity(held).map_err(|e| e.to_string())? != identity
        || winbridge_platform::file_identity(&File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            != identity
    {
        return Err(
            "Archive destination was rebound; preserve partial files for inspection".into(),
        );
    }
    Ok(())
}
impl Store {
    /// Only metadata reads; the application ID is preserved, not assigned by
    /// export. Existing worker schema16 uses zero and is not migrated here.
    pub(crate) fn archive_sqlite_metadata(&self) -> Result<(u32, u64, u32)> {
        if !self.read_only {
            return Err("Archive metadata requires read-only source".into());
        }
        self.check_authority()?;
        Ok((
            self.connection
                .query_row("PRAGMA page_size", [], |r| r.get(0))
                .map_err(sql)?,
            u64::try_from(
                self.connection
                    .query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0))
                    .map_err(sql)?,
            )
            .map_err(|_| "Negative archive page count")?,
            self.connection
                .query_row("PRAGMA application_id", [], |r| r.get(0))
                .map_err(sql)?,
        ))
    }
    pub fn archive_read_only(&self, destination: &Path) -> Result<()> {
        self.archive_read_only_created(destination, || Ok(()))
    }
    fn archive_read_only_created(
        &self,
        destination: &Path,
        created: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if !self.read_only {
            return Err("Archive requires a read-only source journal".into());
        }
        self.check_authority()?;
        winbridge_platform::require_safe_path(destination).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(destination).map_err(|e| e.to_string())?;
        // An incomplete destination is retained on error, never overwritten.
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // SQLite may write this file, but its name must remain bound to the
            // create-new handle throughout backup, verification and sync.
            options.share_mode(0x1 | 0x2); // FILE_SHARE_READ | FILE_SHARE_WRITE
        }
        let file = options.open(destination).map_err(|e| e.to_string())?;
        let identity = winbridge_platform::file_identity(&file).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        created()?;
        require_destination(destination, &file, &identity)?;
        {
            // Never recreate the path if the create-new destination disappears.
            let mut target = Connection::open_with_flags(
                destination,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(sql)?;
            require_destination(destination, &file, &identity)?;
            rusqlite::backup::Backup::new(&self.connection, &mut target)
                .map_err(sql)?
                .run_to_completion(64, Duration::from_millis(10), None)
                .map_err(sql)?;
        }
        require_destination(destination, &file, &identity)?;
        file.sync_all().map_err(|e| e.to_string())?;
        winbridge_platform::source_files::sync_parent(destination).map_err(|e| e.to_string())?;
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
        self.check_authority()?;
        require_destination(destination, &file, &identity)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_captures_uncheckpointed_wal_and_preserves_source_authority_and_bytes() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("journal.sqlite");
        let mut writer = Store::open(&source).unwrap();
        writer.begin_authority().unwrap();
        writer.connection.execute_batch("PRAGMA wal_autocheckpoint=0; CREATE TABLE archive_probe(value TEXT); INSERT INTO archive_probe VALUES('final callback after stop receipt');").unwrap();
        let wal = root.path().join("journal.sqlite-wal");
        assert!(std::fs::metadata(&wal).unwrap().len() > 0);
        let before = (
            std::fs::read(&source).unwrap(),
            std::fs::read(&wal).unwrap(),
        );
        let reader = Store::open_read_only(&source).unwrap();
        let destination = root.path().join("backup.sqlite");
        reader.archive_read_only(&destination).unwrap();
        let backup = Store::open_read_only(&destination).unwrap();
        assert_eq!(backup.journal_authority(), reader.journal_authority());
        assert_eq!(
            backup
                .connection
                .query_row("SELECT value FROM archive_probe", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "final callback after stop receipt"
        );
        assert_eq!(
            (
                std::fs::read(&source).unwrap(),
                std::fs::read(&wal).unwrap()
            ),
            before
        );
        assert_eq!(writer.journal_authority(), reader.journal_authority());
    }
    #[test]
    fn archive_preserves_existing_and_partial_destinations_and_rejects_stale_source() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut writer = Store::open(&path).unwrap();
        writer.begin_authority().unwrap();
        let reader = Store::open_read_only(&path).unwrap();
        let destination = root.path().join("backup.sqlite");
        assert!(writer.archive_read_only(&destination).is_err());
        assert!(!destination.exists());
        for bytes in [b"user history".as_slice(), b"".as_slice()] {
            std::fs::write(&destination, bytes).unwrap();
            assert!(reader.archive_read_only(&destination).is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        }
        let fresh = root.path().join("fresh.sqlite");
        writer.begin_authority().unwrap();
        assert!(reader.archive_read_only(&fresh).is_err());
        assert!(!fresh.exists());
    }
    #[test]
    fn archive_created_handle_prevents_or_detects_destination_rebinding_before_sqlite_write() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let _writer = Store::open(&path).unwrap();
        let reader = Store::open_read_only(&path).unwrap();
        let destination = root.path().join("backup.sqlite");
        let moved = root.path().join("partial.sqlite");
        let result = reader.archive_read_only_created(&destination, || {
            let renamed = std::fs::rename(&destination, &moved);
            if cfg!(windows) {
                assert!(renamed.is_err(), "Retained handle must deny delete/rename");
            } else {
                renamed.map_err(|e| e.to_string())?;
                std::fs::write(&destination, b"unrelated file").map_err(|e| e.to_string())?;
            }
            Ok(())
        });
        if cfg!(windows) {
            result.unwrap();
            assert!(!moved.exists());
        } else {
            assert!(result.is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated file");
            assert_eq!(std::fs::metadata(moved).unwrap().len(), 0);
        }
    }
}
