//! Actual retained13 history ->14, without adopting its process/permission.
//! Explicit Windows fixture only; the reference is opened read-only.
#![cfg(windows)]
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    time::Duration,
};
use winbridge_protocol::ArtifactRole;
use winbridge_update::{
    artifact::query_image,
    installed_set::{InstalledSetManifest, MANIFEST_NAME},
    journal::UpdateJournal,
    sha256,
    state_backup::{StateBackupAdmission, StateBackupPhase},
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reference {
    kind: String,
    source_proof_sha256: String,
    runtime_directory: PathBuf,
    manifest_sha256: String,
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
    let mut query = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names = query
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut tables = Vec::new();
    for name in names {
        // Only the fenced owner and NEW14 tables may differ. Include all actual
        // backup intents/current/history, unlike the earlier12->13 fixture.
        if name == "installation" || name.starts_with("runtime_preparation_") {
            continue;
        }
        assert!(name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'));
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
fn bytes(directory: &Path) -> Vec<(String, Option<String>)> {
    ["updates.sqlite", "updates.sqlite-wal"]
        .into_iter()
        .map(|name| {
            let path = directory.join(name);
            let digest = path
                .is_file()
                .then(|| fs::read(path).unwrap())
                .and_then(|b| {
                    if name.ends_with("-wal") && b.is_empty() {
                        None
                    } else {
                        Some(sha256(&b))
                    }
                });
            (name.into(), digest)
        })
        .collect()
}
fn schema(c: &Connection) -> u32 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
#[test]
#[ignore = "Explicit proven schema13 final-backup runtime; migrate a fresh consistent clone only"]
fn actual_thirteen_backup_history_survives_fourteen_and_old_thirteen_refuses() {
    let input: Input = serde_json::from_slice(
        &fs::read(
            std::env::var_os("WINBRIDGE_RUNTIME_PREPARATION_MIGRATION_INPUT")
                .expect("Explicit native input"),
        )
        .unwrap(),
    )
    .unwrap();
    let r = &input.reference;
    assert_eq!(r.kind, "preparation_schema13_reference");
    assert_eq!(r.source_proof_sha256.len(), 64);
    winbridge_platform::require_safe_path(&r.runtime_directory).unwrap();
    let manifest_bytes = fs::read(r.runtime_directory.join(MANIFEST_NAME)).unwrap();
    assert_eq!(sha256(&manifest_bytes), r.manifest_sha256);
    let manifest: InstalledSetManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    let old_exe = r.runtime_directory.join("winbridge-update.exe");
    let query = query_image(&old_exe, ArtifactRole::Updater).unwrap();
    let updater = manifest
        .artifacts
        .iter()
        .find(|a| a.name == "winbridge-update.exe")
        .unwrap();
    assert_eq!(query.image.sha256, updater.sha256);
    assert_eq!(query.image.bytes, updater.bytes);
    assert_eq!(query.descriptor, updater.descriptor);
    assert_eq!(
        query.descriptor.build.source_commit.as_ref(),
        Some(&r.source_commit)
    );
    assert_eq!(
        query.descriptor.build.source_manifest_sha256.as_ref(),
        Some(&r.source_manifest_sha256)
    );
    assert!(!query.descriptor.build.source_dirty);
    let source = r.runtime_directory.join("update-state");
    let source_bytes = bytes(&source);
    let before = UpdateJournal::inspect(&source).unwrap();
    assert_eq!(before.schema_version, 13);
    assert_eq!(before.installation_id, manifest.installation_id);
    assert_eq!(before.scope_sha256, manifest.scope_sha256);
    assert_eq!(
        before.active_update_id.as_deref(),
        Some(r.update_id.as_str())
    );
    let backup = UpdateJournal::inspect_state_backup(&source, &r.update_id)
        .unwrap()
        .unwrap();
    assert_eq!(backup.phase, StateBackupPhase::Verified);
    let shutdown = UpdateJournal::inspect_shutdown(&source, &r.update_id)
        .unwrap()
        .unwrap();
    assert_eq!(*backup.intent.binding.shutdown, shutdown);
    let supervisor =
        UpdateJournal::inspect_supervisor(&source, &shutdown.intent.binding.launch_id).unwrap();
    let original = Connection::open_with_flags(
        source.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let history = rows(&original);
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
    assert_eq!(migrated.schema_version, 14);
    assert_eq!(migrated.owner_epoch, before.owner_epoch + 1);
    assert_eq!(migrated.installation_id, before.installation_id);
    assert_eq!(migrated.scope_sha256, before.scope_sha256);
    assert_eq!(
        UpdateJournal::inspect_state_backup(&copy, &r.update_id)
            .unwrap()
            .unwrap(),
        backup
    );
    assert_eq!(
        UpdateJournal::inspect_shutdown(&copy, &r.update_id)
            .unwrap()
            .unwrap(),
        shutdown
    );
    let inspected = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(rows(&inspected), history);
    for table in [
        "runtime_preparation_intents",
        "runtime_preparation_records",
        "runtime_preparation_history",
    ] {
        assert_eq!(
            inspected
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let backups = fs::read_dir(&copy)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v13-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let archived =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema(&archived), 13);
    assert_eq!(rows(&archived), history);
    assert_eq!(
        u64::try_from(
            archived
                .query_row("SELECT epoch FROM installation", [], |r| r.get::<_, i64>(0))
                .unwrap()
        )
        .unwrap(),
        before.owner_epoch
    );
    assert_eq!(
        archived
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    // New ownership must not turn saved Exited/Verified into physical custody.
    let owner = journal.acquire_owner().unwrap();
    match journal
        .admit_state_backup(&owner, &supervisor, None)
        .unwrap()
    {
        StateBackupAdmission::Existing(record) => assert_eq!(*record, backup),
        StateBackupAdmission::Fresh(_) => panic!("Historical backup minted fresh permission"),
    };
    assert!(journal
        .admit_runtime_preparation(&owner, &supervisor, None)
        .is_err());
    assert!(
        UpdateJournal::inspect_runtime_preparation(&copy, &r.update_id)
            .unwrap()
            .is_none()
    );
    drop(owner);
    drop(journal);
    drop(inspected);
    drop(archived);
    let project = input.root.join("project");
    fs::create_dir(&project).unwrap();
    let worker_config = runtime.join("worker.local.json");
    fs::write(&worker_config, b"{}").unwrap();
    let config = runtime.join("update.local.json");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":project,"runtimeDirectory":runtime,"workerConfigPath":worker_config})).unwrap()).unwrap();
    let unchanged = bytes(&copy);
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
        assert_eq!(bytes(&copy), unchanged);
    }
    drop(original);
    assert_eq!(bytes(&source), source_bytes);
    assert_eq!(UpdateJournal::inspect(&source).unwrap(), before);
    let proof = serde_json::json!({"schemaVersion":1,"kind":"runtime_preparation_schema_migration_proof","passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),"sourceProofSha256":r.source_proof_sha256,"referenceManifestSha256":r.manifest_sha256,"referenceManifestJson":String::from_utf8(manifest_bytes).unwrap(),"referenceUpdaterSha256":query.image.sha256,"updateId":r.update_id,"fromSchema":13,"toSchema":14,"epochBefore":before.owner_epoch,"epochAfter":migrated.owner_epoch,"retainedRowsSha256":history,"allOldRowsPreserved":true,"verifiedBackupHistoryPreserved":true,"shutdownHistoryPreserved":true,"migrationBackupVerified":true,"noPreparationInvented":true,"historicalBackupObservationOnly":true,"noCustodyAdopted":true,"oldThirteenInitRefused":true,"oldThirteenStatusRefused":true,"sourceBytesUnchanged":true,"runtimeApplied":false,"windowsOpened":false});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input.root.join("migration-proof.json"))
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}
