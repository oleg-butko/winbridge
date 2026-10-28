//! Intent before effect, same original native owner; no public JSON writer.
use super::*;
use crate::activation::{controller::*, ActivationPreparation};
use winbridge_protocol::update::{SupervisorRecord, WorkerControllerActivationTopology};

impl UpdateJournal {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_activation_controller(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        key: &str,
        topology: &WorkerControllerActivationTopology,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<ActivationControllerAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(value) = read(&tx, &s.spec.update_id)? {
            return existing_for_key(value, key, topology)
                .map(|v| ActivationControllerAdmission::Existing(Box::new(v)));
        }
        let graph = custody::owned_local(&tx, a, s, preparation, local)?;
        let command = controller::prepare_command(
            &graph.physical,
            &graph.writer,
            &graph.service,
            &graph.local,
            topology.clone(),
            key,
            now_ms()?
                .max(local.updated_at_ms)
                .max(graph.service.updated_at_ms),
        )
        .map_err(bad)?;
        let value = ActivationControllerRecord::issued(command).map_err(bad)?;
        seed_checked(&tx, &value, &graph, || {
            checkpoint().map_err(bad)?;
            preparation.require_live_window().map_err(bad)
        })?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Controller intent commit unconfirmed; observe same command, no send/replay: {e}"
            ))
        })?;
        Ok(ActivationControllerAdmission::Issued(Box::new(
            ActivationControllerPermit { record: value },
        )))
    }

    pub(crate) fn require_activation_controller_issued(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        permit: &ActivationControllerPermit,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        let graph = custody::owned_local(&tx, a, s, preparation, local)?;
        if ledger(&tx, &s.spec.update_id, &graph)? != permit.record
            || permit.record.state != (ActivationControllerState::Issued {})
        {
            return Err(bad(
                "Controller intent is no longer original Issued; no send",
            ));
        }
        preparation.require_live_window().map_err(bad)?;
        tx.commit().map_err(storage)
    }

    pub(crate) fn append_activation_controller_owned(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &ActivationPreparation,
        local: &ActivationRecord,
        next: &ActivationControllerRecord,
    ) -> Result<ActivationControllerRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let graph = custody::owned_local(&tx, a, s, preparation, local)?;
        let old = ledger(&tx, &s.spec.update_id, &graph)?;
        if old == *next {
            return Ok(old);
        }
        persist(&tx, &old, next, &graph)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Controller result commit unconfirmed; observe same command, no replay: {e}"
            ))
        })?;
        Ok(next.clone())
    }
}
