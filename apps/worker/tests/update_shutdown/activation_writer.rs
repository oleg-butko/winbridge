//! Main24 writer effect on the original child; no normal service or lease release.
use super::*;
use rusqlite::{Connection, OpenFlags};
use winbridge_platform::{
    infrastructure,
    process_identity::{self, ProcessObservation},
};
use winbridge_update::activation::{
    writer, ActivationRecord, ActivationWriterRecord, ActivationWriterState,
};
use winbridge_worker::store::Store;

pub(super) struct Baseline {
    stale: Store,
    authority: StateBackupAuthority,
    lease: WorkerUpdateLease,
    shutdown: WorkerShutdownReceipt,
    preserved: serde_json::Value,
}
fn connection(root: &Path) -> Connection {
    let c = Connection::open_with_flags(
        root.join("state/journal.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    c
}
// Hash the exact values of every table other than the two deliberately changed
// by activation. Never export private authority IDs, tasks or command payloads.
fn preserved_rows(root: &Path) -> serde_json::Value {
    let mut c = connection(root);
    let tx = c.transaction().unwrap();
    let names: Vec<String> = tx.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT IN ('journal_authority','worker_journal_activations') ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    let mut data = Vec::new();
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut q = tx
            .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
            .unwrap();
        let columns = q.column_count();
        let rows = q
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        data.push((name, format!("{rows:?}")));
    }
    let tasks: i64 = tx
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    let output: i64 = tx
        .query_row("SELECT count(*) FROM output", [], |r| r.get(0))
        .unwrap();
    tx.commit().unwrap();
    json!({"sha256":winbridge_update::sha256(&serde_json::to_vec(&data).unwrap()),"tasks":tasks,"output":output})
}
impl Baseline {
    pub(super) fn capture(root: &Path, id: &str) -> Self {
        let stale = Store::open_read_only(&root.join("state/journal.sqlite")).unwrap();
        let authority = stale.state_backup_authority().unwrap();
        let lease = stale.worker_update_lease(id).unwrap();
        assert_eq!(lease.phase, WorkerUpdatePhase::Quiesced);
        let shutdown = UpdateJournal::inspect_shutdown(&root.join("runtime/update-state"), id)
            .unwrap()
            .unwrap();
        let ShutdownDeliveryState::Exited { receipt, .. } = shutdown.delivery else {
            panic!("Actual accepted shutdown required")
        };
        let shutdown = *receipt;
        assert_eq!(shutdown.lease, lease);
        assert_eq!(
            stale
                .worker_shutdown_receipt(id, &shutdown.command)
                .unwrap()
                .as_ref(),
            Some(&shutdown)
        );
        assert_eq!(
            connection(root)
                .query_row("SELECT count(*) FROM worker_journal_activations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        Self {
            stale,
            authority,
            lease,
            shutdown,
            preserved: preserved_rows(root),
        }
    }
}
fn call(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> Option<UpdatePreparationView> {
    let r: SupervisorResponse = rt
        .block_on(winbridge_transport::supervisor::call_with_timeout(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
            q,
            winbridge_transport::supervisor::CONTROL_TIMEOUT,
        ))
        .unwrap();
    assert_eq!(r.contract_version, 24);
    assert_eq!(r.launch_id, q.launch_id);
    assert!(
        r.error.is_none(),
        "Observe the same attempt, never resend: {:?}",
        r.error
    );
    assert!(r.record.is_none() && r.worker_control.is_none());
    if let Some(v) = &r.preparation {
        assert_eq!(v.schema_version, 16);
        assert_eq!(v.update.update_id, sup.spec.update_id);
    }
    r.preparation
}
fn dispatch_once(rt: &tokio::runtime::Runtime, sup: &SupervisorRecord, q: &SupervisorRequest) {
    let mut stream = rt
        .block_on(winbridge_transport::supervisor::connect(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
        ))
        .unwrap();
    rt.block_on(winbridge_transport::write_frame(
        &mut stream,
        &serde_json::to_vec(q).unwrap(),
    ))
    .unwrap();
    // Deliberately lose only the start ACK; all subsequent requests observe.
    drop(stream);
}
fn history(directory: &Path, id: &str) -> Vec<ActivationWriterRecord> {
    let c = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q = c
        .prepare(
            "SELECT record FROM activation_writer_history WHERE id=?1 ORDER BY revision LIMIT 4",
        )
        .unwrap();
    q.query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}
fn lock_occupied(expected: &WorkerShutdownState) {
    let pins = winbridge_update::shutdown::ShutdownStatePins::capture(Path::new(
        &expected.state_directory,
    ))
    .unwrap();
    assert_eq!(pins.observation(), expected);
    let error = pins
        .try_lock()
        .err()
        .expect("Exact same child must still own instance lock");
    assert!(
        error.kind() == std::io::ErrorKind::WouldBlock
            || error.raw_os_error().is_some()
                && error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
    );
}
fn worker_receipt(
    root: &Path,
    r: &ActivationWriterRecord,
) -> (StateBackupAuthority, WorkerJournalActivationReceipt) {
    let store = Store::open_read_only(&root.join("state/journal.sqlite")).unwrap();
    let receipt = store
        .journal_activation_receipt(&r.command)
        .unwrap()
        .unwrap();
    let authority = store.state_backup_authority().unwrap();
    assert_eq!(receipt.authority, authority);
    assert_eq!(
        connection(root)
            .query_row("SELECT count(*) FROM worker_journal_activations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    (authority, receipt)
}
pub(super) fn execute(
    root: &Path,
    sup: &SupervisorRecord,
    rt: &tokio::runtime::Runtime,
    mut q: SupervisorRequest,
    baseline: Baseline,
    mut proof: serde_json::Value,
) -> serde_json::Value {
    // FIRST action after the StateOwned observation: no hash/DB/legacy audits
    // before this dispatch, and no extension of the original preparation window.
    q.contract_version = 24;
    q.action = SupervisorAction::ActivateJournalWriter;
    dispatch_once(rt, sup, &q);
    q.action = SupervisorAction::ObserveJournalWriter;
    let until = Instant::now() + Duration::from_secs(300);
    let mut busy_observations = 0u64;
    let view = loop {
        if let Some(v) = call(rt, sup, &q) {
            assert!(
                !v.driver_active && v.driver_error.is_none(),
                "{:?}",
                v.driver_error
            );
            assert_eq!(
                v.activation_writer.as_ref().unwrap().phase,
                ActivationWriterPhase::Confirmed
            );
            break v;
        }
        busy_observations += 1;
        assert!(
            Instant::now() < until,
            "Observe the existing writer task; do not start again"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let id = &sup.spec.update_id;
    let directory = root.join("runtime/update-state");
    let w = view.activation_writer.as_ref().unwrap();
    let actual = w.resident_process.as_ref().unwrap();
    assert_eq!(w.process_alive, Some(true));
    assert_eq!(&infrastructure::stamp(actual.process_id).unwrap(), actual);
    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(record.revision, 6);
    let physical = record.state_owned.as_ref().unwrap();
    assert_eq!(&physical.process, actual);
    let actual_job = super::activation_process::job(&record);
    assert!(matches!(
        process_identity::observe_process(&actual_job),
        ProcessObservation::Occupied(_)
    ));
    assert_eq!(infrastructure::job_observation(actual).unwrap(), Some(true));
    lock_occupied(&physical.state);
    let saved = UpdateJournal::inspect_activation_writer(&directory, id)
        .unwrap()
        .unwrap();
    writer::validate_command(&saved.command, &record, &baseline.authority).unwrap();
    let ActivationWriterState::Confirmed { receipt } = &saved.state else {
        panic!("Committed writer receipt required")
    };
    assert_eq!(saved.revision, 1);
    assert_eq!(w.command_id.as_ref(), Some(&saved.command.command_id));
    assert_eq!(w.saved_receipt.as_ref(), Some(receipt));
    assert_eq!(
        w.durable_phase,
        Some(ActivationWriterDurablePhase::Confirmed)
    );
    let stale_error = baseline.stale.check_authority().unwrap_err();
    assert!(stale_error.contains("stale journal incarnation/owner epoch"));
    let (authority, actual_receipt) = worker_receipt(root, &saved);
    assert_eq!(&actual_receipt, receipt);
    assert_eq!(authority.incarnation, baseline.authority.incarnation);
    assert_eq!(authority.epoch, baseline.authority.epoch + 1);
    assert_ne!(
        authority.fingerprint_sha256,
        baseline.authority.fingerprint_sha256
    );
    let current = Store::open_read_only(&root.join("state/journal.sqlite")).unwrap();
    assert_eq!(current.worker_update_lease(id).unwrap(), baseline.lease);
    assert_eq!(
        current
            .worker_shutdown_receipt(id, &baseline.shutdown.command)
            .unwrap()
            .as_ref(),
        Some(&baseline.shutdown)
    );
    let preserved = preserved_rows(root);
    assert_eq!(preserved, baseline.preserved);
    let rows = history(&directory, id);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        ActivationWriterRecord::issued(saved.command.clone()).unwrap()
    );
    rows[0].advance(&rows[1]).unwrap();
    assert_eq!(rows.last(), Some(&saved));
    assert_eq!(view.update.phase, UpdatePhase::VerifyingLocal);
    assert_eq!(
        serde_json::to_value(&view.update).unwrap(),
        proof["owned"]["update"]
    );
    // A second start is an observation after durable confirmation. It must not
    // add another key/history row, child, receipt or epoch.
    q.action = SupervisorAction::ActivateJournalWriter;
    let duplicate = call(rt, sup, &q).expect("Completed original writer must return stable view");
    assert_eq!(duplicate.activation_writer, view.activation_writer);
    assert_eq!(history(&directory, id), rows);
    assert_eq!(
        worker_receipt(root, &saved),
        (authority.clone(), actual_receipt.clone())
    );
    let mut old = q.clone();
    old.contract_version = 23;
    old.action = SupervisorAction::ObserveActivationState;
    let legacy_state = super::activation_process::call(rt, sup, &old);
    assert_eq!(legacy_state.schema_version, 15);
    assert!(serde_json::to_value(&legacy_state)
        .unwrap()
        .get("activationWriter")
        .is_none());
    assert_eq!(
        legacy_state.activation_state.as_ref().unwrap().phase,
        ActivationStatePhase::Indeterminate
    );
    old.contract_version = 22;
    old.action = SupervisorAction::ObserveActivationProcess;
    let legacy_process = super::activation_process::call(rt, sup, &old);
    assert_eq!(legacy_process.schema_version, 14);
    assert!(serde_json::to_value(&legacy_process)
        .unwrap()
        .get("activationWriter")
        .is_none());
    assert_eq!(
        legacy_process.activation_process.as_ref().unwrap().phase,
        ActivationProcessPhase::Indeterminate
    );
    let source = &record.intent.source;
    let loaded = winbridge_update::installed_set::pin_installed_set(
        &root.join("runtime"),
        &source.installation_id,
        &source.scope_sha256,
    )
    .unwrap();
    loaded.revalidate().unwrap();
    let set = loaded.observation();
    let mut sharing = Vec::new();
    for image in set.images.iter().chain(std::iter::once(&set.manifest_file)) {
        assert!(fs::OpenOptions::new()
            .write(true)
            .open(&image.path)
            .is_err());
        assert!(fs::remove_file(&image.path).is_err());
        sharing.push(json!({"image":image,"writeDenied":true,"deleteDenied":true}));
    }
    let physical_history = super::activation_preparation::history(&directory, id);
    assert_eq!(physical_history.len(), 7);
    assert_eq!(physical_history.last(), Some(&record));
    let health = UpdateJournal::inspect_health(&directory, id)
        .unwrap()
        .unwrap();
    let held: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("activation-state-before_release.json")).unwrap(),
    )
    .unwrap();
    let released: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("activation-state-after_release.json")).unwrap(),
    )
    .unwrap();
    let before_send: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("activation-writer-before-send.json")).unwrap())
            .unwrap();
    assert_eq!(before_send["commandId"], saved.command.command_id);
    assert_eq!(
        before_send["commandSha256"],
        writer::command_sha256(&saved.command).unwrap()
    );
    assert_eq!(
        before_send["record"],
        serde_json::to_value(&record).unwrap()
    );
    lock_occupied(&physical.state);
    assert_eq!(
        worker_receipt(root, &saved),
        (authority.clone(), actual_receipt.clone())
    );
    proof.as_object_mut().unwrap().extend(json!({"confirmed":view,"duplicate":duplicate,
        "legacyState":legacy_state,"legacyProcess":legacy_process,"record":record,"history":physical_history,
        "writerRecord":saved,"writerHistory":rows,"workerAuthorityBefore":baseline.authority,
        "workerAuthorityAfter":authority,"actualWorkerReceipt":actual_receipt,
        "workerLeaseBefore":baseline.lease,"workerLeaseAfter":current.worker_update_lease(id).unwrap(),
        "acceptedShutdownBefore":baseline.shutdown,"acceptedShutdownAfter":current.worker_shutdown_receipt(id,&baseline.shutdown.command).unwrap(),
        "preservedRowsBefore":baseline.preserved,"preservedRowsAfter":preserved,"oldCachedAuthorityRefused":true,
        "actualProcess":actual,"actualJob":actual_job,"actualJobOccupied":true,"actualProcessInJob":true,
        "actualSet":set,"sharing":sharing,"childInstanceLockOccupied":true,"beforeRelease":held,"afterRelease":released,"beforeSend":before_send,
        "health":health,"healthEncoded":String::from_utf8(serde_json::to_vec(&health).unwrap()).unwrap(),"supervisor":sup,
        "workerBytesAfter":journal_bytes(root),"lostWriterAckSentOnce":true,"busyObservations":busy_observations,
        "workerLaunchCount":1,"stateHandoffCount":1,"savedWorkerCommandCount":1,"lostHandoffAckSentOnce":true,"normalServiceStarted":false,
        "publicActivationEnabled":false}).as_object().unwrap().clone());
    proof
}
pub(super) fn after_owned_exit(
    root: &Path,
    id: &str,
    saved: &serde_json::Value,
    pin: &infrastructure::PinnedProcess,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let record: ActivationRecord = serde_json::from_value(saved["record"].clone()).unwrap();
    let writer: ActivationWriterRecord =
        serde_json::from_value(saved["writerRecord"].clone()).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while !pin.exited().unwrap() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        pin.exited().unwrap(),
        "Retained actual receiver handle must be signalled"
    );
    assert!(matches!(
        process_identity::observe_process(&super::activation_process::job(&record)),
        ProcessObservation::Empty(_)
    ));
    let view = UpdateJournal::activation_writer_snapshot(&directory, id, false, None).unwrap();
    let w = view.activation_writer.as_ref().unwrap();
    assert_eq!(w.phase, ActivationWriterPhase::NotResident);
    assert_eq!(
        w.saved_receipt.as_ref(),
        Some(&worker_receipt(root, &writer).1)
    );
    assert!(w.resident_process.is_none() && w.process_alive.is_none());
    let pins = winbridge_update::shutdown::ShutdownStatePins::capture(Path::new(
        &record.intent.context.worker_state.state_directory,
    ))
    .unwrap();
    assert_eq!(pins.observation(), &record.intent.context.worker_state);
    let probe = pins.try_lock().unwrap();
    probe.revalidate().unwrap();
    drop(probe);
    let rows = history(&directory, id);
    let physical_history = super::activation_preparation::history(&directory, id);
    assert_eq!(
        serde_json::to_value(&physical_history).unwrap(),
        saved["history"]
    );
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&record)
    );
    assert_eq!(serde_json::to_value(&rows).unwrap(), saved["writerHistory"]);
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation_writer(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        saved["writerRecord"]
    );
    let (authority, receipt) = worker_receipt(root, &writer);
    assert_eq!(
        serde_json::to_value(&authority).unwrap(),
        saved["workerAuthorityAfter"]
    );
    assert_eq!(
        serde_json::to_value(&receipt).unwrap(),
        saved["actualWorkerReceipt"]
    );
    assert_eq!(preserved_rows(root), saved["preservedRowsBefore"]);
    assert_eq!(journal_bytes(root), saved["workerBytesAfter"]);
    json!({"view":view,"record":record,"history":physical_history,"writerRecord":writer,"writerHistory":rows,"workerAuthority":authority,"actualWorkerReceipt":receipt,
        "actualRetainedProcessExited":true,"ownedJobEmpty":true,"activationJobEmpty":true,"sameInstanceLockAvailable":true,
        "observedState":record.intent.context.worker_state,"workerBytesAfter":journal_bytes(root),"preservedRowsAfter":preserved_rows(root),
        "workerLaunchCount":1,"stateHandoffCount":1,"savedWorkerCommandCount":1,"automaticColdLaunchCount":0,"normalServiceStarted":false})
}
