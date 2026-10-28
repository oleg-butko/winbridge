//! Actual retained21 controller/local7 history ->22; consistent copy only.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual21 controller reference; copy-only22 migration, no installed update"]
fn actual_twenty_one_controller_history_survives_twenty_two_and_old_twenty_one_refuses() {
    migration::run(migration::Profile::Lifetime21To22);
}
