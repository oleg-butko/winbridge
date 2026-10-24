//! Synthetic receipts test data binding, never physical activation or OS authority.
use super::*;
use crate::runtime_replacement::tests::{completed, planned};

#[test]
fn activation_source_names_installed_files_not_candidate_or_displaced_paths() {
    let record = completed();
    let source = ActivationSource::from_files_moved(&record).unwrap();
    source.validate(&record).unwrap();
    let plan = &record.intent.preparation.plan;
    assert_eq!(source.replacement_revision, 22);
    assert_eq!(source.parent_revision, plan.parent_revision + 1);
    assert_eq!(source.owner_epoch, plan.owner_epoch);
    assert_eq!(source.set_id, plan.manifest.set_id);
    assert_eq!(source.worker.path, plan.files[0].old.path);
    assert_eq!(source.manifest.path, plan.files[4].old.path);
    assert_ne!(source.worker.file_identity, plan.files[0].old.file_identity);
    assert_ne!(source.worker.path, plan.files[0].destination);
    assert_ne!(
        source.worker.path,
        plan.files[0].source.as_ref().unwrap().path
    );
    assert_eq!(
        source.worker_descriptor,
        plan.manifest.artifacts[0].descriptor
    );
}
#[test]
fn partial_or_indeterminate_replacement_never_becomes_activation_source() {
    let mut record = planned();
    assert!(ActivationSource::from_files_moved(&record).is_err());
    record = record.issue_directory().unwrap();
    assert!(ActivationSource::from_files_moved(&record).is_err());
    let uncertain = record
        .indeterminate("Directory creation outcome unknown".into())
        .unwrap();
    assert!(ActivationSource::from_files_moved(&uncertain).is_err());
    record = record
        .directory_created("retained-directory".into())
        .unwrap();
    for index in 0..5 {
        assert!(ActivationSource::from_files_moved(&record).is_err());
        record = crate::runtime_replacement::tests::moved(record, index);
    }
    assert!(ActivationSource::from_files_moved(&record).is_ok());
}
#[test]
fn activation_binding_rejects_changed_owner_generation_hash_or_image() {
    let record = completed();
    let source = ActivationSource::from_files_moved(&record).unwrap();
    for index in 0..8 {
        let mut changed = source.clone();
        match index {
            0 => changed.owner_epoch += 1,
            1 => changed.parent_revision += 1,
            2 => changed.set_id = uuid::Uuid::new_v4().to_string(),
            3 => changed.replacement_sha256 = "0".repeat(64),
            4 => changed.worker.file_identity = "foreign-image".into(),
            5 => changed.manifest.sha256 = "0".repeat(64),
            6 => changed.worker_descriptor.build.source_commit = Some("0".repeat(40)),
            _ => changed.shutdown_sha256 = "0".repeat(64),
        }
        assert!(changed.validate(&record).is_err());
    }
    let mut newer = record.clone();
    newer.intent.preparation.plan.owner_epoch += 1;
    newer.validate().unwrap();
    assert!(source.validate(&newer).is_err());
}
#[test]
fn activation_digest_covers_retained_files_and_not_only_the_new_worker() {
    let record = completed();
    let source = ActivationSource::from_files_moved(&record).unwrap();
    let mut changed = record.clone();
    changed.intent.preparation.plan.files[1].old.sha256 = "a".repeat(64);
    changed.batch.files[1].spec.old.sha256 = "a".repeat(64);
    let FileExchangeState::Installed { retained, .. } = &mut changed.batch.files[1].state else {
        unreachable!()
    };
    retained.sha256 = "a".repeat(64);
    changed.validate().unwrap();
    let other = ActivationSource::from_files_moved(&changed).unwrap();
    assert_eq!(other.worker, source.worker);
    assert_eq!(other.manifest, source.manifest);
    assert_ne!(other.replacement_sha256, source.replacement_sha256);
    assert!(source.validate(&changed).is_err());
    // The flattened value does not duplicate the entire preparation/history.
    let json = serde_json::to_value(&source).unwrap();
    assert!(json.get("preparation").is_none() && json.get("batch").is_none());
    for field in ["permit", "canSpawn", "physicalCustodyHeld"] {
        let mut forged = json.clone();
        forged[field] = true.into();
        assert!(serde_json::from_value::<ActivationSource>(forged).is_err());
    }
}
#[test]
fn exhausted_root_revision_has_no_activation_source() {
    let mut record = completed();
    record.intent.preparation.plan.parent_revision = MAX_UPDATE_COUNTER;
    record.validate().unwrap();
    assert!(ActivationSource::from_files_moved(&record).is_err());
}
