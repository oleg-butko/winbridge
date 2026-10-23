//! Native effects in this test's own runtime only. Never an installed cutover.
#[path = "replacement/faults.rs"]
mod faults;
use super::*;
pub(super) use faults::{after_fault_owned_exit, execute_fault, ReplacementFault};
use winbridge_update::{
    artifact::ImageEvidence,
    runtime_exchange::FileExchangeState,
    runtime_replacement::{RuntimeReplacementRecord, RuntimeReplacementState},
};

/// Observation is not a pin/permission. Share DELETE so this read can coexist
/// with the supervisor's original READ+DELETE handle. Its deny-write/delete
/// policy remains authoritative; never weaken artifact::pin for this test.
fn observe_owned_file(path: &Path) -> ImageEvidence {
    use sha2::{Digest, Sha256};
    use std::os::windows::fs::OpenOptionsExt;
    winbridge_platform::require_safe_path(path).unwrap();
    let mut options = fs::OpenOptions::new();
    options.read(true).share_mode(1 | 4); // FILE_SHARE_READ | FILE_SHARE_DELETE
    let mut file = options.open(path).unwrap();
    let meta = file.metadata().unwrap();
    assert!(meta.is_file() && meta.len() > 0 && meta.len() <= 256 * 1024 * 1024);
    let identity = winbridge_platform::file_identity(&file).unwrap();
    let mut digest = Sha256::new();
    let mut count = 0;
    let mut buffer = [0; 65536];
    let mut tail = Vec::new();
    let mut supported = false;
    let marker = winbridge_protocol::ARTIFACT_QUERY_MARKER;
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        count += n as u64;
        assert!(count <= meta.len());
        digest.update(&buffer[..n]);
        if !supported {
            tail.extend_from_slice(&buffer[..n]);
            supported = tail.windows(marker.len()).any(|w| w == marker);
            if tail.len() >= marker.len() {
                tail.drain(..tail.len() - (marker.len() - 1));
            }
        }
    }
    assert_eq!(count, meta.len());
    assert_eq!(file.metadata().unwrap().len(), count);
    assert_eq!(
        winbridge_platform::file_identity(&options.open(path).unwrap()).unwrap(),
        identity
    );
    winbridge_platform::require_safe_path(path).unwrap();
    ImageEvidence {
        path: path.canonicalize().unwrap().to_str().unwrap().into(),
        bytes: count,
        sha256: format!("{:x}", digest.finalize()),
        file_identity: identity,
        query_supported: supported,
    }
}

#[test]
fn observer_reads_owned_file_without_weakening_owner() {
    use winbridge_platform::runtime_files::OwnedRuntimeFile;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.exe");
    fs::write(
        &path,
        [winbridge_protocol::ARTIFACT_QUERY_MARKER, b"preserve bytes"].concat(),
    )
    .unwrap();
    let expected = inspect_image(&path).unwrap();
    let mut owner = OwnedRuntimeFile::acquire(
        &path,
        &expected.file_identity,
        expected.bytes,
        &expected.sha256,
    )
    .unwrap();
    assert!(
        inspect_image(&path).is_err(),
        "Strict pin must still refuse concurrent DELETE ownership"
    );
    assert_eq!(observe_owned_file(&path), expected);
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
    let moved = temp.path().join("new.exe");
    owner
        .move_new(
            &moved,
            &winbridge_platform::source_files::directory_identity(temp.path()).unwrap(),
        )
        .unwrap();
    let mut expected = expected;
    expected.path = moved.canonicalize().unwrap().to_str().unwrap().into();
    assert_eq!(observe_owned_file(&moved), expected);
    assert!(fs::OpenOptions::new().write(true).open(&moved).is_err());
    assert!(fs::remove_file(&moved).is_err());
    drop(owner);
    assert_eq!(inspect_image(&moved).unwrap(), expected);
}

