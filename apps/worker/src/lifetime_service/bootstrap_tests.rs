//! Real Store/seal/locks with synthetic keeper metadata. No native Job/bootstrap
//! creation or full updater workflow is claimed by these portable tests.
use super::super::{active_tests, finalization_service::active::ActiveReadiness};
use super::*;

#[cfg(windows)]
#[test]
fn original_keeper_observation_future_is_send() {
    fn check(scope: &ActiveWorkerBootstrap) {
        fn require_send(_: impl std::future::Future + Send) {}
        require_send(scope.observe_keeper());
    }
    let _: fn(&ActiveWorkerBootstrap) = check;
}

async fn active(root: &Path) -> (RunningLocalService, Arc<AtomicBool>) {
    let (mut running, pending, live) = active_tests::pending(root).await;
    let commit = active_tests::commit(&running, &pending);
    running
        .seal_active_commit(commit, ActiveReadiness::fixture(pending), || Ok(()))
        .unwrap();
    (running, live)
}

#[tokio::test]
async fn bootstrap_rejects_verification_pending_and_saved_release_without_active_seal() {
    let root = tempfile::tempdir().unwrap();
    let service = super::super::tests::fixture(root.path());
    assert!(ActiveModeWitness::capture(&service.worker).is_err());
    drop(service);

    let root = tempfile::tempdir().unwrap();
    let (running, pending, _) = active_tests::pending(root.path()).await;
    assert!(ActiveModeWitness::capture(&running.service.worker).is_err());
    drop(active_tests::commit(&running, &pending));
    assert!(
        ActiveModeWitness::capture(&running.service.worker).is_err(),
        "SQL release alone cannot reconstruct Active"
    );
    running.close().await.unwrap();
}

#[tokio::test]
async fn bootstrap_retains_exact_active_owner_and_allows_work_after_original_deadline() {
    let root = tempfile::tempdir().unwrap();
    let (running, live) = active(root.path()).await;
    let worker = &running.service.worker;
    let witness = ActiveModeWitness::capture(worker).unwrap();
    worker
        .check_local_service_scope_at(running.service.deadline + Duration::from_secs(1))
        .unwrap();
    worker.active_jobs.store(1, Ordering::Release);
    witness.require_current(worker).unwrap();
    worker.active_jobs.store(0, Ordering::Release);

    live.store(false, Ordering::Release);
    assert!(witness.require_current(worker).is_err());
    live.store(true, Ordering::Release);
    witness.require_current(worker).unwrap();

    let scope = worker.local_activation.as_ref().unwrap();
    *scope.lifetime.lock().unwrap() = ServiceLifetime::Active(Arc::new(ActiveLifetime {
        resident: Arc::clone(&witness.active.resident),
        receipt: witness.active.receipt.clone(),
    }));
    assert!(
        witness.require_current(worker).is_err(),
        "Equal metadata are not the original sealed owner"
    );
    *scope.lifetime.lock().unwrap() = ServiceLifetime::Active(Arc::clone(&witness.active));
    witness.require_current(worker).unwrap();
    running.close().await.unwrap();
}

#[tokio::test]
async fn bootstrap_checkpoints_refuse_contended_worker_locks_without_waiting() {
    let root = tempfile::tempdir().unwrap();
    let (running, _) = active(root.path()).await;
    let worker = &running.service.worker;
    let witness = ActiveModeWitness::capture(worker).unwrap();
    let scope = worker.local_activation.as_ref().unwrap();
    {
        let _held = worker.store.lock().unwrap();
        assert!(ActiveModeWitness::capture(worker).is_err());
        assert!(witness
            .require_current(worker)
            .unwrap_err()
            .contains("Store busy"));
    }
    {
        let _held = scope.lifetime.lock().unwrap();
        assert!(ActiveModeWitness::capture(worker).is_err());
        assert!(witness
            .require_current(worker)
            .unwrap_err()
            .contains("mode busy"));
    }
    {
        let _held = worker.unreconciled_tasks.lock().unwrap();
        assert!(ActiveModeWitness::capture(worker).is_err());
        assert!(witness
            .require_current(worker)
            .unwrap_err()
            .contains("registry busy"));
    }
    witness.require_current(worker).unwrap();
    running.close().await.unwrap();
}

#[tokio::test]
async fn bootstrap_rejects_recovery_and_stopping_and_keeps_store_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let (running, _) = active(root.path()).await;
    let worker = &running.service.worker;
    let witness = ActiveModeWitness::capture(worker).unwrap();
    let before = worker.store().unwrap().state_backup_authority().unwrap();
    let mut changed = ActiveModeWitness {
        active: Arc::clone(&witness.active),
        authority: witness.authority.clone(),
    };
    changed.authority.authority_id = uuid::Uuid::new_v4().to_string();
    assert!(changed.require_current(worker).is_err());
    worker
        .unreconciled_tasks
        .lock()
        .unwrap()
        .insert("unknown-task".into(), "unknown outcome".into());
    assert!(witness.require_current(worker).is_err());
    worker.unreconciled_tasks.lock().unwrap().clear();
    worker.stop.store(true, Ordering::Release);
    assert!(witness.require_current(worker).is_err());
    worker.stop.store(false, Ordering::Release);
    witness.require_current(worker).unwrap();
    assert_eq!(
        worker.store().unwrap().state_backup_authority().unwrap(),
        before
    );
    running.close().await.unwrap();
}
