//! A recovered journal can never regain implicit v1 mutation admission.
use super::{sql, Result, Store};
use rusqlite::{params, Connection, TransactionBehavior};

impl Store {
    pub fn requires_fenced_requests(&self) -> Result<bool> {
        Self::read_admission_boundary(&self.connection, &self.authority.incarnation)
    }
    pub(super) fn read_admission_boundary(
        connection: &Connection,
        expected_incarnation: &str,
    ) -> Result<bool> {
        let (incarnation, required): (String, i64) = connection
            .query_row(
                "SELECT incarnation,require_fenced FROM journal_admission WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql)?;
        if incarnation != expected_incarnation || !matches!(required, 0 | 1) {
            return Err("Invalid durable request admission boundary; no effects allowed".into());
        }
        Ok(required == 1)
    }

    /// Only preparation of a fresh empty journal may establish this boundary.
    /// It does not prove old physical owners stopped or authorize cutover.
    pub fn require_fenced_requests(&mut self) -> Result<()> {
        self.require_writable()?;
        self.check_authority()?;
        if self.authority.epoch != 0 {
            return Err("Fenced preparation must happen before any worker authority is activated; live policy cannot change under an in-flight request".into());
        }
        if self.requires_fenced_requests()? {
            return Ok(());
        }
        let incarnation = self.authority.incarnation.clone();
        let transaction = self.write_transaction(TransactionBehavior::Immediate)?;
        let records: i64 = transaction.query_row("SELECT (SELECT count(*) FROM tasks)+(SELECT count(*) FROM imports)+(SELECT count(*) FROM workflows)+(SELECT count(*) FROM applications)+(SELECT count(*) FROM resource_leases)+(SELECT count(*) FROM execution_attempts)+(SELECT count(*) FROM attention)+(SELECT count(*) FROM output)+(SELECT count(*) FROM output_cleanup)", [], |r| r.get(0)).map_err(sql)?;
        if records != 0 {
            return Err("Fenced recovery preparation requires a new empty journal; existing history must remain archived".into());
        }
        let changed = transaction.execute("UPDATE journal_admission SET require_fenced=1 WHERE singleton=1 AND incarnation=?1 AND require_fenced=0",params![incarnation]).map_err(sql)?;
        if changed != 1 {
            return Err("Fenced admission was not committed".into());
        }
        transaction.commit().map_err(sql)
    }
}
