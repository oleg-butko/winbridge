//! Expected faults in owned test namespaces; never installed update/recovery.
use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReplacementFault {
    #[default]
    None,
    ExistingDirectory,
    LockedOldFile,
    ReceiptWriteDenied,
    HoldBeforeReceipt,
}

fn actual_files(
    r: &RuntimeReplacementRecord,
    fault: ReplacementFault,
) -> (Vec<ImageEvidence>, Vec<ImageEvidence>) {
    let mut old = Vec::new();
    let mut prepared = Vec::new();
    for (index, file) in r.batch.files.iter().enumerate() {
        let mut expected = file.spec.old.clone();
        if matches!(
            fault,
            ReplacementFault::ReceiptWriteDenied | ReplacementFault::HoldBeforeReceipt
        ) && index == 0
        {
            expected.path = file.spec.retained_path().to_str().unwrap().into();
            assert!(!Path::new(&file.spec.old.path).exists());
        }
        let observed = observe_owned_file(Path::new(&expected.path));
        assert_eq!(observed, expected);
        old.push(observed);
        let observed = observe_owned_file(Path::new(&file.spec.prepared.path));
        assert_eq!(observed, file.spec.prepared);
        prepared.push(observed);
    }
    assert_eq!(
        fs::read_dir(r.intent.preparation.plan.directory())
            .unwrap()
            .count(),
        5
    );
    if fault != ReplacementFault::ExistingDirectory {
        assert_eq!(
            r.intent.retained_directory().exists(),
            matches!(
                fault,
                ReplacementFault::ReceiptWriteDenied | ReplacementFault::HoldBeforeReceipt
            )
        );
    }
    (old, prepared)
}

