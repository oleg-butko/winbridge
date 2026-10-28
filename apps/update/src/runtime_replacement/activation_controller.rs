//! Consuming controller preparation from the same LocalVerified child/Job.
use super::*;
use crate::activation::controller::{
    self, ActivationControllerAdmission, ActivationControllerPermit, ActivationControllerState,
};
use std::{fs::File, io::Read, os::windows::fs::OpenOptionsExt};

/// Retains the original child/channel/file pins and one fresh committed intent.
/// No deserialization, cloning, or factory from retained observations.
#[must_use = "Dropping this original preparation closes the managed child"]
pub struct ActivationControllerPreparation {
    owner: Box<ActivationProcessGuard>,
    permit: Box<ActivationControllerPermit>,
    _config_pin: File,
}
impl ActivationControllerPreparation {
    pub fn record(&self) -> &controller::ActivationControllerRecord {
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
    /// Preparation only. No network command is sent here. The future consuming
    /// sender must recheck this original owner and use its remaining window.
    pub fn prepare_controller_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
        selection: Option<&str>,
    ) -> std::result::Result<ActivationControllerPreparation, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.prepare_controller(j, a, s, key, selection) {
                Ok((permit, config)) => Ok(ActivationControllerPreparation {
                    owner,
                    permit,
                    _config_pin: config,
                }),
                Err(reason) => {
                    owner.record_controller_uncertainty(j, a, s, key);
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

    fn prepare_controller(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
        selection: Option<&str>,
    ) -> Result<(Box<ActivationControllerPermit>, File)> {
        if self.record.revision != 7
            || self.record.state != (ActivationState::LocalVerified {})
            || self.released.is_none()
            || self.channel.is_none()
            || self.resident_process().as_ref()
                != self.record.state_owned.as_ref().map(|p| &p.process)
        {
            return Err(
                "Controller preparation requires original same-child LocalVerified service/channel"
                    .into(),
            );
        }
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(
                &worker,
                winbridge_protocol::update::ACTIVATION_CONTROLLER_MARKER,
            )?
        {
            return Err("Resident child lacks private controller V5; no intent/send".into());
        }
        let path = Path::new(&self.record.intent.context.worker_config.path);
        let mut pin = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .map_err(|e| e.to_string())?;
        let evidence = crate::artifact::inspect_image(path)?;
        if evidence != self.record.intent.context.worker_config
            || evidence.bytes > 65536
            || winbridge_platform::file_identity(&pin).map_err(|e| e.to_string())?
                != evidence.file_identity
        {
            return Err("Controller config pin differs from original local service".into());
        }
        let mut bytes = Vec::new();
        pin.by_ref()
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65536 || crate::sha256(&bytes) != evidence.sha256 {
            return Err("Controller config bytes changed while pinned".into());
        }
        let config: winbridge_core::config::WorkerConfig =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        config.validate().map_err(|e| e.to_string())?;
        let topology = controller::topology_from_config(&config, selection)?;
        self.require_fixed_custody(true)?;
        let live = infrastructure::PinnedProcess::live(
            &self.resident_process().ok_or("Controller child missing")?,
        )
        .map_err(|e| e.to_string())?;
        let admission = j
            .admit_activation_controller(
                a,
                s,
                &self.files.preparation,
                &self.record,
                key,
                &topology,
                || {
                    self.require_fixed_custody(true)?;
                    live.require_live().map_err(|e| e.to_string())
                },
            )
            .map_err(|e| e.to_string())?;
        let ActivationControllerAdmission::Issued(permit) = admission else {
            return Err(
                "Controller was already admitted; observe same command without another send".into(),
            );
        };
        j.require_activation_controller_issued(
            a,
            s,
            &self.files.preparation,
            &self.record,
            &permit,
        )
        .map_err(|e| e.to_string())?;
        self.require_fixed_custody(true)?;
        live.require_live().map_err(|e| e.to_string())?;
        Ok((permit, pin))
    }

    fn record_controller_uncertainty(
        &self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) {
        let Ok(Some(saved)) = j.activation_controller(&s.spec.update_id) else {
            return;
        };
        if saved.command.command_id != key || saved.state != (ActivationControllerState::Issued {})
        {
            return;
        }
        let Ok(at) = self.at() else {
            return;
        };
        let Ok(next) = saved.indeterminate(
            "Controller preparation unconfirmed; no automatic send/replay".into(),
            at.max(saved.updated_at_ms),
        ) else {
            return;
        };
        let _ = j.append_activation_controller_owned(
            a,
            s,
            &self.files.preparation,
            &self.record,
            &next,
        );
    }
}
