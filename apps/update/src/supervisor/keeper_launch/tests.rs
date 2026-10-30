use super::*;
use std::cell::Cell;
fn fixture() -> (KeeperBinding, KeeperLaunchTicket) {
    let i = crate::bootstrap::tests::fixture();
    let file = |f: crate::bootstrap::BootstrapFile| KeeperLaunchFile {
        sha256: f.sha256,
        bytes: f.bytes,
        file_identity: f.file_identity,
    };
    let t = KeeperLaunchTicket {
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
    (i.previous, t)
}
fn process() -> SupervisorProcess {
    SupervisorProcess {
        process_id: 99,
        birth: "999".into(),
        host_id: "a".repeat(64),
        session_id: 1,
    }
}
#[test]
fn duplicate_start_after_lost_reply_only_observes_the_original_creation() {
    let (binding, t) = fixture();
    let mut m = Machine::default();
    let calls = Cell::new(0);
    let made = m
        .start(&binding, &t, t.prepared_at_ms, || {
            calls.set(calls.get() + 1);
            Ok(process())
        })
        .unwrap();
    for at in [t.prepared_at_ms, t.prepared_at_ms + 10_001, u64::MAX] {
        assert_eq!(
            m.start(&binding, &t, at, || panic!("duplicate must never create"))
                .unwrap(),
            made
        );
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(
        m.observe(&t.launch_id, &ticket_sha(&t).unwrap()).unwrap(),
        made
    );
    let mut changed = t.clone();
    changed.token = "8".repeat(64);
    assert!(m
        .start(&binding, &changed, t.prepared_at_ms, || panic!())
        .is_err());
    changed = t.clone();
    changed.prepared_at_ms += 1;
    assert!(m
        .start(&binding, &changed, changed.prepared_at_ms, || panic!())
        .is_err());
}
#[test]
fn known_failure_and_unknown_creation_never_reopen_the_slot() {
    for may_have_created in [false, true] {
        let (b, t) = fixture();
        let mut m = Machine::default();
        let failed = m
            .start(&b, &t, t.prepared_at_ms, || {
                Err(CreationFailure {
                    reason: "injected OS boundary error".into(),
                    may_have_created,
                })
            })
            .unwrap();
        assert_eq!(
            failed.phase,
            if may_have_created {
                KeeperLaunchPhase::Indeterminate
            } else {
                KeeperLaunchPhase::Failed
            }
        );
        assert!(failed.process.is_none());
        assert_eq!(
            m.start(&b, &t, t.prepared_at_ms, || panic!("no creation replay"))
                .unwrap(),
            failed
        );
        assert!(m
            .ready(&t.launch_id, &ticket_sha(&t).unwrap(), &process())
            .is_err());
        let mut different = t.clone();
        different.launch_id = uuid::Uuid::new_v4().to_string();
        assert!(m
            .start(&b, &different, t.prepared_at_ms, || panic!())
            .is_err());
    }
}
#[test]
fn ready_confirmation_is_exact_idempotent_and_never_creates() {
    let (b, t) = fixture();
    let mut m = Machine::default();
    let s = m.start(&b, &t, t.prepared_at_ms, || Ok(process())).unwrap();
    assert!(!m.successor_confirmed);
    let mut wrong = process();
    wrong.birth = "1000".into();
    assert!(m.ready(&t.launch_id, &s.ticket_sha256, &wrong).is_err());
    assert!(m.ready(&t.launch_id, &"0".repeat(64), &process()).is_err());
    assert!(!m.successor_confirmed);
    let ready = m.ready(&t.launch_id, &s.ticket_sha256, &process()).unwrap();
    assert_eq!(ready.phase, KeeperLaunchPhase::ReadyConfirmed);
    assert!(m.successor_confirmed);
    assert_eq!(
        m.ready(&t.launch_id, &s.ticket_sha256, &process()).unwrap(),
        ready
    );
    assert_eq!(m.start(&b, &t, u64::MAX, || panic!()).unwrap(), ready);
    m.slot.as_mut().unwrap().phase = KeeperLaunchPhase::Exited;
    assert!(
        m.successor_confirmed,
        "Known readiness is not lost when the successor later exits"
    );
    assert!(m.ready(&t.launch_id, &s.ticket_sha256, &process()).is_err());
}
#[test]
fn invalid_scope_files_and_expired_budget_never_reach_create() {
    let (b, t) = fixture();
    for fault in 0..12 {
        let mut t = t.clone();
        match fault {
            0 => t.launch_id = b.launch_id.clone(),
            1 => t.update_id = b.update_id.clone(),
            2 => t.token = "A".repeat(64),
            3 => t.excluded_job = b.job_name.clone(),
            4 => t.image.bytes = 0,
            5 => t.config.bytes = 65537,
            6 => t.stdout_identity = t.stderr_identity.clone(),
            7 => t.config.file_identity = t.image.file_identity.clone(),
            8 => t.excluded_job = "bad\njob".into(),
            9 => t.image.sha256 = "bad".into(),
            10 => t.launch_id = "../bad".into(),
            _ => t.prepared_at_ms = u64::MAX,
        }
        let mut m = Machine::default();
        assert!(
            m.start(&b, &t, 11, || panic!(
                "invalid ticket reached CreateProcess"
            ))
            .is_err(),
            "fault {fault}"
        );
        assert!(m.slot.is_none());
    }
    for at in [
        t.prepared_at_ms - 1,
        t.prepared_at_ms + KEEPER_LAUNCH_STARTUP_MS,
    ] {
        let mut m = Machine::default();
        assert!(m.start(&b, &t, at, || panic!()).is_err());
        assert!(m.slot.is_none());
    }
    assert_eq!(startup_remaining(10, 10).unwrap(), Duration::from_secs(10));
    assert_eq!(
        startup_remaining(10, 10009).unwrap(),
        Duration::from_millis(1)
    );
    assert!(startup_remaining(u64::MAX, 10).is_err());
}
#[test]
fn private_ticket_debug_is_redacted_and_digest_binds_all_fields() {
    let (_, t) = fixture();
    assert!(!format!("{t:?}").contains(&t.token));
    let digest = ticket_sha(&t).unwrap();
    assert!(!digest.contains(&t.token));
    let mut changed = t.clone();
    changed.excluded_job.push('x');
    assert_ne!(ticket_sha(&changed).unwrap(), digest);
    changed = t.clone();
    changed.predecessor = Some(process());
    assert_ne!(ticket_sha(&changed).unwrap(), digest);
    assert_eq!(
        ticket_sha(
            &serde_json::from_slice::<KeeperLaunchTicket>(&serde_json::to_vec(&t).unwrap())
                .unwrap()
        )
        .unwrap(),
        digest
    );
}
