use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use winbridge_core::config::DeliverySettings;
use winbridge_protocol::*;
use winbridge_workspace_sync::*;
fn git(root: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success());
}
fn setup() -> (TempDir, DeliverySettings, SnapshotReady, ImportSubmission) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("git")).unwrap();
    fs::create_dir(root.join("shared")).unwrap();
    fs::create_dir(root.join("local")).unwrap();
    let repo = root.join("git");
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("Привет.txt"), "committed ✓\n").unwrap();
    fs::write(repo.join("build.sh"), "exit 0\n").unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let delivery = DeliverySettings {
        adapter: "shared-folder".into(),
        root: root.join("shared"),
        namespace: "WinBridge".into(),
        peer_root: None,
    };
    let ready = publish(&delivery, &repo, &root.join("state"), "fixture", "HEAD").unwrap();
    let request = ImportSubmission {
        protocol_version: 1,
        request_id: "one".into(),
        project_id: "fixture".into(),
        snapshot_id: ready.snapshot_id.clone(),
        manifest_sha256: ready.manifest_sha256.clone(),
        idempotency_key: "one".into(),
        timeout_ms: 10000,
    };
    (temp, delivery, ready, request)
}

#[test]
fn publication_measurements_distinguish_new_bytes_from_verified_reuse() {
    let (temp, delivery, ready, _) = setup();
    let measure = || {
        publish_measured(
            &delivery,
            &temp.path().join("git"),
            &temp.path().join("state"),
            "fixture",
            "HEAD",
        )
    };
    let repeat = measure().unwrap();
    assert!(repeat.reused_package);
    assert_eq!(repeat.ready, ready);
    assert_eq!(repeat.timings.shared_write_ms, 0.0);
    assert_eq!(repeat.timings.publish_ready_ms, 0.0);
    fs::write(temp.path().join("git/additional.dat"), b"new bytes").unwrap();
    git(&temp.path().join("git"), &["add", "."]);
    git(
        &temp.path().join("git"),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "new measurement fixture",
        ],
    );
    let fresh = measure().unwrap();
    assert!(!fresh.reused_package);
    assert_eq!(fresh.file_count, repeat.file_count + 1);
    assert_eq!(fresh.source_bytes, repeat.source_bytes + 9);
    assert_ne!(fresh.ready.snapshot_id, repeat.ready.snapshot_id);
    let t = &fresh.timings;
    assert!(
        t.total_ms
            >= t.enumerate_ms
                + t.capture_pack_ms
                + t.local_verify_ms
                + t.shared_write_ms
                + t.shared_readback_ms
                + t.publish_ready_ms
    );
    let payload = package_path(&delivery, "fixture", &fresh.ready.snapshot_id)
        .unwrap()
        .join("payload.zip");
    fs::write(&payload, b"corrupt").unwrap();
    assert!(measure().is_err());
    assert_eq!(fs::read(payload).unwrap(), b"corrupt");
}

