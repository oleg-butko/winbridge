//! Actual actor22 Main: durable intent, suspended PID, private Ready, no Store.
use super::*;
use winbridge_platform::{
    infrastructure,
    process_identity::{self, ProcessIdentity, ProcessObservation},
};
use winbridge_update::activation::{ActivationRecord, ActivationState};
fn call(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> UpdatePreparationView {
    let response: SupervisorResponse = rt
        .block_on(winbridge_transport::supervisor::call_with_timeout(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
            q,
            winbridge_transport::supervisor::CONTROL_TIMEOUT,
        ))
        .unwrap();
    assert_eq!(response.contract_version, q.contract_version);
    assert!(response.error.is_none(), "{:?}", response.error);
    response.preparation.unwrap()
}
fn job(record: &ActivationRecord) -> ProcessIdentity {
    let actual = record.created.as_ref().map(|v| &v.process);
    ProcessIdentity {
        job_name: record.intent.job_name().unwrap(),
        host_id: record.intent.context.supervisor.host_id.clone(),
        session_id: record.intent.context.supervisor.session_id,
        process_id: actual.map_or(0, |p| p.process_id),
        creation_time: actual.map_or(0, |p| p.birth.parse().unwrap()),
    }
}
fn marker(
    root: &Path,
    phase: &str,
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> serde_json::Value {
    let path = root.join(format!("activation-process-{phase}.json"));
    let until = Instant::now() + Duration::from_secs(90);
    let mut poll = Instant::now();
    loop {
        if let Ok(raw) = fs::read(&path) {
            if let Ok(value) = serde_json::from_slice(&raw) {
                return value;
            }
        }
        if poll.elapsed() > Duration::from_secs(1) {
            let v = call(rt, sup, q);
            assert!(
                v.driver_error.is_none(),
                "Process stopped: {:?}; stderr: {:?}",
                v.driver_error,
                v.activation_process
                    .as_ref()
                    .and_then(|p| p.stderr_tail.as_ref())
            );
            poll = Instant::now();
        }
        assert!(
            Instant::now() < until,
            "Observe existing barrier, never repeat launch"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn release(root: &Path, phase: &str, held: &serde_json::Value) {
    let destination = root.join(format!("activation-process-{phase}-release.json"));
    let staging = destination.with_extension("part");
    assert!(!destination.exists());
    write(
        &staging,
        &json!({"schemaVersion":1,"kind":"winbridge_activation_process_release",
        "phase":phase,"attemptId":held["record"]["intent"]["attemptId"],"inputSha256":held["inputSha256"],"recordSha256":held["recordSha256"]}),
    );
    fs::rename(staging, destination).unwrap();
}

fn locked(root: &Path) {
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&lock).is_err());
}
pub(super) fn execute(
    root: &Path,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
) -> serde_json::Value {
    let timer = Instant::now();
    let directory = root.join("runtime/update-state");
    let id = &sup.spec.update_id;
    let bytes = journal_bytes(root);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 20;
    q.action = SupervisorAction::ObserveActivationPreparation;
    let before = call(&rt, sup, &q);
    assert_eq!(
        before.activation_preparation.as_ref().unwrap().phase,
        ActivationPreparationPhase::NotPrepared
    );
    q.action = SupervisorAction::PrepareActivation;
    let prepared_view = call(&rt, sup, &q);
    let until = Instant::now() + Duration::from_secs(90);
    let prepared_view = if prepared_view.driver_active {
        q.action = SupervisorAction::ObserveActivationPreparation;
        loop {
            let v = call(&rt, sup, &q);
            assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
            if !v.driver_active {
                break v;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
    } else {
        prepared_view
    };
    let prepared = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.state, ActivationState::Prepared {});
    assert_eq!(prepared.revision, 0);
    let source = &prepared.intent.source;
    write(
        &root.join("activation-process-validation.json"),
        &json!({"schemaVersion":1,"kind":"winbridge_activation_process_validation",
        "updateId":source.update_id,"launchId":source.launch_id,"ownerId":source.owner_id,"ownerEpoch":source.owner_epoch,
        "attemptId":prepared.intent.attempt_id,"runtimeIdentity":source.runtime_identity,"updaterConfigSha256":source.updater_config_sha256}),
    );
    assert!(winbridge_update::installed_set::pin_installed_set(
        &root.join("runtime"),
        &source.installation_id,
        &source.scope_sha256
    )
    .is_err());
    q.contract_version = 21;
    q.action = SupervisorAction::BindActivationFiles;
    let binding = call(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationFiles;
    let bound = loop {
        let v = call(&rt, sup, &q);
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if !v.driver_active
            && v.activation_files.as_ref().unwrap().phase == ActivationFilesPhase::Bound
        {
            break v;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(bound.activation_files.as_ref().unwrap().held_files, 5);
    println!(
        "Activation process timing: Bound at {}ms",
        timer.elapsed().as_millis()
    );
    q.contract_version = 22;
    q.action = SupervisorAction::ObserveActivationProcess;
    let initial = call(&rt, sup, &q);
    assert_eq!(
        initial.activation_process.as_ref().unwrap().phase,
        ActivationProcessPhase::NotStarted
    );
    q.action = SupervisorAction::LaunchActivationReceiver;
    // Lost ACK: dispatch once, do not consume its response or repeat execution.
    let mut stream = rt
        .block_on(winbridge_transport::supervisor::connect(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
        ))
        .unwrap();
    rt.block_on(winbridge_transport::write_frame(
        &mut stream,
        &serde_json::to_vec(&q).unwrap(),
    ))
    .unwrap();
    drop(stream);
    q.action = SupervisorAction::ObserveActivationProcess;
    let creating = marker(root, "creating", &rt, sup, &q);
    let creating_record: ActivationRecord =
        serde_json::from_value(creating["record"].clone()).unwrap();
    assert_eq!(creating_record.revision, 1);
    assert_eq!(creating_record.state, ActivationState::Creating {});
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&creating_record)
    );
    let creating_view = call(&rt, sup, &q);
    assert_eq!(
        creating_view
            .activation_process
            .as_ref()
            .unwrap()
            .durable_phase,
        Some(ActivationDurablePhase::Creating)
    );
    assert!(creating_view.driver_active);
    locked(root);
    assert_eq!(journal_bytes(root), bytes);
    let absent_job = job(&creating_record);
    assert!(
        matches!(
            process_identity::observe_process(&absent_job),
            ProcessObservation::Empty(_)
        ),
        "No activation process in its unique Job before CreateProcess"
    );
    q.action = SupervisorAction::LaunchActivationReceiver;
    let duplicate_creating = call(&rt, sup, &q);
    assert_eq!(
        duplicate_creating
            .activation_process
            .as_ref()
            .unwrap()
            .attempt_id,
        Some(prepared.intent.attempt_id.clone())
    );
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&creating_record)
    );
    release(root, "creating", &creating);
    q.action = SupervisorAction::ObserveActivationProcess;
    let created = marker(root, "created", &rt, sup, &q);
    let created_record: ActivationRecord =
        serde_json::from_value(created["record"].clone()).unwrap();
    assert_eq!(created_record.revision, 2);
    assert_eq!(created_record.state, ActivationState::Created {});
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&created_record)
    );
    let actual =
        infrastructure::stamp(created_record.created.as_ref().unwrap().process.process_id).unwrap();
    assert_eq!(actual, created_record.created.as_ref().unwrap().process);
    let created_job = job(&created_record);
    assert!(matches!(
        process_identity::observe_process(&created_job),
        ProcessObservation::Occupied(_)
    ));
    assert_eq!(
        infrastructure::job_observation(&actual).unwrap(),
        Some(true)
    );
    locked(root);
    assert_eq!(journal_bytes(root), bytes);
    let created_view = call(&rt, sup, &q);
    assert!(created_view.driver_active);
    assert_eq!(
        created_view
            .activation_process
            .as_ref()
            .unwrap()
            .saved_process,
        Some(actual.clone())
    );
    assert_eq!(
        created_view
            .activation_process
            .as_ref()
            .unwrap()
            .durable_phase,
        Some(ActivationDurablePhase::Created)
    );
    q.action = SupervisorAction::LaunchActivationReceiver;
    let duplicate_created = call(&rt, sup, &q);
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&created_record)
    );
    release(root, "created", &created);
    q.action = SupervisorAction::ObserveActivationProcess;
    let ready = loop {
        let v = call(&rt, sup, &q);
        assert!(
            v.driver_error.is_none(),
            "Process stopped: {:?}; stderr: {:?}",
            v.driver_error,
            v.activation_process
                .as_ref()
                .and_then(|p| p.stderr_tail.as_ref())
        );
        if !v.driver_active
            && v.activation_process.as_ref().unwrap().phase == ActivationProcessPhase::ReadyForState
        {
            break v;
        }
        assert!(
            Instant::now() < until + Duration::from_secs(180),
            "Observe the same process; no replay"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    println!(
        "Activation process timing: Ready at {}ms",
        timer.elapsed().as_millis()
    );
    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(record.revision, 4);
    assert_eq!(record.state, ActivationState::ReadyForState {});
    let process = ready.activation_process.as_ref().unwrap();
    assert_eq!(process.saved_process, Some(actual.clone()));
    assert_eq!(process.resident_process, Some(actual.clone()));
    assert_eq!(process.process_custody_held, Some(true));
    assert_eq!(process.process_alive, Some(true));
    assert!(process.original_state_held);
    assert_eq!(
        process.saved_ready.as_ref(),
        record.ready.as_ref().map(|r| &r.identity)
    );
    assert_eq!(ready.update, initial.update);
    assert_eq!(
        serde_json::to_value(&ready.update).unwrap(),
        replacement["view"]["update"]
    );
    let loaded = winbridge_update::installed_set::pin_installed_set(
        &root.join("runtime"),
        &source.installation_id,
        &source.scope_sha256,
    )
    .unwrap();
    loaded.revalidate().unwrap();
    assert_eq!(loaded.observation().images.first(), Some(&source.worker));
    assert_eq!(loaded.observation().manifest_file, source.manifest);
    assert!(fs::OpenOptions::new()
        .write(true)
        .open(&source.worker.path)
        .is_err());
    assert!(fs::remove_file(&source.worker.path).is_err());
    locked(root);
    assert_eq!(journal_bytes(root), bytes);
    q.action = SupervisorAction::LaunchActivationReceiver;
    let duplicate = call(&rt, sup, &q);
    assert_eq!(
        duplicate.activation_process.as_ref().unwrap().saved_process,
        Some(actual.clone())
    );
    assert_eq!(
        duplicate.activation_process.as_ref().unwrap().revision,
        Some(4)
    );
    assert!(!duplicate.driver_active);
    q.contract_version = 21;
    q.action = SupervisorAction::ObserveActivationFiles;
    let legacy = call(&rt, sup, &q);
    assert_eq!(legacy.schema_version, 13);
    assert_eq!(legacy.activation_files.as_ref().unwrap().held_files, 5);
    assert!(
        legacy
            .activation_files
            .as_ref()
            .unwrap()
            .original_state_held
    );
    assert!(serde_json::to_value(&legacy)
        .unwrap()
        .get("activationProcess")
        .is_none());
    let history = super::activation_preparation::history(&directory, id);
    assert_eq!(history.len(), 5);
    assert_eq!(history.last(), Some(&record));
    let health = UpdateJournal::inspect_health(&directory, id)
        .unwrap()
        .unwrap();
    json!({"before":before,"preparedView":prepared_view,"prepared":prepared,"binding":binding,"bound":bound,"initial":initial,
        "creating":creating,"creatingView":creating_view,"duplicateCreating":duplicate_creating,"jobBeforeCreate":absent_job,"jobEmptyBeforeCreate":true,
        "created":created,"createdView":created_view,"duplicateCreated":duplicate_created,"createdProcess":actual,"createdJob":created_job,"createdJobOccupied":true,"createdProcessInJob":true,
        "ready":ready,"duplicate":duplicate,"record":record,"history":history,"actualSet":loaded.observation(),"legacy":legacy,
        "workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),"supervisor":sup,"health":health,
        "healthEncoded":String::from_utf8(serde_json::to_vec(&health).unwrap()).unwrap(),"lostAckSentOnce":true,"originalInstanceLockHeld":true,
        "workerWriteDenied":true,"workerDeleteDenied":true,"workerLaunchCount":1,"stateHandoffCount":0,"publicActivationEnabled":false})
}
pub(super) fn after_owned_exit(
    root: &Path,
    id: &str,
    saved: &serde_json::Value,
    pin: &infrastructure::PinnedProcess,
) -> serde_json::Value {
    assert!(
        pin.exited().unwrap(),
        "Actual retained activation process handle must be signalled after owned Job exit"
    );
    let record: ActivationRecord = serde_json::from_value(saved["record"].clone()).unwrap();
    assert!(matches!(
        process_identity::observe_process(&job(&record)),
        ProcessObservation::Empty(_)
    ));
    let directory = root.join("runtime/update-state");
    let view = UpdateJournal::activation_process_snapshot(&directory, id, false, None).unwrap();
    let process = view.activation_process.as_ref().unwrap();
    assert_eq!(process.phase, ActivationProcessPhase::NotResident);
    assert_eq!(process.process_custody_held, Some(false));
    assert!(process.resident_process.is_none() && process.process_alive.is_none());
    assert!(!process.original_state_held);
    assert_eq!(
        serde_json::to_value(
            UpdateJournal::inspect_activation(&directory, id)
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        saved["record"]
    );
    assert_eq!(journal_bytes(root), saved["workerBytesBefore"]);
    json!({"view":view,"record":record,"history":super::activation_preparation::history(&directory,id),"actualRetainedProcessExited":true,"ownedJobEmpty":true,"activationJobEmpty":true,
        "workerBytesUnchanged":true,"workerLaunchCount":1,"stateHandoffCount":0,"automaticColdLaunchCount":0})
}
