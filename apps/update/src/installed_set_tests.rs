use super::*;
use std::fs;
fn fixture() -> (tempfile::TempDir, InstalledSetManifest) {
    let root = tempfile::tempdir().unwrap();
    let mut artifacts = vec![];
    for (name, role) in FILES {
        let mut bytes = b"known installed metadata fixture; never executed".to_vec();
        bytes.extend_from_slice(winbridge_protocol::ARTIFACT_QUERY_MARKER);
        bytes.extend_from_slice(name.as_bytes());
        fs::write(root.path().join(name), &bytes).unwrap();
        let mut descriptor = winbridge_protocol::artifact_descriptor(role);
        descriptor.build.source_dirty = false;
        descriptor.build.source_commit = Some("a".repeat(40));
        descriptor.build.source_manifest_sha256 = Some("b".repeat(64));
        artifacts.push(InstalledArtifact {
            name: name.into(),
            bytes: bytes.len() as u64,
            sha256: crate::sha256(&bytes),
            descriptor,
        });
    }
    let manifest = InstalledSetManifest {
        schema_version: 1,
        kind: "winbridge_installed_set".into(),
        set_id: uuid::Uuid::new_v4().to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        scope_sha256: "c".repeat(64),
        artifacts,
    };
    fs::write(
        root.path().join(MANIFEST_NAME),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    (root, manifest)
}

fn candidate(
    old: &InstalledSetManifest,
) -> (
    winbridge_protocol::update::UpdateSourcePin,
    crate::candidate_artifacts::CandidateArtifactInspection,
) {
    let target = winbridge_protocol::update::UpdateSourcePin {
        archive_name: "target.zip".into(),
        archive_sha256: "e".repeat(64),
        manifest_sha256: "f".repeat(64),
        source_commit: "c".repeat(40),
    };
    let artifacts = old
        .artifacts
        .iter()
        .map(|a| {
            let mut descriptor = a.descriptor.clone();
            descriptor.build.source_commit = Some(target.source_commit.clone());
            descriptor.build.source_manifest_sha256 = Some(target.manifest_sha256.clone());
            crate::candidate_artifacts::CandidateArtifact {
                name: a.name.clone(),
                descriptor,
                image: ImageEvidence {
                    path: format!("candidate/{}", a.name),
                    bytes: a.bytes,
                    sha256: "d".repeat(64),
                    file_identity: format!("candidate-{}", a.name),
                    query_supported: true,
                },
                query_process: winbridge_platform::infrastructure::stamp(std::process::id())
                    .unwrap(),
            }
        })
        .collect();
    let mut candidate = crate::candidate_artifacts::CandidateArtifactInspection {
        schema_version: 1,
        kind: "candidate_artifact_inspection".into(),
        read_only: true,
        runtime_applied: false,
        candidate_verified: false,
        report: crate::build_report::ReportEvidence {
            schema_version: 1,
            update_id: uuid::Uuid::new_v4().to_string(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
            build_spec_sha256: "a".repeat(64),
            candidate_directory_identity: "candidate-dir".into(),
            file_identity: "report-file".into(),
            bytes: 100,
            sha256: "b".repeat(64),
        },
        payload_directory_identity: "payload-dir".into(),
        artifacts,
        set_sha256: String::new(),
    };
    candidate.set_sha256 = crate::candidate_artifacts::fingerprint(&candidate).unwrap();
    (target, candidate)
}
#[test]
fn replacement_manifest_plan_preserves_installation_scope_and_exact_target_without_publication() {
    let (root, old) = fixture();
    let before = fs::read(root.path().join(MANIFEST_NAME)).unwrap();
    let (target, candidate) = candidate(&old);
    let id = uuid::Uuid::new_v4().to_string();
    let new = plan_replacement_manifest(&old, &target, &candidate, &id).unwrap();
    assert_eq!(new.installation_id, old.installation_id);
    assert_eq!(new.scope_sha256, old.scope_sha256);
    assert_eq!(new.set_id, id);
    for (a, expected) in new.artifacts.iter().zip(&candidate.artifacts) {
        assert_eq!(a.sha256, expected.image.sha256);
        assert_eq!(a.descriptor, expected.descriptor);
    }
    assert_eq!(fs::read(root.path().join(MANIFEST_NAME)).unwrap(), before);
    assert!(plan_replacement_manifest(&old, &target, &candidate, &old.set_id).is_err());
    let mut aliased = old.clone();
    aliased.set_id = uuid::Uuid::parse_str(&old.set_id)
        .unwrap()
        .urn()
        .to_string();
    assert!(plan_replacement_manifest(&aliased, &target, &candidate, &old.set_id).is_err());
}
#[test]
fn replacement_manifest_plan_rejects_mixed_roles_generations_aliases_and_forged_observations() {
    let (_root, old) = fixture();
    let (target, candidate) = candidate(&old);
    let id = uuid::Uuid::new_v4().to_string();
    for kind in 0..7 {
        let mut c = candidate.clone();
        match kind {
            0 => c.artifacts.swap(0, 1),
            1 => c.artifacts[1].descriptor.build.source_commit = Some("a".repeat(40)),
            2 => c.artifacts[2].descriptor.build.source_dirty = true,
            3 => c.artifacts[3].image.file_identity = c.artifacts[0].image.file_identity.clone(),
            4 => c.artifacts[0].image.query_supported = false,
            5 => c.runtime_applied = true,
            _ => c.read_only = false,
        }
        c.set_sha256 = crate::candidate_artifacts::fingerprint(&c).unwrap();
        assert!(plan_replacement_manifest(&old, &target, &c, &id).is_err());
    }
    let mut c = candidate;
    c.set_sha256 = "0".repeat(64);
    assert!(plan_replacement_manifest(&old, &target, &c, &id).is_err());
}
#[test]
fn known_installed_set_pins_exact_files_without_execution_or_adoption() {
    let (root, m) = fixture();
    let bytes = fs::read(root.path().join(MANIFEST_NAME)).unwrap();
    let pin = pin_installed_set(root.path(), &m.installation_id, &m.scope_sha256).unwrap();
    assert_eq!(pin.observation().manifest, m);
    assert_eq!(pin.observation().images.len(), 4);
    assert!(pin.observation().read_only);
    pin.revalidate().unwrap();
    assert!(pin
        .bind_running_worker()
        .unwrap_err()
        .contains("Loaded worker path/build"));
    assert_eq!(fs::read(root.path().join(MANIFEST_NAME)).unwrap(), bytes);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 5);
}
#[test]
fn missing_or_foreign_manifest_never_adopts_present_binaries() {
    let (root, m) = fixture();
    fs::remove_file(root.path().join(MANIFEST_NAME)).unwrap();
    assert!(
        pin_installed_set(root.path(), &m.installation_id, &m.scope_sha256)
            .err()
            .unwrap()
            .contains("Bootstrap required")
    );
    fs::write(
        root.path().join(MANIFEST_NAME),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    assert!(pin_installed_set(
        root.path(),
        &uuid::Uuid::new_v4().to_string(),
        &m.scope_sha256
    )
    .is_err());
    assert!(pin_installed_set(root.path(), &m.installation_id, &"d".repeat(64)).is_err());
}
#[test]
fn mixed_dirty_duplicate_or_changed_runtime_is_rejected_against_known_manifest() {
    for mode in ["duplicate", "dirty", "role", "source", "file", "hard-link"] {
        let (root, mut m) = fixture();
        match mode {
            "duplicate" => m.artifacts[1] = m.artifacts[0].clone(),
            "dirty" => m.artifacts[0].descriptor.build.source_dirty = true,
            "role" => m.artifacts[0].descriptor.role = ArtifactRole::Desktop,
            "source" => {
                m.artifacts[1].descriptor.build.source_manifest_sha256 = Some("f".repeat(64))
            }
            "file" => {
                let path = root.path().join(FILES[0].0);
                let mut b = fs::read(&path).unwrap();
                b[0] = b'X';
                fs::write(path, b).unwrap();
            }
            "hard-link" => {
                fs::hard_link(root.path().join(FILES[0].0), root.path().join("alias.exe")).unwrap()
            }
            _ => unreachable!(),
        }
        fs::write(
            root.path().join(MANIFEST_NAME),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        assert!(
            pin_installed_set(root.path(), &m.installation_id, &m.scope_sha256).is_err(),
            "{mode}"
        );
    }
}
#[test]
fn pinned_installed_files_reject_changes_or_detect_them_before_use() {
    let (root, m) = fixture();
    let pin = pin_installed_set(root.path(), &m.installation_id, &m.scope_sha256).unwrap();
    let path = root.path().join(FILES[0].0);
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] = b'X';
    let changed = fs::write(&path, bytes);
    if cfg!(windows) {
        assert!(changed.is_err());
        pin.revalidate().unwrap();
    } else {
        changed.unwrap();
        assert!(pin.revalidate().is_err());
    }
}
