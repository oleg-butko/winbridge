//! Internal29 client: one original finalization or readonly reconciliation.
use super::*;
impl SupervisorHandle {
    pub async fn finalize_activation(
        &self,
        observe: bool,
    ) -> Result<Option<UpdatePreparationView>> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Supervisor exact process changed".into());
        }
        let request = SupervisorRequest {
            contract_version: 29,
            launch_id: self.record.spec.launch_id.clone(),
            expected_revision: self.record.revision,
            action: if observe {
                SupervisorAction::ObserveActivationFinalization
            } else {
                SupervisorAction::FinalizeActivation
            },
            token: self.token.clone(),
            worker_command: None,
        };
        let response: SupervisorResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &request,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if response.contract_version != 29
            || response.launch_id != request.launch_id
            || response.record.is_some()
            || response.worker_control.is_some()
        {
            return Err("Finalization reply scope/version changed; no fallback".into());
        }
        if let Some(error) = response.error {
            return Err(error);
        }
        if let Some(view) = response.preparation {
            if view.schema_version != 21
                || view.activation_finalization.is_none()
                || view.update.update_id != self.record.spec.update_id
            {
                return Err("Finalization view belongs to another contract/update".into());
            }
            Ok(Some(view))
        } else {
            Ok(None)
        }
    }
}
