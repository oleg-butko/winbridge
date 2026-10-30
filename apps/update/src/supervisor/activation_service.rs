//! One resident Written owner -> one service dispatch; cold facts never adopt it.
use super::*;
impl FilesActor {
    pub(in crate::supervisor) fn service_started(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(
                self.state,
                State::StartingService(_)
                    | State::StartingController(_)
                    | State::StartingLifetime(_)
                    | State::ObservingLifetime(_)
                    | State::ObservingResident(_)
                    | State::Serviced(_)
                    | State::ServiceUnconfirmed(_)
                    | State::ServiceFailed(_)
                    | State::Finalizing(_)
                    | State::PreparingKeeper(_)
                    | State::ObservingFinalization(_)
                    | State::FinalizationPrepareFailed(_)
                    | State::FinalizationHeld(_)
            )
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(in crate::supervisor) fn can_start_service(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Written(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_for_service(
        &mut self,
    ) -> Result<(Box<crate::activation::ActivationProcessGuard>, String)> {
        let record = match &self.state {
            State::Written(g) => Box::new(g.record().clone()),
            _ => {
                return Err(
                    "Local service requires the one confirmed resident writer; no adoption".into(),
                )
            }
        };
        let key = uuid::Uuid::new_v4().to_string();
        let State::Written(guard) =
            std::mem::replace(&mut self.state, State::StartingService(record))
        else {
            unreachable!()
        };
        self.service_key = Some(key.clone());
        Ok((guard, key))
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn complete_service(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
        saved: Result<Option<crate::activation::service::ActivationServiceRecord>>,
    ) -> Result<()> {
        let expected = match &self.state {
            State::StartingService(record) => Some(record.intent.clone()),
            _ => None,
        };
        let (state, outcome) = match result {
            Ok(guard) => {
                let check = (|| {
                    if expected.as_ref() != Some(&guard.record().intent) {
                        return Err(
                            "Service completion changed pending scope; custody retained".into()
                        );
                    }
                    validate_saved_service(
                        &saved?.ok_or("Service completion lacks durable evidence")?,
                        guard.record(),
                        self.service_key
                            .as_deref()
                            .ok_or("Resident service command missing")?,
                    )
                })();
                if check.is_ok() {
                    (State::Serviced(guard), check)
                } else {
                    (State::ServiceUnconfirmed(guard), check)
                }
            }
            Err(failure) => {
                let reason = failure.reason.clone();
                (State::ServiceFailed(Box::new(failure)), Err(reason))
            }
        };
        self.state = state;
        outcome
    }
    pub(in crate::supervisor) fn service_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_service_snapshot(directory, id, active, error)
            .map_err(super::super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        self.patch_state(&mut view, signal)?;
        self.patch_writer(&mut view, active)?;
        self.patch_service(&mut view, active)?;
        Ok(view)
    }
    pub(in crate::supervisor) fn patch_service(
        &self,
        view: &mut UpdatePreparationView,
        active: bool,
    ) -> Result<()> {
        #[cfg(not(windows))]
        let _ = active;
        let service = view
            .activation_service
            .as_mut()
            .ok_or("Service view missing")?;
        if service.command_id.is_none() {
            service.command_id = self.service_key.clone();
        }
        #[cfg(windows)]
        match &self.state {
            State::StartingLifetime(_)
            | State::ObservingLifetime(_)
            | State::ObservingResident(_) => {
                service.phase = ActivationServicePhase::Indeterminate;
                service.reason = Some("Original owner is handling lifetime/current-connection diagnostics; historical service receipt is not a concurrent handle observation".into());
            }
            State::StartingService(_) => {
                service.phase = if active && view.driver_error.is_none() {
                    ActivationServicePhase::Activating
                } else {
                    ActivationServicePhase::Indeterminate
                };
                service.reason = Some(
                    "Original service dispatch staged; ACK alone proves no admission/readiness"
                        .into(),
                );
            }
            State::Serviced(g) | State::ServiceUnconfirmed(g) => {
                service.resident_process = g.resident_process();
                service.process_alive = g.process_alive();
                let durable = service.saved_receipt.as_ref().is_some_and(|receipt| {
                    g.record().local.as_ref() == Some(&receipt.compatibility)
                });
                if matches!(self.state, State::Serviced(_))
                    && durable
                    && service.durable_phase == Some(ActivationServiceDurablePhase::Confirmed)
                    && service.process_alive == Some(true)
                    && view.driver_error.is_none()
                    && view.update.phase == UpdatePhase::AwaitingController
                {
                    service.phase = ActivationServicePhase::Confirmed;
                    service.reason=Some("Local IPC verified and original owner resident; controller status is separate, lifetime/commit remain pending".into());
                } else {
                    service.phase = ActivationServicePhase::Indeterminate;
                    service.reason = view.driver_error.clone().or_else(|| {
                        Some(
                            "Saved local proof does not confirm current service ownership/liveness"
                                .into(),
                        )
                    });
                }
            }
            State::ServiceFailed(f) => {
                service.phase = ActivationServicePhase::Indeterminate;
                service.resident_process = f.resident_process();
                service.process_alive = f.process_alive();
                service.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        Ok(())
    }
}
#[cfg(any(windows, test))]
fn validate_saved_service(
    saved: &crate::activation::service::ActivationServiceRecord,
    record: &crate::activation::ActivationRecord,
    key: &str,
) -> Result<()> {
    saved.validate()?;
    let crate::activation::service::ActivationServiceState::Confirmed { receipt } = &saved.state
    else {
        return Err("Local service confirmation missing".into());
    };
    if saved.command.command_id != key
        || record.state != (crate::activation::ActivationState::LocalVerified {})
        || record.revision != 7
        || record.local.as_ref() != Some(&receipt.compatibility)
        || record.state_owned.as_ref()
            != Some(&saved.command.binding.writer_command.binding.physical)
    {
        return Err(
            "Local service completion changed command, physical scope or local proof".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::service::*;
    use winbridge_protocol::WorkerRuntimeCompatibility;
    fn fixture() -> (ActivationServiceRecord, crate::activation::ActivationRecord) {
        let (mut physical, writer) = crate::activation::writer_view_fixture();
        let command = prepare_command(
            &physical,
            &writer,
            &uuid::Uuid::new_v4().to_string(),
            writer.updated_at_ms + 1,
        )
        .unwrap();
        let b = &command.binding;
        let w = &b.writer_command.binding;
        let compatibility = WorkerRuntimeCompatibility {
            schema_version: 1,
            kind: "worker_runtime_compatibility".into(),
            identity: b.expected_worker.clone(),
            context: w.physical.context.clone(),
            generation: w.physical.generation.clone(),
            worker_config_sha256: w.worker_config_sha256.clone(),
            worker_config_schema: b.worker_config_schema,
            worker_journal_schema: w.worker_journal_schema,
            updater_journal_schema: w.updater_journal_schema,
            protocol_version: b.protocol_version,
            observed_at_ms: command.issued_at_ms + 1,
        };
        let receipt = WorkerServiceActivationReceipt {
            schema_version: 1,
            command_id: command.command_id.clone(),
            update_id: w.update_id.clone(),
            attempt_id: w.attempt_id.clone(),
            command_sha256: command_sha256(&command).unwrap(),
            authority: b.writer_receipt.authority.clone(),
            compatibility: compatibility.clone(),
        };
        let at = compatibility.observed_at_ms;
        let saved = ActivationServiceRecord::issued(command)
            .unwrap()
            .confirmed(receipt, at)
            .unwrap();
        // Metadata validator fixture only, never a constructed process owner.
        physical.state = crate::activation::ActivationState::LocalVerified {};
        physical.revision = 7;
        physical.updated_at_ms = at;
        physical.local = Some(compatibility);
        (saved, physical)
    }
    #[test]
    fn service_completion_requires_same_command_physical_scope_and_local_verification() {
        let (saved, physical) = fixture();
        validate_saved_service(&saved, &physical, &saved.command.command_id).unwrap();
        assert!(
            validate_saved_service(&saved, &physical, &uuid::Uuid::new_v4().to_string()).is_err()
        );
        for n in 0..4 {
            let mut bad = physical.clone();
            match n {
                0 => bad.local = None,
                1 => bad.state = crate::activation::ActivationState::StateOwned {},
                2 => bad.local.as_mut().unwrap().generation = uuid::Uuid::new_v4().to_string(),
                _ => bad.state_owned.as_mut().unwrap().process.birth.push('0'),
            }
            assert!(validate_saved_service(&saved, &bad, &saved.command.command_id).is_err());
        }
    }
    #[test]
    fn cold_service_history_never_claims_residency_and_idle_actor_cannot_adopt_it() {
        let (saved, _) = fixture();
        let view = crate::activation::service_view(Some(&saved));
        assert_eq!(view.phase, ActivationServicePhase::NotResident);
        assert_eq!(
            view.durable_phase,
            Some(ActivationServiceDurablePhase::Confirmed)
        );
        assert!(view.saved_receipt.is_some());
        assert!(view.resident_process.is_none() && view.process_alive.is_none());
        let actor = FilesActor::default();
        assert!(!actor.can_start_service() && !actor.service_started());
    }
}
