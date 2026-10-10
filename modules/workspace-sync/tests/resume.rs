#[cfg(unix)]
use std::io::Read;
use std::{
    fs,
    io::{Seek, Write},
    path::Path,
    process::Command,
};
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
struct Fixture {
    temp: TempDir,
    delivery: DeliverySettings,
    request: ImportSubmission,
}
impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        for name in ["git", "local"] {
            fs::create_dir(temp.path().join(name)).unwrap();
        }
        let shared = std::env::var_os("WINBRIDGE_RESUME_GATE_SHARED")
            .map(|p| Path::new(&p).join(format!("resume-{}", uuid::Uuid::new_v4())))
            .unwrap_or_else(|| temp.path().join("shared"));
        fs::create_dir(&shared).unwrap();
        let repo = temp.path().join("git");
        fs::write(repo.join("blob.dat"), vec![0xA5; 3 * 1024 * 1024 + 17]).unwrap();
        fs::write(repo.join("Привет.txt"), "resume ✓").unwrap();
        git(&repo, &["init", "-q"]);
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
                "resume",
            ],
        );
        let delivery = DeliverySettings {
            adapter: "shared-folder".into(),
            root: shared,
            namespace: "WinBridge".into(),
            peer_root: None,
        };
        let ready = publish_resumable_measured(
            &delivery,
            &repo,
            &temp.path().join("state"),
            "fixture",
            "HEAD",
            None,
        )
        .unwrap()
        .ready;
        assert_eq!(ready.schema_version, 3);
        let request = ImportSubmission {
            protocol_version: 1,
            request_id: "resume".into(),
            project_id: "fixture".into(),
            snapshot_id: ready.snapshot_id,
            manifest_sha256: ready.manifest_sha256,
            idempotency_key: "resume".into(),
            timeout_ms: 30000,
        };
        Self {
            temp,
            delivery,
            request,
        }
    }
    fn local(&self) -> std::path::PathBuf {
        self.temp.path().join("local")
    }
    fn package(&self) -> std::path::PathBuf {
        package_path(&self.delivery, "fixture", &self.request.snapshot_id).unwrap()
    }
    fn cache(&self) -> std::path::PathBuf {
        self.local()
            .join(".winbridge/payload-cache")
            .join(&self.request.manifest_sha256)
    }
    fn interrupt(&self) {
        let error = import(
            &self.delivery,
            &self.local(),
            &self.request,
            "interrupted",
            |phase, _, bytes| {
                if phase == "copy_payload" && bytes >= 1024 * 1024 + 65536 {
                    Err("planned interruption".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("planned interruption"));
        assert!(!generation_path(&self.local(), &self.request.snapshot_id)
            .unwrap()
            .exists());
        assert!(self.cache().join("0000.chunk").exists());
        assert!(!self.cache().join("0001.chunk").exists());
    }
    fn finish(&self, operation: &str) -> SnapshotTransfer {
        let mut summary = None;
        import_with_transfer(
            &self.delivery,
            &self.local(),
            &self.request,
            operation,
            |_, _, _| Ok(()),
            |s| {
                summary = Some(s.clone());
                Ok(())
            },
        )
        .unwrap();
        summary.unwrap()
    }
}

#[test]
fn explicit_resume_verifies_cached_blocks_and_reads_only_the_missing_shared_bytes() {
    let f = Fixture::new();
    f.interrupt();
    // A changed shared prefix must not be read again. Its original block is manifest-pinned locally.
    let mut shared = fs::OpenOptions::new()
        .write(true)
        .open(f.package().join("payload.zip"))
        .unwrap();
    shared.write_all(b"changed prefix").unwrap();
    shared.sync_all().unwrap();
    drop(shared);
    let summary = f.finish("explicit-resume");
    assert_eq!(summary.reused_bytes, 1024 * 1024);
    assert_eq!(
        summary.reused_bytes + summary.shared_bytes,
        summary.payload_bytes
    );
    let root = generation_path(&f.local(), &f.request.snapshot_id).unwrap();
    assert_eq!(
        fs::read(root.join("blob.dat")).unwrap(),
        vec![0xA5; 3 * 1024 * 1024 + 17]
    );
    assert_eq!(
        fs::read_to_string(root.join("Привет.txt")).unwrap(),
        "resume ✓"
    );
    assert!(f
        .local()
        .join(".winbridge/imports/interrupted/block-0001.part")
        .exists());
    let mut phases = vec![];
    import(
        &f.delivery,
        &f.local(),
        &f.request,
        "verified-repeat",
        |phase, _, _| {
            phases.push(phase.to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert!(!phases.iter().any(|p| p == "copy_payload"));
}

#[test]
fn corrupt_cached_or_shared_block_never_publishes_a_generation() {
    for corrupt_cache in [true, false] {
        let f = Fixture::new();
        f.interrupt();
        let path = if corrupt_cache {
            f.cache().join("0000.chunk")
        } else {
            f.package().join("payload.zip")
        };
        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(std::io::SeekFrom::Start(if corrupt_cache {
            0
        } else {
            1024 * 1024
        }))
        .unwrap();
        file.write_all(b"bad").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let error = import(
            &f.delivery,
            &f.local(),
            &f.request,
            "corrupt-retry",
            |_, _, _| Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("SHA-256 mismatch"), "{error}");
        assert!(!generation_path(&f.local(), &f.request.snapshot_id)
            .unwrap()
            .exists());
        assert!(path.exists());
        assert!(!f.cache().join("0001.chunk").exists());
    }
}

#[test]
fn manifest_pin_whole_hash_and_payload_tail_fail_closed() {
    for kind in ["pin", "whole", "tail"] {
        let mut f = Fixture::new();
        if kind == "tail" {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(f.package().join("payload.zip"))
                .unwrap();
            file.write_all(b"tail").unwrap();
            file.sync_all().unwrap();
        } else if kind == "pin" {
            f.request.manifest_sha256 = "f".repeat(64);
        } else {
            let mut m: SnapshotManifest =
                serde_json::from_slice(&fs::read(f.package().join("manifest.json")).unwrap())
                    .unwrap();
            m.payload_sha256 = "e".repeat(64);
            let bytes = serde_json::to_vec_pretty(&m).unwrap();
            let mut r: SnapshotReady =
                serde_json::from_slice(&fs::read(f.package().join("ready.json")).unwrap()).unwrap();
            r.payload_sha256 = m.payload_sha256;
            r.manifest_sha256 = sha256(&bytes);
            fs::write(f.package().join("manifest.json"), bytes).unwrap();
            fs::write(
                f.package().join("ready.json"),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
            f.request.manifest_sha256 = r.manifest_sha256;
        }
        assert!(import(&f.delivery, &f.local(), &f.request, "bad", |_, _, _| Ok(())).is_err());
        assert!(!generation_path(&f.local(), &f.request.snapshot_id)
            .unwrap()
            .exists());
    }
}

#[test]
fn mount_loss_keeps_checkpoint_and_new_explicit_import_resumes() {
    let f = Fixture::new();
    f.interrupt();
    let payload = f.package().join("payload.zip");
    let offline = f.package().join("offline.zip");
    fs::rename(&payload, &offline).unwrap();
    assert!(import(
        &f.delivery,
        &f.local(),
        &f.request,
        "offline",
        |_, _, _| Ok(())
    )
    .is_err());
    assert!(f.cache().join("0000.chunk").exists());
    fs::rename(&offline, &payload).unwrap();
    assert_eq!(f.finish("online").reused_bytes, 1024 * 1024);
}

#[test]
fn resumable_delta_and_legacy_full_have_identical_materialized_files() {
    let f = Fixture::new();
    f.finish("base");
    let repo = f.temp.path().join("git");
    fs::write(repo.join("Привет.txt"), "delta resume ✓").unwrap();
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
            "delta",
        ],
    );
    let result = publish_resumable_measured(
        &f.delivery,
        &repo,
        &f.temp.path().join("state"),
        "fixture",
        "HEAD",
        Some(&f.request.snapshot_id),
    )
    .unwrap();
    let mut request = f.request.clone();
    request.snapshot_id = result.ready.snapshot_id;
    request.manifest_sha256 = result.ready.manifest_sha256;
    let delta = import(&f.delivery, &f.local(), &request, "delta", |_, _, _| Ok(())).unwrap();
    assert!(delta.delta.is_some());
    let full = publish(
        &f.delivery,
        &repo,
        &f.temp.path().join("state"),
        "fixture",
        "HEAD",
    )
    .unwrap();
    request.snapshot_id = full.snapshot_id;
    request.manifest_sha256 = full.manifest_sha256;
    let full = import(&f.delivery, &f.local(), &request, "full", |_, _, _| Ok(())).unwrap();
    assert_eq!(delta.files, full.files);
}

#[cfg(unix)]
#[test]
fn linked_cache_and_blocks_are_rejected_without_touching_the_target() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.interrupt();
    let cached = f.cache().join("0000.chunk");
    let original = f.temp.path().join("preserved.chunk");
    fs::rename(&cached, &original).unwrap();
    symlink(&original, &cached).unwrap();
    assert!(
        import(&f.delivery, &f.local(), &f.request, "linked", |_, _, _| Ok(
            ()
        ))
        .unwrap_err()
        .contains("Linked/reparse")
    );
    let mut original_bytes = Vec::new();
    fs::File::open(original)
        .unwrap()
        .read_to_end(&mut original_bytes)
        .unwrap();
    assert_eq!(original_bytes.len(), 1024 * 1024);
}

#[cfg(windows)]
#[test]
fn junction_cache_is_rejected_without_touching_the_retained_blocks() {
    let f = Fixture::new();
    f.interrupt();
    let cache = f.cache();
    let retained = f.temp.path().join("retained-cache");
    fs::rename(&cache, &retained).unwrap();
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(cache.to_string_lossy().replace('/', "\\"))
        .arg(retained.to_string_lossy().replace('/', "\\"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = import(
        &f.delivery,
        &f.local(),
        &f.request,
        "junction",
        |_, _, _| Ok(()),
    );
    fs::remove_dir(&cache).unwrap();
    assert!(result.unwrap_err().contains("Linked/reparse"));
    assert_eq!(
        fs::read(retained.join("0000.chunk")).unwrap().len(),
        1024 * 1024
    );
}
