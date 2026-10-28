//! Consuming lifetime preparation on the original controller-verified owner.
//! One private send and independent observation; root commit/release stay closed.
use super::*;
use crate::activation::{
    controller,
    lifetime::{
        self, ActivationLifetimeAdmission, ActivationLifetimePermit, ActivationLifetimeState,
    },
};
use std::io::{Seek, SeekFrom};
use winbridge_protocol::update::WorkerControllerActivationTopology;

#[must_use = "Dropping lifetime preparation closes the original managed child"]
pub struct ActivationLifetimePreparation {
    owner: Box<ActivationProcessGuard>,
    permit: Box<ActivationLifetimePermit>,
}
impl ActivationLifetimePreparation {
    /// Consume the fresh permit once. A Ready alone never confirms lifetime:
    /// the same actual normal IPC/PID must independently observe quarantine.
    pub fn enter_lifetime_blocking(
        self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<ActivationProcessGuard>, ActivationLaunchFailure> {
        let Self { owner, permit } = self;
        let key = permit.record().command.command_id.clone();
        let topology = permit.topology().clone();
        let journal = &mut *j;
        match crate::activation::on_original_io(owner, move |mut owner| async move {
            match owner.deliver_lifetime(journal, a, s, &permit).await {
                Ok(()) => Ok(owner),
                Err(reason) => {
                    owner.record_lifetime_uncertainty(
                        journal,
                        a,
                        s,
                        &permit.record().command.command_id,
                        permit.topology(),
                    );
                    Err(ActivationLaunchFailure {
                        reason,
                        custody: owner,
                    })
                }
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => {
                owner.record_lifetime_uncertainty(j, a, s, &key, &topology);
                Err(ActivationLaunchFailure {
                    reason: reason.into(),
                    custody: owner,
                })
            }
        }
    }
    pub fn record(&self) -> &lifetime::ActivationLifetimeRecord {
        self.permit.record()
    }
    pub fn activation(&self) -> &ActivationRecord {
        self.owner.record()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.owner.process_id()
    }
}

impl ActivationProcessGuard {
    pub fn lifetime_observation(
        &self,
    ) -> Option<&winbridge_protocol::update::ActivationLifetimeObservation> {
        self.lifetime_observation.as_ref()
    }

    async fn deliver_lifetime(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &ActivationLifetimePermit,
    ) -> Result<()> {
        self.require_lifetime_owner()?;
        let observed = self
            .controller_observation
            .as_ref()
            .ok_or("Original controller observation missing")?;
        j.require_activation_lifetime_issued(
            a,
            s,
            &self.files.preparation,
            &self.record,
            observed,
            permit,
        )
        .map_err(|e| e.to_string())?;
        let process = self
            .resident_process()
            .ok_or("Original lifetime child missing")?;
        let live = infrastructure::PinnedProcess::live(&process).map_err(|e| e.to_string())?;
        live.require_job_membership(&permit.record().command.binding.job_name)
            .map_err(|e| e.to_string())?;
        let snapshot = Box::new(
            UpdateJournal::inspect_activation_lifetime_receiver(
                Path::new(&self.record.intent.source.runtime_directory)
                    .join("update-state")
                    .as_path(),
                &s.spec.update_id,
                &self.record.intent.attempt_id,
                &process,
            )
            .map_err(|e| e.to_string())?,
        );
        if snapshot.lifetime != *permit.record() {
            return Err("Lifetime receiver graph differs from original permit".into());
        }
        let deadline = self.files.preparation.deadline;
        let remaining = || {
            deadline
                .checked_duration_since(std::time::Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| "Original lifetime delivery window expired".to_string())
        };
        let command = &permit.record().command;
        let stream = self
            .channel
            .as_mut()
            .ok_or("Original lifetime channel missing")?;
        channel::write(
            stream,
            &channel::LifetimeCommand {
                contract_version: channel::LIFETIME_CONTRACT,
                command: command.clone(),
            },
            remaining()?,
        )
        .await?;
        let ready: channel::LifetimeReady = channel::read(stream, remaining()?).await?;
        channel::validate_lifetime_ready(&ready, command)?;
        let observation = crate::activation::lifetime_probe::probe(
            command,
            &ready.receipt,
            &snapshot.controller.service.command,
            deadline,
            || {
                self.require_lifetime_owner()?;
                live.require_job_membership(&command.binding.job_name)
                    .map_err(|e| e.to_string())?;
                j.require_activation_lifetime_issued(
                    a,
                    s,
                    &self.files.preparation,
                    &self.record,
                    observed,
                    permit,
                )
                .map_err(|e| e.to_string())
            },
        )
        .await?;
        let winbridge_protocol::update::ActivationLifetimeObservationResult::Observed {
            receipt,
            ..
        } = &observation.result
        else {
            return Err("Lifetime normal IPC did not confirm resident quarantine".into());
        };
        let next = permit.record().confirmed(
            receipt.as_ref().clone(),
            self.at()?.max(receipt.compatibility.observed_at_ms),
        )?;
        j.append_activation_lifetime_owned(
            a,
            s,
            &self.files.preparation,
            &self.record,
            observed,
            permit.topology(),
            &next,
        )
        .map_err(|e| e.to_string())?;
        self.lifetime_observation = Some(observation);
        Ok(())
    }

    /// One fresh durable intent under the original owner; Existing is never a
    /// reconstructed preparation. The sender must consume this value once.
    pub fn prepare_lifetime_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> std::result::Result<ActivationLifetimePreparation, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.prepare_lifetime(j, a, s, key) {
                Ok(permit) => Ok(ActivationLifetimePreparation { owner, permit }),
                Err(reason) => Err(ActivationLaunchFailure {
                    reason,
                    custody: owner,
                }),
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }

    fn require_lifetime_owner(&self) -> Result<()> {
        let expected = self.record.intent.job_name()?;
        let identity = self
            .child
            .as_ref()
            .and_then(ManagedChild::identity)
            .ok_or("Original lifetime child/Job missing")?;
        if self.record.revision != 7
            || self.record.state != (ActivationState::LocalVerified {})
            || self.released.is_none()
            || self.channel.is_none()
            || self._controller_config_pin.is_none()
            || self.resident_process().as_ref()
                != self.record.state_owned.as_ref().map(|p| &p.process)
            || identity.job_name != expected
            || self.record.created.as_ref().map(|p| p.job_name.as_str()) != Some(expected.as_str())
        {
            return Err(
                "Lifetime requires the original same-child local7/channel/config/Job custody"
                    .into(),
            );
        }
        self.require_lifetime_config_pin()?;
        self.require_fixed_custody(true)
    }

    fn require_lifetime_config_pin(&self) -> Result<&std::fs::File> {
        let pin = self
            ._controller_config_pin
            .as_ref()
            .ok_or("Original controller config pin missing")?;
        let expected = &self.record.intent.context.worker_config;
        if crate::artifact::inspect_image(Path::new(&expected.path))? != *expected
            || expected.bytes > 65536
            || winbridge_platform::file_identity(pin).map_err(|e| e.to_string())?
                != expected.file_identity
        {
            return Err("Lifetime config pin/path changed from original local service".into());
        }
        Ok(pin)
    }

    fn lifetime_config(&self) -> Result<winbridge_core::config::WorkerConfig> {
        let expected = &self.record.intent.context.worker_config;
        let mut file = self
            .require_lifetime_config_pin()?
            .try_clone()
            .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(65537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65536 || crate::sha256(&bytes) != expected.sha256 {
            return Err("Lifetime original config bytes changed".into());
        }
        let config: winbridge_core::config::WorkerConfig =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        config.validate().map_err(|e| e.to_string())?;
        if config.instance_id != self.record.intent.context.worker_context.instance_id {
            return Err("Lifetime config changed instance".into());
        }
        Ok(config)
    }

    fn prepare_lifetime(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> Result<Box<ActivationLifetimePermit>> {
        self.require_lifetime_owner()?;
        let observed = self
            .controller_observation
            .as_ref()
            .ok_or("Lifetime independent controller observation missing")?;
        let winbridge_protocol::update::ActivationControllerObservationResult::Observed {
            receipt,
            ..
        } = &observed.result
        else {
            return Err("Lifetime controller was not independently observed".into());
        };
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        // Construct the sender's pattern at runtime; retaining the receiver's
        // verbatim marker here would advertise V6 in linked worker images.
        let marker = winbridge_protocol::update::activation_lifetime_marker_pattern();
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&worker, &marker)?
        {
            return Err("Resident child lacks private lifetime V6; no intent/send".into());
        }
        let config = self.lifetime_config()?;
        let topology =
            controller::topology_from_config(&config, Some(&receipt.session.controller_id))?;
        let actual = self.resident_process().ok_or("Lifetime child missing")?;
        let live = infrastructure::PinnedProcess::live(&actual).map_err(|e| e.to_string())?;
        let admission = match j.admit_activation_lifetime(
            a,
            s,
            &self.files.preparation,
            &self.record,
            observed,
            &topology,
            key,
            || {
                self.require_lifetime_owner()?;
                live.require_live().map_err(|e| e.to_string())
            },
        ) {
            Ok(value) => value,
            Err(error) => {
                self.record_lifetime_uncertainty(j, a, s, key, &topology);
                return Err(error.to_string());
            }
        };
        let ActivationLifetimeAdmission::Issued(permit) = admission else {
            return Err("Lifetime already admitted; observe same key without another send".into());
        };
        let checked = (|| {
            j.require_activation_lifetime_issued(
                a,
                s,
                &self.files.preparation,
                &self.record,
                observed,
                &permit,
            )
            .map_err(|e| e.to_string())?;
            self.require_lifetime_owner()?;
            live.require_live().map_err(|e| e.to_string())
        })();
        if let Err(reason) = checked {
            self.record_lifetime_uncertainty(j, a, s, key, &topology);
            return Err(reason);
        }
        Ok(permit)
    }

    fn record_lifetime_uncertainty(
        &self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
        topology: &WorkerControllerActivationTopology,
    ) {
        let Some(observed) = self.controller_observation.as_ref() else {
            return;
        };
        let Ok(Some(saved)) = j.activation_lifetime(&s.spec.update_id) else {
            return;
        };
        if saved.command.command_id != key || saved.state != (ActivationLifetimeState::Issued {}) {
            return;
        }
        let Ok(at) = self.at() else { return };
        let Ok(next) = saved.indeterminate(
            "Original lifetime preparation unconfirmed; no automatic send/replay".into(),
            at.max(saved.updated_at_ms),
        ) else {
            return;
        };
        let _ = j.append_activation_lifetime_owned(
            a,
            s,
            &self.files.preparation,
            &self.record,
            observed,
            topology,
            &next,
        );
    }
}
