use winbridge_protocol::*;
#[test]
fn queue_metadata_matches_the_shared_typed_reader_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/discovery/queue-control-cases.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut info = fixture["baseInfo"].clone();
        if let Some(pointer) = case["pointer"].as_str() {
            if case["remove"] == true {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                info.pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
            } else {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                let target = info.pointer_mut(parent).unwrap();
                if let Some(object) = target.as_object_mut() {
                    object.insert(key.into(), case["value"].clone());
                } else {
                    *target.get_mut(key.parse::<usize>().unwrap()).unwrap() = case["value"].clone();
                }
            }
        }
        let info: WorkerInfo = serde_json::from_value(info).unwrap();
        assert_eq!(
            interpret_queue_control(&info).is_some(),
            case["expected"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn strict_queue_commands_and_legacy_hashes_stay_separate() {
    for revision in ["-1", "01", "1.0", "9223372036854775808"] {
        assert!(!valid_queue_revision(revision));
    }
    let request = serde_json::json!({"queueControlVersion":1,"context":{"instanceId":"local","journalIncarnation":"journal"},"command":{"kind":"set","commandId":"pause","expectedRevision":"0","mode":"paused"}});
    assert!(serde_json::from_value::<QueueControlRequest>(request.clone()).is_ok());
    let mut bad = request;
    bad["command"]["future"] = true.into();
    assert!(serde_json::from_value::<QueueControlRequest>(bad).is_err());
    #[cfg(feature = "jsonschema")]
    {
        assert_eq!(
            fenced_rpc_schema_sha256(),
            "dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12"
        );
        assert_eq!(
            operation_schema_sha256(),
            "7c078b1bfedbdf2b1488fe782f6ab52e35ba33f9b8f28e560aa683458d9c9749"
        );
        assert_eq!(
            work_scheduling_schema_sha256(),
            "d89abdc35d712b7cced4a8c922ece1c7dca469b330c7160ec1df356f12a00bc8"
        );
    }
}

#[test]
fn queue_receipt_validation_binds_the_original_command_without_rebinding_state() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/discovery/queue-control-cases.json"
    ))
    .unwrap();
    let info: WorkerInfo = serde_json::from_value(fixture["baseInfo"].clone()).unwrap();
    let mut observation = interpret_queue_control(&info).unwrap();
    let request = QueueControlRequest {
        queue_control_version: 1,
        context: observation.context.clone(),
        command: QueueControlCommand::Set {
            command_id: "pause".into(),
            expected_revision: "6".into(),
            mode: QueueMode::Paused,
        },
    };
    let receipt = QueueControlReceipt {
        context: request.context.clone(),
        command_id: "pause".into(),
        from_revision: "6".into(),
        to_revision: "7".into(),
        mode: QueueMode::Paused,
        changed: true,
        changed_at_ms: 123,
    };
    let make = |o: QueueControlObservation, r: QueueControlReceipt| QueueControlResponse {
        queue_control_version: 1,
        context: Some(request.context.clone()),
        result: QueueControlResult::Applied {
            observation: o,
            receipt: r,
        },
    };
    make(observation.clone(), receipt.clone())
        .validate_for(&request)
        .unwrap();
    observation.revision = "8".into();
    observation.mode = QueueMode::Running;
    observation.blockers = queue_blockers(
        observation.mode,
        observation.policy.as_ref(),
        observation.pending_roots,
        &observation.projects,
    );
    make(observation.clone(), receipt.clone())
        .validate_for(&request)
        .unwrap(); // Historical receipt does not overwrite a newer state.
    let mut bad = receipt.clone();
    bad.context.journal_incarnation = "foreign".into();
    assert!(make(observation.clone(), bad)
        .validate_for(&request)
        .is_err());
    let mut bad = receipt.clone();
    bad.command_id = "other".into();
    assert!(make(observation.clone(), bad)
        .validate_for(&request)
        .is_err());
    let mut bad = receipt.clone();
    bad.to_revision = "8".into();
    assert!(make(observation.clone(), bad)
        .validate_for(&request)
        .is_err());
    let mut bad = receipt;
    bad.changed_at_ms = 9_007_199_254_740_992;
    assert!(make(observation, bad).validate_for(&request).is_err());
}
