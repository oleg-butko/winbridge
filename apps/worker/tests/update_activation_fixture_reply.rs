//! Exercise the same response adapter the native coordinator actually calls.
#[path = "support/activation_reply.rs"]
mod activation_reply;
use winbridge_protocol::update::*;
use winbridge_update::journal::UpdateJournal;

fn fixture(version: u16) -> (SupervisorRequest, SupervisorResponse) {
    let root = tempfile::tempdir().unwrap();
    let mut journal = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
    let owner = journal.acquire_owner().unwrap();
    let info = journal.info().unwrap();
    let request = UpdateRequest {
        contract_version: 1,
        idempotency_key: "fixture-reply".into(),
        source: UpdateSourcePin {
            archive_name: "fixture.zip".into(),
            archive_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            source_commit: "d".repeat(40),
        },
        expected: UpdateInstallationPin {
            installation_id: info.installation_id,
            scope_sha256: info.scope_sha256,
            worker_build_id: "fixture".into(),
            worker_config_sha256: "e".repeat(64),
            baseline_manifest_sha256: "f".repeat(64),
        },
    };
    let record = journal.accept(&owner, &request, &request.expected).unwrap();
    let view = if version == 29 {
        UpdateJournal::activation_finalization_snapshot(root.path(), &record.update_id, false, None)
            .unwrap()
    } else {
        UpdateJournal::activation_resident_snapshot(root.path(), &record.update_id, false, None)
            .unwrap()
    };
    let query = SupervisorRequest {
        contract_version: version,
        launch_id: uuid::Uuid::new_v4().to_string(),
        expected_revision: 1,
        action: if version == 29 {
            SupervisorAction::ObserveActivationFinalization
        } else {
            SupervisorAction::ObserveResidentConnection
        },
        token: "never-used-for-transport".into(),
        worker_command: None,
    };
    let response = SupervisorResponse {
        contract_version: version,
        launch_id: query.launch_id.clone(),
        record: None,
        worker_control: None,
        preparation: Some(view),
        error: None,
    };
    (query, response)
}

#[test]
fn native_adapter_accepts_a_real_compact_v28_view_and_checks_its_block() {
    for version in [28, 29] {
        let (query, response) = fixture(version);
        let id = response
            .preparation
            .as_ref()
            .unwrap()
            .update
            .update_id
            .clone();
        let view = activation_reply::validate(&id, &query, response.clone())
            .unwrap()
            .unwrap();
        assert_eq!(view.schema_version, version - 8);
        if version == 28 {
            assert_eq!(
                view.activation_resident.unwrap().phase,
                ActivationResidentPhase::NotResident
            );
        } else {
            assert_eq!(
                view.activation_finalization.unwrap().phase,
                ActivationFinalizationPhase::NotStarted
            );
        }
        let mut missing = response;
        if version == 28 {
            missing.preparation.as_mut().unwrap().activation_resident = None;
        } else {
            missing
                .preparation
                .as_mut()
                .unwrap()
                .activation_finalization = None;
        }
        assert!(activation_reply::validate(&id, &query, missing).is_err());
    }
}

#[test]
fn native_adapter_rejects_foreign_schema_scope_and_reported_errors() {
    let (query, response) = fixture(28);
    let id = response
        .preparation
        .as_ref()
        .unwrap()
        .update
        .update_id
        .clone();
    for mutation in 0..5 {
        let mut changed = response.clone();
        match mutation {
            0 => changed.preparation.as_mut().unwrap().schema_version = 19,
            1 => {
                changed.preparation.as_mut().unwrap().update.update_id =
                    uuid::Uuid::new_v4().to_string()
            }
            2 => changed.contract_version = 27,
            3 => changed.launch_id = uuid::Uuid::new_v4().to_string(),
            _ => changed.error = Some("Original driver did not confirm observation".into()),
        }
        assert!(
            activation_reply::validate(&id, &query, changed).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn native_adapter_never_treats_unknown_contracts_as_successful_busy_replies() {
    let (mut query, mut response) = fixture(28);
    let id = response
        .preparation
        .as_ref()
        .unwrap()
        .update
        .update_id
        .clone();
    response.preparation = None;
    for version in [0, 1, 23, 24, 25, 26, 27, 28, 29, 30, u16::MAX] {
        query.contract_version = version;
        response.contract_version = version;
        let result = activation_reply::validate(&id, &query, response.clone());
        if (24..=29).contains(&version) {
            assert!(result.unwrap().is_none());
        } else {
            assert!(
                result.is_err(),
                "unknown {version} cannot become a busy acknowledgement"
            );
        }
    }
}
