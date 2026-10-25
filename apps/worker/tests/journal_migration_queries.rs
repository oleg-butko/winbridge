//! Real Store/SQLite projection regression; not actual old16 migration proof.
#[path = "support/journal_migration_queries.rs"]
mod queries;
#[test]
fn shutdown_projection_matches_the_real_persisted_table_before_native_migration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let store = winbridge_worker::store::Store::open(&path).unwrap();
    let c =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert!(queries::shutdown(&c, "missing-update").unwrap().is_none());
    store.check_authority().unwrap();
}
