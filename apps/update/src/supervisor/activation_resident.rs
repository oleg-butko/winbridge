//! A readonly operation retains custody and never rewrites lifetime history.
use super::*;
#[cfg(any(windows, test))]
use crate::activation::lifetime::{ActivationLifetimeSnapshot, ActivationLifetimeState};

impl FilesActor {
    pub(in crate::supervisor) fn can_observe_resident(&self) -> bool {
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
    pub(in crate::supervisor) fn take_for_resident(
        &mut self,
    ) -> Result<(Box<crate::activation::ActivationProcessGuard>, String)> {
        if !self.can_observe_resident() {
            return Err(
                "Resident observation requires the original lifetime owner; no cold adoption"
                    .into(),
            );
        }
        let record = match &self.state {
            State::Serviced(g) | State::ServiceUnconfirmed(g) => g.record(),
            State::ServiceFailed(f) => f.record(),
            _ => unreachable!(),
        };
        let pending = Box::new(record.clone());
        let key = self
            .lifetime_key
            .clone()
            .ok_or("Original lifetime key missing")?;
        let guard = match std::mem::replace(&mut self.state, State::ObservingResident(pending)) {
            State::Serviced(g) | State::ServiceUnconfirmed(g) => g,
            State::ServiceFailed(f) => f.into_custody(),
            _ => unreachable!(),
        };
        self.resident_error = None;
        Ok((guard, key))
    }

    #[cfg(windows)]
    pub(in crate::supervisor) fn complete_resident(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
        directory: &Path,
        id: &str,
    ) -> Result<()> {
        let expected = match &self.state {
            State::ObservingResident(r) => Some(r.intent.clone()),
            _ => None,
        };
        let (guard, outcome) = match result {
            Ok(g) => {
                let outcome = (|| {
                    if expected.as_ref() != Some(&g.record().intent) {
                        return Err("Resident completion changed pending original scope".into());
                    }
                    let snapshot = resident_snapshot(directory, id, &g)?;
                    validate_saved_resident(
                        &snapshot,
                        g.record(),
                        self.lifetime_key
                            .as_deref()
                            .ok_or("Original lifetime key missing")?,
                        g.lifetime_observation()
                            .ok_or("Independent lifetime observation missing")?,
                        g.resident_query()
                            .ok_or("Original resident query missing")?,
                        g.resident_observation()
                            .ok_or("Resident normal observation missing")?,
                    )
                })();
                (g, outcome)
            }
            Err(f) => {
                let reason = f.reason.clone();
                (f.into_custody(), Err(reason))
            }
        };
        // An unavailable read is scoped diagnostic data, not a failed V6 entry
        // or preparation effect. The same actual owner survives either result.
        self.resident_error = outcome.err().map(|e| e.chars().take(1000).collect());
        self.state = State::Serviced(guard);
        Ok(())
    }

    pub(in crate::supervisor) fn resident_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        // New view20 deliberately carries no recursive historical prefix. V27
        // remains the separate historical/lifetime view; keep control bounded.
        let mut view = UpdateJournal::activation_resident_snapshot(directory, id, active, error)
            .map_err(super::super::error)?;
        let mut resident = view
            .activation_resident
            .take()
            .ok_or("Resident base missing")?;
        resident.lifetime_command_id = self.lifetime_key.clone().or(resident.lifetime_command_id);
        #[cfg(windows)]
        match &self.state {
            State::ObservingResident(_) => {
                resident.phase = if active {
                    ActivationResidentPhase::Observing
                } else {
                    ActivationResidentPhase::Unconfirmed
                };
                resident.reason = Some("Readonly current connection observation is pending; no readiness or release inferred".into());
            }
            State::Serviced(g) | State::ServiceUnconfirmed(g) if self.lifetime_key.is_some() => {
                resident.phase = ActivationResidentPhase::Unconfirmed;
                resident.resident_process = g.resident_process();
                resident.process_alive = g.process_alive();
                resident.original_custody_held = Some(g.lifetime_custody_held());
                let verified = (|| {
                    if resident.process_alive != Some(true)
                        || resident.original_custody_held != Some(true)
                    {
                        return Err(
                            "Original resident process/Job/config custody is not confirmed".into(),
                        );
                    }
                    if let Some(e) = self.resident_error.as_ref().or(view.driver_error.as_ref()) {
                        return Err(e.clone());
                    }
                    validate_saved_resident(
                        &resident_snapshot(directory, id, g)?,
                        g.record(),
                        self.lifetime_key
                            .as_deref()
                            .ok_or("Original lifetime key missing")?,
                        g.lifetime_observation()
                            .ok_or("Independent lifetime observation missing")?,
                        g.resident_query().ok_or("No current resident query yet")?,
                        g.resident_observation()
                            .ok_or("No current resident observation yet")?,
                    )
                })();
                match verified {
                    Ok(()) => {
                        resident.phase = ActivationResidentPhase::Observed;
                        resident.observation = g.resident_observation().cloned();
                        resident.reason = Some("Current selected connection was observed at observation.observedAtMs; root commit/release/Active remain closed".into());
                    }
                    Err(e) => resident.reason = Some(e.chars().take(1000).collect()),
                }
            }
            State::ServiceFailed(f) if self.lifetime_key.is_some() => {
                resident.phase = ActivationResidentPhase::Unconfirmed;
                resident.resident_process = f.resident_process();
                resident.process_alive = f.process_alive();
                resident.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        #[cfg(not(windows))]
        {
            resident.reason = self.resident_error.clone().or(resident.reason);
        }
        view.activation_resident = Some(resident);
        Ok(view)
    }
}

#[cfg(windows)]
fn resident_snapshot(
    directory: &Path,
    id: &str,
    guard: &crate::activation::ActivationProcessGuard,
) -> Result<ActivationLifetimeSnapshot> {
    let saved = UpdateJournal::inspect_activation_lifetime(directory, id)
        .map_err(super::super::error)?
        .ok_or("Saved lifetime missing")?;
    let ActivationLifetimeState::Confirmed { receipt } = &saved.state else {
        return Err("Saved lifetime is not independently confirmed".into());
    };
    UpdateJournal::inspect_activation_lifetime_continuation(
        directory,
        id,
        &saved.command.binding.attempt_id,
        &guard
            .resident_process()
            .ok_or("Original resident process missing")?,
        &saved.command,
        receipt,
    )
    .map_err(super::super::error)
}

#[cfg(any(windows, test))]
fn validate_saved_resident(
    snapshot: &ActivationLifetimeSnapshot,
    local: &crate::activation::ActivationRecord,
    key: &str,
    lifetime_observation: &ActivationLifetimeObservation,
    query: &ActivationResidentQuery,
    observed: &ActivationResidentObservation,
) -> Result<()> {
    lifetime::validate_saved_lifetime(&snapshot.lifetime, local, key, lifetime_observation)?;
    crate::activation::lifetime::validate_command(
        &snapshot.lifetime.command,
        &snapshot.controller,
    )?;
    if query.command != snapshot.lifetime.command {
        return Err("Resident query changed original lifetime command".into());
    }
    let ActivationLifetimeState::Confirmed { receipt } = &snapshot.lifetime.state else {
        return Err("Saved lifetime confirmation missing".into());
    };
    crate::activation::resident_observation::validate_observation(
        query,
        receipt,
        &snapshot.controller.controller.command.binding.topology,
        observed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resident_completion_binds_the_original_graph_query_and_historical_lifetime() {
        let (query, _, entry, observed, controller) =
            crate::activation::resident_observation::test_fixture();
        let snapshot = ActivationLifetimeSnapshot {
            lifetime: crate::activation::lifetime::ActivationLifetimeRecord::issued(
                query.command.clone(),
            )
            .unwrap()
            .confirmed(entry.clone(), entry.compatibility.observed_at_ms)
            .unwrap(),
            controller,
        };
        let historical = ActivationLifetimeObservation {
            lifetime_observation_version: 1,
            result: ActivationLifetimeObservationResult::Observed {
                receipt: Box::new(entry),
                observed_at_ms: query.command.deadline_at_ms + 1,
            },
        };
        let key = &query.command.command_id;
        validate_saved_resident(
            &snapshot,
            &snapshot.controller.local,
            key,
            &historical,
            &query,
            &observed,
        )
        .unwrap();
        for mutation in 0..6 {
            let mut q = query.clone();
            let mut o = observed.clone();
            let mut h = historical.clone();
            let mut local = snapshot.controller.local.clone();
            let foreign = uuid::Uuid::new_v4().to_string();
            let mut k = key.as_str();
            match mutation {
                0 => q.request_id = foreign,
                1 => q.command.command_id = foreign,
                2 => h.lifetime_observation_version = 7,
                3 => local.intent.source.owner_epoch += 1,
                4 => k = &foreign,
                _ => {
                    o.result = ActivationResidentObservationResult::Unavailable {
                        message: "Disconnected".into(),
                    }
                }
            }
            assert!(
                validate_saved_resident(&snapshot, &local, k, &h, &q, &o).is_err(),
                "mutation {mutation}"
            );
        }
    }
    #[test]
    fn a_cold_actor_cannot_observe_even_with_a_saved_lifetime_key() {
        let mut actor = FilesActor {
            lifetime_key: Some(uuid::Uuid::new_v4().to_string()),
            ..Default::default()
        };
        assert!(!actor.can_observe_resident());
        #[cfg(windows)]
        {
            assert!(actor.take_for_resident().is_err());
            let (graph, command, _) = crate::activation::lifetime::test_fixture();
            actor.state = State::ObservingResident(Box::new(graph.local));
            actor.lifetime_key = Some(command.command_id);
            assert!(!actor.can_observe_resident());
            assert!(actor.take_for_resident().is_err());
        }
        #[cfg(not(windows))]
        {
            actor.resident_error = Some("No actual owner".into());
            assert!(!actor.can_observe_resident());
        }
    }
    #[cfg(windows)]
    #[test]
    fn pending_lifetime_reads_keep_service_started_without_enabling_a_second_start() {
        let (graph, _, _) = crate::activation::lifetime::test_fixture();
        for state in [
            State::StartingLifetime(Box::new(graph.local.clone())),
            State::ObservingLifetime(Box::new(graph.local.clone())),
            State::ObservingResident(Box::new(graph.local.clone())),
        ] {
            let actor = FilesActor {
                state,
                ..Default::default()
            };
            assert!(actor.service_started());
            assert!(!actor.can_start_service());
            assert!(!actor.can_observe_resident());
        }
    }
}
