//! Snapshot the exact history rows for both Main25 and fast portable regressions.
//! The optional normalization validates only resource_policy.epoch. All other
//! values remain in the digest; an unexpected table shape is an error.
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::path::Path;
use winbridge_worker::store::Store;

pub fn snapshot(path: &Path, policy_epoch: Option<(u64, u64)>) -> serde_json::Value {
    if let Some((before, after)) = policy_epoch {
        assert_eq!(before.checked_add(1), Some(after));
    }
    let mut c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    let tx = c.transaction().unwrap();
    let names: Vec<String> = tx.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT IN ('journal_authority','worker_journal_activations') ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    let mut data = Vec::new();
    let mut policy_checked = false;
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut q = tx
            .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
            .unwrap();
        let columns = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if let Some((before, after)) = policy_epoch.filter(|_| name == "resource_policy") {
            assert_eq!(q.column_names(), ["singleton", "epoch", "policy"]);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0][0], rusqlite::types::Value::Integer(1));
            assert_eq!(
                rows[0][1],
                rusqlite::types::Value::Integer(i64::try_from(after).unwrap())
            );
            rows[0][1] = rusqlite::types::Value::Integer(i64::try_from(before).unwrap());
            policy_checked = true;
        }
        data.push((name, format!("{rows:?}")));
    }
    assert!(policy_epoch.is_none() || policy_checked);
    let tasks: i64 = tx
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    let output: i64 = tx
        .query_row("SELECT count(*) FROM output", [], |r| r.get(0))
        .unwrap();
    tx.commit().unwrap();
    json!({"sha256":winbridge_update::sha256(&serde_json::to_vec(&data).unwrap()),"tasks":tasks,"output":output})
}
/// Exercise real current-schema SQLite; no process/IPC/activation proof is claimed.
pub fn assert_reconciliation_contract() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = Store::open(&path).unwrap();
    let request = serde_json::from_value(json!({"protocolVersion":1,"requestId":"history",
        "projectId":"fixture","idempotencyKey":"history","timeoutMs":1000,
        "command":{"kind":"direct","executable":"not-executed","arguments":[],"workingDirectory":"."}})).unwrap();
    let task = store
        .accept(
            request,
            winbridge_protocol::ResolvedCommand {
                executable: "not-executed".into(),
                arguments: vec![],
                working_directory: temp.path().to_str().unwrap().into(),
                timeout_ms: 1000,
            },
            None,
        )
        .unwrap();
    let original_output = b"preserved output";
    store
        .append_output(
            &task.task_id,
            winbridge_protocol::OutputStream::Stdout,
            0,
            original_output,
            1024,
        )
        .unwrap();
    drop(store);
    let baseline = snapshot(&path, None);
    assert_eq!(baseline["tasks"], 1);
    assert_eq!(baseline["output"], 1);
    let c = Connection::open(&path).unwrap();
    c.execute("UPDATE resource_policy SET epoch=1", []).unwrap();
    assert_ne!(snapshot(&path, None), baseline);
    assert_eq!(snapshot(&path, Some((0, 1))), baseline);
    assert_eq!(
        c.query_row("SELECT epoch FROM resource_policy", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    c.execute("UPDATE output SET bytes=?1", [b"corrupt output".as_slice()])
        .unwrap();
    assert_ne!(snapshot(&path, Some((0, 1))), baseline);
    c.execute("UPDATE output SET bytes=?1", [original_output.as_slice()])
        .unwrap();
    assert_eq!(snapshot(&path, Some((0, 1))), baseline);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot(&path, Some((0, 2)))
    }))
    .is_err());
    c.execute("UPDATE resource_policy SET policy='{}'", [])
        .unwrap();
    assert_ne!(snapshot(&path, Some((0, 1))), baseline);
    c.execute("UPDATE resource_policy SET epoch=2", []).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot(&path, Some((0, 1)))
    }))
    .is_err());
    c.execute("UPDATE resource_policy SET epoch=1", []).unwrap();
    c.execute("ALTER TABLE resource_policy ADD COLUMN unexpected TEXT", [])
        .unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot(&path, Some((0, 1)))
    }))
    .is_err());
}
