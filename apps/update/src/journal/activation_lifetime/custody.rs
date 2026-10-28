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

impl UpdateJournal {
    /// A snapshot is not a live receiver/keeper permit. The caller must already
    /// own the same RunningLocalService, OS-authenticated parent/channel and
    /// ORIGINAL monotonic deadline, checked before and after this read.
    pub fn inspect_activation_lifetime_receiver(
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
        require_issued(&graph, &record)?;
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
