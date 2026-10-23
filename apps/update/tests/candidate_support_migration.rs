//! Migrate only a fresh SQLite backup of actual retained15 replacement history.
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
    drain::DrainDeliveryState,
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
    manifest: RuntimeImageEvidence,
    update_id: String,
    replacement_sha256: String,
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
        if name == "installation" || name.starts_with("candidate_support_") {
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
#[ignore = "Requires actual pinned schema15 replacement runtime; copy-only migration"]
fn actual_fifteen_replacement_history_survives_sixteen_and_old_fifteen_refuses() {
    let input: Input = serde_json::from_slice(
        &fs::read(std::env::var_os("WINBRIDGE_CANDIDATE_SUPPORT_MIGRATION_INPUT").unwrap())
            .unwrap(),
    )
    .unwrap();
    let r = &input.reference;
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.kind, "candidate_support_schema15_reference");
    assert_eq!(r.report_sha256.len(), 64);
    assert_eq!(r.proof_sha256.len(), 64);
    assert!(!r.reference_run.is_empty());
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
    assert_eq!(artifact::inspect_image(&manifest_file).unwrap(), r.manifest);
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
    assert_eq!(before.schema_version, 15);
    assert_eq!(before.installation_id, manifest.installation_id);
    assert_eq!(before.scope_sha256, manifest.scope_sha256);
    assert_eq!(
        before.active_update_id.as_deref(),
        Some(r.update_id.as_str())
    );
    let replacement = UpdateJournal::inspect_runtime_replacement(&source, &r.update_id)
        .unwrap()
        .unwrap();
    assert_eq!(replacement.revision, 22);
    let value = serde_json::to_value(&replacement).unwrap();
    assert_eq!(
        sha256(&winbridge_protocol::canonical_evidence_json(&value).unwrap()),
        r.replacement_sha256
    );
    let backup_before = UpdateJournal::inspect_state_backup(&source, &r.update_id).unwrap();
    let shutdown_before = UpdateJournal::inspect_shutdown(&source, &r.update_id).unwrap();
    let preparation_before =
        UpdateJournal::inspect_runtime_preparation(&source, &r.update_id).unwrap();
    let supervisor =
        UpdateJournal::inspect_supervisor(&source, &replacement.intent.preparation.plan.launch_id)
            .unwrap();
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
        let mut destination =
            Connection::open_with_flags(&copy_file, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        rusqlite::backup::Backup::new(&original, &mut destination)
            .unwrap()
            .run_to_completion(64, Duration::from_millis(10), None)
            .unwrap();
    }
    let mut journal = UpdateJournal::open(&copy, &before.scope_sha256).unwrap();
    let after = journal.info().unwrap();
    assert_eq!(after.schema_version, 16);
    assert_eq!(after.owner_epoch, before.owner_epoch + 1);
    assert_eq!(after.installation_id, before.installation_id);
    assert_eq!(after.scope_sha256, before.scope_sha256);
    assert_eq!(after.last_sequence, before.last_sequence);
    assert_eq!(after.active_update_id, before.active_update_id);
    assert_eq!(
        UpdateJournal::inspect_runtime_replacement(&copy, &r.update_id)
            .unwrap()
            .unwrap(),
        replacement
    );
    assert_eq!(
        UpdateJournal::inspect_state_backup(&copy, &r.update_id).unwrap(),
        backup_before
    );
    assert_eq!(
        UpdateJournal::inspect_shutdown(&copy, &r.update_id).unwrap(),
        shutdown_before
    );
    assert_eq!(
        UpdateJournal::inspect_runtime_preparation(&copy, &r.update_id).unwrap(),
        preparation_before
    );
    let inspected =
        Connection::open_with_flags(&copy_file, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows_after = rows(&inspected);
    assert_eq!(rows_after, rows_before);
    let support_rows = [
        "candidate_support_intents",
        "candidate_support_records",
        "candidate_support_history",
    ]
    .map(|table| {
        inspected
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
    });
    assert_eq!(support_rows, [0, 0, 0]);
    assert!(
        UpdateJournal::inspect_candidate_support(&copy, &r.update_id)
            .unwrap()
            .is_none()
    );
    let backups = fs::read_dir(&copy)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("updates-v15-")
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
    let drain = UpdateJournal::inspect_drain(&copy, &r.update_id).unwrap();
    let DrainDeliveryState::Confirmed { receipt, .. } = &drain.commands.last().unwrap().delivery
    else {
        panic!("Original quiescence receipt missing")
    };
    assert!(
        winbridge_update::candidate_support::admit(&mut journal, &owner, &supervisor, receipt)
            .is_err()
    );
    assert!(
        UpdateJournal::inspect_candidate_support(&copy, &r.update_id)
            .unwrap()
            .is_none()
    );
    let view = UpdateJournal::runtime_replacement_snapshot(&copy, &r.update_id, false, None)
        .unwrap()
        .runtime_replacement
        .unwrap();
    assert!(!view.physical_custody_held);
    assert_eq!(view.revision, Some(22));
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
        let output = std::process::Command::new(old_exe)
            .creation_flags(0x08000000)
            .arg("--config")
            .arg(&config)
            .args(["journal", operation])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("UnsupportedSchema"));
        assert_eq!(bytes(&copy), unchanged);
    }
    drop(original);
    assert_eq!(bytes(&source), source_bytes);
    assert_eq!(UpdateJournal::inspect(&source).unwrap(), before);
    assert_eq!(artifact::inspect_image(old_exe).unwrap(), r.image);
    assert_eq!(artifact::inspect_image(&manifest_file).unwrap(), r.manifest);
    let proof = serde_json::json!({"schemaVersion":1,"kind":"candidate_support_schema_migration_proof","passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),
        "referenceProofSha256":r.proof_sha256,"referenceImage":r.image,"referenceManifest":r.manifest,"before":before,"after":after,"backupInfo":backup_info,
        "retainedReplacement":value,"rowsBefore":rows_before,"rowsAfter":rows_after,"backupRows":backup_rows,"supportRows":support_rows,"replacementView":view,
        "oldFifteenInitRefused":true,"oldFifteenStatusRefused":true,"sourceBytesUnchanged":true,"copyBytesUnchangedByOld":true,"supportAbsent":true,"newSupportAttemptDenied":true,"migrationBackupVerified":true,
        "runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input.proof)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}