fn history(directory: &Path, id: &str) -> Vec<RuntimeReplacementRecord> {
    let c = rusqlite::Connection::open_with_flags(
        directory.join("updates.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = c
        .prepare(
            "SELECT record FROM runtime_replacement_history WHERE id=?1 ORDER BY revision LIMIT 25",
        )
        .unwrap();
    statement
        .query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}
fn retained_files(r: &RuntimeReplacementRecord) -> (Vec<ImageEvidence>, Vec<ImageEvidence>) {
    let mut old = Vec::new();
    let mut new = Vec::new();
    for file in &r.batch.files {
        let FileExchangeState::Installed {
            retained,
            installed,
        } = &file.state
        else {
            panic!("Missing native move receipt");
        };
        assert_eq!(observe_owned_file(Path::new(&retained.path)), *retained);
        assert_eq!(observe_owned_file(Path::new(&installed.path)), *installed);
        assert_eq!(retained.file_identity, file.spec.old.file_identity);
        assert_eq!(installed.file_identity, file.spec.prepared.file_identity);
        old.push(retained.clone());
        new.push(installed.clone());
    }
    assert_eq!(
        fs::read_dir(r.intent.preparation.plan.directory())
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(r.intent.retained_directory()).unwrap().count(),
        5
    );
    let RuntimeReplacementState::FilesMoved { directory_identity } = &r.state else {
        panic!("Files not moved")
    };
    assert_eq!(
        winbridge_platform::source_files::directory_identity(&r.intent.retained_directory())
            .unwrap(),
        *directory_identity
    );
    (old, new)
}
fn preserved(root: &Path, before: &serde_json::Value) -> serde_json::Value {
    let images: Vec<ImageEvidence> =
        serde_json::from_value(before["runtimePreparation"]["sourceFiles"].clone()).unwrap();
    for image in &images {
        assert_eq!(inspect_image(Path::new(&image.path)).unwrap(), *image);
    }
    let preimages = UpdateJournal::inspect_runtime_preimages(
        &root.join("runtime/update-state"),
        before["root"]["updateId"].as_str().unwrap(),
    )
    .unwrap()
    .receipt
    .unwrap();
    for image in &preimages.files {
        assert_eq!(inspect_image(Path::new(&image.path)).unwrap(), *image);
    }
    let backup: winbridge_update::state_backup::StateBackupRecord =
        serde_json::from_value(before["finalStateBackup"]["record"].clone()).unwrap();
    let image = backup.report.unwrap().backup;
    assert_eq!(
        winbridge_update::state_backup::pin_backup_file(Path::new(&image.path))
            .unwrap()
            .evidence(),
        &image
    );
    json!({"sourceFiles":images,"preimages":preimages,"backupFile":image})
}
pub(super) fn execute(
    root: &Path,
    sup: &SupervisorRecord,
    before: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let id = &sup.spec.update_id;
    let bytes = journal_bytes(root);
    let mut request: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    request.contract_version = 17;
    request.action = SupervisorAction::ObserveRuntimeReplacement;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let call = |q: &SupervisorRequest| -> SupervisorResponse {
        let response: SupervisorResponse = rt
            .block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                sup.process.as_ref().unwrap().process_id,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ))
            .unwrap();
        assert_eq!(response.contract_version, 17);
        response
    };
    let view = |q: &SupervisorRequest| -> UpdatePreparationView {
        let response = call(q);
        assert!(response.error.is_none(), "{:?}", response.error);
        let v = response.preparation.unwrap();
        assert_eq!(v.schema_version, 9);
        assert_eq!(v.update.update_id, *id);
        v
    };
    let initial = view(&request);
    assert!(!initial.driver_active);
    assert_eq!(
        initial.runtime_replacement.as_ref().unwrap().phase,
        RuntimeReplacementPhase::NotAdmitted
    );
    assert!(
        initial
            .candidate_shutdown
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    let mut invalid = request.clone();
    invalid.contract_version = 16;
    invalid.action = SupervisorAction::ReplaceRuntime;
    assert!(call(&invalid).error.is_some());
    assert!(UpdateJournal::inspect_runtime_replacement(&directory, id)
        .unwrap()
        .is_none());
    request.action = SupervisorAction::ReplaceRuntime;
    let admitted = view(&request);
    let attempt = admitted
        .runtime_replacement
        .as_ref()
        .unwrap()
        .attempt_id
        .clone()
        .unwrap();
    let saved = UpdateJournal::inspect_runtime_replacement(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.intent.attempt_id, attempt,
        "Admission must precede ACK"
    );
    assert_eq!(
        serde_json::to_value(&saved.intent.preparation).unwrap(),
        before["runtimePreparation"]["record"]
    );
    let duplicate_active = view(&request);
    assert_eq!(
        duplicate_active
            .runtime_replacement
            .as_ref()
            .unwrap()
            .attempt_id
            .as_deref(),
        Some(attempt.as_str())
    );
    request.action = SupervisorAction::ObserveRuntimeReplacement;
    let until = Instant::now() + Duration::from_secs(600);
    let mut revisions = Vec::new();
    let done = loop {
        let v = view(&request);
        assert!(
            v.driver_error.is_none(),
            "Replacement stopped: {:?}",
            v.driver_error
        );
        let p = v.runtime_replacement.as_ref().unwrap();
        assert_eq!(p.attempt_id.as_deref(), Some(attempt.as_str()));
        if revisions.last() != Some(&p.revision.unwrap()) {
            eprintln!(
                "Replacement revision {:?}, phase {:?}, installed {}/{}",
                p.revision, p.phase, p.installed_files, p.total_files
            );
            revisions.push(p.revision.unwrap());
        }
        if !v.driver_active && p.phase == RuntimeReplacementPhase::FilesMoved {
            break v;
        }
        assert!(
            Instant::now() < until,
            "Replacement observation deadline; preserve original attempt"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let r = UpdateJournal::inspect_runtime_replacement(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(r.revision, 22);
    assert!(matches!(
        r.state,
        RuntimeReplacementState::FilesMoved { .. }
    ));
    assert!(
        done.runtime_replacement
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    assert_eq!(done.update.phase, UpdatePhase::VerifyingLocal);
    assert_eq!(done.update.revision, initial.update.revision + 1);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    let error = fs2::FileExt::try_lock_exclusive(&lock).unwrap_err();
    assert!(
        error.kind() == std::io::ErrorKind::WouldBlock
            || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
    );
    let records = history(&directory, id);
    assert_eq!(records.len(), 23);
    for (i, entry) in records.iter().enumerate() {
        assert_eq!(entry.revision, i as u64);
        if i > 0 {
            records[i - 1].advance(entry).unwrap();
        }
    }
    assert_eq!(records.last(), Some(&r));
    let (old, new) = retained_files(&r);
    let actual_manifest: InstalledSetManifest =
        serde_json::from_slice(&fs::read(root.join("runtime").join(MANIFEST_NAME)).unwrap())
            .unwrap();
    assert_eq!(actual_manifest, r.intent.preparation.plan.manifest);
    let preservation = preserved(root, before);
    assert_eq!(journal_bytes(root), bytes);
    request.action = SupervisorAction::ReplaceRuntime;
    let duplicate = view(&request);
    assert_eq!(duplicate.runtime_replacement, done.runtime_replacement);
    assert_eq!(
        UpdateJournal::inspect_runtime_replacement(&directory, id)
            .unwrap()
            .unwrap(),
        r
    );
    assert_eq!(history(&directory, id), records);
    let mut legacy = request;
    legacy.contract_version = 16;
    legacy.action = SupervisorAction::ObserveRuntimeFiles;
    let legacy: serde_json::Value = rt
        .block_on(winbridge_transport::supervisor::call_with_timeout(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
            &legacy,
            winbridge_transport::supervisor::CONTROL_TIMEOUT,
        ))
        .unwrap();
    assert!(legacy["error"].is_null());
    assert!(legacy["preparation"].get("runtimeReplacement").is_none());
    json!({"beforeAdmission":initial,"admitted":admitted,"duplicateActive":duplicate_active,"view":done,"duplicate":duplicate,
        "record":r,"history":records,"observedRevisions":revisions,"actualRetainedFiles":old,"actualInstalledFiles":new,"preservation":preservation,
        "actualManifest":actual_manifest,"workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),
        "wrongVersionRefused":true,"intentBeforeAck":true,"originalInstanceLockHeld":true,"oldViewsOmitReplacement":true,
        "preparedDirectoryEmpty":true,"sourcePayloadUnchanged":true,"preimagesUnchanged":true,"finalBackupUnchanged":true,
        "activationStarted":false,"installedRuntimeApplied":false})
}
pub(super) fn after_owned_exit(
    root: &Path,
    config: &Path,
    worker: &Path,
    id: &str,
    proof: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let (r, _) =
        observe_after_owned_exit(|| UpdateJournal::inspect_runtime_replacement(&directory, id))
            .unwrap();
    let r = r.unwrap();
    assert_eq!(serde_json::to_value(&r).unwrap(), proof["record"]);
    let view = UpdateJournal::runtime_replacement_snapshot(&directory, id, false, None).unwrap();
    assert_eq!(view.update.phase, UpdatePhase::VerifyingLocal);
    assert_eq!(
        view.runtime_replacement.as_ref().unwrap().phase,
        RuntimeReplacementPhase::FilesMoved
    );
    assert!(
        !view
            .runtime_replacement
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    let actual = retained_files(&r);
    assert_eq!(
        serde_json::to_value(actual.0).unwrap(),
        proof["actualRetainedFiles"]
    );
    assert_eq!(
        serde_json::to_value(actual.1).unwrap(),
        proof["actualInstalledFiles"]
    );
    let bytes = journal_bytes(root);
    restart_refused(worker, config, root, id);
    assert_eq!(journal_bytes(root), bytes);
    assert_eq!(bytes, proof["workerBytesBefore"]);
    json!({"record":r,"view":view,"normalStartupRefused":true,"workerBytesUnchanged":true,"ownedJobEmpty":true,"activationStarted":false})
}
