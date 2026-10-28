//! Actual retained20 LocalVerified/service history ->21; consistent copy only.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual20 local-service reference; copy-only21 migration, no installed update"]
fn actual_twenty_local_service_history_survives_twenty_one_and_old_twenty_refuses() {
    migration::run(migration::Profile::Controller20To21);
}
