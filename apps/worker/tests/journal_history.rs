//! Fast SQL preservation check shared with the native Main25 gate.
#[path = "support/journal_history.rs"]
mod journal_history;

#[test]
fn local_service_history_hash_allows_only_the_exact_policy_owner_epoch() {
    journal_history::assert_reconciliation_contract();
}
