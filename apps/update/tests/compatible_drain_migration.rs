//! Actual retained16 support history ->17 on a consistent copy, never adoption.
#![cfg(windows)]
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    time::Duration,
};
use winbridge_protocol::{
    update::UpdateSourcePin, ArtifactDescriptor, ArtifactRole, RuntimeImageEvidence,
};
use winbridge_update::{
    artifact,
    installed_set::{InstalledSetManifest, MANIFEST_NAME},
    journal::{UpdateJournal, UpdateJournalInfo},
    sha256,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reference {
    schema_version: u16,
    kind: String,
    reference_run: String,
    report_sha256: String,
    proof_sha256: String,
    source: UpdateSourcePin,
    image: RuntimeImageEvidence,
    descriptor: ArtifactDescriptor,
    manifest_sha256: String,
    update_id: String,
    support_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    root: PathBuf,
    reference: Reference,
    proof: PathBuf,
}
fn rows(c: &Connection) -> String {
    let mut q = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut tables = vec![];
    for name in names {
        if name == "installation" || name == "compatible_drain_records" {
            continue;
        }
        assert!(name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'));
        let mut q = c.prepare(&format!("SELECT * FROM {name}")).unwrap();
        let columns = q.column_count();
        let mut data = q
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .map(|r| format!("{:?}", r.unwrap()))
            .collect::<Vec<_>>();
        data.sort();
        tables.push((name, data));
    }
    sha256(&serde_json::to_vec(&tables).unwrap())
}
fn bytes(directory: &Path) -> Vec<(String, Option<String>)> {
    ["updates.sqlite", "updates.sqlite-wal"]
        .into_iter()
        .map(|name| {
            let path = directory.join(name);
            let data = path.is_file().then(|| fs::read(path).unwrap());
            (
                name.into(),
                data.and_then(|b| {
                    if name.ends_with("-wal") && b.is_empty() {
                        None
                    } else {
                        Some(sha256(&b))
                    }
                }),
            )
        })
        .collect()
}
fn info(c: &Connection) -> UpdateJournalInfo {
    let (installation_id, scope_sha256, owner_epoch) = c
        .query_row(
            "SELECT installation,scope,epoch FROM installation WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?)),
        )
        .unwrap();
    UpdateJournalInfo {
        schema_version: c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap(),
        installation_id,
        scope_sha256,
        owner_epoch: owner_epoch.try_into().unwrap(),
        active_update_id: c
            .query_row("SELECT id FROM updates WHERE active=1", [], |r| r.get(0))
            .optional()
            .unwrap(),
        last_sequence: c
            .query_row(
                "SELECT coalesce(max(sequence),0) FROM transitions",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            .try_into()
            .unwrap(),
    }
}
#[test]
#[ignore = "Pinned actual16 support reference; copy-only migration, no installed update"]
fn actual_sixteen_support_history_survives_seventeen_and_old_sixteen_refuses() {
    let input: Input = serde_json::from_slice(
        &fs::read(std::env::var_os("WINBRIDGE_COMPATIBLE_DRAIN_MIGRATION_INPUT").unwrap()).unwrap(),
    )
    .unwrap();
    let r = &input.reference;
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.kind, "compatible_drain_schema16_reference");
    assert!(!r.reference_run.is_empty());
    assert_eq!(r.report_sha256.len(), 64);
    let old_exe = Path::new(&r.image.path);
    let runtime = old_exe.parent().unwrap();
    winbridge_platform::require_safe_path(runtime).unwrap();
    winbridge_platform::require_local_disk(runtime).unwrap();
    let query = artifact::query_image(old_exe, ArtifactRole::Updater).unwrap();
    assert_eq!(query.image, r.image);
    assert_eq!(query.descriptor, r.descriptor);
    assert_eq!(
        query.descriptor.build.source_commit.as_ref(),
        Some(&r.source.source_commit)
    );
    assert_eq!(
        query.descriptor.build.source_manifest_sha256.as_ref(),
        Some(&r.source.manifest_sha256)
    );
    let manifest_file = runtime.join(MANIFEST_NAME);
    let manifest_evidence = artifact::inspect_image(&manifest_file).unwrap();
    assert_eq!(manifest_evidence.sha256, r.manifest_sha256);
    let manifest: InstalledSetManifest =
        serde_json::from_slice(&fs::read(&manifest_file).unwrap()).unwrap();
    let updater = manifest
        .artifacts
        .iter()
        .find(|a| a.name == "winbridge-update.exe")
        .unwrap();
    assert_eq!(updater.descriptor, r.descriptor);
    assert_eq!(updater.sha256, r.image.sha256);
    let source = runtime.join("update-state");
    let source_bytes = bytes(&source);
    let before = UpdateJournal::inspect(&source).unwrap();
    assert_eq!(before.schema_version, 16);
    assert_eq!(before.installation_id, manifest.installation_id);
    assert_eq!(before.scope_sha256, manifest.scope_sha256);
    let support = UpdateJournal::inspect_candidate_support(&source, &r.update_id)
        .unwrap()
        .unwrap();
    support.validate().unwrap();
    assert_eq!(support.revision, 5);
    let value = serde_json::to_value(&support).unwrap();
    assert_eq!(
        sha256(&winbridge_protocol::canonical_evidence_json(&value).unwrap()),
        r.support_sha256
    );
    let parent = UpdateJournal::read_snapshot(&source, Some(&r.update_id)).unwrap();
    let supervisor = UpdateJournal::inspect_supervisor(&source, &support.intent.launch_id).unwrap();
    let abort = UpdateJournal::inspect_abort(&source, &r.update_id).unwrap();
    let original = Connection::open_with_flags(
        source.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let rows_before = rows(&original);
    fs::create_dir(&input.root).unwrap();
    let copied_runtime = input.root.join("runtime");
    fs::create_dir(&copied_runtime).unwrap();
    let copy = copied_runtime.join("update-state");
    fs::create_dir(&copy).unwrap();
    let copy_file = copy.join("updates.sqlite");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&copy_file)
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    {
        let mut dest =
            Connection::open_with_flags(&copy_file, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        rusqlite::backup::Backup::new(&original, &mut dest)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let mut journal = UpdateJournal::open(&copy, &before.scope_sha256).unwrap();
    let after = journal.info().unwrap();
    assert_eq!(after.schema_version, 17);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.scope_sha256, before.scope_sha256);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(after.active_update_id, before.active_update_id);
    assert_eq!(
        UpdateJournal::inspect_candidate_support(&copy, &r.update_id)
            .unwrap()
            .as_ref(),
        Some(&support)
    );
    assert_eq!(
        UpdateJournal::read_snapshot(&copy, Some(&r.update_id))
            .unwrap()
            .1,
        parent.1
    );
    let inspected =
        Connection::open_with_flags(&copy_file, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows_after = rows(&inspected);
    assert_eq!(rows_after, rows_before);
    let gate_rows: i64 = inspected
        .query_row("SELECT count(*) FROM compatible_drain_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(gate_rows, 0);
    let backups = fs::read_dir(&copy)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v16-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let archived =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let backup_info = info(&archived);
    assert_eq!(backup_info, before);
    let backup_rows = rows(&archived);
    assert_eq!(backup_rows, rows_before);
    assert_eq!(
        archived
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let owner = journal.acquire_owner().unwrap();
    assert!(journal
        .consider_compatible_drain(&owner, &supervisor, abort.worker_before.as_ref().unwrap())
        .is_err());
    assert!(UpdateJournal::inspect_compatible_drain(&copy, &r.update_id)
        .unwrap()
        .is_none());
    drop(owner);
    drop(journal);
    drop(inspected);
    drop(archived);
    let project = input.root.join("project");
    fs::create_dir(&project).unwrap();
    let worker = copied_runtime.join("worker.local.json");
    fs::write(&worker, b"{}").unwrap();
    let config = copied_runtime.join("update.local.json");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":project,"runtimeDirectory":copied_runtime,"workerConfigPath":worker})).unwrap()).unwrap();
    let unchanged = bytes(&copy);
    for operation in ["init", "status"] {
        let out = std::process::Command::new(old_exe)
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
    assert_eq!(artifact::inspect_image(old_exe).unwrap(), r.image);
    assert_eq!(
        artifact::inspect_image(&manifest_file).unwrap(),
        manifest_evidence
    );
    let proof = serde_json::json!({"schemaVersion":1,"kind":"compatible_drain_schema_migration_proof","passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),
        "referenceProofSha256":r.proof_sha256,"referenceImage":r.image,"referenceManifestSha256":r.manifest_sha256,
        "before":before,"after":after,"backupInfo":backup_info,"retainedSupport":value,
        "rowsBefore":rows_before,"rowsAfter":rows_after,"backupRows":backup_rows,"gateRows":gate_rows,
        "oldSixteenInitRefused":true,"oldSixteenStatusRefused":true,"sourceBytesUnchanged":true,"copyBytesUnchangedByOld":true,
        "newAdmissionDenied":true,"migrationBackupVerified":true,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input.proof)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}
