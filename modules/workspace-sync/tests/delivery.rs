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
