//! Actual retained24 Keeper with later owner ->25, consistent-copy only.
#![cfg(windows)]
#[path = "support/retained_activation_migration.rs"]
mod migration;
#[test]
#[ignore = "Pinned actual Main31 schema24 reference; copy-only25 migration, never install"]
fn actual_twenty_four_keeper_history_survives_twenty_five_without_bootstrap_adoption() {
    migration::run(migration::Profile::Bootstrap24To25);
}
