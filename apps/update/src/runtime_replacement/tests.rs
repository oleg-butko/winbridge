use super::*;
use crate::runtime_preparation::tests::{fixture, stages};
fn planned() -> RuntimeReplacementRecord {
    let preparation = stages(&fixture()).pop().unwrap();
    RuntimeReplacementRecord::planned(RuntimeReplacementIntent {
        schema_version: 1,
        attempt_id: uuid::Uuid::new_v4().to_string(),
        admitted_at_ms: preparation.plan.admitted_at_ms + 1,
        preparation,
    })
    .unwrap()
}
#[test]
fn readonly_diagnostics_do_not_claim_unchanged_runtime_after_cutover() {
    use winbridge_protocol::update::UpdatePhase::*;
    assert_eq!(applied_observation(None), None);
    for phase in [Replacing, ManualIntervention] {
        assert_eq!(applied_observation(Some(phase)), None);
    }
    for phase in [VerifyingLocal, AwaitingController, Committed] {
        assert_eq!(applied_observation(Some(phase)), Some(true));
    }
    for phase in [
        Requested,
        Preparing,
        SourceApplying,
        Building,
        ValidatingCandidate,
        CandidateVerified,
        Draining,
        Quiesced,
        Stopping,
        Aborting,
        Deferred,
        FailedBeforeCutover,
    ] {
        assert_eq!(applied_observation(Some(phase)), Some(false));
    }
}
#[test]
fn replacement_projection_separates_saved_progress_from_live_custody() {
    use winbridge_protocol::update::{
        RuntimeReplacementMove as Move, RuntimeReplacementPhase as Phase,
    };
    let absent = view(None, false, false, false);
    assert_eq!(absent.phase, Phase::NotAdmitted);
    assert!(!absent.physical_custody_held);
    assert_eq!(view(None, false, true, false).phase, Phase::Blocked);
    let r = planned();
    let active = view(Some(&r), true, false, false);
    assert_eq!(active.phase, Phase::Acquiring);
    assert!(!active.physical_custody_held);
    let cold = view(Some(&r), false, false, false);
    assert_eq!(cold.phase, Phase::Indeterminate);
    assert!(cold.reason.unwrap().contains("Автоматического повтора нет"));
    let mut r = directory();
    let n = r.batch.files[0].issue_retire().unwrap();
    r = r.advance_file(0, n).unwrap();
    let issued = view(Some(&r), true, false, true);
    assert_eq!(issued.phase, Phase::MovingFiles);
    assert_eq!(issued.issued_move, Some(Move::Retire));
    assert_eq!(issued.retired_files, 0);
    assert_eq!(issued.installed_files, 0);
    assert!(issued.physical_custody_held);
    let mut receipt = r.batch.files[0].spec.old.clone();
    receipt.path = r.batch.files[0]
        .spec
        .retained_path()
        .to_str()
        .unwrap()
        .into();
    let n = r.batch.files[0].retired(receipt).unwrap();
    r = r.advance_file(0, n).unwrap();
    let n = r.batch.files[0].issue_place().unwrap();
    r = r.advance_file(0, n).unwrap();
    let issued = view(Some(&r), true, false, true);
    assert_eq!(issued.issued_move, Some(Move::Place));
    assert_eq!(issued.retired_files, 1);
    assert_eq!(issued.installed_files, 0);
    let failed = r.indeterminate("Move outcome unknown".into()).unwrap();
    let v = view(Some(&failed), false, true, true);
    assert_eq!(v.phase, Phase::Indeterminate);
    assert!(v.physical_custody_held, "Failure retains original locks");
    assert_eq!(v.reason.as_deref(), Some("Move outcome unknown"));
}
#[test]
fn completed_projection_remains_historical_without_activation_or_custody() {
    use winbridge_protocol::update::RuntimeReplacementPhase as Phase;
    let mut r = directory();
    for i in 0..5 {
        r = moved(r, i);
    }
    for held in [true, false] {
        let v = view(Some(&r), false, false, held);
        assert_eq!(v.phase, Phase::FilesMoved);
        assert_eq!(v.revision, Some(22));
        assert_eq!((v.retired_files, v.installed_files), (5, 5));
        assert_eq!(v.physical_custody_held, held);
        assert_eq!(v.issued_file, None);
        assert_eq!(v.issued_move, None);
        assert_eq!(v.reason, None);
    }
}
fn directory() -> RuntimeReplacementRecord {
    planned()
        .issue_directory()
        .unwrap()
        .directory_created("displaced-directory".into())
        .unwrap()
}
fn moved(mut r: RuntimeReplacementRecord, i: usize) -> RuntimeReplacementRecord {
    fn step(
        old: &RuntimeReplacementRecord,
        next: RuntimeReplacementRecord,
    ) -> RuntimeReplacementRecord {
        old.advance(&next).unwrap();
        next
    }
    let issue = r.batch.files[i].issue_retire().unwrap();
    r = step(&r, r.advance_file(i, issue).unwrap());
    let mut old = r.batch.files[i].spec.old.clone();
    old.path = r.batch.files[i]
        .spec
        .retained_path()
        .to_str()
        .unwrap()
        .into();
    let receipt = r.batch.files[i].retired(old).unwrap();
    r = step(&r, r.advance_file(i, receipt).unwrap());
    let issue = r.batch.files[i].issue_place().unwrap();
    r = step(&r, r.advance_file(i, issue).unwrap());
    let mut new = r.batch.files[i].spec.prepared.clone();
    new.path = r.batch.files[i]
        .spec
        .installed_path()
        .to_str()
        .unwrap()
        .into();
    let receipt = r.batch.files[i].installed(new).unwrap();
    step(&r, r.advance_file(i, receipt).unwrap())
}
#[test]
fn replacement_model_orders_directory_and_twenty_file_transitions_manifest_last() {
    let r = planned();
    let issued = r.issue_directory().unwrap();
    r.advance(&issued).unwrap();
    let mut r = issued
        .directory_created("displaced-directory".into())
        .unwrap();
    issued.advance(&r).unwrap();
    assert_eq!(
        r.intent.retained_directory().file_name().unwrap(),
        "displaced-runtime"
    );
    for i in 0..5 {
        assert_eq!(r.revision, 2 + i as u64 * 4);
        let previous = r.clone();
        r = moved(r, i);
        r.validate().unwrap();
        assert!(
            previous.advance(&r).is_err(),
            "Four steps cannot be one CAS"
        );
    }
    assert_eq!(r.revision, 22);
    assert!(matches!(
        r.state,
        RuntimeReplacementState::FilesMoved { .. }
    ));
    assert!(r.issue_directory().is_err());
    assert!(r
        .indeterminate("Cannot erase completed receipt".into())
        .is_err());
}
#[test]
fn replacement_requires_completed_preparation_and_distinct_attempt_scope() {
    let p = planned();
    let mut intent = p.intent.clone();
    intent.preparation = fixture();
    assert!(RuntimeReplacementRecord::planned(intent).is_err());
    for id in [
        &p.intent.preparation.plan.update_id,
        &p.intent.preparation.plan.attempt_id,
        &p.intent.preparation.plan.owner_id,
        &p.intent.preparation.plan.launch_id,
        &p.intent.preparation.plan.manifest.set_id,
    ] {
        let mut intent = p.intent.clone();
        intent.attempt_id = id.clone();
        assert!(intent.validate().is_err());
    }
    let mut intent = p.intent.clone();
    intent.admitted_at_ms = 0;
    assert!(intent.validate().is_err());
}
#[test]
fn replacement_rejects_alias_directory_early_files_and_manifest() {
    let p = planned();
    let issue = p.batch.files[0].issue_retire().unwrap();
    assert!(p.advance_file(0, issue).is_err());
    let issued = p.issue_directory().unwrap();
    for id in [
        &p.intent.preparation.plan.runtime_identity,
        &p.intent.preparation.plan.parent_identity,
        p.intent.preparation.directory_identity.as_ref().unwrap(),
        &p.batch.files[0].spec.old.file_identity,
        &p.batch.files[0].spec.prepared.file_identity,
        &p.intent.preparation.plan.files[0]
            .source
            .as_ref()
            .unwrap()
            .file_identity,
    ] {
        assert!(issued.directory_created(id.clone()).is_err());
    }
    let d = directory();
    let manifest = d.batch.files[4].issue_retire().unwrap();
    assert!(d.advance_file(4, manifest).is_err());
}
#[test]
fn replacement_unknown_outcome_preserves_issued_prefix_and_reopen_has_no_new_effect() {
    let d = directory();
    let issue = d.batch.files[0].issue_retire().unwrap();
    let issued = d.advance_file(0, issue).unwrap();
    let failed = issued
        .indeterminate("Rename outcome unknown".into())
        .unwrap();
    issued.advance(&failed).unwrap();
    assert_eq!(failed.batch, issued.batch);
    let reopened: RuntimeReplacementRecord =
        serde_json::from_slice(&serde_json::to_vec(&failed).unwrap()).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened, failed);
    assert!(reopened
        .advance_file(0, reopened.batch.files[0].clone())
        .is_err());
    assert!(reopened.indeterminate("second failure".into()).is_err());
    let mut changed = failed.clone();
    if let RuntimeReplacementState::Indeterminate { previous, .. } = &mut changed.state {
        **previous = RuntimeReplacementState::DirectoryIssued {};
    }
    assert!(changed.validate().is_err());
}
#[test]
fn replacement_cas_rejects_changed_preparation_prior_receipts_and_directory() {
    let d = directory();
    let n = d
        .advance_file(0, d.batch.files[0].issue_retire().unwrap())
        .unwrap();
    d.advance(&n).unwrap();
    let mut changed = n.clone();
    changed.intent.preparation.plan.owner_epoch += 1;
    assert!(d.advance(&changed).is_err());
    let mut changed = n.clone();
    changed.state = RuntimeReplacementState::Replacing {
        directory_identity: "other-directory".into(),
    };
    changed.validate().unwrap();
    assert!(d.advance(&changed).is_err());
    let mut changed = n.clone();
    changed.revision += 1;
    assert!(d.advance(&changed).is_err());
}
#[test]
fn replacement_strict_json_and_terminal_uncertainty_reject_nested_or_extra_data() {
    let d = directory();
    let failed = d.indeterminate("Unknown outcome".into()).unwrap();
    let mut changed = failed.clone();
    if let RuntimeReplacementState::Indeterminate { previous, .. } = &mut changed.state {
        **previous = failed.state.clone();
    }
    assert!(changed.validate().is_err());
    for state in [
        serde_json::json!({"phase":"planned","unexpected":true}),
        serde_json::json!({"phase":"directory_issued","unexpected":true}),
    ] {
        assert!(serde_json::from_value::<RuntimeReplacementState>(state).is_err());
    }
    assert!(d.indeterminate("bad\nreason".into()).is_err());
}
