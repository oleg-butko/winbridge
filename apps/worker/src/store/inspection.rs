//! Opening a restored journal must not activate an owner, migrate it, or replay work.
use super::{sql, Result, Store, JOURNAL_SCHEMA_VERSION};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use winbridge_protocol::{ResourceRecovery, ResourceRecoveryEntry};

impl Store {
    /// Minimal frozen schema-12 metadata for cutover guards. This does not
    /// interpret old execution rows or migrate an archived/target journal.
    /// Keeping this separate from the current-schema runtime reader allows
    /// future migrations to preserve earlier recovery receipts.
    pub fn read_admission_header(path: &Path) -> Result<(super::JournalAuthority, bool)> {
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        for suffix in ["-wal", "-shm"] {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            winbridge_platform::require_safe_path(Path::new(&name)).map_err(|e| e.to_string())?;
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sql)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(sql)?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(sql)?;
        if !(12..=JOURNAL_SCHEMA_VERSION).contains(&version) {
            return Err(format!("Unsupported recovery admission metadata schema {version}; no migration/effects allowed"));
        }
        let authority = Self::read_authority(&connection)?;
        let required = Self::read_admission_boundary(&connection, &authority.incarnation)?;
        Ok((authority, required))
    }
    pub fn open_read_only(path: &Path) -> Result<Self> {
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        for suffix in ["-wal", "-shm"] {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            winbridge_platform::require_safe_path(Path::new(&name)).map_err(|e| e.to_string())?;
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sql)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(sql)?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        if version != JOURNAL_SCHEMA_VERSION {
            return Err(format!("read-only inspection requires journal schema {JOURNAL_SCHEMA_VERSION}, found {version}; no migration performed. Keep the recovery marker and inspect with a compatible worker"));
        }
        connection
            .execute_batch("PRAGMA query_only=ON;")
            .map_err(sql)?;
        let authority = Self::read_authority(&connection)?;
        Ok(Self {
            connection,
            authority,
            callback_attempt: None,
            read_only: true,
        })
    }

    /// Includes held leases and legacy executions without inventing a lease or
    /// cleanup receipt. Nothing is physically reconciled in inspection mode.
    pub(crate) fn inspection_resource_summary(&self) -> Result<Option<ResourceRecovery>> {
        let cte = "WITH resources(project,operation,kind,binding,reason) AS (
            SELECT project,operation,kind,physical IS NOT NULL,
                COALESCE(reason,'Physical ownership has not been checked in read-only inspection')
                FROM resource_leases WHERE state!='released'
            UNION ALL
            SELECT t.project,t.id,'task',0,'Legacy task has no durable physical ownership; its saved status is historical'
                FROM tasks t WHERE json_extract(t.record,'$.status') IN ('running','indeterminate')
                AND NOT EXISTS (SELECT 1 FROM resource_leases l WHERE l.kind='task' AND l.operation=t.id)
                AND NOT EXISTS (SELECT 1 FROM resource_leases l,json_each(l.members) m
                    WHERE json_extract(m.value,'$.kind')='task' AND json_extract(m.value,'$.id')=t.id)
            UNION ALL
            SELECT i.project,i.id,'import',0,'Legacy import has no durable ownership; its saved status is historical'
                FROM imports i WHERE json_extract(i.record,'$.status')='running'
                AND NOT EXISTS (SELECT 1 FROM resource_leases l WHERE l.kind='import' AND l.operation=i.id)
                AND NOT EXISTS (SELECT 1 FROM resource_leases l,json_each(l.members) m
                    WHERE json_extract(m.value,'$.kind')='import' AND json_extract(m.value,'$.id')=i.id)
        )";
        let count: i64 = self
            .connection
            .query_row(&format!("{cte} SELECT count(*) FROM resources"), [], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        if count == 0 {
            return Ok(None);
        }
        let mut statement = self.connection.prepare(&format!("{cte} SELECT project,operation,kind,binding,substr(reason,1,512) FROM resources ORDER BY project,kind,operation LIMIT 50")).map_err(sql)?;
        let resources = statement
            .query_map([], |r| {
                Ok(ResourceRecoveryEntry {
                    project_id: r.get(0)?,
                    operation_id: r.get(1)?,
                    operation_kind: r.get(2)?,
                    physical_binding: r.get(3)?,
                    reason: r.get(4)?,
                })
            })
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        Ok(Some(ResourceRecovery {
            unresolved_count: u64::try_from(count)
                .map_err(|_| "negative inspection resource count")?,
            truncated: count > resources.len() as i64,
            resources,
        }))
    }
}
