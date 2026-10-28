//! Readonly data for the already authenticated original same-child receiver.
use super::*;
use crate::activation::controller::ActivationControllerState;
use crate::activation::lifetime::{ActivationLifetimeSnapshot, ActivationLifetimeState};
use winbridge_protocol::update::SupervisorProcess;

fn require_issued(
    graph: &ActivationControllerSnapshot,
    record: &ActivationLifetimeRecord,
) -> Result<()> {
    record.validate().map_err(bad)?;
    lifetime::validate_command(&record.command, graph).map_err(bad)?;
    if !matches!(
        graph.controller.state,
        ActivationControllerState::Confirmed { .. }
    ) || record.state != (ActivationLifetimeState::Issued {})
    {
        return Err(bad("Lifetime receiver requires confirmed controller and original Issued lifetime intent; no replay"));
    }
    Ok(())
}

fn require_continuation(
    snapshot: &ActivationLifetimeSnapshot,
    expected: &winbridge_protocol::update::WorkerLifetimeActivationCommand,
    entry: &winbridge_protocol::update::WorkerLifetimeActivationReceipt,
) -> Result<()> {
    snapshot.lifetime.validate().map_err(bad)?;
    lifetime::validate_command(&snapshot.lifetime.command, &snapshot.controller).map_err(bad)?;
    if snapshot.lifetime.command != *expected {
        return Err(bad("Resident lifetime command changed; no rebind"));
    }
    lifetime::validate_receipt(entry, expected).map_err(bad)?;
    if matches!(&snapshot.lifetime.state, ActivationLifetimeState::Confirmed { receipt } if receipt.as_ref() != entry)
    {
        return Err(bad(
            "Confirmed lifetime receipt differs from resident entry",
        ));
    }
    Ok(())
}

impl UpdateJournal {
    /// Continuation of a resident mode only. Unlike initial delivery, a lost
    /// ACK may already be Indeterminate/Confirmed. No record recreates custody.
    pub fn inspect_activation_lifetime_continuation(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &SupervisorProcess,
        expected: &winbridge_protocol::update::WorkerLifetimeActivationCommand,
        entry: &winbridge_protocol::update::WorkerLifetimeActivationReceipt,
    ) -> Result<ActivationLifetimeSnapshot> {
        let snapshot = Self::lifetime_receiver_snapshot(directory, id, attempt, actual)?;
        require_continuation(&snapshot, expected, entry)?;
        Ok(snapshot)
    }

    /// A snapshot is not a live receiver/keeper permit. The caller must already
    /// own the same RunningLocalService, OS-authenticated parent/channel and
    /// ORIGINAL monotonic deadline, checked before and after this read.
    pub fn inspect_activation_lifetime_receiver(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &SupervisorProcess,
    ) -> Result<ActivationLifetimeSnapshot> {
        let snapshot = Self::lifetime_receiver_snapshot(directory, id, attempt, actual)?;
        require_issued(&snapshot.controller, &snapshot.lifetime)?;
        Ok(snapshot)
    }

    fn lifetime_receiver_snapshot(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &SupervisorProcess,
    ) -> Result<ActivationLifetimeSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        if schema(&tx)? != UPDATE_SCHEMA_VERSION || !present(&tx, id)? {
            return Err(bad(
                "Lifetime receiver requires exact current schema and committed intent",
            ));
        }
        let graph = activation_controller::current_snapshot(&tx, id, attempt, actual)?;
        let record = ledger(&tx, id, &graph)?;
        record.validate().map_err(bad)?;
        lifetime::validate_command(&record.command, &graph).map_err(bad)?;
        tx.commit().map_err(storage)?;
        Ok(ActivationLifetimeSnapshot {
            controller: graph,
            lifetime: record,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resident_continuation_keeps_same_command_after_lost_ack_or_confirmation() {
        let (graph, command, receipt) = lifetime::test_fixture();
        let issued = ActivationLifetimeRecord::issued(command.clone()).unwrap();
        let unknown = issued
            .indeterminate("ACK unknown".into(), issued.updated_at_ms + 1)
            .unwrap();
        let confirmed = unknown
            .confirmed(receipt.clone(), unknown.updated_at_ms + 1)
            .unwrap();
        for record in [issued, unknown, confirmed] {
            let snapshot = ActivationLifetimeSnapshot {
                controller: graph.clone(),
                lifetime: record,
            };
            require_continuation(&snapshot, &command, &receipt).unwrap();
            let mut other = command.clone();
            other.command_id = uuid::Uuid::new_v4().to_string();
            assert!(require_continuation(&snapshot, &other, &receipt).is_err());
            if matches!(
                snapshot.lifetime.state,
                ActivationLifetimeState::Confirmed { .. }
            ) {
                let mut changed = receipt.clone();
                changed.compatibility.observed_at_ms += 1;
                lifetime::validate_receipt(&changed, &command).unwrap();
                assert!(require_continuation(&snapshot, &command, &changed).is_err());
            }
        }
    }
    #[test]
    fn lifetime_receiver_data_requires_issued_intent_and_exact_prefix() {
        let (graph, command, receipt) = lifetime::test_fixture();
        let issued = ActivationLifetimeRecord::issued(command).unwrap();
        require_issued(&graph, &issued).unwrap();
        let unknown = issued
            .indeterminate("ACK unknown".into(), issued.updated_at_ms + 1)
            .unwrap();
        let confirmed = issued.confirmed(receipt, issued.updated_at_ms + 2).unwrap();
        for saved in [unknown, confirmed] {
            assert!(require_issued(&graph, &saved).is_err());
        }
        let mut changed = graph.clone();
        changed.controller.updated_at_ms += 1;
        assert!(require_issued(&changed, &issued).is_err());
        let mut changed = graph;
        changed.local.revision = 6;
        assert!(require_issued(&changed, &issued).is_err());
    }
}
