//! Consistent clone of a proven schema12 shutdown archive. Never opens installed
//! state or adopts its historical process/guard. Requires explicit native input.
#![cfg(windows)]
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use winbridge_protocol::ArtifactRole;
use winbridge_update::{
    artifact::query_image,
    installed_set::{InstalledSetManifest, MANIFEST_NAME},
    journal::UpdateJournal,
    sha256,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reference {
    kind: String,
    source_proof_sha256: String,
    runtime_directory: PathBuf,
    manifest_sha256: String,
    updater_sha256: String,
    update_id: String,
    source_commit: String,
    source_manifest_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    root: PathBuf,
    reference: Reference,
}
fn rows(c: &Connection) -> String {
    let mut q = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut tables = Vec::new();
    for name in names {
        if name == "installation" || name.starts_with("state_backup_") {
            continue;
        }
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut q = c.prepare(&format!("SELECT * FROM {name}")).unwrap();
        let count = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                (0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .map(|r| format!("{:?}", r.unwrap()))
            .collect::<Vec<_>>();
        rows.sort();
        tables.push((name, rows));
    }
    sha256(&serde_json::to_vec(&tables).unwrap())
}
fn bytes(path: &Path) -> Vec<(String, Option<String>)> {
    ["updates.sqlite", "updates.sqlite-wal"]
        .into_iter()
        .map(|n| {
            let p = path.join(n);
            let digest = p.is_file().then(|| fs::read(p).unwrap()).and_then(|b| {
                if n.ends_with("-wal") && b.is_empty() {
                    None
                } else {
                    Some(sha256(&b))
                }
            });
            (n.into(), digest)
        })
        .collect()
}
fn schema(c: &Connection) -> u32 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
#[test]
#[ignore = "Explicit retained proven schema12 archive required; isolated migration clone only"]
fn archived_schema_twelve_shutdown_migrates_without_adopting_custody_and_old_twelve_refuses() {
    let input: Input = serde_json::from_slice(
        &fs::read(
            std::env::var_os("WINBRIDGE_STATE_BACKUP_MIGRATION_INPUT")
                .expect("Explicit native input"),
        )
        .unwrap(),
    )
    .unwrap();
    let r = &input.reference;
    assert_eq!(r.kind, "state_backup_schema12_reference");
    assert_eq!(r.source_proof_sha256.len(), 64);
    winbridge_platform::require_safe_path(&r.runtime_directory).unwrap();
    let manifest_bytes = fs::read(r.runtime_directory.join(MANIFEST_NAME)).unwrap();
    assert_eq!(sha256(&manifest_bytes), r.manifest_sha256);
    let manifest: InstalledSetManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    let old_exe = r.runtime_directory.join("winbridge-update.exe");
    let descriptor = query_image(&old_exe, ArtifactRole::Updater).unwrap();
    assert_eq!(descriptor.image.sha256, r.updater_sha256);
    let artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.name == "winbridge-update.exe")
        .unwrap();
    assert_eq!(artifact.sha256, r.updater_sha256);
    assert_eq!(artifact.descriptor, descriptor.descriptor);
    assert_eq!(
        descriptor.descriptor.build.source_commit.as_ref(),
        Some(&r.source_commit)
    );
    assert_eq!(
        descriptor.descriptor.build.source_manifest_sha256.as_ref(),
        Some(&r.source_manifest_sha256)
    );
    let source = r.runtime_directory.join("update-state");
    let source_bytes = bytes(&source);
    let before = UpdateJournal::inspect(&source).unwrap();
    assert_eq!(before.schema_version, 12);
    assert_eq!(before.installation_id, manifest.installation_id);
    assert_eq!(before.scope_sha256, manifest.scope_sha256);
    assert_eq!(before.active_update_id.as_ref(), Some(&r.update_id));
    let shutdown = UpdateJournal::inspect_shutdown(&source, &r.update_id)
        .unwrap()
        .unwrap();
    assert!(matches!(
        shutdown.delivery,
        winbridge_update::shutdown::ShutdownDeliveryState::Exited { .. }
    ));
    let sup =
        UpdateJournal::inspect_supervisor(&source, &shutdown.intent.binding.launch_id).unwrap();
    let original = Connection::open_with_flags(
        source.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let history = rows(&original);
    // Create a fresh isolated destination; the original DB/WAL are never copied
    // with filesystem operations, opened for writing, migrated or owner-claimed.
    fs::create_dir(&input.root).unwrap();
    let runtime = input.root.join("runtime");
    fs::create_dir(&runtime).unwrap();
    let copy = runtime.join("update-state");
    fs::create_dir(&copy).unwrap();
    let path = copy.join("updates.sqlite");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    {
        let mut destination =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        rusqlite::backup::Backup::new(&original, &mut destination)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let mut journal = UpdateJournal::open(&copy, &before.scope_sha256).unwrap();
    let migrated = journal.info().unwrap();
    assert_eq!(migrated.schema_version, 13);
    assert!(migrated.owner_epoch > before.owner_epoch);
    assert_eq!(migrated.installation_id, before.installation_id);
    assert_eq!(migrated.scope_sha256, before.scope_sha256);
    assert_eq!(
        UpdateJournal::inspect_shutdown(&copy, &r.update_id)
            .unwrap()
            .unwrap(),
        shutdown
    );
    let inspected = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(rows(&inspected), history);
    for table in [
        "state_backup_intents",
        "state_backup_records",
        "state_backup_history",
    ] {
        assert_eq!(
            inspected
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let backup = fs::read_dir(&copy)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v12-")
        })
        .unwrap();
    let archived = Connection::open_with_flags(&backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&archived), 12);
    assert_eq!(rows(&archived), history);
    assert_eq!(
        archived
            .query_row("SELECT epoch FROM installation", [], |r| r.get::<_, i64>(0))
            .unwrap() as u64,
        before.owner_epoch
    );
    assert_eq!(
        archived
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let owner = journal.acquire_owner().unwrap();
    assert!(journal.admit_state_backup(&owner, &sup, None).is_err());
    assert!(journal.state_backup(&r.update_id).unwrap().is_none());
    drop(owner);
    drop(journal);
    drop(inspected);
    drop(archived);
    // Synthetic checked config only for the old CLI's early version refusal.
    let project = input.root.join("project");
    fs::create_dir(&project).unwrap();
    let worker_config = runtime.join("winbridge-worker.local.json");
    fs::write(&worker_config, b"{}").unwrap();
    let config = runtime.join("winbridge-update.local.json");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":project,"runtimeDirectory":runtime,"workerConfigPath":worker_config})).unwrap()).unwrap();
    let current_before = bytes(&copy);
    for operation in ["init", "status"] {
        let out = std::process::Command::new(&old_exe)
            .creation_flags(0x08000000)
            .arg("--config")
            .arg(&config)
            .args(["journal", operation])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("UnsupportedSchema"));
        assert_eq!(bytes(&copy), current_before);
    }
    drop(original);
    assert_eq!(bytes(&source), source_bytes);
    assert_eq!(UpdateJournal::inspect(&source).unwrap(), before);
    let proof = serde_json::json!({"schemaVersion":1,"kind":"state_backup_schema_migration_proof","passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),"sourceProofSha256":r.source_proof_sha256,"referenceManifestSha256":r.manifest_sha256,"referenceUpdaterSha256":r.updater_sha256,"updateId":r.update_id,"fromSchema":12,"toSchema":13,"epochBefore":before.owner_epoch,"epochAfter":migrated.owner_epoch,"retainedRowsSha256":history,"backupRowsPreserved":true,"shutdownHistoryPreserved":true,"migrationBackupVerified":true,"noBackupAttemptInvented":true,"noCustodyAdopted":true,"oldTwelveInitRefused":true,"oldTwelveStatusRefused":true,"sourceBytesUnchanged":true,"runtimeApplied":false,"windowsOpened":false});
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input.root.join("migration-proof.json"))
        .unwrap();
    f.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
        .unwrap();
    f.sync_all().unwrap();
}
