//! Synthetic data, not physical custody or admission evidence.
use super::*;
pub(crate) fn fixture() -> RuntimePreparationRecord {
    let runtime = std::env::temp_dir().join("synthetic-runtime-preparation");
    let update_id = uuid::Uuid::new_v4().to_string();
    let artifacts = FILE_ORDER[..4]
        .iter()
        .zip([
            winbridge_protocol::ArtifactRole::Worker,
            winbridge_protocol::ArtifactRole::Fixture,
            winbridge_protocol::ArtifactRole::Desktop,
            winbridge_protocol::ArtifactRole::Updater,
        ])
        .map(|(file, role)| {
            let mut descriptor = winbridge_protocol::artifact_descriptor(role);
            descriptor.build.source_dirty = false;
            descriptor.build.source_commit = Some("a".repeat(40));
            descriptor.build.source_manifest_sha256 = Some("b".repeat(64));
            crate::installed_set::InstalledArtifact {
                name: file.name().into(),
                bytes: 1024,
                sha256: "c".repeat(64),
                descriptor,
            }
        })
        .collect();
    let manifest = InstalledSetManifest {
        schema_version: 1,
        kind: "winbridge_installed_set".into(),
        set_id: uuid::Uuid::new_v4().to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        scope_sha256: "d".repeat(64),
        artifacts,
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let files = FILE_ORDER
        .iter()
        .map(|file| {
            let query = *file != RuntimeFile::Manifest;
            let source = query.then(|| ImageEvidence {
                path: runtime
                    .join("update")
                    .join(&update_id)
                    .join("candidate")
                    .join("payload")
                    .join(file.name())
                    .to_str()
                    .unwrap()
                    .into(),
                bytes: 1024,
                sha256: "c".repeat(64),
                file_identity: format!("candidate-{}", file.name()),
                query_supported: true,
            });
            PreparationFile {
                file: *file,
                old: ImageEvidence {
                    path: runtime.join(file.name()).to_str().unwrap().into(),
                    bytes: 1024,
                    sha256: "e".repeat(64),
                    file_identity: format!("old-{}", file.name()),
                    query_supported: query,
                },
                source,
                destination: runtime
                    .join("update")
                    .join(&update_id)
                    .join("prepared-runtime")
                    .join(file.name())
                    .to_str()
                    .unwrap()
                    .into(),
                bytes: if query { 1024 } else { bytes.len() as u64 },
                sha256: if query {
                    "c".repeat(64)
                } else {
                    crate::sha256(&bytes)
                },
            }
        })
        .collect();
    RuntimePreparationRecord::planned(RuntimePreparationPlan {
        schema_version: 1,
        update_id,
        attempt_id: uuid::Uuid::new_v4().to_string(),
        parent_revision: 16,
        owner_id: uuid::Uuid::new_v4().to_string(),
        owner_epoch: 3,
        launch_id: uuid::Uuid::new_v4().to_string(),
        config_sha256: "f".repeat(64),
        runtime_directory: runtime.to_str().unwrap().into(),
        runtime_identity: "runtime-directory".into(),
        parent_identity: "transaction-parent".into(),
        admitted_at_ms: 100,
        available_bytes_before: 8 * 1024 * 1024 * 1024,
        proofs: PreparationProofs {
            request_sha256: "1".repeat(64),
            supervisor_spec_sha256: "2".repeat(64),
            baseline_sha256: "3".repeat(64),
            preimages_sha256: "4".repeat(64),
            artifacts_sha256: "5".repeat(64),
            health_sha256: "6".repeat(64),
            shutdown_sha256: "7".repeat(64),
            backup_sha256: "8".repeat(64),
        },
        manifest,
        files,
    })
    .unwrap()
}
pub(crate) fn receipt(r: &RuntimePreparationRecord, index: usize) -> ImageEvidence {
    let f = &r.plan.files[index];
    ImageEvidence {
        path: f.destination.clone(),
        bytes: f.bytes,
        sha256: f.sha256.clone(),
        file_identity: format!("prepared-file-{index}"),
        query_supported: f.file != RuntimeFile::Manifest,
    }
}
pub(crate) fn stages(r: &RuntimePreparationRecord) -> Vec<RuntimePreparationRecord> {
    let mut stages = vec![r.issue_directory().unwrap()];
    stages.push(
        stages
            .last()
            .unwrap()
            .directory_created("prepared-directory".into())
            .unwrap(),
    );
    for index in 0..5 {
        stages.push(stages.last().unwrap().issue_file(index).unwrap());
        stages.push(
            stages
                .last()
                .unwrap()
                .file_created(index, receipt(r, index))
                .unwrap(),
        );
    }
    stages
}
#[test]
fn independent_preparation_requires_intent_before_directory_and_every_copy() {
    let initial = fixture();
    assert!(initial.directory_created("dir".into()).is_err());
    assert!(initial.issue_file(0).is_err());
    assert!(initial.exchange_batch().is_err());
    let mut previous = initial;
    for next in stages(&previous) {
        previous.validate_step(&next).unwrap();
        assert!(next.validate_step(&previous).is_err());
        previous = next;
    }
    assert_eq!(previous.revision, 12);
    assert_eq!(previous.phase, PreparationPhase::Prepared);
    let batch = previous.exchange_batch().unwrap();
    batch.validate().unwrap();
    assert_eq!(batch.next_file().unwrap(), Some(0));
    assert!(previous.indeterminate("late unknown".into()).is_err());
}
#[test]
fn preparation_receipts_reject_changed_copied_aliases_and_skipped_files() {
    let initial = fixture();
    let ready = initial
        .issue_directory()
        .unwrap()
        .directory_created("dir".into())
        .unwrap();
    assert!(ready.issue_file(1).is_err());
    let issued = ready.issue_file(0).unwrap();
    assert!(ready.file_created(0, receipt(&ready, 0)).is_err());
    for kind in 0..7 {
        let mut file = receipt(&issued, 0);
        match kind {
            0 => file.bytes += 1,
            1 => file.sha256 = "0".repeat(64),
            2 => file.path.push_str(".other"),
            3 => file.file_identity = issued.plan.files[0].old.file_identity.clone(),
            4 => {
                file.file_identity = issued.plan.files[3]
                    .source
                    .as_ref()
                    .unwrap()
                    .file_identity
                    .clone()
            }
            5 => file.file_identity = "dir".into(),
            _ => file.query_supported = false,
        }
        assert!(issued.file_created(0, file).is_err(), "mutation {kind}");
    }
    let copied = issued.file_created(0, receipt(&issued, 0)).unwrap();
    assert!(copied.file_created(0, receipt(&copied, 0)).is_err());
    assert!(copied.issue_file(2).is_err());
}
#[test]
fn preparation_uncertainty_and_json_reopen_preserve_prefix_without_new_issue() {
    let initial = fixture();
    for r in std::iter::once(initial.clone()).chain(stages(&initial).into_iter().take(11)) {
        let unknown = r
            .indeterminate("Unknown copy outcome; inspect only".into())
            .unwrap();
        let restored: RuntimePreparationRecord =
            serde_json::from_slice(&serde_json::to_vec(&unknown).unwrap()).unwrap();
        restored.validate().unwrap();
        r.validate_step(&restored).unwrap();
        assert_eq!(restored.files, r.files);
        assert_eq!(restored.directory_identity, r.directory_identity);
        assert!(restored.issue_directory().is_err());
        assert!(restored.issue_file(0).is_err());
        assert!(restored.indeterminate("retry".into()).is_err());
        assert!(restored.exchange_batch().is_err());
    }
    let issued = stages(&initial)[2].clone();
    let restored: RuntimePreparationRecord =
        serde_json::from_slice(&serde_json::to_vec(&issued).unwrap()).unwrap();
    assert!(restored.issue_file(0).is_err());
}
#[test]
fn preparation_plan_rejects_wrong_scope_roles_manifest_bytes_and_space() {
    let r = fixture();
    for kind in 0..12 {
        let mut p = r.plan.clone();
        match kind {
            0 => p.files.swap(0, 1),
            1 => p.files[4].sha256 = "0".repeat(64),
            2 => p.files[4].source = p.files[0].source.clone(),
            3 => p.files[2].source = None,
            4 => p.files[0].destination.push_str(".other"),
            5 => p.files[1].old.file_identity = p.files[0].old.file_identity.clone(),
            6 => p.files[3].source.as_mut().unwrap().path.push_str(".other"),
            7 => p.available_bytes_before = 6 * 1024 * 1024 * 1024,
            8 => p.proofs.backup_sha256 = "not-a-hash".into(),
            9 => p.runtime_identity = p.parent_identity.clone(),
            10 => p.owner_epoch = 0,
            _ => p.update_id = "../escape".into(),
        }
        assert!(p.validate().is_err(), "mutation {kind}");
    }
    let mut p = r.plan;
    p.available_bytes_before = 6 * 1024 * 1024 * 1024 + p.required_bytes().unwrap();
    p.validate().unwrap();
}
#[test]
fn preparation_causal_step_rejects_mutated_plan_prefix_and_unknown_json() {
    let r = fixture();
    let next = r.issue_directory().unwrap();
    let mut changed = next.clone();
    changed.plan.proofs.backup_sha256 = "a".repeat(64);
    changed.validate().unwrap();
    assert!(r.validate_step(&changed).is_err());
    let mut changed = next;
    changed.revision += 1;
    assert!(changed.validate().is_err());
    assert!(
        serde_json::from_str::<PreparationFileState>(r#"{"phase":"pending","extra":true}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<PreparationFileState>(r#"{"phase":"issued","extra":true}"#).is_err()
    );
    let mut changed = stages(&r)[4].clone();
    changed.files[0] = PreparationFileState::Pending {};
    assert!(changed.validate().is_err());
}
#[test]
fn preparation_projection_separates_saved_progress_from_live_execution() {
    use winbridge_protocol::update::RuntimePreparationPhase as Phase;
    assert_eq!(view(None, false, false).phase, Phase::NotAdmitted);
    assert_eq!(view(None, false, true).phase, Phase::Blocked);
    let original = fixture();
    let issued = stages(&original)[2].clone();
    let active = view(Some(&issued), true, false);
    assert_eq!(active.phase, Phase::PreparingFiles);
    assert!(active.issued_file.is_some());
    let historical = view(Some(&issued), false, false);
    assert_eq!(historical.phase, Phase::Indeterminate);
    assert_eq!(historical.attempt_id, active.attempt_id);
    assert!(historical.reason.is_some());
    let ready = stages(&original).pop().unwrap();
    let historical = view(Some(&ready), false, false);
    assert_eq!(historical.phase, Phase::Prepared);
    assert_eq!(historical.prepared_files, 5);
}
