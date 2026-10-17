use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    process::Command,
};
use winbridge_core::update::{MAX_SOURCE_FILE_BYTES, SOURCE_MANIFEST_NAME};
use winbridge_protocol::update::{SourceFile, SourceManifest, UpdateSourcePin};
use winbridge_update::{
    archive::{stage_archive, verify_archive},
    sha256,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn manifest() -> SourceManifest {
    SourceManifest {
        schema_version: 1,
        source_commit: "a".repeat(40),
        dirty: false,
        created_at: "2026-10-17T00:00:00Z".into(),
        includes_built_ui: false,
        files: vec![
            SourceFile {
                path: "Cargo.toml".into(),
                bytes: 4,
                sha256: sha256(b"test"),
            },
            SourceFile {
                path: "src/main.rs".into(),
                bytes: 5,
                sha256: sha256(b"hello"),
            },
        ],
    }
}
fn entries() -> Vec<(String, Vec<u8>)> {
    vec![
        ("winbridge/Cargo.toml".into(), b"test".to_vec()),
        ("winbridge/src/main.rs".into(), b"hello".to_vec()),
    ]
}
fn write_zip(
    path: &Path,
    manifest: &SourceManifest,
    entries: Vec<(String, Vec<u8>)>,
    method: CompressionMethod,
) -> UpdateSourcePin {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, bytes) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let bytes = serde_json::to_vec(manifest).unwrap();
    zip.start_file(format!("winbridge/{SOURCE_MANIFEST_NAME}"), options)
        .unwrap();
    zip.write_all(&bytes).unwrap();
    zip.finish().unwrap();
    UpdateSourcePin {
        archive_name: path.file_name().unwrap().to_str().unwrap().into(),
        archive_sha256: sha256(&fs::read(path).unwrap()),
        manifest_sha256: sha256(&bytes),
        source_commit: manifest.source_commit.clone(),
    }
}

#[test]
fn deflated_and_stored_delivery_are_pinned_and_stage_is_create_only() {
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.zip");
        let pin = write_zip(&path, &manifest(), entries(), method);
        let before = fs::read(&path).unwrap();
        let verified = verify_archive(&path, &pin.archive_sha256, &pin.manifest_sha256).unwrap();
        assert!(verified.read_only);
        assert_eq!(verified.source.file_count, 2);
        assert_eq!(verified.source.bytes, 9);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        let destination = temp.path().join("stage");
        let staged = stage_archive(&path, &pin, &destination).unwrap();
        assert!(!staged.read_only);
        assert_eq!(
            fs::read(destination.join("source/src/main.rs")).unwrap(),
            b"hello"
        );
        assert_eq!(fs::read(destination.join("source.zip")).unwrap(), before);
        assert!(destination.join("source-ready.json").is_file());
        assert!(stage_archive(&path, &pin, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("source/src/main.rs")).unwrap(),
            b"hello"
        );
    }
}

#[test]
fn bad_archive_manifest_commit_and_payload_hashes_never_produce_ready_stage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    let mut pin = write_zip(&path, &manifest(), entries(), CompressionMethod::Deflated);
    let stage = temp.path().join("stage");
    assert!(verify_archive(&path, &"0".repeat(64), &pin.manifest_sha256).is_err());
    assert!(verify_archive(&path, &pin.archive_sha256, &"0".repeat(64)).is_err());
    pin.source_commit = "b".repeat(40);
    assert!(stage_archive(&path, &pin, &stage).is_err());
    assert!(!stage.exists());
    let mut changed = entries();
    changed[1].1 = b"wrong".to_vec();
    let pin = write_zip(&path, &manifest(), changed, CompressionMethod::Deflated);
    assert!(stage_archive(&path, &pin, &stage).is_err());
    assert!(!stage.exists());
}

#[test]
fn extra_traversal_case_variant_and_directory_entries_are_not_extracted() {
    for name in [
        "winbridge/../outside",
        "winbridge/CON.txt",
        "winbridge/.env",
        "winbridge/target/cache",
        "winbridge/cargo.toml",
        "winbridge/empty/",
        "other/source.rs",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.zip");
        let mut all = entries();
        all.push((name.into(), b"extra".to_vec()));
        let pin = write_zip(&path, &manifest(), all, CompressionMethod::Stored);
        let stage = temp.path().join("stage");
        assert!(stage_archive(&path, &pin, &stage).is_err(), "{name}");
        assert!(!stage.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[test]
fn duplicate_entry_names_are_detected_even_if_zip_parser_collapses_them() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    // Change equal-length names only in ZIP local/central headers.
    let mut m = manifest();
    m.files[1].path = "src/file-a.rs".into();
    m.files.push(SourceFile {
        path: "src/file-b.rs".into(),
        bytes: 5,
        sha256: sha256(b"hello"),
    });
    let all = vec![
        ("winbridge/Cargo.toml".into(), b"test".to_vec()),
        ("winbridge/src/file-a.rs".into(), b"hello".to_vec()),
        ("winbridge/src/file-b.rs".into(), b"hello".to_vec()),
    ];
    let pin = write_zip(&path, &m, all, CompressionMethod::Stored);
    let mut bytes = fs::read(&path).unwrap();
    let old = b"winbridge/src/file-b.rs";
    let new = b"winbridge/src/file-a.rs";
    for offset in 0..bytes.len() - old.len() {
        if &bytes[offset..offset + old.len()] == old {
            bytes[offset..offset + old.len()].copy_from_slice(new);
        }
    }
    fs::write(&path, &bytes).unwrap();
    assert!(verify_archive(&path, &sha256(&bytes), &pin.manifest_sha256).is_err());
}

#[test]
fn dirty_oversized_and_linked_sources_are_refused_before_staging() {
    for oversized in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.zip");
        let mut m = manifest();
        if oversized {
            m.files[1].bytes = MAX_SOURCE_FILE_BYTES + 1;
        } else {
            m.dirty = true;
        }
        let pin = write_zip(&path, &m, entries(), CompressionMethod::Deflated);
        assert!(stage_archive(&path, &pin, &temp.path().join("stage")).is_err());
        assert!(!temp.path().join("stage").exists());
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    zip.add_symlink(
        "winbridge/src/main.rs",
        "../../outside",
        SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(verify_archive(&path, &sha256(&bytes), &"0".repeat(64)).is_err());
}

#[test]
fn zip64_count_and_oversized_directory_are_rejected_before_parser_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    let pin = write_zip(&path, &manifest(), entries(), CompressionMethod::Stored);
    let original = fs::read(&path).unwrap();
    let end = original.len() - 22;
    for patch in [0, 1] {
        let mut bytes = original.clone();
        if patch == 0 {
            bytes[end + 8..end + 12].fill(0xff);
        } else {
            bytes[end + 12..end + 16].copy_from_slice(&(9u32 * 1024 * 1024).to_le_bytes());
        }
        fs::write(&path, &bytes).unwrap();
        let error = verify_archive(&path, &sha256(&bytes), &pin.manifest_sha256).unwrap_err();
        assert!(error.contains("directory refused"));
    }
}

#[test]
fn cli_archive_verification_is_readonly_and_needs_no_installation_config() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    let pin = write_zip(&path, &manifest(), entries(), CompressionMethod::Deflated);
    let output = Command::new(env!("CARGO_BIN_EXE_winbridge-update"))
        .arg("verify-archive")
        .arg(&path)
        .args([&pin.archive_sha256, &pin.manifest_sha256])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["kind"], "source_archive_verified");
    assert_eq!(value["readOnly"], true);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
