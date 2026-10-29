//! Real IPC peer, synthetic lifecycle: metadata must never mint an Active fact.
use super::finalization_service::active::ActiveReadiness;
use super::*;

#[tokio::test]
async fn observation_rejects_mutating_envelopes_and_foreign_actual_peer_without_changing_pending() {
    let root = tempfile::tempdir().unwrap();
    let (running, owner, _) = super::active_tests::pending(root.path()).await;
    let worker = &running.service.worker;
    let authority = worker.store().unwrap().state_backup_authority().unwrap();
    let query = ActivationFinalizationQuery {
        finalization_observation_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: owner.command.clone(),
    };
    for mutation in 0..7 {
        let mut raw = serde_json::to_value(&query).unwrap();
        match mutation {
            0 => {}
            1 => raw["force"] = true.into(),
            2 => raw["release"] = true.into(),
            3 => raw["finalizationObservationVersion"] = true.into(),
            4 => raw["command"]["active"] = true.into(),
            5 => raw["residentObservationVersion"] = 7.into(),
            _ => raw["request"] = serde_json::json!({"kind":"queue_resume"}),
        }
        let reply: ActivationFinalizationObservation = winbridge_transport::supervisor::call(
            &running.endpoint().unwrap(),
            std::process::id(),
            &raw,
        )
        .await
        .unwrap();
        let ActivationFinalizationObservationResult::Rejected { message } = reply.result else {
            panic!("Must reject malformed or foreign keeper");
        };
        if mutation == 0 {
            assert_eq!(reply.request_id.as_ref(), Some(&query.request_id));
            assert!(message.contains("actual original keeper peer"));
        }
        assert!(worker.activation_read_only());
        assert!(running.service._config_pin.is_some());
        assert_eq!(
            worker.store().unwrap().state_backup_authority().unwrap(),
            authority
        );
        worker.check_local_service_scope().unwrap();
    }
    running.close().await.unwrap();
}

#[tokio::test]
async fn observed_sql_and_fixture_active_cannot_authenticate_a_foreign_keeper_query() {
    let root = tempfile::tempdir().unwrap();
    let (mut running, owner, _) = super::active_tests::pending(root.path()).await;
    let sql = super::active_tests::commit(&running, &owner);
    running
        .seal_active_commit(sql, ActiveReadiness::fixture(Arc::clone(&owner)), || Ok(()))
        .unwrap();
    let query = ActivationFinalizationQuery {
        finalization_observation_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: owner.command.clone(),
    };
    let reply: ActivationFinalizationObservation = winbridge_transport::supervisor::call(
        &running.endpoint().unwrap(),
        std::process::id(),
        &query,
    )
    .await
    .unwrap();
    assert!(matches!(
        reply.result,
        ActivationFinalizationObservationResult::Rejected { .. }
    ));
    assert!(!running.service.worker.activation_read_only());
    assert!(running.service._config_pin.is_none());
    running.close().await.unwrap();
}
