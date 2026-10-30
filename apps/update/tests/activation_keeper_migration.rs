//! Actual retained23 finalization ->24 on a consistent copy, never install.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual23 Main30 reference; copy-only24 migration, no installed update"]
fn actual_twenty_three_finalization_survives_twenty_four_and_old_twenty_three_refuses() {
    migration::run(migration::Profile::Keeper23To24);
}
