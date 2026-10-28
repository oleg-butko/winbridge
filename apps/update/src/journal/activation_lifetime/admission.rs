//! Original custody only: durable intent before a single future private send.
use super::*;
use crate::activation::{lifetime::*, ActivationPreparation, ActivationRecord};
use winbridge_protocol::update::{
    ActivationControllerObservation, SupervisorRecord, WorkerControllerActivationTopology,
};

impl UpdateJournal {
    /// A current owned snapshot for observation, not a delivery grant. In
    /// particular, the controller prefix is already Confirmed, not Issued.
    pub(crate) fn activation_lifetime_owned_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        key: &str,
    ) -> Result<ActivationLifetimeSnapshot> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let controller = activation_controller::owned_snapshot(&tx, a, s, preparation, local)?;
        let lifetime = ledger(&tx, &s.spec.update_id, &controller)?;
        let snapshot = ActivationLifetimeSnapshot {
            controller,
            lifetime,
        };
        super::custody::require_observation_key(&snapshot, key)?;
        tx.commit().map_err(storage)?;
        Ok(snapshot)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_activation_lifetime(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        observed: &ActivationControllerObservation,
        topology: &WorkerControllerActivationTopology,
        key: &str,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<ActivationLifetimeAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(saved) = read(&tx, &s.spec.update_id)? {
            let graph = activation_controller::snapshot(&tx, &s.spec.update_id)?;
            return existing_for_request(saved, key, topology, &graph)
                .map(|v| ActivationLifetimeAdmission::Existing(Box::new(v)));
        }
        let graph = activation_controller::owned_snapshot(&tx, a, s, preparation, local)?;
        lifetime::validate_controller_prefix(&graph, topology, observed).map_err(bad)?;
        let command =
            lifetime::prepare_command(&graph, key, now_ms()?.max(graph.controller.updated_at_ms))
                .map_err(bad)?;
        let record = ActivationLifetimeRecord::issued(command).map_err(bad)?;
        seed_checked(&tx, &record, &graph, || {
            checkpoint().map_err(bad)?;
            preparation.require_live_window().map_err(bad)
        })?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Lifetime intent commit unconfirmed; observe same command without replay: {e}"
            ))
        })?;
        Ok(ActivationLifetimeAdmission::Issued(Box::new(
            ActivationLifetimePermit {
                record,
                topology: topology.clone(),
            },
        )))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn require_activation_lifetime_issued(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        observed: &ActivationControllerObservation,
        permit: &ActivationLifetimePermit,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let graph = activation_controller::owned_snapshot(&tx, a, s, preparation, local)?;
        lifetime::validate_controller_prefix(&graph, permit.topology(), observed).map_err(bad)?;
        if ledger(&tx, &s.spec.update_id, &graph)? != permit.record
            || permit.record.state != (ActivationLifetimeState::Issued {})
        {
            return Err(bad(
                "Lifetime is no longer original Issued; no send/reissue",
            ));
        }
        preparation.require_live_window().map_err(bad)?;
        tx.commit().map_err(storage)
    }

    /// Facts may arrive after expiry. This does not mint a new window/permit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_activation_lifetime_owned(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        observed: &ActivationControllerObservation,
        topology: &WorkerControllerActivationTopology,
        next: &ActivationLifetimeRecord,
    ) -> Result<ActivationLifetimeRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let graph = activation_controller::owned_snapshot(&tx, a, s, preparation, local)?;
        lifetime::validate_controller_prefix(&graph, topology, observed).map_err(bad)?;
        let old = ledger(&tx, &s.spec.update_id, &graph)?;
        if old == *next {
            return Ok(old);
        }
        persist(&tx, &old, next, &graph)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Lifetime fact commit unconfirmed; observe without replay: {e}"
            ))
        })?;
        Ok(next.clone())
    }
}