#[test]
fn git_batch_captures_empty_binary_and_repeated_objects_without_framing_leaks() {
    let (temp, delivery, _, _) = setup();
    let repo = temp.path().join("git");
    let content = b"\0\nblob header-like content\xff\0\n";
    fs::write(repo.join("empty.dat"), []).unwrap();
    fs::write(repo.join("binary.dat"), content).unwrap();
    fs::write(repo.join("duplicate.dat"), content).unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "binary batch fixture",
        ],
    );
    let ready = publish(
        &delivery,
        &repo,
        &temp.path().join("state"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    let request = ImportSubmission {
        protocol_version: 1,
        request_id: "binary".into(),
        project_id: "fixture".into(),
        snapshot_id: ready.snapshot_id.clone(),
        manifest_sha256: ready.manifest_sha256,
        idempotency_key: "binary".into(),
        timeout_ms: 10000,
    };
    import(
        &delivery,
        &temp.path().join("local"),
        &request,
        "binary",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let root = generation_path(&temp.path().join("local"), &request.snapshot_id).unwrap();
    assert!(fs::read(root.join("empty.dat")).unwrap().is_empty());
    assert_eq!(fs::read(root.join("binary.dat")).unwrap(), content);
    assert_eq!(fs::read(root.join("duplicate.dat")).unwrap(), content);
}
#[test]
fn discovery_pages_metadata_without_verifying_payload_or_creating_directories() {
    let (temp, delivery, ready, _) = setup();
    let parent = package_path(&delivery, "fixture", &ready.snapshot_id).unwrap();
    // Discovery must not claim that a metadata-valid package has valid payload bytes.
    fs::write(parent.join("payload.zip"), "corrupt").unwrap();
    let first = list_snapshots(&delivery, "fixture", 0).unwrap();
    assert_eq!(first.snapshots.len(), 1);
    assert!(first.snapshots[0].metadata_error.is_none());
    assert_eq!(first.snapshots[0].total_files, Some(2));
    let parent = parent.parent().unwrap();
    for index in 0..105 {
        fs::create_dir(parent.join(format!("candidate-{index:03}"))).unwrap();
    }
    fs::create_dir(parent.join(".unfinished.part")).unwrap();
    fs::write(parent.join("plain-file"), "ignored").unwrap();
    let first = list_snapshots(&delivery, "fixture", 0).unwrap();
    assert_eq!(first.snapshots.len(), 100);
    assert!(first.snapshots.iter().all(|s| s.metadata_error.is_some()));
    assert_eq!(first.next_offset, Some(100));
    let next = list_snapshots(&delivery, "fixture", 100).unwrap();
    assert_eq!(next.snapshots.len(), 6);
    assert_eq!(next.next_offset, None);
    assert_eq!(
        next.snapshots.last().unwrap().snapshot_id,
        ready.snapshot_id
    );
    assert!(list_snapshots(&delivery, "fixture", u64::MAX).is_err());
    assert!(list_snapshots(&delivery, "../outside", 0).is_err());
    assert!(list_snapshots(&delivery, "empty-project", 0)
        .unwrap()
        .snapshots
        .is_empty());
    assert!(!parent.parent().unwrap().join("empty-project").exists());
    fs::rename(&delivery.root, temp.path().join("shared-offline")).unwrap();
    assert!(list_snapshots(&delivery, "fixture", 0).is_err());
}

#[cfg(unix)]
#[test]
fn discovery_refuses_linked_parent_and_reports_linked_candidate() {
    let (temp, delivery, ready, _) = setup();
    let package = package_path(&delivery, "fixture", &ready.snapshot_id).unwrap();
    std::os::unix::fs::symlink(&package, package.parent().unwrap().join("linked-package")).unwrap();
    let page = list_snapshots(&delivery, "fixture", 0).unwrap();
    let linked = page
        .snapshots
        .iter()
        .find(|s| s.snapshot_id == "linked-package")
        .unwrap();
    assert!(linked
        .metadata_error
        .as_ref()
        .unwrap()
        .contains("Linked/reparse"));
    std::os::unix::fs::symlink(
        package.parent().unwrap(),
        package
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("linked-project"),
    )
    .unwrap();
    assert!(list_snapshots(&delivery, "linked-project", 0)
        .unwrap_err()
        .contains("Linked/reparse"));
    assert!(temp.path().join("shared").exists());
}

#[test]
fn exact_commit_ignores_dirty_tree_and_repeat_is_immutable() {
    let (temp, delivery, ready, request) = setup();
    let root = temp.path();
    fs::write(root.join("git/Привет.txt"), "dirty").unwrap();
    fs::write(root.join("git/not-committed"), "excluded").unwrap();
    let repeat = publish(
        &delivery,
        &root.join("git"),
        &root.join("state"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    assert_eq!(ready, repeat);
    import(
        &delivery,
        &root.join("local"),
        &request,
        "operation",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let generation = generation_path(&root.join("local"), &ready.snapshot_id).unwrap();
    assert_eq!(
        fs::read_to_string(generation.join("Привет.txt")).unwrap(),
        "committed ✓\n"
    );
    assert!(!generation.join("not-committed").exists());
    fs::rename(root.join("shared"), root.join("shared-offline")).unwrap();
    import(
        &delivery,
        &root.join("local"),
        &request,
        "repeat",
        |_, _, _| Ok(()),
    )
    .unwrap(); // Mount not needed for verified local reuse.
    fs::write(generation.join("Привет.txt"), "tampered").unwrap();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "tamper",
        |_, _, _| Ok(())
    )
    .is_err());
}
#[test]
fn incomplete_corrupt_or_changed_manifest_never_becomes_buildable() {
    let (temp, delivery, ready, request) = setup();
    let root = temp.path();
    let package = package_path(&delivery, "fixture", &ready.snapshot_id).unwrap();
    let marker = fs::read(package.join("ready.json")).unwrap();
    fs::remove_file(package.join("ready.json")).unwrap();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "missing",
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(!generation_path(&root.join("local"), &ready.snapshot_id)
        .unwrap()
        .exists());
    fs::write(package.join("ready.json"), marker).unwrap();
    fs::write(package.join("payload.zip"), b"incomplete ZIP").unwrap();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "corrupt",
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(!generation_path(&root.join("local"), &ready.snapshot_id)
        .unwrap()
        .exists());
    let mut manifest = fs::read(package.join("manifest.json")).unwrap();
    manifest.push(b' ');
    fs::write(package.join("manifest.json"), manifest).unwrap();
    assert!(metadata(&delivery, "fixture", &ready.snapshot_id, None).is_err());
}
#[test]
fn cancellation_and_mount_loss_leave_only_owned_staging() {
    let (temp, delivery, ready, request) = setup();
    let root = temp.path();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "cancel",
        |phase, _, _| if phase == "extract" {
            Err("cancel requested".into())
        } else {
            Ok(())
        }
    )
    .is_err());
    assert!(!generation_path(&root.join("local"), &ready.snapshot_id)
        .unwrap()
        .exists());
    assert!(root.join("local/.winbridge/imports/cancel").exists());
    fs::rename(root.join("shared"), root.join("offline")).unwrap();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "offline",
        |_, _, _| Ok(())
    )
    .is_err());
}
#[test]
fn archive_extra_paths_links_and_content_mismatch_are_rejected_after_valid_payload_hash() {
    for fault in ["extra", "symlink", "content"] {
        let (temp, delivery, ready, mut request) = setup();
        let root = temp.path();
        let package = package_path(&delivery, "fixture", &ready.snapshot_id).unwrap();
        let (mut manifest, _, mut marker) =
            metadata(&delivery, "fixture", &ready.snapshot_id, None).unwrap();
        let file = fs::File::create(package.join("payload.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        use std::io::Write;
        for record in &manifest.files {
            if fault == "symlink" && record.path == "build.sh" {
                zip.add_symlink(&record.path, "../../outside", options)
                    .unwrap();
                continue;
            }
            zip.start_file(&record.path, options).unwrap();
            zip.write_all(if record.path == "build.sh" {
                b"exit 0\n"
            } else if fault == "content" {
                b"wrong bytes!\n"
            } else {
                "committed ✓\n".as_bytes()
            })
            .unwrap();
        }
        if fault == "extra" {
            zip.start_file("../escape", options).unwrap();
            zip.write_all(b"escaped").unwrap();
        }
        zip.finish().unwrap();
        let bytes = fs::read(package.join("payload.zip")).unwrap();
        manifest.payload_bytes = bytes.len() as u64;
        manifest.payload_sha256 = sha256(&bytes);
        let bytes = serde_json::to_vec(&manifest).unwrap();
        marker.manifest_sha256 = sha256(&bytes);
        marker.payload_sha256 = manifest.payload_sha256;
        marker.payload_bytes = manifest.payload_bytes;
        request.manifest_sha256 = marker.manifest_sha256.clone();
        fs::write(package.join("manifest.json"), bytes).unwrap();
        fs::write(
            package.join("ready.json"),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert!(
            import(
                &delivery,
                &root.join("local"),
                &request,
                "fault",
                |_, _, _| Ok(())
            )
            .is_err(),
            "{fault}"
        );
        assert!(!root.join("escape").exists());
    }
}
#[cfg(unix)]
#[test]
fn source_links_and_destination_symlinks_cannot_escape_scope() {
    use std::os::unix::fs::symlink;
    let (temp, delivery, _, request) = setup();
    let root = temp.path();
    fs::create_dir(root.join("outside")).unwrap();
    symlink(root.join("outside"), root.join("local/.winbridge")).unwrap();
    assert!(import(
        &delivery,
        &root.join("local"),
        &request,
        "linked",
        |_, _, _| Ok(())
    )
    .is_err());
    assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 0);
    symlink("../private", root.join("git/link")).unwrap();
    git(&root.join("git"), &["add", "link"]);
    git(
        &root.join("git"),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "link",
        ],
    );
    assert!(publish(
        &delivery,
        &root.join("git"),
        &root.join("state"),
        "fixture",
        "HEAD"
    )
    .is_err());
}
#[test]
fn unexpected_local_files_are_not_verified_and_namespace_paths_are_bounded() {
    let (temp, delivery, ready, request) = setup();
    let root = temp.path();
    import(&delivery, &root.join("local"), &request, "ok", |_, _, _| {
        Ok(())
    })
    .unwrap();
    fs::write(
        generation_path(&root.join("local"), &ready.snapshot_id)
            .unwrap()
            .join("injected-config.json"),
        "bad",
    )
    .unwrap();
    assert!(verified_generation(
        &root.join("local"),
        "fixture",
        &ready.snapshot_id,
        None,
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(package_path(&delivery, "../outside", "id").is_err());
    assert!(package_path(&delivery, "fixture", "CON").is_err());
}
