//! Historical actual19 ->20; only its exact pinned SDK may execute this test.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual19 writer reference; copy-only20 migration, no installed update"]
fn actual_nineteen_writer_history_survives_twenty_and_old_nineteen_refuses() {
    migration::run(migration::Profile::Service19To20);
}
