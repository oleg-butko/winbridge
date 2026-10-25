//! Preparation only in the original live supervisor, then cold read without custody.
use super::*;
use winbridge_update::activation::{ActivationRecord, ActivationState};

pub(super) fn history(directory: &Path, id: &str) -> Vec<ActivationRecord> {
    let c = rusqlite::Connection::open_with_flags(
        directory.join("updates.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q = c
        .prepare("SELECT record FROM activation_history WHERE id=?1 ORDER BY revision LIMIT 10")
        .unwrap();
    q.query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}

pub(super) fn execute(
    root: &Path,
    sup: &SupervisorRecord,
    replacement: &serde_json::Value,
    bind_files: bool,
) -> serde_json::Value {
    let timing = Instant::now();
    let directory = root.join("runtime/update-state");
    let id = &sup.spec.update_id;
    let bytes = journal_bytes(root);
    let mut request: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    request.contract_version = 20;
    request.action = SupervisorAction::ObserveActivationPreparation;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pid = sup.process.as_ref().unwrap().process_id;
    let call = |q: &SupervisorRequest| -> SupervisorResponse {
        let r: SupervisorResponse = rt
            .block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                pid,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ))
            .unwrap();
        assert_eq!(r.contract_version, 20);
        r
    };
    let view = |q: &SupervisorRequest| -> UpdatePreparationView {
        let r = call(q);
        assert!(r.error.is_none(), "{:?}", r.error);
        let v = r.preparation.unwrap();
        assert_eq!(v.schema_version, 12);
        assert_eq!(v.update.update_id, *id);
        v
    };
    let initial = view(&request);
    assert_eq!(initial.update.phase, UpdatePhase::VerifyingLocal);
    assert_eq!(
        initial.activation_preparation.as_ref().unwrap().phase,
        ActivationPreparationPhase::NotPrepared
    );
    assert!(
        initial
            .runtime_replacement
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    let mut wrong = request.clone();
    wrong.contract_version = 19;
    wrong.action = SupervisorAction::PrepareActivation;
    assert!(call(&wrong).error.is_some());
    assert!(UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .is_none());

    request.action = SupervisorAction::PrepareActivation;
    println!(
        "Activation preparation timing: request at {}ms",
        timing.elapsed().as_millis()
    );
    // Deliberately do not consume the response. Read committed metadata first.
    let mut stream = rt
        .block_on(winbridge_transport::supervisor::connect(
            &sup.spec.endpoint,
            pid,
        ))
        .unwrap();
    rt.block_on(winbridge_transport::write_frame(
        &mut stream,
        &serde_json::to_vec(&request).unwrap(),
    ))
    .unwrap();
    let until = Instant::now() + Duration::from_secs(65);
    let record = loop {
        if let Some(r) = UpdateJournal::inspect_activation(&directory, id).unwrap() {
            break r;
        }
        assert!(
            Instant::now() < until,
            "Preparation did not commit before ACK consumption"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    println!(
        "Activation preparation timing: durable record at {}ms",
        timing.elapsed().as_millis()
    );
    drop(stream);
    assert_eq!(record.revision, 0);
    assert_eq!(record.state, ActivationState::Prepared {});
    request.action = SupervisorAction::ObserveActivationPreparation;
    let observed = loop {
        let v = view(&request);
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if !v.driver_active {
            break v;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(25));
    };
    println!(
        "Activation preparation timing: resident preparation at {}ms",
        timing.elapsed().as_millis()
    );
    assert_eq!(observed.update, initial.update);
    assert_eq!(
        observed.activation_preparation.as_ref().unwrap().phase,
        ActivationPreparationPhase::Prepared
    );
    assert!(
        observed
            .activation_preparation
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    // Spend the original live window on binding, not duplicate/history audits.
    // Those observations remain mandatory after the consuming handoff; neither
    // binding nor a duplicate may change the durable Prepared record or nonce.
    if bind_files {
        let diagnostic_log = root
            .join("runtime/update")
            .join(id)
            .join("supervisor/stderr.log");
        let diagnostic = fs::read_to_string(diagnostic_log).unwrap();
        assert!(
            diagnostic.contains("WinBridge activation timing "),
            "The actual copied supervisor must emit validation timings; harness features alone do not enable them"
        );
    }
    let binding = bind_files.then(|| super::activation_files::execute(root, sup, &record));
    println!(
        "Activation preparation timing: binding returned at {}ms",
        timing.elapsed().as_millis()
    );
    let replaced = UpdateJournal::inspect_runtime_replacement(&directory, id)
        .unwrap()
        .unwrap();
    record.validate(&replaced).unwrap();
    request.action = SupervisorAction::PrepareActivation;
    let duplicate = view(&request);
    assert_eq!(
        duplicate.activation_preparation,
        observed.activation_preparation
    );
    assert_eq!(duplicate.update, observed.update);
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&record)
    );
    assert_eq!(history(&directory, id), vec![record.clone()]);
    println!(
        "Activation preparation timing: duplicate/history audited at {}ms",
        timing.elapsed().as_millis()
    );
    let health = UpdateJournal::inspect_health(&directory, id)
        .unwrap()
        .unwrap();
    let c = rusqlite::Connection::open_with_flags(
        directory.join("updates.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let edge: String = c
        .query_row(
            "SELECT transition FROM transitions WHERE update_id=?1 AND revision=?2",
            rusqlite::params![id, observed.update.revision as i64],
            |r| r.get(0),
        )
        .unwrap();
    let edge: UpdateTransition = serde_json::from_str(&edge).unwrap();
    assert_eq!(
        edge.command.as_ref().unwrap().event,
        UpdateEvent::FilesVerified
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&lock).is_err());
    assert_eq!(journal_bytes(root), bytes);
    assert_eq!(
        serde_json::to_value(&replaced).unwrap(),
        replacement["record"]
    );
    for image in replaced.batch.files.iter().flat_map(|f| match &f.state {
        winbridge_update::runtime_exchange::FileExchangeState::Installed {
            retained,
            installed,
        } => [retained, installed],
        _ => panic!("Incomplete replacement"),
    }) {
        assert_eq!(
            replacement::observe_owned_file(Path::new(&image.path)),
            *image
        );
    }
    let mut old = request;
    old.contract_version = 19;
    old.action = SupervisorAction::ObserveCompatibleDrain;
    let legacy: SupervisorResponse = rt
        .block_on(winbridge_transport::supervisor::call(
            &sup.spec.endpoint,
            pid,
            &old,
        ))
        .unwrap();
    assert!(legacy.error.is_none());
    let legacy = legacy.preparation.unwrap();
    assert_eq!(legacy.schema_version, 11);
    assert!(serde_json::to_value(legacy)
        .unwrap()
        .get("activationPreparation")
        .is_none());
    let mut proof = json!({"before":initial,"view":observed,"duplicate":duplicate,"record":record,"history":history(&directory,id),"health":health,"rootEdge":edge,
        "healthEncoded":String::from_utf8(serde_json::to_vec(&health).unwrap()).unwrap(),
        "supervisor":sup,"workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),"lostAckSentOnce":true,"durableBeforeAckConsumption":true,
        "wrongVersionRefused":true,"oldViewFieldAbsent":true,"instanceLockHeld":true,"filesUnchanged":true,
        "workerLaunchCount":0,"stateHandoffCount":0,"publicActivationEnabled":false});
    if let Some(binding) = binding {
        proof["activationFiles"] = binding;
    }
    proof
}

pub(super) fn after_owned_exit(
    root: &Path,
    id: &str,
    proof: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let (record, _) =
        observe_after_owned_exit(|| UpdateJournal::inspect_activation(&directory, id)).unwrap();
    let record = record.unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap(), proof["record"]);
    let view = UpdateJournal::activation_preparation_snapshot(&directory, id, false, None).unwrap();
    assert_eq!(
        view.activation_preparation.as_ref().unwrap().phase,
        ActivationPreparationPhase::Prepared
    );
    assert!(
        !view
            .activation_preparation
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    assert!(
        !view
            .runtime_replacement
            .as_ref()
            .unwrap()
            .physical_custody_held
    );
    assert_eq!(history(&directory, id), vec![record.clone()]);
    assert_eq!(journal_bytes(root), proof["workerBytesBefore"]);
    json!({"record":record,"view":view,"history":history(&directory,id),"workerLaunchCount":0,"stateHandoffCount":0,"ownedJobEmpty":true})
}