pub(crate) fn execute_fault(
    root: &Path,
    sup: &SupervisorRecord,
    before: &serde_json::Value,
    fault: ReplacementFault,
) -> serde_json::Value {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = root.join("runtime/update-state");
    let id = &sup.spec.update_id;
    let preparation = UpdateJournal::inspect_runtime_preparation(&directory, id)
        .unwrap()
        .unwrap();
    let displaced = preparation
        .plan
        .directory()
        .with_file_name("displaced-runtime");
    let bytes = journal_bytes(root);
    let sentinel = if fault == ReplacementFault::ExistingDirectory {
        fs::create_dir(&displaced).unwrap();
        let path = displaced.join("preserve-sentinel.txt");
        fs::write(&path, b"preserve unknown displaced directory").unwrap();
        Some(inspect_image(&path).unwrap())
    } else {
        None
    };
    let blocked = if fault == ReplacementFault::LockedOldFile {
        Some(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&preparation.plan.files[0].old.path)
                .unwrap(),
        )
    } else {
        None
    };
    if fault == ReplacementFault::ReceiptWriteDenied {
        // Deny the first retired receipt, NOT its prior durable issue or the
        // subsequent uncertainty append. No worker DB/config/user files change.
        let c = rusqlite::Connection::open_with_flags(
            directory.join("updates.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        c.execute_batch("CREATE TRIGGER deny_replacement_retired_receipt BEFORE INSERT ON runtime_replacement_history WHEN NEW.revision=4 AND json_extract(NEW.record,'$.state.phase')='replacing' BEGIN SELECT RAISE(IGNORE);END;").unwrap();
    }
    let validation_input = if fault == ReplacementFault::HoldBeforeReceipt {
        let p = &preparation.plan;
        let path = displaced.with_file_name("runtime-replacement-validation.json");
        write(
            &path,
            &json!({"schemaVersion":1,"kind":"winbridge_runtime_replacement_validation",
            "updateId":p.update_id,"launchId":p.launch_id,"ownerId":p.owner_id,"ownerEpoch":p.owner_epoch,
            "configSha256":p.config_sha256,"runtimeIdentity":p.runtime_identity,"preparationAttemptId":p.attempt_id}),
        );
        Some(fs::read_to_string(path).unwrap())
    } else {
        None
    };
    let mut request: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    request.contract_version = 17;
    request.action = SupervisorAction::ObserveRuntimeReplacement;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let call = |q: &SupervisorRequest| -> UpdatePreparationView {
        let response: SupervisorResponse = rt
            .block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                sup.process.as_ref().unwrap().process_id,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ))
            .unwrap();
        assert_eq!(response.contract_version, 17);
        assert!(response.error.is_none(), "{:?}", response.error);
        let view = response.preparation.unwrap();
        assert_eq!(view.schema_version, 9);
        assert_eq!(view.update.update_id, *id);
        view
    };
    let initial = call(&request);
    assert!(!initial.driver_active && initial.driver_error.is_none());
    assert_eq!(
        serde_json::to_value(&initial.update).unwrap(),
        before["root"]
    );
    assert!(
        initial
            .candidate_shutdown
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    assert_eq!(
        initial.runtime_replacement.as_ref().unwrap().phase,
        RuntimeReplacementPhase::NotAdmitted
    );
    request.action = SupervisorAction::ReplaceRuntime;
    let admitted = call(&request);
    let attempt = admitted
        .runtime_replacement
        .as_ref()
        .unwrap()
        .attempt_id
        .clone()
        .unwrap();
    request.action = SupervisorAction::ObserveRuntimeReplacement;
    let until = Instant::now() + Duration::from_secs(600);
    let mut held_marker = None;
    let done = loop {
        let view = call(&request);
        assert_eq!(
            view.runtime_replacement
                .as_ref()
                .unwrap()
                .attempt_id
                .as_deref(),
            Some(attempt.as_str())
        );
        if fault == ReplacementFault::HoldBeforeReceipt {
            let path = displaced.with_file_name("runtime-replacement-held.json");
            if let Some(marker) = fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            {
                assert!(view.driver_active);
                assert_eq!(view.runtime_replacement.as_ref().unwrap().revision, Some(3));
                assert!(
                    view.runtime_replacement
                        .as_ref()
                        .unwrap()
                        .physical_custody_held
                );
                held_marker = Some(marker);
                break view;
            }
        }
        if !view.driver_active {
            break view;
        }
        assert!(
            Instant::now() < until,
            "Fault observation deadline; preserve original attempt"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let r = UpdateJournal::inspect_runtime_replacement(&directory, id)
        .unwrap()
        .unwrap();
    if fault == ReplacementFault::HoldBeforeReceipt {
        assert_eq!(r.revision, 3);
        assert!(matches!(r.state, RuntimeReplacementState::Replacing { .. }));
        assert!(matches!(
            r.batch.files[0].state,
            FileExchangeState::RetireIssued {}
        ));
        assert!(done.driver_active && done.driver_error.is_none());
        let marker = held_marker.as_ref().unwrap();
        assert_eq!(marker["record"], serde_json::to_value(&r).unwrap());
        assert_eq!(
            marker["processId"],
            sup.process.as_ref().unwrap().process_id
        );
        assert_eq!(
            marker["inputSha256"],
            winbridge_update::sha256(validation_input.as_ref().unwrap().as_bytes())
        );
        let next: RuntimeReplacementRecord =
            serde_json::from_value(marker["afterEffect"].clone()).unwrap();
        r.advance(&next).unwrap();
        assert_eq!(next.revision, 4);
        assert!(matches!(
            next.batch.files[0].state,
            FileExchangeState::Retired { .. }
        ));
    } else {
        let RuntimeReplacementState::Indeterminate { previous, reason } = &r.state else {
            panic!("Expected fault did not retain uncertainty")
        };
        assert!(done.driver_error.is_some());
        assert_eq!(done.update, initial.update);
        assert_eq!(done.update.phase, UpdatePhase::Replacing);
        assert_eq!(
            done.runtime_replacement.as_ref().unwrap().phase,
            RuntimeReplacementPhase::Indeterminate
        );
        assert!(
            done.runtime_replacement
                .as_ref()
                .unwrap()
                .physical_custody_held
        );
        let expected = match fault {
            ReplacementFault::ExistingDirectory => "Displaced runtime directory already exists",
            ReplacementFault::LockedOldFile => "os error 32",
            ReplacementFault::ReceiptWriteDenied => "Required update write was not applied",
            ReplacementFault::None | ReplacementFault::HoldBeforeReceipt => {
                panic!("Explicit fault required")
            }
        };
        assert!(
            reason.contains(expected),
            "Unexpected fault cause: {reason}"
        );
        if fault == ReplacementFault::ReceiptWriteDenied {
            assert_eq!(r.revision, 4);
            assert!(matches!(
                previous.as_ref(),
                RuntimeReplacementState::Replacing { .. }
            ));
            assert!(matches!(
                r.batch.files[0].state,
                FileExchangeState::RetireIssued {}
            ));
        } else {
            assert_eq!(r.revision, 1);
            assert!(matches!(
                previous.as_ref(),
                RuntimeReplacementState::Planned {}
            ));
        }
    }
    let records = history(&directory, id);
    assert_eq!(records.len() as u64, r.revision + 1);
    for pair in records.windows(2) {
        pair[0].advance(&pair[1]).unwrap();
    }
    let files = actual_files(&r, fault);
    let preservation = preserved(root, before);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    let err = fs2::FileExt::try_lock_exclusive(&lock).unwrap_err();
    assert!(
        err.kind() == std::io::ErrorKind::WouldBlock
            || err.raw_os_error() == fs2::lock_contended_error().raw_os_error()
    );
    // Removing the original cause does not mint a second permission or replay
    // even one rename. The terminal record and physical set must stay identical.
    drop(blocked);
    if fault == ReplacementFault::ReceiptWriteDenied {
        let c = rusqlite::Connection::open_with_flags(
            directory.join("updates.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        c.execute_batch("DROP TRIGGER deny_replacement_retired_receipt")
            .unwrap();
    }
    request.action = SupervisorAction::ReplaceRuntime;
    let duplicate = call(&request);
    assert_eq!(duplicate.runtime_replacement, done.runtime_replacement);
    assert_eq!(
        UpdateJournal::inspect_runtime_replacement(&directory, id)
            .unwrap()
            .unwrap(),
        r
    );
    assert_eq!(history(&directory, id), records);
    assert_eq!(actual_files(&r, fault), files);
    assert_eq!(journal_bytes(root), bytes);
    if let Some(sentinel) = &sentinel {
        assert_eq!(inspect_image(Path::new(&sentinel.path)).unwrap(), *sentinel);
    }
    json!({"expectedFault":fault,"beforeAdmission":initial,"admitted":admitted,"view":done,"duplicate":duplicate,
        "record":r,"history":records,"actualOldFiles":files.0,"actualPreparedFiles":files.1,"preservation":preservation,
        "sentinel":sentinel,"heldMarker":held_marker,"validationInputJson":validation_input,"workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),
        "causeRemovedBeforeDuplicate":matches!(fault, ReplacementFault::LockedOldFile | ReplacementFault::ReceiptWriteDenied),"duplicateNoNewAttempt":true,
        "originalInstanceLockHeld":true,"preparedDirectoryComplete":true,"activationStarted":false,"installedRuntimeApplied":false})
}

pub(crate) fn after_fault_owned_exit(
    root: &Path,
    config: &Path,
    worker: &Path,
    id: &str,
    proof: &serde_json::Value,
    fault: ReplacementFault,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let (saved, _) =
        observe_after_owned_exit(|| UpdateJournal::inspect_runtime_replacement(&directory, id))
            .unwrap();
    let r = saved.unwrap();
    assert_eq!(serde_json::to_value(&r).unwrap(), proof["record"]);
    let view = UpdateJournal::runtime_replacement_snapshot(&directory, id, false, None).unwrap();
    assert_eq!(view.update.phase, UpdatePhase::Replacing);
    assert_eq!(
        view.runtime_replacement.as_ref().unwrap().phase,
        RuntimeReplacementPhase::Indeterminate
    );
    assert!(
        !view
            .runtime_replacement
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    let files = actual_files(&r, fault);
    assert_eq!(
        serde_json::to_value(files.0).unwrap(),
        proof["actualOldFiles"]
    );
    assert_eq!(
        serde_json::to_value(files.1).unwrap(),
        proof["actualPreparedFiles"]
    );
    let sentinel: Option<ImageEvidence> =
        serde_json::from_value(proof["sentinel"].clone()).unwrap();
    if let Some(sentinel) = &sentinel {
        assert_eq!(inspect_image(Path::new(&sentinel.path)).unwrap(), *sentinel);
    }
    let bytes = journal_bytes(root);
    let path = if matches!(
        fault,
        ReplacementFault::ReceiptWriteDenied | ReplacementFault::HoldBeforeReceipt
    ) {
        // First old worker was retired without a committed receipt. Even trying
        // this legacy-looking retained executable must hit the DB barrier.
        r.batch.files[0].spec.retained_path()
    } else {
        worker.to_owned()
    };
    restart_refused(&path, config, root, id);
    assert_eq!(journal_bytes(root), bytes);
    assert_eq!(bytes, proof["workerBytesBefore"]);
    json!({"record":r,"view":view,"sentinel":sentinel,"normalStartupRefused":true,"workerBytesUnchanged":true,"ownedJobEmpty":true,"activationStarted":false})
}
