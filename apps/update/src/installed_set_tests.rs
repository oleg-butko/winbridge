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
