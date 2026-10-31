//! Real worker/config/IPC registration. These tests never launch a supervisor.
use super::*;
use crate::update_control::bootstrap::RegistrationLease;

#[tokio::test]
async fn bootstrap_registration_slot_and_nonblocking_config_checks_keep_original_owner() {
    let mut f = Fixture::new(None).await;
    let worker = &f.worker;
    let reg = worker.control_registration().unwrap();
    let lease = RegistrationLease::capture(worker).unwrap();
    assert!(RegistrationLease::capture(worker).is_err());
    {
        let _store = worker.store.lock().unwrap();
        lease.require_current(worker).unwrap();
        {
            let _path = worker.config_path.lock().unwrap();
            assert!(lease
                .require_current(worker)
                .unwrap_err()
                .contains("config path busy"));
        }
        {
            let _slot = worker.update_control.lock().unwrap();
            assert!(lease
                .require_current(worker)
                .unwrap_err()
                .contains("registration busy"));
        }
    }
    reg.ready.store(false, Ordering::Release);
    assert!(lease.require_current(worker).is_err());
    reg.ready.store(true, Ordering::Release);
    lease.require_current(worker).unwrap();
    drop(lease);
    assert!(!reg.launching.load(Ordering::Acquire));
    drop(RegistrationLease::capture(worker).unwrap());
    f.worker.stop();
    (&mut f.server).await.unwrap().unwrap();
}

#[tokio::test]
async fn bootstrap_registration_rejects_config_bytes_and_slot_replacement_without_rebinding() {
    let mut f = Fixture::new(None).await;
    let worker = &f.worker;
    let lease = RegistrationLease::capture(worker).unwrap();
    for path in [&f.config, &f.updater_config] {
        let original = fs::read(path).unwrap();
        let mut changed = original.clone();
        changed.push(b'\n');
        fs::write(path, changed).unwrap();
        assert!(lease.require_current(worker).is_err());
        fs::write(path, original).unwrap();
        lease.require_current(worker).unwrap();
    }
    let reg = worker.update_control.lock().unwrap().take().unwrap();
    assert!(lease.require_current(worker).is_err());
    *worker.update_control.lock().unwrap() = Some(reg);
    lease.require_current(worker).unwrap();
    drop(lease);
    f.worker.stop();
    (&mut f.server).await.unwrap().unwrap();
}

#[cfg(windows)]
#[test]
fn resident_bootstrap_async_operations_are_send() {
    fn check(original: &mut crate::WorkerBootstrap) {
        fn require_send(_: impl std::future::Future + Send) {}
        require_send(original.admit());
        require_send(original.send_start());
        require_send(original.observe_launch());
        require_send(original.wait_ready_and_confirm());
        require_send(original.handoff());
        require_send(original.observe_handoff());
    }
    let _: fn(&mut crate::WorkerBootstrap) = check;
}
