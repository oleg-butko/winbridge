//! Main27: real supervisor mailbox/child/Job/normal IPC, original window frozen.
use super::*;
use winbridge_update::activation::lifetime::{ActivationLifetimeRecord, ActivationLifetimeState};

fn history(directory: &Path, id: &str) -> Vec<ActivationLifetimeRecord> {
    let c = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q = c
        .prepare("SELECT record FROM activation_lifetime_history WHERE id=?1 ORDER BY revision")
        .unwrap();
    q.query_map([id], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}
fn observe(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> UpdatePreparationView {
    let until = Instant::now() + Duration::from_secs(300);
    loop {
        if let Some(view) = call(rt, sup, q) {
            assert!(
                !view.driver_active && view.driver_error.is_none(),
                "Lifetime failed: {:?}",
                view.driver_error
            );
            assert_eq!(view.schema_version, 19);
            assert_eq!(
                view.activation_lifetime.as_ref().unwrap().phase,
                ActivationLifetimePhase::Observed
            );
            return view;
        }
        assert!(
            Instant::now() < until,
            "Observe the same lifetime attempt, never resend"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub(in super::super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    ready_loss: bool,
) -> serde_json::Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut q: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    q.contract_version = 27;
    q.action = SupervisorAction::EnterActivationLifetime;
    let directory = root.runtime().join("update-state");
    let id = &sup.spec.update_id;
    if ready_loss {
        let original = UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .unwrap();
        let hash = canonical_record_hash(&original);
        // Separate create-new input; historical state/writer trace bytes stay frozen.
        write(
            &root
                .runtime()
                .join("update")
                .join(id)
                .join("lifetime-ready-loss-validation.json"),
            &json!({"schemaVersion":1,"kind":"winbridge_lifetime_ready_body_loss_validation","recordSha256":hash}),
        );
    }
    // Drop the supervisor dispatch ACK. Main28 separately truncates private Ready.
    // No audit before dispatch may consume the remaining entry window.
    dispatch_once(&rt, sup, &q);
    let unknown = if ready_loss {
        Some(wait_for_unknown(&rt, root, sup, &q))
    } else {
        None
    };
    q.action = SupervisorAction::ObserveActivationLifetime;
    let entered = observe(&rt, sup, &q);
    let saved = UpdateJournal::inspect_activation_lifetime(&directory, id)
        .unwrap()
        .unwrap();
    let ActivationLifetimeState::Confirmed { receipt } = &saved.state else {
        panic!("Lifetime entry missing");
    };
    let physical = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    let controller = UpdateJournal::inspect_activation_controller(&directory, id)
        .unwrap()
        .unwrap();
    let rows = history(&directory, id);
    assert_eq!(rows.len(), if ready_loss { 3 } else { 2 });
    let child =
        infrastructure::PinnedProcess::live(&receipt.compatibility.identity.process).unwrap();
    child
        .require_job_membership(&saved.command.binding.job_name)
        .unwrap();
    assert_eq!(
        physical.state_owned.as_ref().unwrap().process,
        *child.identity()
    );
    let bytes = journal_bytes(root);
    let deadline = saved.command.deadline_at_ms;
    assert_eq!(
        deadline,
        physical.intent.admitted_at_ms + physical.intent.context.startup_timeout_ms
    );
    assert_eq!(physical.intent.context.startup_timeout_ms, 240_000);
    eprintln!("Main27 lifetime entered; same child {}; wait for original deadline {deadline} without renewal", child.identity().process_id);
    let wait_limit = Instant::now() + Duration::from_secs(245);
    while std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        <= deadline
    {
        assert!(
            Instant::now() < wait_limit,
            "Clock rollback must not create an unbounded test wait"
        );
        child.require_live().unwrap();
        std::thread::sleep(Duration::from_millis(250));
    }
    let after_expiry = observe(&rt, sup, &q);
    let lifetime = after_expiry.activation_lifetime.as_ref().unwrap();
    assert_eq!(lifetime.saved_receipt.as_ref(), Some(receipt.as_ref()));
    assert_eq!(lifetime.observed_receipt.as_ref(), Some(receipt.as_ref()));
    assert!(lifetime.observed_at_ms.unwrap() > deadline);
    assert_eq!(lifetime.original_custody_held, Some(true));
    assert_eq!(lifetime.process_alive, Some(true));
    child
        .require_job_membership(&saved.command.binding.job_name)
        .unwrap();
    let service_command = &controller.command.binding.service_command;
    let normal: ActivationLifetimeObservation = service::normal(
        &rt,
        service_command,
        &ActivationLifetimeQuery {
            lifetime_observation_version: 1,
            command: saved.command.clone(),
        },
    );
    let ActivationLifetimeObservationResult::Observed {
        receipt: actual,
        observed_at_ms,
    } = &normal.result
    else {
        panic!("Actual normal endpoint missing lifetime");
    };
    assert_eq!(actual, receipt);
    assert!(*observed_at_ms > deadline);
    let info: winbridge_protocol::LocalResponse = service::normal(
        &rt,
        service_command,
        &winbridge_protocol::LocalRequest::Info,
    );
    let winbridge_protocol::LocalResponse::Info { info } = info else {
        panic!("Normal Info missing");
    };
    assert!(!info.maintenance_available && info.active_task_count == Some(0));
    assert_eq!(info.generation, receipt.compatibility.generation);
    assert!(
        !info
            .fenced_requests
            .as_ref()
            .unwrap()
            .legacy_mutations_allowed
    );
    let submission = winbridge_protocol::TaskSubmission {
        protocol_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        project_id: "source".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        timeout_ms: 1000,
        snapshot_id: None,
        command: winbridge_protocol::CommandSource::Direct {
            executable: "never-run".into(),
            arguments: vec![],
            working_directory: ".".into(),
        },
    };
    let refused: winbridge_protocol::LocalResponse = service::normal(
        &rt,
        service_command,
        &winbridge_protocol::LocalRequest::Submit {
            request: submission,
        },
    );
    assert!(matches!(
        refused,
        winbridge_protocol::LocalResponse::Error { .. }
    ));
    q.action = SupervisorAction::EnterActivationLifetime;
    let duplicate = call(&rt, sup, &q).unwrap();
    assert_eq!(
        serde_json::to_value(&duplicate.activation_lifetime).unwrap(),
        serde_json::to_value(&after_expiry.activation_lifetime).unwrap()
    );
    assert_eq!(history(&directory, id), rows);
    assert_eq!(
        UpdateJournal::inspect_activation(&directory, id)
            .unwrap()
            .as_ref(),
        Some(&physical)
    );
    assert_eq!(journal_bytes(root), bytes);
    assert_eq!(after_expiry.update.phase, UpdatePhase::AwaitingController);
    let mut result = json!({"record":saved,"history":rows,"entered":entered,"afterExpiry":after_expiry,"duplicate":duplicate,
        "normalObservation":normal,"normalInfo":info,"physicalRecord":physical,"workerBytesAfter":bytes,
        "lostLifetimeDispatchAckSentOnce":true,"savedLifetimeCommandCount":1,"submitRefusedAfterExpiry":true,
        "sameJobVerified":true,"rootCommitted":false,"maintenanceReleased":false,"privateReadyLossTested":ready_loss});
    if let Some(unknown) = unknown {
        result["readyLoss"] = unknown;
    }
    result
}

fn canonical_record_hash(record: &winbridge_update::activation::ActivationRecord) -> String {
    let mut bytes = b"winbridge.activation.barrier.record.v1\n".to_vec();
    bytes.extend(
        winbridge_protocol::canonical_evidence_json(&serde_json::to_value(record).unwrap())
            .unwrap(),
    );
    winbridge_update::sha256(&bytes)
}
fn wait_for_unknown(
    rt: &tokio::runtime::Runtime,
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    q: &SupervisorRequest,
) -> serde_json::Value {
    let directory = root.runtime().join("update-state");
    let id = &sup.spec.update_id;
    let until = Instant::now() + Duration::from_secs(300);
    let unknown = loop {
        if let Some(saved) = UpdateJournal::inspect_activation_lifetime(&directory, id).unwrap() {
            if matches!(saved.state, ActivationLifetimeState::Indeterminate { .. }) {
                break saved;
            }
            assert!(
                matches!(saved.state, ActivationLifetimeState::Issued {}),
                "Fault must not confirm before independent reconciliation"
            );
        }
        assert!(
            Instant::now() < until,
            "Observe original uncertainty; never resend"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let unknown_view = loop {
        if let Some(v) = call(rt, sup, q) {
            break v;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        !unknown_view.driver_active
            && unknown_view
                .driver_error
                .as_deref()
                .is_some_and(|e| e.contains("Ready body intentionally unread"))
    );
    let view = unknown_view.activation_lifetime.as_ref().unwrap();
    assert_eq!(view.phase, ActivationLifetimePhase::Indeterminate);
    assert!(
        view.saved_receipt.is_none()
            && view.observed_receipt.is_none()
            && view.observed_at_ms.is_none()
    );
    let rows = history(&directory, id);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1], unknown);
    let physical = UpdateJournal::inspect_activation(&directory, id)
        .unwrap()
        .unwrap();
    let child =
        infrastructure::PinnedProcess::live(&physical.state_owned.as_ref().unwrap().process)
            .unwrap();
    let marker_path = root
        .runtime()
        .join("update")
        .join(id)
        .join("activation-lifetime-ready-body-loss.json");
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(marker_path).unwrap()).unwrap();
    assert_eq!(marker["bodyBytesRead"], 0);
    assert_eq!(marker["readyValidated"], false);
    assert_eq!(marker["recordSha256"], canonical_record_hash(&physical));
    let deadline = unknown.command.deadline_at_ms;
    let bound = Instant::now() + Duration::from_secs(245);
    while std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        <= deadline
    {
        assert!(
            Instant::now() < bound,
            "Fault wait cannot renew execution budget"
        );
        child.require_live().unwrap();
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(
        UpdateJournal::inspect_activation_lifetime(&directory, id)
            .unwrap()
            .unwrap(),
        unknown
    );
    json!({"unknownRecord":unknown,"historyBeforeReconciliation":rows,"unknownView":unknown_view,
        "marker":marker,"reconciliationStartedAfterOriginalDeadline":true,"privateCommandSentOnce":true})
}

pub(in super::super) fn after_owned_exit(
    root: &FixtureLayout,
    id: &str,
    saved: &serde_json::Value,
) -> serde_json::Value {
    let directory = root.runtime().join("update-state");
    let view = UpdateJournal::activation_lifetime_snapshot(&directory, id, false, None).unwrap();
    let lifetime = view.activation_lifetime.as_ref().unwrap();
    assert_eq!(lifetime.phase, ActivationLifetimePhase::NotResident);
    assert!(
        lifetime.resident_process.is_none()
            && lifetime.process_alive.is_none()
            && lifetime.original_custody_held.is_none()
    );
    assert!(lifetime.observed_receipt.is_none() && lifetime.observed_at_ms.is_none());
    assert_eq!(
        serde_json::to_value(history(&directory, id)).unwrap(),
        saved["history"]
    );
    json!({"view":view,"historyUnchanged":true,"automaticColdLaunchCount":0,"noColdLifetimeReplay":true})
}
