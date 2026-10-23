//! Synthetic SQL histories, not actual rename receipts or physical admission.
use super::*;
use crate::runtime_exchange::FileExchangeRecord;
use crate::runtime_preparation::tests::{fixture, stages as preparation_stages};
fn planned() -> RuntimeReplacementRecord {
    RuntimeReplacementRecord::planned(RuntimeReplacementIntent {
        schema_version: 1,
        attempt_id: Uuid::new_v4().to_string(),
        admitted_at_ms: 200,
        preparation: preparation_stages(&fixture()).pop().unwrap(),
    })
    .unwrap()
}
fn seed(c: &Connection) -> RuntimeReplacementRecord {
    c.execute_batch(
        "PRAGMA foreign_keys=ON;CREATE TABLE runtime_preparation_records(id TEXT PRIMARY KEY);",
    )
    .unwrap();
    migrate(c).unwrap();
    let r = planned();
    c.execute(
        "INSERT INTO runtime_preparation_records VALUES(?1)",
        [id(&r.intent)],
    )
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    insert(&tx, &r).unwrap();
    tx.commit().unwrap();
    r
}
fn write(
    c: &mut Connection,
    old: &RuntimeReplacementRecord,
    next: &RuntimeReplacementRecord,
) -> Result<()> {
    let tx = c.transaction().map_err(storage)?;
    persist(&tx, old, next)?;
    tx.commit().map_err(storage)
}
fn stages(r: &RuntimeReplacementRecord) -> Vec<RuntimeReplacementRecord> {
    fn step(
        r: &mut RuntimeReplacementRecord,
        all: &mut Vec<RuntimeReplacementRecord>,
        next: FileExchangeRecord,
        i: usize,
    ) {
        *r = r.advance_file(i, next).unwrap();
        all.push(r.clone());
    }
    let issued = r.issue_directory().unwrap();
    let mut r = issued
        .directory_created("retained-directory".into())
        .unwrap();
    let mut all = vec![issued, r.clone()];
    for i in 0..5 {
        let n = r.batch.files[i].issue_retire().unwrap();
        step(&mut r, &mut all, n, i);
        let mut evidence = r.batch.files[i].spec.old.clone();
        evidence.path = r.batch.files[i]
            .spec
            .retained_path()
            .to_str()
            .unwrap()
            .into();
        let n = r.batch.files[i].retired(evidence).unwrap();
        step(&mut r, &mut all, n, i);
        let n = r.batch.files[i].issue_place().unwrap();
        step(&mut r, &mut all, n, i);
        let mut evidence = r.batch.files[i].spec.prepared.clone();
        evidence.path = r.batch.files[i]
            .spec
            .installed_path()
            .to_str()
            .unwrap()
            .into();
        let n = r.batch.files[i].installed(evidence).unwrap();
        step(&mut r, &mut all, n, i);
    }
    all
}
#[test]
fn replacement_ledger_preserves_all_causal_steps_and_terminal_rows() {
    let mut c = Connection::open_in_memory().unwrap();
    let r = seed(&c);
    let mut old = r.clone();
    for next in stages(&r) {
        write(&mut c, &old, &next).unwrap();
        assert!(write(&mut c, &old, &next).is_err());
        assert_eq!(ledger(&c, &r.intent).unwrap(), next);
        old = next;
    }
    assert_eq!(old.revision, 22);
    for sql in [
        "UPDATE runtime_replacement_intents SET record=record",
        "DELETE FROM runtime_replacement_intents",
        "UPDATE runtime_replacement_history SET record=record",
        "DELETE FROM runtime_replacement_history",
        "DELETE FROM runtime_replacement_records",
        "UPDATE runtime_replacement_records SET revision=revision+1",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(ledger(&c, &r.intent).unwrap(), old);
}
#[test]
fn replacement_ignored_intent_current_or_history_writes_roll_back_all_rows() {
    for table in [
        "runtime_replacement_intents",
        "runtime_replacement_records",
        "runtime_replacement_history",
    ] {
        let mut c = Connection::open_in_memory().unwrap();
        let initial = seed(&c);
        let other = planned();
        c.execute(
            "INSERT INTO runtime_preparation_records VALUES(?1)",
            [id(&other.intent)],
        )
        .unwrap();
        c.execute_batch(&format!(
            "CREATE TRIGGER deny BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE);END;"
        ))
        .unwrap();
        {
            let tx = c.transaction().unwrap();
            assert!(insert(&tx, &other).is_err());
        }
        for t in [
            "runtime_replacement_intents",
            "runtime_replacement_records",
            "runtime_replacement_history",
        ] {
            assert_eq!(
                c.query_row(
                    &format!("SELECT count(*) FROM {t} WHERE id=?1"),
                    [id(&other.intent)],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        }
        assert_eq!(ledger(&c, &initial.intent).unwrap(), initial);
        c.execute_batch("DROP TRIGGER deny").unwrap();
        if table != "runtime_replacement_intents" {
            let verb = if table == "runtime_replacement_records" {
                "UPDATE"
            } else {
                "INSERT"
            };
            c.execute_batch(&format!(
                "CREATE TRIGGER deny BEFORE {verb} ON {table} BEGIN SELECT RAISE(IGNORE);END;"
            ))
            .unwrap();
            assert!(write(&mut c, &initial, &initial.issue_directory().unwrap()).is_err());
            assert_eq!(ledger(&c, &initial.intent).unwrap(), initial);
        }
    }
}
#[test]
fn replacement_reopen_of_issued_rename_retains_prefix_without_fresh_permission() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal.sqlite");
    let mut c = Connection::open(&path).unwrap();
    let r = seed(&c);
    let mut current = r.clone();
    for next in stages(&r).into_iter().take(3) {
        write(&mut c, &current, &next).unwrap();
        current = next;
    }
    drop(c);
    let mut c = Connection::open(&path).unwrap();
    let saved = ledger(&c, &r.intent).unwrap();
    assert_eq!(saved, current);
    let next = saved
        .indeterminate("Original rename outcome unknown".into())
        .unwrap();
    write(&mut c, &saved, &next).unwrap();
    drop(c);
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let saved = ledger(&c, &r.intent).unwrap();
    assert_eq!(saved, next);
    assert!(saved.issue_directory().is_err());
    assert!(saved.advance_file(0, saved.batch.files[0].clone()).is_err());
}
#[test]
fn replacement_history_tampering_and_changed_intent_are_detected() {
    let mut c = Connection::open_in_memory().unwrap();
    let r = seed(&c);
    let next = r.issue_directory().unwrap();
    write(&mut c, &r, &next).unwrap();
    let mut forged = r.intent.clone();
    forged.attempt_id = Uuid::new_v4().to_string();
    assert!(ledger(&c, &forged).is_err());
    c.execute_batch("DROP TRIGGER runtime_replacement_history_immutable")
        .unwrap();
    c.execute(
        "UPDATE runtime_replacement_history SET record=?1 WHERE revision=0",
        [encode(&next).unwrap()],
    )
    .unwrap();
    assert!(ledger(&c, &r.intent).is_err());
}
#[test]
fn schema_fourteen_migration_backs_up_fences_owner_and_invents_no_replacement() {
    use winbridge_protocol::update::{UpdateRequest, UpdateSourcePin};
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("state");
    let scope = "a".repeat(64);
    let mut j = UpdateJournal::open(&dir, &scope).unwrap();
    let installation_info = j.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "replacement-migration".into(),
        source: UpdateSourcePin {
            archive_name: "source.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: installation_info.installation_id,
            scope_sha256: scope.clone(),
            worker_build_id: "123".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let owner = j.acquire_owner().unwrap();
    let saved = j.accept(&owner, &request, &request.expected).unwrap();
    let before = j.info().unwrap();
    drop(owner);
    drop(j);
    let c = Connection::open(dir.join("updates.sqlite")).unwrap();
    c.execute_batch("DROP TABLE runtime_replacement_history;DROP TABLE runtime_replacement_records;DROP TABLE runtime_replacement_intents;PRAGMA user_version=14;").unwrap();
    drop(c);
    let mut j = UpdateJournal::open(&dir, &scope).unwrap();
    let after = j.info().unwrap();
    assert_eq!(after.schema_version, 15);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(j.get(&saved.update_id).unwrap(), saved);
    assert!(
        UpdateJournal::inspect_runtime_replacement(&dir, &saved.update_id)
            .unwrap()
            .is_none()
    );
    let backups = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("updates-v14-"))
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let c =
        Connection::open_with_flags(backups[0].path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&c).unwrap(), 14);
    assert_eq!(record(&c, &saved.update_id).unwrap(), saved);
    assert_eq!(
        c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(info(&c).unwrap().owner_epoch, before.owner_epoch);
    for t in [
        "runtime_replacement_intents",
        "runtime_replacement_records",
        "runtime_replacement_history",
    ] {
        assert_eq!(
            j.connection
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
