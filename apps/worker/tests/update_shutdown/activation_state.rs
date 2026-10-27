//! Main23: physical ownership by the same private child, never a Store writer.
use super::activation_process::{call, job};
use super::*;
use winbridge_platform::{
    infrastructure,
    process_identity::{self, ProcessObservation},
};
use winbridge_update::{
    activation::{ActivationRecord, ActivationState},
    shutdown::ShutdownStatePins,
};

fn locked(expected: &WorkerShutdownState) {
    let pins = ShutdownStatePins::capture(Path::new(&expected.state_directory)).unwrap();
    assert_eq!(pins.observation(), expected);
    let error = pins
        .try_lock()
        .err()
        .expect("The child must own the exact existing lock");
    assert!(
        error.kind() == std::io::ErrorKind::WouldBlock
            || error.raw_os_error().is_some()
                && error.raw_os_error() == fs2::lock_contended_error().raw_os_error(),
        "An unrelated IO failure is not proof of an occupied lock: {error}"
    );
}
fn dispatch_without_ack(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) {
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
    drop(stream);
}
fn wait(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
    mut done: impl FnMut(&UpdatePreparationView) -> bool,
) -> UpdatePreparationView {
    // An observation deadline, never a renewal of the original startup window.
    let until = Instant::now() + Duration::from_secs(300);
    loop {
        let v = call(rt, sup, q);
        assert!(
            v.driver_error.is_none(),
            "Observe the same attempt, never replay: {:?}",
            v.driver_error
        );
        if !v.driver_active && done(&v) {
            return v;
        }
        assert!(
            Instant::now() < until,
            "Observe the existing task; no new launch or grant"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub(super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
) -> serde_json::Value {
    execute_inner(root, sup, replacement, Target::State)
}
pub(super) fn execute_writer(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
) -> serde_json::Value {
    execute_inner(root, sup, replacement, Target::Writer)
}
pub(super) fn execute_service(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
) -> serde_json::Value {
    execute_inner(root, sup, replacement, Target::Service)
}
#[derive(Clone, Copy)]
enum Target {
    State,
    Writer,
    Service,
}
fn execute_inner(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
    target: Target,
) -> serde_json::Value {
    let timer = Instant::now();
    let directory = root.runtime().join("update-state");
    let id = &sup.spec.update_id;
    let bytes = journal_bytes(root);
    // All baseline observations precede preparation admission. Do not spend
    // the remaining original window on them after StateOwned.
    let baseline = (!matches!(target, Target::State))
        .then(|| super::activation_writer::Baseline::capture(root, id));
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
    let preparing = call(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationPreparation;
    let prepared_view = wait(&rt, sup, &q, |v| {
        v.activation_preparation.as_ref().unwrap().phase == ActivationPreparationPhase::Prepared
    });
    let prepared = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.state, ActivationState::Prepared {});
    assert_eq!(prepared.revision, 0);
    let source = &prepared.intent.source;
    write(
        &activation_trace_root(root, id).join("activation-process-validation.json"),
        &json!({"schemaVersion":1,"kind":"winbridge_activation_process_validation",
            "updateId":source.update_id,"launchId":source.launch_id,"ownerId":source.owner_id,
            "ownerEpoch":source.owner_epoch,"attemptId":prepared.intent.attempt_id,
            "runtimeIdentity":source.runtime_identity,"updaterConfigSha256":source.updater_config_sha256,
            "stateTraceOnly":true}),
    );
    q.contract_version = 21;
    q.action = SupervisorAction::BindActivationFiles;
    let binding = call(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationFiles;
    let bound = wait(&rt, sup, &q, |v| {
        v.activation_files.as_ref().unwrap().phase == ActivationFilesPhase::Bound
    });
    assert_eq!(bound.activation_files.as_ref().unwrap().held_files, 5);
    q.contract_version = 22;
    q.action = SupervisorAction::LaunchActivationReceiver;
    let launching = call(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationProcess;
    let ready = wait(&rt, sup, &q, |v| {
        v.activation_process.as_ref().unwrap().phase == ActivationProcessPhase::ReadyForState
    });
    // Do not read/hash files/history or make diagnostic IPC roundtrips here.
    // Main22 already proved the original Creating/Created held barriers.
    let ready_elapsed_ms = timer.elapsed().as_millis();
    q.contract_version = 23;
    q.action = SupervisorAction::HandoffActivationState;
    dispatch_without_ack(&rt, sup, &q);
    q.action = SupervisorAction::ObserveActivationState;
    let owned = wait(&rt, sup, &q, |v| {
        v.activation_state.as_ref().unwrap().phase == ActivationStatePhase::StateOwned
    });
    let owned_elapsed_ms = timer.elapsed().as_millis();
    println!("Activation state timing: Ready at {ready_elapsed_ms}ms; StateOwned at {owned_elapsed_ms}ms");

    if let Some(baseline) = baseline {
        return super::activation_writer::execute(
            root,
            sup,
            &rt,
            q,
            baseline,
            matches!(target, Target::Service),
            json!({"before":before,"preparing":preparing,"preparedView":prepared_view,
                "prepared":prepared,"binding":binding,"bound":bound,"launching":launching,
                "ready":ready,"owned":owned,"workerBytesBefore":bytes}),
        );
    }

    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(record.revision, 6);
    assert_eq!(record.state, ActivationState::StateOwned {});
    assert_eq!(record.intent, prepared.intent);
    let actual = &record.created.as_ref().unwrap().process;
    assert_eq!(&infrastructure::stamp(actual.process_id).unwrap(), actual);
    let actual_job = job(&record);
    assert!(matches!(
        process_identity::observe_process(&actual_job),
        ProcessObservation::Occupied(_)
    ));
    assert_eq!(infrastructure::job_observation(actual).unwrap(), Some(true));
    assert_eq!(&record.ready.as_ref().unwrap().identity.process, actual);
    let receipt = record.state_owned.as_ref().unwrap();
    assert_eq!(&receipt.process, actual);
    assert_eq!(receipt.context, record.intent.context.worker_context);
    assert_eq!(receipt.state, record.intent.context.worker_state);
    assert_ne!(
        receipt.generation,
        record.intent.context.previous_generation
    );
    assert_eq!(
        uuid::Uuid::parse_str(&receipt.generation)
            .unwrap()
            .to_string(),
        receipt.generation
    );
    let state = owned.activation_state.as_ref().unwrap();
    assert_eq!(owned.schema_version, 15);
    assert_eq!(state.original_lock_held, Some(false));
    assert_eq!(state.resident_process.as_ref(), Some(actual));
    assert_eq!(state.process_alive, Some(true));
    assert_eq!(state.saved_ownership.as_ref(), Some(receipt));
    assert_eq!(owned.update, ready.update);
    assert_eq!(
        serde_json::to_value(&owned.update).unwrap(),
        replacement["view"]["update"]
    );
    locked(&receipt.state);
    assert_eq!(journal_bytes(root), bytes);

    let held: serde_json::Value = serde_json::from_slice(
        &fs::read(activation_trace_root(root, id).join("activation-state-before_release.json"))
            .unwrap(),
    )
    .unwrap();
    let released: serde_json::Value = serde_json::from_slice(
        &fs::read(activation_trace_root(root, id).join("activation-state-after_release.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(held["record"], released["record"]);
    assert_eq!(held["record"]["revision"], 5);
    assert_eq!(held["lockAvailable"], false);
    assert_eq!(released["lockAvailable"], true);
    assert!(!root.join("activation-process-creating.json").exists());
    assert!(!root.join("activation-process-created.json").exists());

    q.action = SupervisorAction::HandoffActivationState;
    let duplicate = call(&rt, sup, &q);
    assert!(!duplicate.driver_active);
    assert_eq!(
        duplicate
            .activation_state
            .as_ref()
            .unwrap()
            .saved_ownership
            .as_ref(),
        Some(receipt)
    );
    assert_eq!(
        duplicate.activation_state.as_ref().unwrap().revision,
        Some(6)
    );
    q.contract_version = 22;
    q.action = SupervisorAction::ObserveActivationProcess;
    let legacy_process = call(&rt, sup, &q);
    assert_eq!(legacy_process.schema_version, 14);
    assert_eq!(
        legacy_process.activation_process.as_ref().unwrap().phase,
        ActivationProcessPhase::Indeterminate
    );
    assert!(
        !legacy_process
            .activation_process
            .as_ref()
            .unwrap()
            .original_state_held
    );
    assert!(serde_json::to_value(&legacy_process)
        .unwrap()
        .get("activationState")
        .is_none());
    q.contract_version = 21;
    q.action = SupervisorAction::ObserveActivationFiles;
    let legacy_files = call(&rt, sup, &q);
    assert_eq!(legacy_files.schema_version, 13);
    assert_eq!(
        legacy_files.activation_files.as_ref().unwrap().held_files,
        5
    );
    assert!(
        !legacy_files
            .activation_files
            .as_ref()
            .unwrap()
            .original_state_held
    );
    assert!(serde_json::to_value(&legacy_files)
        .unwrap()
        .get("activationState")
        .is_none());

    let loaded = winbridge_update::installed_set::pin_installed_set(
        root.runtime(),
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
    let history = super::activation_preparation::history(&directory, id);
    assert_eq!(history.len(), 7);
    assert_eq!(history.first(), Some(&prepared));
    assert_eq!(history.last(), Some(&record));
    assert_eq!(serde_json::to_value(&history[5]).unwrap(), held["record"]);
    assert_eq!(
        history.iter().map(|r| r.revision).collect::<Vec<_>>(),
        (0..7).collect::<Vec<_>>()
    );
    locked(&receipt.state);
    assert_eq!(journal_bytes(root), bytes);
    let health = UpdateJournal::inspect_health(&directory, id)
        .unwrap()
        .unwrap();
    json!({"before":before,"preparing":preparing,"preparedView":prepared_view,"prepared":prepared,
        "binding":binding,"bound":bound,"launching":launching,"ready":ready,"owned":owned,
        "duplicate":duplicate,"legacyProcess":legacy_process,"legacyFiles":legacy_files,
        "record":record,"history":history,"beforeRelease":held,"afterRelease":released,
        "actualProcess":actual,"actualJob":actual_job,"actualJobOccupied":true,"actualProcessInJob":true,
        "actualSet":set,"sharing":sharing,"childInstanceLockOccupied":true,
        "workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),"supervisor":sup,"health":health,
        "healthEncoded":String::from_utf8(serde_json::to_vec(&health).unwrap()).unwrap(),
        "lostHandoffAckSentOnce":true,"workerLaunchCount":1,"stateHandoffCount":1,
        "writableStoreStarted":false,"publicActivationEnabled":false})
}
pub(super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    saved: &serde_json::Value,
    pin: &infrastructure::PinnedProcess,
) -> serde_json::Value {
    assert!(
        pin.exited().unwrap(),
        "Retained actual receiver handle must be signalled"
    );
    let record: ActivationRecord = serde_json::from_value(saved["record"].clone()).unwrap();
    assert!(matches!(
        process_identity::observe_process(&job(&record)),
        ProcessObservation::Empty(_)
    ));
    let directory = root.runtime().join("update-state");
    let view = UpdateJournal::activation_state_snapshot(&directory, id, false, None).unwrap();
    let state = view.activation_state.as_ref().unwrap();
    assert_eq!(state.phase, ActivationStatePhase::NotResident);
    assert_eq!(state.saved_ownership, record.state_owned);
    assert_eq!(state.original_lock_held, Some(false));
    assert!(state.resident_process.is_none() && state.process_alive.is_none());
    let expected = &record.intent.context.worker_state;
    let pins = ShutdownStatePins::capture(Path::new(&expected.state_directory)).unwrap();
    assert_eq!(pins.observation(), expected);
    let probe = pins.try_lock().unwrap();
    probe.revalidate().unwrap();
    drop(probe);
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&record)
    );
    let history = super::activation_preparation::history(&directory, id);
    assert_eq!(serde_json::to_value(&history).unwrap(), saved["history"]);
    assert_eq!(journal_bytes(root), saved["workerBytesBefore"]);
    json!({"view":view,"record":record,"history":history,"actualRetainedProcessExited":true,
        "ownedJobEmpty":true,"activationJobEmpty":true,"sameInstanceLockAvailable":true,
        "observedState":expected,"workerBytesUnchanged":true,"workerLaunchCount":1,
        "stateHandoffCount":1,"automaticColdLaunchCount":0,"writableStoreStarted":false})
}
