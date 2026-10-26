//! Consuming service preparation. No command is sent and no readiness is claimed.
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
    /// preparation only; service4 capability/send/IPC verification come later.
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
