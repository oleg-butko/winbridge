//! Explicit performance-experiment policy probe. No runtime or updater actions.
#[test]
#[ignore = "profile pilot explicitly verifies its compiled safety policy"]
fn experiment_requires_assertions_and_runtime_overflow_checks() {
    assert!(std::hint::black_box(cfg!(debug_assertions)));
    let result =
        std::panic::catch_unwind(|| std::hint::black_box(u64::MAX) + std::hint::black_box(1));
    assert!(
        result.is_err(),
        "Experiment binary disabled runtime overflow checks"
    );
}
