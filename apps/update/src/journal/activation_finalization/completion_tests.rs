//! Real SQLite append/rollback with synthetic observations; no native custody.
use super::super::completion::{append_record, confirmation};
use super::*;
fn capture(f: &Fixture) -> FinalizationConfirmationPermit {
    let query = ActivationFinalizationQuery {
        finalization_observation_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: f.intent.command.clone(),
    };
    FinalizationConfirmationPermit {
        observation: ActivationFinalizationObservation {
            finalization_observation_version: 1,
            request_id: Some(query.request_id.clone()),
            result: ActivationFinalizationObservationResult::Observed {
                receipt: Box::new(f.data.receipt.clone()),
                observed_at_ms: f.data.receipt.compatibility.observed_at_ms + 1,
            },
        },
        query,
        observation_deadline: std::time::Instant::now() + winbridge_transport::IPC_TIMEOUT,
    }
}
#[test]
fn independent_confirmation_appends_once_and_does_not_rewrite_the_committed_root() {
    let mut f = fixture();
    let pending = commit(&mut f).unwrap();
    let root = record(&f.c, &f.data.root.update_id).unwrap();
    let permit = capture(&f);
    let at = f.data.receipt.compatibility.observed_at_ms + 2;
    let next = confirmation(&pending, &f.data.shutdown, &permit, at)
        .unwrap()
        .unwrap();
    persist(&mut f, &next).unwrap();
    assert_eq!(get(&f).unwrap(), next);
    assert_eq!(rows(&f), [1, 1, 2]);
    assert_eq!(record(&f.c, &root.update_id).unwrap(), root);
    assert!(!next.blocks_next_update(&f.data.shutdown).unwrap());
    assert!(confirmation(&next, &f.data.shutdown, &permit, at + 1)
        .unwrap()
        .is_none());
    assert!(
        confirmation(&next, &f.data.shutdown, &permit, at - 1).is_err(),
        "Even an existing confirmation must not ignore clock rollback"
    );
    assert_eq!(rows(&f), [1, 1, 2]);
}
#[test]
fn failed_append_checks_and_ignored_writes_roll_back_terminal_fact_and_history() {
    for fault in 0..4 {
        let mut f = fixture();
        let pending = commit(&mut f).unwrap();
        let root = record(&f.c, &f.data.root.update_id).unwrap();
        let permit = capture(&f);
        let next = confirmation(
            &pending,
            &f.data.shutdown,
            &permit,
            f.data.receipt.compatibility.observed_at_ms + 2,
        )
        .unwrap()
        .unwrap();
        if fault < 2 {
            let table = if fault == 0 {
                "activation_finalization_records"
            } else {
                "activation_finalization_history"
            };
            let event = if fault == 0 { "UPDATE" } else { "INSERT" };
            f.c.execute_batch(&format!("CREATE TRIGGER ignore_append BEFORE {event} ON {table} BEGIN SELECT RAISE(IGNORE); END;")).unwrap();
        }
        let tx = f.c.transaction().unwrap();
        let mut checks = 0;
        let result = append_record(&tx, &f.a, &f.data.snapshot, &f.data.shutdown, &next, || {
            checks += 1;
            if fault == 2 && checks == 2 {
                return Err("Original child/Job lost after writes".into());
            }
            if fault == 3 && checks == 2 {
                tx.execute("UPDATE installation SET epoch=epoch+1", [])
                    .unwrap();
            }
            Ok(())
        });
        assert!(result.is_err(), "fault {fault}");
        drop(tx);
        assert_eq!(get(&f).unwrap(), pending);
        assert_eq!(rows(&f), [1, 1, 1]);
        assert_eq!(record(&f.c, &root.update_id).unwrap(), root);
    }
}
#[test]
fn late_capture_cannot_renew_a_budget_or_confirm_a_pending_or_foreign_reply() {
    let mut f = fixture();
    let pending = commit(&mut f).unwrap();
    let at = f.data.receipt.compatibility.observed_at_ms + 2;
    for fault in 0..7 {
        let mut permit = capture(&f);
        let mut now = at;
        match fault {
            0 => permit.observation_deadline = std::time::Instant::now(),
            1 => now = at + winbridge_transport::IPC_TIMEOUT.as_millis() as u64,
            2 => now = 0,
            3 => permit.query.command.command_id = uuid::Uuid::new_v4().to_string(),
            4 => permit.observation.request_id = Some(uuid::Uuid::new_v4().to_string()),
            5 => {
                permit.observation.result = ActivationFinalizationObservationResult::Pending {
                    command_id: pending.command.command_id.clone(),
                    message: "Only SQL release".into(),
                }
            }
            _ => {
                permit.observation.result = ActivationFinalizationObservationResult::Rejected {
                    message: "No live peer".into(),
                }
            }
        }
        assert!(
            confirmation(&pending, &f.data.shutdown, &permit, now).is_err(),
            "fault {fault}"
        );
    }
    let unknown = pending
        .indeterminate("ACK unknown".into(), pending.updated_at_ms + 1)
        .unwrap();
    persist(&mut f, &unknown).unwrap();
    let next = confirmation(&unknown, &f.data.shutdown, &capture(&f), at)
        .unwrap()
        .unwrap();
    persist(&mut f, &next).unwrap();
    assert_eq!(rows(&f), [1, 1, 3]);
    assert!(!next.blocks_next_update(&f.data.shutdown).unwrap());
}
