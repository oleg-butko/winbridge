//! Fresh readonly current session under original physical and journal custody.
use super::*;
use crate::activation::{
    lifetime::{self, ActivationLifetimeState},
    resident_probe,
};
use winbridge_protocol::update::*;

impl ActivationProcessGuard {
    pub fn resident_observation(&self) -> Option<&ActivationResidentObservation> {
        self.resident_observation.as_ref()
    }

    pub fn observe_resident_blocking(
        mut self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> std::result::Result<Box<Self>, ActivationLaunchFailure> {
        self.resident_observation = None;
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.observe_resident(j, a, s, key).await {
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

    async fn observe_resident(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> Result<()> {
        self.require_lifetime_custody(false)?;
        let snapshot = Box::new(
            j.activation_lifetime_owned_current(a, s, &self.files.preparation, &self.record, key)
                .map_err(|e| e.to_string())?,
        );
        let saved = &snapshot.lifetime;
        let ActivationLifetimeState::Confirmed { receipt } = &saved.state else {
            return Err(
                "Resident current session requires independently confirmed lifetime".into(),
            );
        };
        let graph = &snapshot.controller;
        lifetime::validate_command(&saved.command, graph)?;
        let topology = &graph.controller.command.binding.topology;
        lifetime::validate_controller_prefix(
            graph,
            topology,
            self.controller_observation
                .as_ref()
                .ok_or("Original controller observation missing")?,
        )?;
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&worker, &resident_observation_marker_pattern())?
        {
            return Err("Resident child lacks readonly V7 receiver; no fallback".into());
        }
        let process = self
            .resident_process()
            .ok_or("Original resident child missing")?;
        let live = infrastructure::PinnedProcess::live(&process).map_err(|e| e.to_string())?;
        let query = ActivationResidentQuery {
            resident_observation_version: ACTIVATION_RESIDENT_OBSERVATION_CONTRACT,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: saved.command.clone(),
        };
        let checkpoint = || {
            self.require_lifetime_custody(false)?;
            live.require_job_membership(&saved.command.binding.job_name)
                .map_err(|e| e.to_string())?;
            let current = j
                .activation_lifetime_owned_current(a, s, &self.files.preparation, &self.record, key)
                .map_err(|e| e.to_string())?;
            if current != *snapshot {
                return Err("Resident original graph changed during readonly observation".into());
            }
            Ok(())
        };
        let observed = resident_probe::observe(
            &query,
            receipt,
            &graph.service.command,
            topology,
            std::time::Instant::now() + winbridge_transport::IPC_TIMEOUT,
            checkpoint,
        )
        .await?;
        // No append, new admission, root transition, private read or release.
        self.resident_observation = Some(observed);
        Ok(())
    }
}
