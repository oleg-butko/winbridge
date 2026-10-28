//! Atomic local-proof/root transition. Only original native custody can call it.
use super::*;
#[cfg(any(windows, test))]
fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::PreconditionsChanged, message)
}

#[cfg(any(windows, test))]
fn write_root(
    c: &Connection,
    a: &UpdateAuthority,
    old: &UpdateRecord,
    key: &str,
    at: u64,
) -> Result<UpdateRecord> {
    if old.phase != UpdatePhase::VerifyingLocal || record(c, &old.update_id)? != *old {
        return Err(bad("Local proof requires the exact VerifyingLocal root"));
    }
    let command=UpdateCommand { idempotency_key:format!("local-service-verified-{key}"),expected_revision:old.revision,event:winbridge_protocol::update::UpdateEvent::LocalHealthVerified,reason:"Локальный Worker и IPC проверены; состояние controller и передачи владения показано отдельно, обновление ещё не завершено".into() };
    supervisor::require_driver(c, a, &old.update_id, command.event)?;
    let mut root = old.clone();
    root.phase = advance_update(root.phase, command.event).map_err(bad)?;
    root.revision = root
        .revision
        .checked_add(1)
        .filter(|v| *v <= MAX_UPDATE_COUNTER)
        .ok_or_else(|| bad("Local root revision exhausted"))?;
    root.updated_at_ms = at.max(old.updated_at_ms);
    root.reason = Some(command.reason.clone());
    let raw = encode(&root)?;
    one(c.execute("UPDATE updates SET record=?1,revision=?2,phase=?3,active=?4 WHERE id=?5 AND revision=?6",params![raw,root.revision as i64,encode(&root.phase)?,update_blocks_installation(root.phase),root.update_id,old.revision as i64]).map_err(storage)?)?;
    let edge = UpdateTransition {
        sequence: 0,
        update_id: root.update_id.clone(),
        revision: root.revision,
        previous_phase: Some(old.phase),
        phase: root.phase,
        occurred_at_ms: root.updated_at_ms,
        owner_epoch: a.epoch(),
        owner_id: a.owner_id().into(),
        command: Some(command.clone()),
    };
    append_transition(c, &edge, Some(&encode(&command)?), &raw)?;
    if record(c, &root.update_id)? != root {
        return Err(bad("Local root/history readback differs"));
    }
    Ok(root)
}

