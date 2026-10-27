//! Consuming admission, one V4 send and independent normal-IPC verification.
use super::*;
use crate::activation::service::{
    ActivationServiceAdmission, ActivationServicePermit, ActivationServiceState,
};

/// Holds the SAME child/Job/channel/file pins together with its one fresh intent.
/// No public constructor, serde implementation, or conversion from saved rows.
#[must_use = "Keep original service custody resident; dropping closes the managed child"]
pub struct ActivationServicePreparation {
    owner: Box<ActivationProcessGuard>,
    permit: Box<ActivationServicePermit>,
}
impl ActivationServicePreparation {
    /// One private send followed by independent normal-IPC health readback.
    /// Success retains the child and records local proof/AwaitingController.
    /// No controller, lifetime transfer or final commit is granted.
    pub fn start_local_service_blocking(
        self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<ActivationProcessGuard>, ActivationLaunchFailure> {
        let Self { owner, permit } = self;
        let key = permit.record().command.command_id.clone();
        let result = {
            let journal = &mut *j;
            crate::activation::on_original_io(owner, move |mut owner| async move {
                match owner.deliver_local_service(journal, a, s, &permit).await {
                    Ok(()) => Ok(owner),
                    Err(reason) => {
                        owner.record_service_uncertainty(
                            journal,
                            a,
                            s,
                            &permit.record().command.command_id,
                        );
                        owner.record_uncertainty(journal, a, s);
                        Err(ActivationLaunchFailure {
                            reason,
                            custody: owner,
                        })
                    }
                }
            })
        };
        match result {
            Ok(result) => result,
            Err((mut custody, reason)) => {
                custody.record_service_uncertainty(j, a, s, &key);
                custody.record_uncertainty(j, a, s);
                Err(ActivationLaunchFailure {
                    reason: reason.into(),
                    custody,
                })
            }
        }
    }

    pub fn record(&self) -> &crate::activation::service::ActivationServiceRecord {
        self.permit.record()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.owner.process_id()
    }
    pub fn activation(&self) -> &ActivationRecord {
        self.owner.record()
    }
}

impl ActivationProcessGuard {
    /// Consume an original confirmed writer exactly once. The returned owner is
    /// preparation only; start_local_service_blocking consumes its one send.
    /// Neither a duplicate key nor an uncertain commit can recreate this owner.
    pub fn prepare_local_service_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> std::result::Result<ActivationServicePreparation, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            let result = owner.prepare_local_service(j, a, s, key);
            match result {
                Ok(permit) => Ok(ActivationServicePreparation { owner, permit }),
                Err(reason) => {
                    owner.record_service_uncertainty(j, a, s, key);
                    owner.record_uncertainty(j, a, s);
                    Err(ActivationLaunchFailure {
                        reason,
                        custody: owner,
                    })
                }
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }

    fn prepare_local_service(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> Result<Box<ActivationServicePermit>> {
        let mut timing = crate::activation::ActivationTiming::new(
            "service_admission",
            Some(self.files.preparation.deadline),
        );
        if self.record.state != (ActivationState::StateOwned {})
            || self.record.revision != 6
            || self.released.is_none()
            || self.channel.is_none()
            || self.resident_process().as_ref()
                != self.record.state_owned.as_ref().map(|p| &p.process)
        {
            return Err(
                "Service preparation requires the original same-child StateOwned writer/channel"
                    .into(),
            );
        }
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&worker, channel::SERVICE_MARKER)?
        {
            return Err("Resident child lacks private service V4; no intent/send".into());
        }
        self.require_custody(j, a, s, true)?;
        let actual = self
            .resident_process()
            .ok_or("Service writer process missing")?;
        let live = infrastructure::PinnedProcess::live(&actual).map_err(|e| e.to_string())?;
        timing.mark("custody_checked", None, None);
        let admission = j
            .admit_activation_service(a, s, &self.files.preparation, &self.record, key, || {
                self.require_fixed_custody(true)?;
                live.require_live().map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        let ActivationServiceAdmission::Issued(permit) = admission else {
            return Err(
                "Service was already admitted; observe the same command without another send"
                    .into(),
            );
        };
        timing.mark("intent_committed", None, None);
        j.require_activation_service_issued(a, s, &self.files.preparation, &self.record, &permit)
            .map_err(|e| e.to_string())?;
        self.require_fixed_custody(true)?;
        live.require_live().map_err(|e| e.to_string())?;
        timing.mark("issued_checked", None, None);
        Ok(permit)
    }

    async fn deliver_local_service(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &ActivationServicePermit,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "service_delivery",
            Some(self.files.preparation.deadline),
        );
        j.require_activation_service_issued(a, s, &self.files.preparation, &self.record, permit)
            .map_err(|e| e.to_string())?;
        timing.mark("service_issued_checked", None, None);
        self.require_fixed_custody(true)?;
        let live = infrastructure::PinnedProcess::live(
            &self.resident_process().ok_or("Service child missing")?,
        )
        .map_err(|e| e.to_string())?;
        let remaining = self
            .files
            .preparation
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("Original service startup window expired before send".into());
        }
        let command = &permit.record().command;
        let stream = self
            .channel
            .as_mut()
            .ok_or("Original service channel missing")?;
        channel::write(
            stream,
            &channel::ServiceCommand {
                contract_version: channel::SERVICE_CONTRACT,
                command: command.clone(),
            },
            remaining,
        )
        .await?;
        timing.mark("service_command_sent", None, None);
        let ready: channel::ServiceReady = channel::read(
            stream,
            Duration::from_millis(self.record.intent.context.startup_timeout_ms),
        )
        .await?;
        timing.mark("service_ready_received", None, None);
        channel::validate_service_ready(&ready, command)?;
        let receipt = crate::activation::service_probe::probe(command, &ready.receipt, || {
            self.require_custody(j, a, s, false)?;
            live.require_live().map_err(|e| e.to_string())
        })
        .await?;
        timing.mark("normal_ipc_verified", None, None);
        let next = permit
            .record()
            .confirmed(receipt, self.at()?.max(permit.record().updated_at_ms))?;
        j.append_activation_service_owned(a, s, &self.files.preparation, &self.record, &next)
            .map_err(|e| e.to_string())?;
        timing.mark("service_confirmation_committed", None, None);
        self.record = j
            .finish_local_activation(a, s, &self.files.preparation, &self.record, &next, || {
                self.require_fixed_custody(false)?;
                live.require_live().map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        timing.mark("local_root_verified", None, None);
        Ok(())
    }

    fn record_service_uncertainty(
        &self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) {
        let Ok(Some(saved)) = j.activation_service(&s.spec.update_id) else {
            return;
        };
        if saved.command.command_id != key
            || !matches!(saved.state, ActivationServiceState::Issued {})
        {
            return;
        }
        let Ok(at) = self.at() else {
            return;
        };
        let Ok(next) = saved.indeterminate(
            "Service preparation unconfirmed; no automatic send/replay".into(),
            at.max(saved.updated_at_ms),
        ) else {
            return;
        };
        let _ =
            j.append_activation_service_owned(a, s, &self.files.preparation, &self.record, &next);
    }
}
