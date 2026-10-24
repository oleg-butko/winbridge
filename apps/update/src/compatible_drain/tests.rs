//! Synthetic decisions are metadata; no fixture creates production authority.
use super::*;
pub(crate) fn checked() -> CandidateSupportRecord {
    crate::candidate_support::tests::stages(&crate::candidate_support::tests::fixture())
        .pop()
        .unwrap()
}
pub(crate) fn blocked_record() -> (CandidateSupportRecord, CompatibleDrainRecord) {
    let s = checked();
    let r = CompatibleDrainRecord::new(&s, blocked(&s).unwrap().unwrap(), 111).unwrap();
    (s, r)
}
#[test]
fn blocked_decision_retains_support_and_cannot_invent_an_admitted_drain() {
    let (s, r) = blocked_record();
    r.validate(&s).unwrap();
    let mut forged = r.clone();
    forged.decision = CompatibleDrainDecision::Admitted {
        drain_attempt_id: uuid::Uuid::new_v4().to_string(),
    };
    assert!(forged.validate(&s).is_err());
    for mutate in 0..5 {
        let mut bad = r.clone();
        match mutate {
            0 => bad.owner_epoch += 1,
            1 => bad.parent_revision += 1,
            2 => bad.support_revision -= 1,
            3 => bad.support_sha256 = "0".repeat(64),
            _ => bad.decided_at_ms = 1,
        };
        assert!(bad.validate(&s).is_err());
    }
    let mut newer = s.clone();
    newer.intent.config_sha256 = "0".repeat(64);
    assert!(r.validate(&newer).is_err());
    let mut bad = r.clone();
    if let CompatibleDrainDecision::Blocked {
        assessment: Some(a),
        ..
    } = &mut bad.decision
    {
        a.blockers.clear();
    }
    assert!(bad.validate(&s).is_err());
}
#[test]
fn pending_support_cannot_be_frozen_as_a_production_decision() {
    let s = crate::candidate_support::tests::fixture();
    assert!(blocked(&s).is_err());
    assert!(CompatibleDrainRecord::new(
        &s,
        CompatibleDrainDecision::Blocked {
            outcome: CandidateSupportOutcome::Indeterminate,
            assessment: None
        },
        111
    )
    .is_err());
}
#[test]
fn future_metadata_may_link_a_drain_but_does_not_enable_missing_current_drivers() {
    let mut s = checked();
    s.intent.descriptor.target_os = "windows".into();
    s.intent.descriptor.target_arch = "x86_64".into();
    s.intent.executor.target_os = "windows".into();
    s.intent.executor.target_arch = "x86_64".into();
    s.intent.drivers.activation = Some(21);
    s.intent.drivers.controller_confirmation = Some(22);
    s.intent.drivers.desktop_update = Some(23);
    let job = s.intent.job_name().unwrap();
    let probe = s.probe.as_mut().unwrap();
    probe.job_name = job.clone();
    probe.cleanup.as_mut().unwrap().job_name = job;
    let result = s.result.as_mut().unwrap();
    let report = result.report.as_mut().unwrap();
    report.descriptor = s.intent.descriptor.clone();
    report.activation_contract = Some(21);
    report.controller_confirmation_contract = Some(22);
    report.desktop_update_contract = Some(23);
    result.assessment = Some(
        winbridge_core::update::assess_updater_compatibility_with_drivers(
            report,
            s.intent.expected_updater_journal_schema,
            &s.intent.drivers,
        ),
    );
    s.validate().unwrap();
    assert!(blocked(&s).unwrap().is_none());
    let r = CompatibleDrainRecord::new(
        &s,
        CompatibleDrainDecision::Admitted {
            drain_attempt_id: uuid::Uuid::new_v4().to_string(),
        },
        111,
    )
    .unwrap();
    r.validate(&s).unwrap();
    assert_ne!(
        s.intent.drivers,
        UpdaterDriverContracts::implemented_windows()
    );
    let mut json = serde_json::to_value(&r).unwrap();
    json["permit"] = true.into();
    assert!(serde_json::from_value::<CompatibleDrainRecord>(json).is_err());
}
