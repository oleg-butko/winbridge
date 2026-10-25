//! Frozen schema16 shutdown projection; data inspection never constructs a Store.
use rusqlite::{Connection, OptionalExtension};
use winbridge_protocol::update::{WorkerShutdownCommand, WorkerShutdownReceipt};

pub fn shutdown(c: &Connection, update: &str) -> Result<Option<WorkerShutdownReceipt>, String> {
    let row: Option<(String, String)> = c
        .query_row(
            "SELECT command,record FROM worker_shutdown_receipts WHERE update_id=?1",
            [update],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((command, record)) = row else {
        return Ok(None);
    };
    let command: WorkerShutdownCommand =
        serde_json::from_str(&command).map_err(|e| e.to_string())?;
    let receipt: WorkerShutdownReceipt =
        serde_json::from_str(&record).map_err(|e| e.to_string())?;
    if receipt.update_id != update || receipt.command != command {
        return Err("Saved shutdown projection differs from its command/index".into());
    }
    Ok(Some(receipt))
}
