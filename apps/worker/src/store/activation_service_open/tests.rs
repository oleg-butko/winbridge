//! Real SQLite/instance locks; synthetic runtime identity, never native readiness.
use super::*;
use crate::store::{
    activation_open::tests::{bytes, fixture, lock_available},
    JournalActivationAcceptance,
};
use std::path::Path;
use winbridge_protocol::{update::*, ArtifactRole, LoadedWorkerIdentity, RuntimeImageEvidence};

fn activated(
    root: &Path,
) -> (
    ActivationJournal,
    Box<JournalActivationPermit>,
    WorkerServiceActivationCommand,
) {
    let (old, state, writer, shutdown) = fixture(root);
    let mut journal = ActivationJournal::open(state, writer.clone(), shutdown, || Ok(())).unwrap();
    let JournalActivationAcceptance::Fresh(fresh) = journal.activate(|| Ok(())).unwrap() else {
        panic!("fresh activation");
    };
    drop(old);
    let mut descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Worker);
    descriptor.build.source_dirty = false;
    descriptor.build.source_commit = Some("d".repeat(40));
    descriptor.build.source_manifest_sha256 = Some("e".repeat(64));
    let command = WorkerServiceActivationCommand {
        schema_version: 1,
        command_id: uuid::Uuid::new_v4().to_string(),
        issued_at_ms: crate::store::now_ms().max(fresh.receipt().committed_at_ms),
        deadline_at_ms: writer.deadline_at_ms,
        binding: WorkerServiceActivationBinding {
            writer_command: writer.clone(),
            writer_receipt: fresh.receipt().clone(),
            writer_record_sha256: "f".repeat(64),
            expected_worker: LoadedWorkerIdentity {
                schema_version: 1,
                kind: "loaded_worker_identity".into(),
                process: writer.binding.physical.process.clone(),
                runtime_identity: "synthetic-service-runtime".into(),
                set_id: uuid::Uuid::new_v4().to_string(),
                manifest_sha256: "b".repeat(64),
                descriptor,
                image: RuntimeImageEvidence {
                    path: root.join("worker.exe").to_str().unwrap().into(),
                    file_identity: "synthetic-service-file".into(),
                    bytes: 1,
                    sha256: "a".repeat(64),
                    query_supported: true,
                },
            },
            worker_config_schema: 1,
            protocol_version: winbridge_protocol::PROTOCOL_VERSION,
        },
    };
    (journal, fresh, command)
}

#[test]
fn service_transfer_retains_same_lock_bytes_epoch_generation_and_quiesced_lease() {
    eprintln!(
        "SERVICE_ADMISSION_BUILD {}",
        serde_json::to_string(&winbridge_protocol::build_identity()).unwrap()
    );
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let before = bytes(root.path());
    let expected = fresh.receipt().clone();
    let expected_state = journal.state_observation().clone();
    let mut checks = 0;
    let prepared = journal
        .prepare_local_service(fresh, command.clone(), || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(checks, 2);
    assert!(!lock_available(root.path()));
    assert_eq!(prepared.receipt().unwrap(), Some(expected));
    assert_eq!(prepared.command(), &command);
    assert_eq!(prepared.state_observation(), &expected_state);
    prepared.revalidate_state().unwrap();
    assert_eq!(bytes(root.path()), before);
    // No new journal/schema/authority transaction or hidden recovery has run.
    drop(prepared);
    assert!(lock_available(root.path()));
}

#[test]
fn service_transfer_failure_consumes_fresh_permit_but_retains_exact_state_owner() {
    for failed_check in [1, 2] {
        let root = tempfile::tempdir().unwrap();
        let (journal, fresh, command) = activated(root.path());
        let before = bytes(root.path());
        let expected_state = journal.state_observation().clone();
        let mut checks = 0;
        let failure = journal
            .prepare_local_service(fresh, command, || {
                checks += 1;
                if checks == failed_check {
                    Err("Original parent/window/graph rejected".into())
                } else {
                    Ok(())
                }
            })
            .err()
            .unwrap();
        assert_eq!(checks, failed_check);
        assert!(failure.reason().contains("rejected"));
        assert_eq!(failure.state_observation(), &expected_state);
        failure.revalidate_state().unwrap();
        assert!(!lock_available(root.path()));
        assert_eq!(bytes(root.path()), before);
        drop(failure);
        assert!(lock_available(root.path()));
    }
}

#[test]
fn service_transfer_rejects_foreign_fresh_permit_without_reopening_or_unlocking() {
    let root = tempfile::tempdir().unwrap();
    let foreign_root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let (foreign, foreign_fresh, _) = activated(foreign_root.path());
    let before = bytes(root.path());
    let failure = journal
        .prepare_local_service(foreign_fresh, command, || {
            panic!("foreign receipt rejected first")
        })
        .err()
        .unwrap();
    assert!(!lock_available(root.path()));
    assert_eq!(bytes(root.path()), before);
    failure.revalidate_state().unwrap();
    drop((fresh, foreign));
}

#[test]
fn service_transfer_rejects_rebound_command_authority_scope_and_window() {
    for mutation in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let (journal, fresh, mut command) = activated(root.path());
        let before = bytes(root.path());
        match mutation {
            0 => command.binding.writer_command.command_id = uuid::Uuid::new_v4().to_string(),
            1 => command.binding.writer_receipt.authority.epoch += 1,
            2 => command.binding.expected_worker.process.birth.push('0'),
            _ => command.issued_at_ms = command.deadline_at_ms + 1,
        }
        let failure = journal
            .prepare_local_service(fresh, command, || Ok(()))
            .err()
            .unwrap();
        failure.revalidate_state().unwrap();
        assert!(!lock_available(root.path()));
        assert_eq!(bytes(root.path()), before);
    }
}

#[test]
fn service_transfer_rejects_a_superseded_header_while_retaining_the_same_lock() {
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let c = rusqlite::Connection::open(root.path().join("journal.sqlite")).unwrap();
    c.execute(
        "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
        [uuid::Uuid::new_v4().to_string()],
    )
    .unwrap();
    drop(c);
    let before = bytes(root.path());
    let failure = journal
        .prepare_local_service(fresh, command, || Ok(()))
        .err()
        .unwrap();
    assert!(
        failure
            .reason()
            .contains("stale journal incarnation/owner epoch"),
        "{}",
        failure.reason()
    );
    assert!(!lock_available(root.path()));
    failure.revalidate_state().unwrap();
    assert_eq!(bytes(root.path()), before);
}

#[test]
fn service_transfer_rechecks_authority_after_the_last_external_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let (journal, fresh, command) = activated(root.path());
    let mut checks = 0;
    let failure = journal
        .prepare_local_service(fresh, command, || {
            checks += 1;
            if checks == 2 {
                let c = rusqlite::Connection::open(root.path().join("journal.sqlite")).unwrap();
                c.execute(
                    "UPDATE journal_authority SET epoch=epoch+1,authority=?1 WHERE singleton=1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            Ok(())
        })
        .err()
        .unwrap();
    assert_eq!(checks, 2);
    assert!(
        failure
            .reason()
            .contains("stale journal incarnation/owner epoch"),
        "{}",
        failure.reason()
    );
    assert!(!lock_available(root.path()));
    failure.revalidate_state().unwrap();
}
