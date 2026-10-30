use super::*;
fn fixture() -> Exchange {
    let i = crate::bootstrap::tests::fixture();
    let file = |f: BootstrapFile| KeeperLaunchFile {
        sha256: f.sha256,
        bytes: f.bytes,
        file_identity: f.file_identity,
    };
    let ticket = KeeperLaunchTicket {
        update_id: i.update.update_id,
        launch_id: i.launch_id,
        image: file(i.image),
        config: file(i.config),
        stdout_identity: i.stdout_identity,
        stderr_identity: i.stderr_identity,
        token: "7".repeat(64),
        excluded_job: i.excluded_domain.job_name,
        predecessor: None,
        prepared_at_ms: i.prepared_at_ms,
    };
    Exchange::new(i.previous, ticket, 11).unwrap()
}
fn process(e: &Exchange) -> SupervisorProcess {
    SupervisorProcess {
        process_id: 99,
        birth: "9900".into(),
        host_id: e.binding.worker.host_id.clone(),
        session_id: e.binding.worker.session_id,
    }
}
fn status(e: &Exchange, phase: KeeperLaunchPhase) -> KeeperLaunchStatus {
    KeeperLaunchStatus {
        launch_id: e.ticket.launch_id.clone(),
        ticket_sha256: e.fingerprint.clone(),
        phase,
        process: matches!(
            phase,
            KeeperLaunchPhase::Created
                | KeeperLaunchPhase::ReadyConfirmed
                | KeeperLaunchPhase::Exited
        )
        .then(|| process(e)),
        expires_at_ms: e.ticket.prepared_at_ms + KEEPER_LAUNCH_STARTUP_MS,
        reason: None,
    }
}
fn reply(q: &KeeperLaunchRequest, s: KeeperLaunchStatus) -> KeeperLaunchResponse {
    KeeperLaunchResponse {
        keeper_launch_version: 1,
        request_id: Some(q.request_id.clone()),
        result: KeeperLaunchResult::Status {
            status: Box::new(s),
        },
    }
}
#[test]
fn lost_start_reply_consumes_start_but_not_late_readonly_observation() {
    let mut e = fixture();
    let q = e.start(11).unwrap();
    drop(q); // Response/future may have been lost.
    assert!(e.start(11).is_err());
    assert!(e.start(u64::MAX).is_err());
    let q = e.observe().unwrap();
    assert!(matches!(q.command, KeeperLaunchCommand::Observe { .. }));
    let r = reply(&q, status(&e, KeeperLaunchPhase::Created));
    let mut reads = 0;
    let observed = e
        .accept(&q, r, |_| {
            reads += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(reads, 1);
    // The original expiresAt stays unchanged even after arbitrarily late Observe.
    assert_eq!(observed.expires_at_ms, 10011);
    assert_eq!(e.status.as_ref(), Some(&observed));
}
#[test]
fn malformed_foreign_and_changed_budget_replies_never_reach_physical_queries() {
    for fault in 0..13 {
        let mut e = fixture();
        let q = e.start(11).unwrap();
        let mut r = reply(&q, status(&e, KeeperLaunchPhase::Created));
        match fault {
            0 => r.keeper_launch_version = 2,
            1 => r.request_id = None,
            _ => {
                let KeeperLaunchResult::Status { status: s } = &mut r.result else {
                    unreachable!()
                };
                match fault {
                    2 => s.launch_id = uuid::Uuid::new_v4().to_string(),
                    3 => s.ticket_sha256 = "0".repeat(64),
                    4 => s.expires_at_ms += 1,
                    5 => s.process = None,
                    6 => s.process.as_mut().unwrap().process_id = e.binding.worker.process_id,
                    7 => s.process.as_mut().unwrap().process_id = e.binding.coordinator.process_id,
                    8 => s.process.as_mut().unwrap().birth = "09900".into(),
                    9 => s.process.as_mut().unwrap().host_id = "a".repeat(64),
                    10 => s.process.as_mut().unwrap().session_id += 1,
                    11 => s.reason = Some("x".repeat(1001)),
                    _ => s.phase = KeeperLaunchPhase::ReadyConfirmed,
                }
            }
        }
        assert!(
            e.accept(&q, r, |_| panic!(
                "Untrusted reply reached exact process checks"
            ))
            .is_err(),
            "fault {fault}"
        );
        assert!(e.status.is_none());
    }
}
#[test]
fn physical_failure_never_publishes_created_and_original_process_cannot_change() {
    let mut e = fixture();
    let q = e.start(11).unwrap();
    let r = reply(&q, status(&e, KeeperLaunchPhase::Created));
    assert!(e
        .accept(&q, r.clone(), |_| Err("Outside Job check failed".into()))
        .is_err());
    assert!(e.status.is_none());
    e.accept(&q, r, |_| Ok(())).unwrap();
    let q = e.observe().unwrap();
    let mut s = status(&e, KeeperLaunchPhase::Created);
    s.process.as_mut().unwrap().process_id = 100;
    assert!(e
        .accept(&q, reply(&q, s), |_| panic!(
            "Changed process reached kernel query"
        ))
        .is_err());
    assert_eq!(
        e.status
            .as_ref()
            .unwrap()
            .process
            .as_ref()
            .unwrap()
            .process_id,
        99
    );
}
#[test]
fn confirmation_is_scoped_idempotent_and_cannot_restart_a_terminal_attempt() {
    let mut e = fixture();
    assert!(e.observe().is_err());
    let q = e.start(11).unwrap();
    let p = process(&e);
    assert!(e.confirm(&p).is_err());
    e.accept(
        &q,
        reply(&q, status(&e, KeeperLaunchPhase::Created)),
        |_| Ok(()),
    )
    .unwrap();
    let mut other = p.clone();
    other.birth.push('1');
    assert!(e.confirm(&other).is_err());
    for _ in 0..2 {
        let q = e.confirm(&p).unwrap();
        let s = status(&e, KeeperLaunchPhase::ReadyConfirmed);
        e.accept(&q, reply(&q, s), |_| Ok(())).unwrap();
    }
    let q = e.observe().unwrap();
    let mut terminal = status(&e, KeeperLaunchPhase::Exited);
    terminal.reason = Some("Owned process exited".into());
    e.accept(&q, reply(&q, terminal.clone()), |_| {
        panic!("Exited metadata must not reopen a PID")
    })
    .unwrap();
    assert!(e.confirm(&p).is_err());
    let created = status(&e, KeeperLaunchPhase::Created);
    assert!(e.accept(&q, reply(&q, created), |_| panic!()).is_err());
    assert_eq!(
        e.accept(&q, reply(&q, terminal.clone()), |_| panic!())
            .unwrap(),
        terminal
    );
    assert!(e.start(11).is_err());
}
#[test]
fn failure_unknown_and_preparing_are_observations_without_a_process_grant() {
    for phase in [KeeperLaunchPhase::Failed, KeeperLaunchPhase::Indeterminate] {
        let mut e = fixture();
        let q = e.start(11).unwrap();
        let mut s = status(&e, phase);
        s.reason = Some("OS outcome".into());
        e.accept(&q, reply(&q, s.clone()), |_| panic!("No process exists"))
            .unwrap();
        let observe = e.observe().unwrap();
        e.accept(&observe, reply(&observe, s), |_| panic!())
            .unwrap();
        let created = status(&e, KeeperLaunchPhase::Created);
        assert!(e
            .accept(&observe, reply(&observe, created), |_| panic!())
            .is_err());
        assert!(e.confirm(&process(&e)).is_err());
        assert!(e.start(11).is_err());
    }
    let mut e = fixture();
    let q = e.start(11).unwrap();
    let preparing = status(&e, KeeperLaunchPhase::Preparing);
    e.accept(&q, reply(&q, preparing), |_| panic!()).unwrap();
    assert!(e.confirm(&process(&e)).is_err());
}