impl UpdateJournal {
    #[cfg(windows)]
    pub(crate) fn finish_local_activation(
        &mut self,
        a: &UpdateAuthority,
        s: &winbridge_protocol::update::SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        old: &crate::activation::ActivationRecord,
        confirmed: &crate::activation::service::ActivationServiceRecord,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<crate::activation::ActivationRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let replacement = activation::owned_current(&tx, a, s, preparation, old)?;
        let saved = activation_service::read(&tx, &s.spec.update_id)?
            .ok_or_else(|| bad("Confirmed service evidence missing"))?;
        if saved != *confirmed || old.revision != 6 {
            return Err(bad(
                "Service confirmation/current original activation differs",
            ));
        }
        let crate::activation::service::ActivationServiceState::Confirmed { receipt } =
            &saved.state
        else {
            return Err(bad("Service confirmation required before local health"));
        };
        let at = now_ms()?.max(saved.updated_at_ms).max(old.updated_at_ms);
        let next = old
            .local_verified(receipt.compatibility.clone(), at, &replacement)
            .map_err(bad)?;
        let root = record(&tx, &s.spec.update_id)?;
        checkpoint().map_err(bad)?;
        activation::persist(&tx, old, &next, &replacement)?;
        write_root(&tx, a, &root, &saved.command.command_id, at)?;
        checkpoint().map_err(bad)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Local proof/root commit unconfirmed; inspect both records, no replay: {e}"
            ))
        })?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_protocol::update::*;
    // Legal lifecycle metadata only. This never simulates a fresh OS grant.
    fn fixture() -> (
        tempfile::TempDir,
        UpdateJournal,
        UpdateAuthority,
        UpdateRecord,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let mut j = UpdateJournal::open(temp.path(), &"a".repeat(64)).unwrap();
        let info = j.info().unwrap();
        let a = j.acquire_owner().unwrap();
        let request = UpdateRequest {
            contract_version: 1,
            idempotency_key: "metadata-local-root".into(),
            source: UpdateSourcePin {
                archive_name: "winbridge-source.zip".into(),
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
        let mut root = j.accept(&a, &request, &request.expected).unwrap();
        for event in [
            UpdateEvent::BeginPreparation,
            UpdateEvent::SourceVerified,
            UpdateEvent::BuildSucceeded,
            UpdateEvent::CandidateAccepted,
            UpdateEvent::BeginDrain,
            UpdateEvent::Quiesce {
                dispatch_closed: true,
                accepted_operations: 0,
                physical_owners: 0,
                uncertain_owners: 0,
            },
            UpdateEvent::BeginShutdown,
            UpdateEvent::ShutdownVerified,
            UpdateEvent::FilesVerified,
        ] {
            let previous = root.phase;
            let rev = root.revision;
            root.phase = advance_update(root.phase, event).unwrap();
            root.revision += 1;
            let cmd = UpdateCommand {
                idempotency_key: format!("metadata-{rev}"),
                expected_revision: rev,
                event,
                reason: "Lifecycle metadata; no physical proof".into(),
            };
            let raw = encode(&root).unwrap();
            j.connection
                .execute(
                    "UPDATE updates SET record=?1,revision=?2,phase=?3,active=1 WHERE id=?4",
                    params![
                        raw,
                        root.revision as i64,
                        encode(&root.phase).unwrap(),
                        root.update_id
                    ],
                )
                .unwrap();
            append_transition(
                &j.connection,
                &UpdateTransition {
                    sequence: 0,
                    update_id: root.update_id.clone(),
                    revision: root.revision,
                    previous_phase: Some(previous),
                    phase: root.phase,
                    occurred_at_ms: root.updated_at_ms,
                    owner_epoch: a.epoch(),
                    owner_id: a.owner_id().into(),
                    command: Some(cmd.clone()),
                },
                Some(&encode(&cmd).unwrap()),
                &raw,
            )
            .unwrap();
        }
        j.connection
            .execute_batch(
                "CREATE TABLE coupled_proof(revision INTEGER);INSERT INTO coupled_proof VALUES(6);",
            )
            .unwrap();
        (temp, j, a, root)
    }
    #[test]
    fn local_root_and_coupled_proof_roll_back_when_root_or_history_write_is_ignored() {
        for deny in ["updates", "transitions"] {
            let (_temp, mut j, a, root) = fixture();
            let event = if deny == "updates" {
                "UPDATE"
            } else {
                "INSERT"
            };
            j.connection.execute_batch(&format!("CREATE TRIGGER deny_local BEFORE {event} ON {deny} BEGIN SELECT RAISE(IGNORE);END;")).unwrap();
            {
                let tx = j.connection.transaction().unwrap();
                tx.execute("UPDATE coupled_proof SET revision=7", [])
                    .unwrap();
                assert!(write_root(&tx, &a, &root, "fixture", root.updated_at_ms).is_err());
            }
            assert_eq!(record(&j.connection, &root.update_id).unwrap(), root);
            assert_eq!(
                j.connection
                    .query_row("SELECT revision FROM coupled_proof", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                6
            );
        }
    }
    #[test]
    fn local_root_enters_awaiting_controller_once_and_stale_root_cannot_advance_again() {
        let (_temp, mut j, a, root) = fixture();
        let next = {
            let tx = j.connection.transaction().unwrap();
            let next = write_root(&tx, &a, &root, "fixture", root.updated_at_ms).unwrap();
            tx.commit().unwrap();
            next
        };
        assert_eq!(next.phase, UpdatePhase::AwaitingController);
        assert_eq!(next.revision, root.revision + 1);
        assert!(write_root(&j.connection, &a, &root, "fixture", root.updated_at_ms).is_err());
        assert!(write_root(&j.connection, &a, &next, "fixture", next.updated_at_ms).is_err());
    }
}
