use std::{fs, path::Path, process::Command, time::Instant};
use tempfile::TempDir;
use winbridge_core::config::DeliverySettings;
use winbridge_protocol::*;
use winbridge_workspace_sync::*;

fn git(root: &Path, args: &[&str]) {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn commit(root: &Path) {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "-qm",
            "delta fixture",
        ],
    );
}
fn request(ready: &SnapshotReady, key: &str) -> ImportSubmission {
    ImportSubmission {
        protocol_version: 1,
        request_id: key.into(),
        project_id: "fixture".into(),
        snapshot_id: ready.snapshot_id.clone(),
        manifest_sha256: ready.manifest_sha256.clone(),
        idempotency_key: key.into(),
        timeout_ms: 30000,
    }
}
fn setup() -> (TempDir, DeliverySettings, SnapshotReady) {
    let temp = TempDir::new().unwrap();
    let shared = std::env::var_os("WINBRIDGE_DELTA_GATE_SHARED")
        .map(|p| Path::new(&p).join(format!("sync-{}", uuid::Uuid::new_v4())))
        .unwrap_or_else(|| temp.path().join("shared"));
    fs::create_dir_all(&shared).unwrap();
    for path in ["git", "local", "full"] {
        fs::create_dir(temp.path().join(path)).unwrap();
    }
    let repo = temp.path().join("git");
    git(&repo, &["init", "-q"]);
    for i in 0..64 {
        fs::write(
            repo.join(format!("small-{i:03}.txt")),
            vec![b'a' + (i % 20) as u8; 4096],
        )
        .unwrap();
    }
    fs::write(repo.join("large.bin"), vec![23; 2 * 1024 * 1024]).unwrap();
    fs::write(repo.join("empty.dat"), []).unwrap();
    fs::write(repo.join("Привет.txt"), "первый commit ✓").unwrap();
    commit(&repo);
    let delivery = DeliverySettings {
        adapter: "shared-folder".into(),
        root: shared,
        namespace: "WinBridge".into(),
        peer_root: None,
    };
    let ready = publish(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    import(
        &delivery,
        &temp.path().join("local"),
        &request(&ready, "base"),
        "base",
        |_, _, _| Ok(()),
    )
    .unwrap();
    (temp, delivery, ready)
}

#[test]
fn delta_materializes_changes_rename_deletion_and_chain_without_mutating_base() {
    let (temp, delivery, base) = setup();
    let repo = temp.path().join("git");
    let local = temp.path().join("local");
    fs::rename(repo.join("large.bin"), repo.join("renamed.bin")).unwrap();
    fs::remove_file(repo.join("small-001.txt")).unwrap();
    fs::write(repo.join("Привет.txt"), "проверенный delta commit ✓").unwrap();
    fs::write(repo.join("new.dat"), b"new\0binary").unwrap();
    git(&repo, &["update-index", "--chmod=+x", "empty.dat"]);
    commit(&repo);
    // git add may refresh Windows mode; set index mode explicitly after staging.
    git(&repo, &["update-index", "--chmod=+x", "empty.dat"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "--amend",
            "--no-edit",
            "-q",
        ],
    );
    let full = publish_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    let delta = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    assert_eq!(delta.ready.schema_version, 2);
    assert_ne!(full.ready.snapshot_id, delta.ready.snapshot_id);
    assert!(delta.ready.payload_bytes * 100 < full.ready.payload_bytes);
    let started = Instant::now();
    let target = import(
        &delivery,
        &local,
        &request(&delta.ready, "delta"),
        "delta",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let delta_import_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let full_manifest = import(
        &delivery,
        &temp.path().join("full"),
        &request(&full.ready, "full"),
        "full",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let full_import_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(target.files, full_manifest.files);
    assert_eq!(target.source, full_manifest.source);
    assert_eq!(
        target.delta.as_ref().unwrap().reused_files.len(),
        target.files.len() - 2
    );
    let generation = generation_path(&local, &delta.ready.snapshot_id).unwrap();
    assert!(!generation.join("small-001.txt").exists());
    assert!(!generation.join("large.bin").exists());
    assert_eq!(
        fs::read(generation.join("renamed.bin")).unwrap(),
        vec![23; 2 * 1024 * 1024]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(generation.join("empty.dat"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    verified_generation(
        &local,
        "fixture",
        &base.snapshot_id,
        Some(&base.manifest_sha256),
        |_, _, _| Ok(()),
    )
    .unwrap();
    fs::write(repo.join("new.dat"), b"chain").unwrap();
    commit(&repo);
    let chain = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &delta.ready.snapshot_id,
    )
    .unwrap();
    import(
        &delivery,
        &local,
        &request(&chain.ready, "chain"),
        "chain",
        |_, _, _| Ok(()),
    )
    .unwrap();
    if let Some(report) = std::env::var_os("WINBRIDGE_DELTA_GATE_SHARED") {
        let proof = serde_json::json!({"platform":std::env::consts::OS,"base":base,"full":full,"delta":delta,"fullImportMs":full_import_ms,"deltaImportMs":delta_import_ms,"chain":chain,"targetFiles":target.files,"generationHashesEqual":true});
        fs::write(
            Path::new(&report).join("delta-delivery-proof.json"),
            serde_json::to_vec_pretty(&proof).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn delta_cancel_corrupt_base_and_missing_base_never_publish_target() {
    let (temp, delivery, base) = setup();
    let local = temp.path().join("local");
    let repo = temp.path().join("git");
    fs::write(repo.join("new.dat"), b"new").unwrap();
    commit(&repo);
    let delta = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    let target = generation_path(&local, &delta.ready.snapshot_id).unwrap();
    assert!(import(
        &delivery,
        &local,
        &request(&delta.ready, "cancel"),
        "cancel",
        |phase, _, bytes| if phase == "reuse_base" && bytes > 0 {
            Err("expected cancellation".into())
        } else {
            Ok(())
        }
    )
    .unwrap_err()
    .contains("expected cancellation"));
    assert!(!target.exists());
    let base_path = generation_path(&local, &base.snapshot_id).unwrap();
    fs::write(base_path.join("large.bin"), b"corrupt").unwrap();
    assert!(import(
        &delivery,
        &local,
        &request(&delta.ready, "corrupt"),
        "corrupt",
        |_, _, _| Ok(())
    )
    .unwrap_err()
    .contains("full snapshot explicitly"));
    assert!(!target.exists());
    assert!(import(
        &delivery,
        &temp.path().join("full"),
        &request(&delta.ready, "missing"),
        "missing",
        |_, _, _| Ok(())
    )
    .unwrap_err()
    .contains("full snapshot explicitly"));
    fs::write(base_path.join("large.bin"), vec![23; 2 * 1024 * 1024]).unwrap();
    import(
        &delivery,
        &local,
        &request(&delta.ready, "success"),
        "success",
        |_, _, _| Ok(()),
    )
    .unwrap();
    fs::rename(base_path, local.join("retained-base")).unwrap();
    fs::rename(
        &delivery.root,
        delivery.root.with_extension("retained-shared"),
    )
    .unwrap();
    import(
        &delivery,
        &local,
        &request(&delta.ready, "repeat"),
        "repeat",
        |_, _, _| Ok(()),
    )
    .unwrap();
}

#[test]
fn delta_metadata_pin_and_empty_zip_are_verified() {
    let (temp, delivery, base) = setup();
    let local = temp.path().join("local");
    let repo = temp.path().join("git");
    // Same tree: ZIP has no entries but still has a bounded, hashed directory.
    let delta = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    assert_eq!(delta.ready.payload_bytes, 22);
    let base_path = generation_path(&local, &base.snapshot_id).unwrap();
    let marker = base_path.join(".winbridge/verified.json");
    let original = fs::read(&marker).unwrap();
    let mut wrong: SnapshotReady = serde_json::from_slice(&original).unwrap();
    wrong.manifest_sha256 = "f".repeat(64);
    fs::write(&marker, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(import(
        &delivery,
        &local,
        &request(&delta.ready, "wrong-pin"),
        "wrong-pin",
        |_, _, _| Ok(())
    )
    .unwrap_err()
    .contains("full snapshot explicitly"));
    fs::write(marker, original).unwrap();
    import(
        &delivery,
        &local,
        &request(&delta.ready, "empty"),
        "empty",
        |_, _, _| Ok(()),
    )
    .unwrap();
    let repeat = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    assert!(repeat.reused_package);
    assert_eq!(repeat.ready, delta.ready);
}

#[test]
fn delta_refuses_zip_reuse_overlap_and_forged_base_metadata() {
    use std::io::Write;
    let (temp, delivery, base) = setup();
    let local = temp.path().join("local");
    let repo = temp.path().join("git");
    fs::write(repo.join("new.dat"), b"changed").unwrap();
    commit(&repo);
    let delta = publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    let package = package_path(&delivery, "fixture", &delta.ready.snapshot_id).unwrap();
    let (mut manifest, _, mut ready) =
        metadata(&delivery, "fixture", &delta.ready.snapshot_id, None).unwrap();
    // A sealed ZIP with the right entry count still cannot contain a reused path.
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(
        "large.bin",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(&vec![23; 2 * 1024 * 1024]).unwrap();
    let payload = zip.finish().unwrap().into_inner();
    manifest.payload_bytes = payload.len() as u64;
    manifest.payload_sha256 = sha256(&payload);
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    ready.payload_bytes = manifest.payload_bytes;
    ready.payload_sha256 = manifest.payload_sha256.clone();
    ready.manifest_sha256 = sha256(&bytes);
    fs::write(package.join("payload.zip"), payload).unwrap();
    fs::write(package.join("manifest.json"), bytes).unwrap();
    fs::write(
        package.join("ready.json"),
        serde_json::to_vec(&ready).unwrap(),
    )
    .unwrap();
    assert!(import(
        &delivery,
        &local,
        &request(&ready, "overlap"),
        "overlap",
        |_, _, _| Ok(())
    )
    .unwrap_err()
    .contains("absent from the manifest"));
    assert!(!generation_path(&local, &ready.snapshot_id)
        .unwrap()
        .exists());
    // Re-sealing metadata cannot make a false base hash describe actual Git bytes.
    let base_package = package_path(&delivery, "fixture", &base.snapshot_id).unwrap();
    let (mut manifest, _, mut ready) =
        metadata(&delivery, "fixture", &base.snapshot_id, None).unwrap();
    manifest
        .files
        .iter_mut()
        .find(|f| f.path == "large.bin")
        .unwrap()
        .sha256 = "a".repeat(64);
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    ready.manifest_sha256 = sha256(&bytes);
    fs::write(base_package.join("manifest.json"), bytes).unwrap();
    fs::write(
        base_package.join("ready.json"),
        serde_json::to_vec(&ready).unwrap(),
    )
    .unwrap();
    assert!(publish_delta_measured(
        &delivery,
        &repo,
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id
    )
    .unwrap_err()
    .contains("actual Git blob"));
}

#[cfg(windows)]
#[test]
fn delta_refuses_junction_base_generation() {
    let (temp, delivery, base) = setup();
    let local = temp.path().join("local");
    let delta = publish_delta_measured(
        &delivery,
        &temp.path().join("git"),
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    let generation = generation_path(&local, &base.snapshot_id).unwrap();
    let retained = local.join("retained-base");
    fs::rename(&generation, &retained).unwrap();
    let result = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&generation)
        .arg(&retained)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let error = import(
        &delivery,
        &local,
        &request(&delta.ready, "junction"),
        "junction",
        |_, _, _| Ok(()),
    )
    .unwrap_err();
    assert!(error.contains("full snapshot explicitly"), "{error}");
    assert!(!generation_path(&local, &delta.ready.snapshot_id)
        .unwrap()
        .exists());
    fs::remove_dir(&generation).unwrap();
}

#[cfg(unix)]
#[test]
fn delta_rejects_linked_base_source() {
    let (temp, delivery, base) = setup();
    let local = temp.path().join("local");
    let delta = publish_delta_measured(
        &delivery,
        &temp.path().join("git"),
        &temp.path().join("capture"),
        "fixture",
        "HEAD",
        &base.snapshot_id,
    )
    .unwrap();
    let file = generation_path(&local, &base.snapshot_id)
        .unwrap()
        .join("large.bin");
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(temp.path().join("git/large.bin"), &file).unwrap();
    assert!(import(
        &delivery,
        &local,
        &request(&delta.ready, "link"),
        "link",
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(!generation_path(&local, &delta.ready.snapshot_id)
        .unwrap()
        .exists());
}
