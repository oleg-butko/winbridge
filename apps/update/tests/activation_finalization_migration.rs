//! Actual retained22 lifetime/local7 history ->23; consistent copy only.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual22 resident reference; copy-only23 migration, no installed update"]
fn actual_twenty_two_lifetime_history_survives_twenty_three_and_old_twenty_two_refuses() {
    migration::run(migration::Profile::Finalization22To23);
}
