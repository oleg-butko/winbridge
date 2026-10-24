//! Real actor21 binding: strict readable set, original state lock and cold refusal.
use super::*;
pub(super) fn execute(
    root: &Path,
    sup: &SupervisorRecord,
    record: &winbridge_update::activation::ActivationRecord,
) -> serde_json::Value {
    let source = &record.intent.source;
    let directory = root.join("runtime/update-state");
    let bytes = journal_bytes(root);
    let pin_set = || {
        winbridge_update::installed_set::pin_installed_set(
            &root.join("runtime"),
            &source.installation_id,
            &source.scope_sha256,
        )
    };
    assert!(
        pin_set().is_err(),
        "Strict reader must conflict with original DELETE handles before binding"
    );
    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 21;
    q.action = SupervisorAction::ObserveActivationFiles;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let call = |q: &SupervisorRequest| -> UpdatePreparationView {
        let r: SupervisorResponse = rt
            .block_on(winbridge_transport::supervisor::call_with_timeout(
                &sup.spec.endpoint,
                sup.process.as_ref().unwrap().process_id,
                q,
                winbridge_transport::supervisor::CONTROL_TIMEOUT,
            ))
            .unwrap();
        assert_eq!(r.contract_version, 21);
        assert!(r.error.is_none(), "{:?}", r.error);
        let v = r.preparation.unwrap();
        assert_eq!(v.schema_version, 13);
        v
    };
    let before = call(&q);
    assert_eq!(
        before.activation_files.as_ref().unwrap().phase,
        ActivationFilesPhase::NotBound
    );
    q.action = SupervisorAction::BindActivationFiles;
    let admitted = call(&q);
    assert!(admitted.driver_active);
    assert_eq!(
        admitted.activation_files.as_ref().unwrap().phase,
        ActivationFilesPhase::Binding
    );
    let duplicate_active = call(&q);
    q.action = SupervisorAction::ObserveActivationFiles;
    let until = Instant::now() + Duration::from_secs(600);
    let done = loop {
        let v = call(&q);
        assert!(v.driver_error.is_none(), "{:?}", v.driver_error);
        if !v.driver_active
            && v.activation_files.as_ref().unwrap().phase == ActivationFilesPhase::Bound
        {
            break v;
        }
        assert!(
            Instant::now() < until,
            "Observe existing binding, never replay"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let held = done.activation_files.as_ref().unwrap();
    assert_eq!(held.held_files, 5);
    assert!(held.original_state_held);
    assert_eq!(held.worker.as_ref(), Some(&source.worker));
    assert_eq!(held.manifest_sha256.as_ref(), Some(&source.manifest.sha256));
    assert_eq!(done.update, before.update);
    let set = pin_set().unwrap();
    set.revalidate().unwrap();
    assert_eq!(set.observation().images.first(), Some(&source.worker));
    assert_eq!(set.observation().manifest_file, source.manifest);
    assert!(fs::OpenOptions::new()
        .write(true)
        .open(&source.worker.path)
        .is_err());
    assert!(fs::remove_file(&source.worker.path).is_err());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/worker.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&lock).is_err());
    assert_eq!(journal_bytes(root), bytes);
    q.action = SupervisorAction::BindActivationFiles;
    let duplicate = call(&q);
    assert_eq!(duplicate.activation_files, done.activation_files);
    assert_eq!(duplicate.update, done.update);
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, &source.update_id)
            .unwrap()
            .as_ref(),
        Some(record)
    );
    let mut old = q;
    old.contract_version = 20;
    old.action = SupervisorAction::ObserveActivationPreparation;
    let old: SupervisorResponse = rt
        .block_on(winbridge_transport::supervisor::call(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
            &old,
        ))
        .unwrap();
    assert!(old.error.is_none());
    assert!(serde_json::to_value(old.preparation.unwrap())
        .unwrap()
        .get("activationFiles")
        .is_none());
    json!({"before":before,"admitted":admitted,"duplicateActive":duplicate_active,"view":done,"duplicate":duplicate,"actualSet":set.observation(),
        "strictPinBeforeRefused":true,"strictPinAfterSucceeded":true,"workerWriteDenied":true,"workerDeleteDenied":true,"originalInstanceLockHeld":true,
        "workerBytesBefore":bytes,"workerBytesAfter":journal_bytes(root),"record":record,"legacyFieldAbsent":true,"workerLaunchCount":0,"stateHandoffCount":0})
}
pub(super) fn after_owned_exit(
    root: &Path,
    id: &str,
    saved: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.join("runtime/update-state");
    let v = UpdateJournal::activation_files_snapshot(&directory, id, false, None).unwrap();
    let files = v.activation_files.as_ref().unwrap();
    assert_eq!(files.phase, ActivationFilesPhase::NotBound);
    assert_eq!(files.held_files, 0);
    assert!(!files.original_state_held);
    let record = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap(), saved["record"]);
    assert_eq!(journal_bytes(root), saved["workerBytesBefore"]);
    json!({"view":v,"record":record,"ownedJobEmpty":true,"workerBytesUnchanged":true,"workerLaunchCount":0,"stateHandoffCount":0})
}
