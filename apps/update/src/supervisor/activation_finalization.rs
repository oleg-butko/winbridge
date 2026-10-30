//! Original FilesActor only. Cold status/duplicates never manufacture custody.
use super::*;
impl FilesActor {
    pub(in crate::supervisor) fn can_finalize(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Serviced(_))
                && self.lifetime_key.is_some()
                && self.finalization_key.is_none()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(in crate::supervisor) fn can_observe_finalization(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::FinalizationHeld(_)) && self.finalization_key.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_for_finalization(
        &mut self,
    ) -> Result<(
        Box<crate::activation::ActivationProcessGuard>,
        String,
        String,
    )> {
        if !self.can_finalize() {
            return Err(
                "Finalization requires original supervised owner; no cold/repeated decision".into(),
            );
        }
        let pending = match &self.state {
            State::Serviced(g) => Box::new(g.record().clone()),
            _ => unreachable!(),
        };
        let guard = match std::mem::replace(&mut self.state, State::Finalizing(pending)) {
            State::Serviced(g) => g,
            _ => unreachable!(),
        };
        let lifetime = self
            .lifetime_key
            .clone()
            .ok_or("Original lifetime key missing")?;
        let key = uuid::Uuid::new_v4().to_string();
        self.finalization_key = Some(key.clone());
        Ok((guard, lifetime, key))
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_finalization_observation(
        &mut self,
    ) -> Result<Box<crate::activation::ActivationFinalizationGuard>> {
        if !self.can_observe_finalization() {
            return Err("No original finalization owner for readonly reconciliation".into());
        }
        let pending = match &self.state {
            State::FinalizationHeld(g) => Box::new(g.activation().clone()),
            _ => unreachable!(),
        };
        Ok(
            match std::mem::replace(&mut self.state, State::ObservingFinalization(pending)) {
                State::FinalizationHeld(g) => g,
                _ => unreachable!(),
            },
        )
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn fail_finalization_preparation(
        &mut self,
        failure: crate::activation::ActivationLaunchFailure,
    ) -> Result<()> {
        let matches =
            matches!(&self.state, State::Finalizing(r) if r.intent==failure.record().intent);
        let reason = if matches {
            failure.reason.clone()
        } else {
            "Finalization preparation changed original scope".into()
        };
        self.state = State::FinalizationPrepareFailed(Box::new(failure));
        Err(reason)
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn complete_finalization(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationFinalizationGuard>,
            crate::activation::ActivationFinalizationFailure,
        >,
    ) -> Result<()> {
        let expected = match &self.state {
            State::Finalizing(r) | State::ObservingFinalization(r) => Some(r.intent.clone()),
            _ => None,
        };
        let (guard, mut outcome) = match result {
            Ok(g) => (g, Ok(())),
            Err(f) => {
                let reason = f.reason.clone();
                (f.into_custody(), Err(reason))
            }
        };
        if expected.as_ref() != Some(&guard.activation().intent)
            || self.finalization_key.as_deref() != Some(guard.record().command.command_id.as_str())
        {
            outcome = Err("Finalization completion changed pending original owner/key".into());
        }
        self.state = State::FinalizationHeld(guard);
        outcome
    }
    pub(in crate::supervisor) fn finalization_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        let mut view =
            UpdateJournal::activation_finalization_snapshot(directory, id, active, error.clone())
                .map_err(super::super::error)?;
        let f = view
            .activation_finalization
            .as_mut()
            .ok_or("Finalization base view missing")?;
        if let (Some(saved), Some(original)) = (&f.command_id, &self.finalization_key) {
            if saved != original {
                return Err("Finalization view has a foreign decision key".into());
            }
        }
        f.command_id = f
            .command_id
            .take()
            .or_else(|| self.finalization_key.clone());
        #[cfg(windows)]
        match &self.state {
            State::Finalizing(_) | State::ObservingFinalization(_) => {
                f.phase = if active {
                    ActivationFinalizationPhase::Finalizing
                } else {
                    ActivationFinalizationPhase::Unconfirmed
                };
                f.reason = Some(
                    "Original finalization operation pending; no Active/keeper inferred".into(),
                );
            }
            State::FinalizationPrepareFailed(g) => {
                f.phase = ActivationFinalizationPhase::Unconfirmed;
                f.resident_process = g.resident_process();
                f.process_alive = g.process_alive();
                f.original_custody_held = Some(g.lifetime_custody_held());
                f.verification_pins_retired = Some(false);
                f.delivery_attempted = Some(false);
                let (out, err, cut, done) = g.diagnostics();
                f.stdout_tail = out;
                f.stderr_tail = err;
                f.output_incomplete = Some(cut);
                f.readers_finished = Some(done);
                f.reason = Some(g.reason.clone());
            }
            State::FinalizationHeld(g) => {
                f.resident_process = g.resident_process();
                f.process_alive = g.process_alive();
                f.original_custody_held = Some(g.physical_custody_held());
                f.verification_pins_retired = Some(true);
                f.delivery_attempted = Some(g.delivery_attempted());
                f.interrupted_reply_prefix = g.interrupted_reply_prefix().map(<[u8]>::to_vec);
                f.observation = g.observation().cloned();
                let (out, err, cut, done) = g.diagnostics();
                f.stdout_tail = out;
                f.stderr_tail = err;
                f.output_incomplete = Some(cut);
                f.readers_finished = Some(done);
                f.phase = if error.is_none()
                    && f.original_custody_held == Some(true)
                    && f.observation.is_some()
                    && f.durable_phase == Some(ActivationFinalizationDurablePhase::Confirmed)
                {
                    ActivationFinalizationPhase::Confirmed
                } else {
                    ActivationFinalizationPhase::Unconfirmed
                };
                f.reason=error.clone().or_else(||Some("Independent Active fact recorded; original Job retained; permanent keeper conversion is still separate".into()));
            }
            _ => {}
        }
        if error.is_some() {
            f.reason = error;
        }
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_actor_and_in_flight_metadata_cannot_mint_a_finalization_owner() {
        let mut actor = FilesActor {
            lifetime_key: Some(uuid::Uuid::new_v4().to_string()),
            ..Default::default()
        };
        assert!(!actor.can_finalize() && !actor.can_observe_finalization());
        actor.finalization_key = Some(uuid::Uuid::new_v4().to_string());
        assert!(!actor.can_finalize() && !actor.can_observe_finalization());
        #[cfg(windows)]
        {
            let (graph, _, _) = crate::activation::lifetime::test_fixture();
            for state in [
                State::Finalizing(Box::new(graph.local.clone())),
                State::ObservingFinalization(Box::new(graph.local)),
            ] {
                actor.state = state;
                assert!(actor.service_started() && !actor.is_idle());
                assert!(
                    !actor.can_start_service()
                        && !actor.can_finalize()
                        && !actor.can_observe_finalization()
                );
                assert!(
                    actor.take_for_finalization().is_err()
                        && actor.take_finalization_observation().is_err()
                );
            }
        }
    }
}
