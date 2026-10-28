//! Read-only reconciliation under the original owner, never a second V6 send.
use super::*;
use crate::activation::lifetime::{self, ActivationLifetimeState};
use winbridge_protocol::update::ActivationLifetimeObservationResult;

impl ActivationProcessGuard {
    pub fn observe_lifetime_blocking(
        mut self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> std::result::Result<Box<Self>, ActivationLaunchFailure> {
        // Also clear it if the original IO runtime cannot be entered.
        self.lifetime_observation = None;
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.observe_lifetime(j, a, s, key).await {
                Ok(()) => Ok(owner),
                Err(reason) => Err(ActivationLaunchFailure {
                    reason,
                    custody: owner,
                }),
            }
        }) {
            Ok(value) => value,
            Err((owner, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }

    async fn observe_lifetime(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "lifetime_observation",
            Some(self.files.preparation.deadline),
        );
        self.require_lifetime_custody(false)?;
        let snapshot = Box::new(
            j.activation_lifetime_owned_current(a, s, &self.files.preparation, &self.record, key)
                .map_err(|e| e.to_string())?,
        );
        let saved = &snapshot.lifetime;
        let process = self
            .resident_process()
            .ok_or("Original lifetime process missing")?;
        let live = infrastructure::PinnedProcess::live(&process).map_err(|e| e.to_string())?;
        let graph = &snapshot.controller;
        lifetime::validate_command(&saved.command, graph)?;
        let controller = self
            .controller_observation
            .as_ref()
            .ok_or("Original controller observation missing")?;
        let topology = &graph.controller.command.binding.topology;
        lifetime::validate_controller_prefix(graph, topology, controller)?;
        let expected = match &saved.state {
            ActivationLifetimeState::Confirmed { receipt } => Some(receipt.as_ref()),
            _ => None,
        };
        timing.mark("original_graph_checked", None, None);
        let checkpoint = || {
            self.require_lifetime_custody(false)?;
            live.require_job_membership(&saved.command.binding.job_name)
                .map_err(|e| e.to_string())?;
            let actual = j
                .activation_lifetime_owned_current(a, s, &self.files.preparation, &self.record, key)
                .map_err(|e| e.to_string())?;
            if actual != *snapshot {
                return Err("Lifetime graph changed during observation".into());
            }
            Ok(())
        };
        let observed = crate::activation::lifetime_probe::observe(
            &saved.command,
            expected,
            &graph.service.command,
            std::time::Instant::now() + winbridge_transport::IPC_TIMEOUT,
            checkpoint,
        )
        .await?;
        timing.mark("normal_mode_observed", None, None);
        let ActivationLifetimeObservationResult::Observed { receipt, .. } = &observed.result else {
            return Err("Normal lifetime observation missing".into());
        };
        // This appends a late fact, not a new admission/window. Both the owned
        // journal graph and the actual child were rechecked around normal IPC.
        if !matches!(saved.state, ActivationLifetimeState::Confirmed { .. }) {
            let next = saved.confirmed(
                receipt.as_ref().clone(),
                self.at()?.max(saved.updated_at_ms),
            )?;
            j.append_activation_lifetime_owned(
                a,
                s,
                &self.files.preparation,
                &self.record,
                controller,
                topology,
                &next,
            )
            .map_err(|e| e.to_string())?;
        } else {
            // Idempotent append still fences current authority/root/nonce.
            j.append_activation_lifetime_owned(
                a,
                s,
                &self.files.preparation,
                &self.record,
                controller,
                topology,
                saved,
            )
            .map_err(|e| e.to_string())?;
        }
        self.lifetime_observation = Some(observed);
        timing.mark("same_command_fact_reconciled", None, None);
        Ok(())
    }
}
