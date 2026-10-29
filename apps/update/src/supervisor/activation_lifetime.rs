//! Entry consumes original custody once; reconciliation never repeats V6.
use super::*;
#[cfg(any(windows, test))]
use crate::activation::lifetime::{ActivationLifetimeRecord, ActivationLifetimeState};

impl FilesActor {
    pub(in crate::supervisor) fn lifetime_started(&self) -> bool {
        self.lifetime_key.is_some()
    }
    pub(in crate::supervisor) fn can_enter_lifetime(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Serviced(_))
                && self.controller_key.is_some()
                && self.lifetime_key.is_none()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(in crate::supervisor) fn can_observe_lifetime(&self) -> bool {
        #[cfg(windows)]
        {
            self.lifetime_key.is_some()
                && matches!(
                    self.state,
                    State::Serviced(_) | State::ServiceUnconfirmed(_) | State::ServiceFailed(_)
                )
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_for_lifetime(
        &mut self,
        observe: bool,
    ) -> Result<(Box<crate::activation::ActivationProcessGuard>, String)> {
        if !(if observe {
            self.can_observe_lifetime()
        } else {
            self.can_enter_lifetime()
        }) {
            return Err(
                "Lifetime requires original confirmed controller custody; no cold adoption/replay"
                    .into(),
            );
        }
        let record = match &self.state {
            State::Serviced(g) | State::ServiceUnconfirmed(g) => Box::new(g.record().clone()),
            State::ServiceFailed(f) => Box::new(f.record().clone()),
            _ => unreachable!(),
        };
        let key = if observe {
            self.lifetime_key.clone().ok_or("Lifetime key missing")?
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        let next = if observe {
            State::ObservingLifetime(record)
        } else {
            State::StartingLifetime(record)
        };
        let guard = match std::mem::replace(&mut self.state, next) {
            State::Serviced(g) | State::ServiceUnconfirmed(g) => g,
            State::ServiceFailed(f) => f.into_custody(),
            _ => unreachable!(),
        };
        self.lifetime_key = Some(key.clone());
        Ok((guard, key))
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn complete_lifetime(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
        saved: Result<Option<ActivationLifetimeRecord>>,
    ) -> Result<()> {
        let expected = match &self.state {
            State::StartingLifetime(r) | State::ObservingLifetime(r) => Some(r.intent.clone()),
            _ => None,
        };
        let (state, result) = match result {
            Ok(g) => {
                let result = (|| {
                    if expected.as_ref() != Some(&g.record().intent) {
                        return Err("Lifetime completion changed original pending scope".into());
                    }
                    validate_saved_lifetime(
                        &saved?.ok_or("Durable lifetime fact missing")?,
                        g.record(),
                        self.lifetime_key.as_deref().ok_or("Lifetime key missing")?,
                        g.lifetime_observation()
                            .ok_or("Normal lifetime observation missing")?,
                    )
                })();
                if result.is_ok() {
                    (State::Serviced(g), result)
                } else {
                    (State::ServiceUnconfirmed(g), result)
                }
            }
            Err(f) => {
                let reason = f.reason.clone();
                (State::ServiceFailed(Box::new(f)), Err(reason))
            }
        };
        self.state = state;
        result
    }
    pub(in crate::supervisor) fn lifetime_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_lifetime_snapshot(directory, id, active, error)
            .map_err(super::super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        self.patch_state(&mut view, signal)?;
        self.patch_writer(&mut view, active)?;
        self.patch_service(&mut view, active)?;
        self.patch_controller(&mut view, active, directory, id)?;
        let lifetime = view
            .activation_lifetime
            .as_mut()
            .ok_or("Lifetime view missing")?;
        if lifetime.command_id.is_none() {
            lifetime.command_id = self.lifetime_key.clone();
        }
        #[cfg(windows)]
        match &self.state {
            State::ObservingResident(_) => {
                lifetime.phase = ActivationLifetimePhase::Indeterminate;
                lifetime.reason = Some("Readonly current connection request holds original handles; the saved lifetime receipt remains historical".into());
            }
            State::StartingLifetime(_) | State::ObservingLifetime(_) => {
                lifetime.phase = if active && view.driver_error.is_none() {
                    if matches!(self.state, State::ObservingLifetime(_)) {
                        ActivationLifetimePhase::Observing
                    } else {
                        ActivationLifetimePhase::Entering
                    }
                } else {
                    ActivationLifetimePhase::Indeterminate
                };
                lifetime.reason = Some("Operation is running under original custody; dispatch ACK is not an entry confirmation".into());
            }
            State::Serviced(g) | State::ServiceUnconfirmed(g) if self.lifetime_started() => {
                lifetime.resident_process = g.resident_process();
                lifetime.process_alive = g.process_alive();
                lifetime.original_custody_held = Some(g.lifetime_custody_held());
                let saved = UpdateJournal::inspect_activation_lifetime(directory, id)
                    .map_err(super::super::error)?;
                let verified =
                    saved
                        .as_ref()
                        .zip(g.lifetime_observation())
                        .is_some_and(|(s, o)| {
                            validate_saved_lifetime(
                                s,
                                g.record(),
                                self.lifetime_key.as_deref().unwrap_or_default(),
                                o,
                            )
                            .is_ok()
                        });
                let current_graph = saved
                    .as_ref()
                    .zip(lifetime.resident_process.as_ref())
                    .is_some_and(|(s, process)| {
                        let ActivationLifetimeState::Confirmed { receipt } = &s.state else {
                            return false;
                        };
                        UpdateJournal::inspect_activation_lifetime_continuation(
                            directory,
                            id,
                            &s.command.binding.attempt_id,
                            process,
                            &s.command,
                            receipt,
                        )
                        .is_ok()
                    });
                if matches!(self.state, State::Serviced(_))
                    && verified
                    && current_graph
                    && lifetime.original_custody_held == Some(true)
                    && lifetime.process_alive == Some(true)
                    && view.driver_error.is_none()
                    && view.update.phase == UpdatePhase::AwaitingController
                {
                    lifetime.phase = ActivationLifetimePhase::Observed;
                    if let Some(ActivationLifetimeObservation {
                        result:
                            ActivationLifetimeObservationResult::Observed {
                                receipt,
                                observed_at_ms,
                            },
                        ..
                    }) = g.lifetime_observation()
                    {
                        lifetime.observed_receipt = Some(receipt.as_ref().clone());
                        lifetime.observed_at_ms = Some(*observed_at_ms);
                    }
                    lifetime.reason = Some("Supervised quarantine was independently observed at observedAtMs; dispatch/commit/release remain closed".into());
                } else {
                    lifetime.phase = ActivationLifetimePhase::Indeterminate;
                    lifetime.reason = view.driver_error.clone().or(Some(
                        "Saved lifetime fact does not confirm current original custody/mode".into(),
                    ));
                }
            }
            State::ServiceFailed(f) if self.lifetime_started() => {
                lifetime.phase = ActivationLifetimePhase::Indeterminate;
                lifetime.resident_process = f.resident_process();
                lifetime.process_alive = f.process_alive();
                lifetime.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        Ok(view)
    }
}

#[cfg(any(windows, test))]
pub(super) fn validate_saved_lifetime(
    saved: &ActivationLifetimeRecord,
    local: &crate::activation::ActivationRecord,
    key: &str,
    observed: &ActivationLifetimeObservation,
) -> Result<()> {
    saved.validate()?;
    let ActivationLifetimeState::Confirmed { receipt } = &saved.state else {
        return Err("Durable lifetime confirmation missing".into());
    };
    crate::activation::lifetime_probe::validate_observation(observed, &saved.command, receipt)?;
    let c = &saved.command.binding.compatibility;
    let mut original = c.clone();
    original.observed_at_ms = local
        .local
        .as_ref()
        .ok_or("Original local proof missing")?
        .observed_at_ms;
    if saved.command.command_id != key
        || local.revision != 7
        || local.state != (crate::activation::ActivationState::LocalVerified {})
        || local.intent.source.update_id != saved.command.binding.update_id
        || local.intent.source.launch_id != saved.command.binding.launch_id
        || local.intent.source.owner_id != saved.command.binding.updater_owner_id
        || local.intent.source.owner_epoch != saved.command.binding.updater_epoch
        || local.intent.source.parent_revision.checked_add(1)
            != Some(saved.command.binding.root_revision)
        || local.intent.attempt_id != saved.command.binding.attempt_id
        || local.intent.context.supervisor != saved.command.binding.keeper
        || local.intent.job_name()? != saved.command.binding.job_name
        || local.local.as_ref() != Some(&original)
        || local.state_owned.as_ref().map(|p| &p.process) != Some(&c.identity.process)
    {
        return Err("Lifetime completion changed original local7/key/keeper/Job/child".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::lifetime;
    fn fixture() -> (
        ActivationLifetimeRecord,
        crate::activation::ActivationRecord,
        ActivationLifetimeObservation,
    ) {
        let (graph, command, receipt) = lifetime::test_fixture();
        let at = receipt.compatibility.observed_at_ms;
        let observed = ActivationLifetimeObservation {
            lifetime_observation_version: 1,
            result: ActivationLifetimeObservationResult::Observed {
                receipt: Box::new(receipt.clone()),
                observed_at_ms: command.deadline_at_ms + 1,
            },
        };
        (
            ActivationLifetimeRecord::issued(command)
                .unwrap()
                .confirmed(receipt, at)
                .unwrap(),
            graph.local,
            observed,
        )
    }
    #[test]
    fn completion_requires_exact_original_local_child_keeper_job_key_and_normal_fact() {
        let (saved, local, observed) = fixture();
        let key = &saved.command.command_id;
        validate_saved_lifetime(&saved, &local, key, &observed).unwrap();
        assert!(validate_saved_lifetime(
            &saved,
            &local,
            &uuid::Uuid::new_v4().to_string(),
            &observed
        )
        .is_err());
        for change in 0..7 {
            let mut bad = local.clone();
            match change {
                0 => bad.local = None,
                1 => bad.revision = 6,
                2 => bad.local.as_mut().unwrap().generation = uuid::Uuid::new_v4().to_string(),
                3 => bad.intent.context.supervisor.birth.push('0'),
                4 => bad.state_owned.as_mut().unwrap().process.birth.push('0'),
                5 => bad.intent.attempt_id = uuid::Uuid::new_v4().to_string(),
                _ => bad.state = crate::activation::ActivationState::StateOwned {},
            }
            assert!(
                validate_saved_lifetime(&saved, &bad, key, &observed).is_err(),
                "changed binding {change}"
            );
        }
        for change in 0..3 {
            let mut bad = observed.clone();
            if change == 0 {
                bad.lifetime_observation_version += 1;
            } else if let ActivationLifetimeObservationResult::Observed {
                receipt,
                observed_at_ms,
            } = &mut bad.result
            {
                if change == 1 {
                    receipt.compatibility.observed_at_ms += 1;
                } else {
                    *observed_at_ms = 0;
                }
            }
            assert!(validate_saved_lifetime(&saved, &local, key, &bad).is_err());
        }
    }
    #[test]
    fn cold_lifetime_records_never_recreate_a_resident_owner_or_an_entry_permit() {
        let (confirmed, _, _) = fixture();
        let issued = ActivationLifetimeRecord::issued(confirmed.command.clone()).unwrap();
        let unknown = issued
            .indeterminate("Lost reply".into(), issued.updated_at_ms + 1)
            .unwrap();
        for saved in [issued, unknown, confirmed] {
            let v = crate::activation::lifetime_view(Some(&saved));
            assert_eq!(v.phase, ActivationLifetimePhase::NotResident);
            assert!(
                v.resident_process.is_none()
                    && v.process_alive.is_none()
                    && v.original_custody_held.is_none()
            );
            assert!(v.observed_receipt.is_none() && v.observed_at_ms.is_none());
        }
        let actor = FilesActor::default();
        assert!(
            !actor.lifetime_started()
                && !actor.can_enter_lifetime()
                && !actor.can_observe_lifetime()
        );
    }
}
