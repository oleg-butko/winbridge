//! Original live preparation; an opaque SQL fact and pinned files still do not
//! authorize a keeper CreateProcess without that keeper's own ancestry checks.
use super::*;
use crate::{
    journal::{
        BootstrapReservation, BootstrapReservationAdmission, BootstrapReservationCommit,
        UpdateAuthority, UpdateJournal,
    },
    supervisor::KeeperProbeCapture,
};
use std::time::{SystemTime, UNIX_EPOCH};
#[must_use = "Retain the original admission resources until its outcome is resolved"]
pub struct PreparedBootstrap {
    pub(super) files: PreparedBootstrapFiles,
    pub(super) capture: KeeperProbeCapture,
    pub(super) known: Box<BootstrapReservationCommit>,
}
pub enum BootstrapPreparation {
    Prepared(Box<PreparedBootstrap>),
    Existing(Box<BootstrapReservation>),
}
#[must_use = "Failed/uncertain admission retains its original pins; do not replay SQL or creation"]
pub struct BootstrapPreparationFailure {
    pub reason: String,
    _files: PreparedBootstrapFiles,
    _capture: KeeperProbeCapture,
    _known: Option<Box<BootstrapReservationCommit>>,
}
impl PreparedBootstrapFiles {
    /// Must be called in the actual worker. `own_state` checks its live mode and
    /// execution-domain witness without waiting for Store locks under SQL locks.
    /// This method does not turn an ancestor's saved PID into an exit fact:
    /// the actual keeper creator must still prove its retained predecessor exited.
    pub fn admit(
        self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        capture: KeeperProbeCapture,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> std::result::Result<BootstrapPreparation, Box<BootstrapPreparationFailure>> {
        let mut trace = PhaseTrace::new("admission", self.launch_id());
        let mut known = None;
        let result = (|| {
            self.require_current()?;
            capture.require_current()?;
            own_state()?;
            let at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis();
            if at > winbridge_core::update::MAX_UPDATE_COUNTER as u128 {
                return Err("Bootstrap clock out of range".into());
            }
            let (intent, spec) = self.intent(capture.binding(), at as u64)?;
            match j
                .commit_bootstrap_reservation(a, intent, spec, |_, _| {
                    self.require_current()?;
                    capture.require_current()?;
                    own_state()
                })
                .map_err(|e| e.to_string())?
            {
                BootstrapReservationAdmission::Committed(k) => {
                    known = Some(k);
                    Ok(None)
                }
                BootstrapReservationAdmission::Existing(r) => Ok(Some(r)),
            }
        })();
        let output = match result {
            Ok(Some(data)) => Ok(BootstrapPreparation::Existing(data)),
            Ok(None) => {
                let attempt = PreparedBootstrap {
                    files: self,
                    capture,
                    known: known.expect("Only the original COMMIT returns Prepared"),
                };
                match attempt.require_current(j, a, own_state) {
                    Ok(()) => Ok(BootstrapPreparation::Prepared(Box::new(attempt))),
                    Err(reason) => Err(Box::new(BootstrapPreparationFailure {
                        reason: format!("Bootstrap SQL COMMIT is known; coordinator not created, original pins retained: {reason}"),
                        _files: attempt.files,
                        _capture: attempt.capture,
                        _known: Some(attempt.known),
                    })),
                }
            }
            Err(reason) => Err(Box::new(BootstrapPreparationFailure {
                reason,
                _files: self,
                _capture: capture,
                _known: known,
            })),
        };
        trace.finish(output.is_ok());
        output
    }
}
impl PreparedBootstrap {
    pub fn reservation(&self) -> &BootstrapReservation {
        self.known.reservation()
    }
    pub fn require_current(
        &self,
        j: &UpdateJournal,
        a: &UpdateAuthority,
        mut own_state: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        self.capture.require_current()?;
        self.files.require_current()?;
        own_state()?;
        j.require_known_bootstrap(a, &self.known)
            .map_err(|e| e.to_string())?;
        self.capture.require_current()
    }
}
