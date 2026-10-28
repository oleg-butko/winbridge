//! Controller operation consumes only the resident confirmed local-service owner.
use super::*;
impl FilesActor {
    pub(in crate::supervisor) fn controller_started(&self) -> bool {
        self.controller_key.is_some()
    }
    pub(in crate::supervisor) fn can_connect_controller(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::Serviced(_)) && self.controller_key.is_none()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_for_controller(
        &mut self,
    ) -> Result<(Box<crate::activation::ActivationProcessGuard>, String)> {
        if !self.can_connect_controller() {
            return Err(
                "Controller requires one idle confirmed local-service owner; no adoption".into(),
            );
        }
        let State::Serviced(guard) = &self.state else {
            unreachable!()
        };
        let record = Box::new(guard.record().clone());
        let key = uuid::Uuid::new_v4().to_string();
        let State::Serviced(guard) =
            std::mem::replace(&mut self.state, State::StartingController(record))
        else {
            unreachable!()
        };
        self.controller_key = Some(key.clone());
        Ok((guard, key))
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn complete_controller(
        &mut self,
        result: std::result::Result<
            Box<crate::activation::ActivationProcessGuard>,
            crate::activation::ActivationLaunchFailure,
        >,
        saved: Result<Option<crate::activation::controller::ActivationControllerRecord>>,
    ) -> Result<()> {
        let expected = match &self.state {
            State::StartingController(r) => Some(r.intent.clone()),
            _ => None,
        };
        let (state, result) = match result {
            Ok(guard) => {
                let verified = (|| {
                    if expected.as_ref() != Some(&guard.record().intent) {
                        return Err(
                            "Controller completion changed pending scope; custody retained".into(),
                        );
                    }
                    validate_saved_controller(
                        &saved?.ok_or("Controller completion lacks durable evidence")?,
                        guard.record(),
                        self.controller_key
                            .as_deref()
                            .ok_or("Controller key missing")?,
                        guard
                            .controller_observation()
                            .ok_or("Independent normal controller observation missing")?,
                    )
                })();
                if verified.is_ok() {
                    (State::Serviced(guard), verified)
                } else {
                    (State::ServiceUnconfirmed(guard), verified)
                }
            }
            Err(failure) => {
                let reason = failure.reason.clone();
                (State::ServiceFailed(Box::new(failure)), Err(reason))
            }
        };
        self.state = state;
        result
    }
    pub(in crate::supervisor) fn controller_view(
        &self,
        directory: &Path,
        id: &str,
        active: bool,
        error: Option<String>,
        signal: &crate::runtime_replacement::CustodySignal,
    ) -> Result<UpdatePreparationView> {
        let mut view = UpdateJournal::activation_controller_snapshot(directory, id, active, error)
            .map_err(super::super::error)?;
        self.patch_files(&mut view, signal)?;
        self.patch_process(&mut view, signal)?;
        self.patch_state(&mut view, signal)?;
        self.patch_writer(&mut view, active)?;
        self.patch_service(&mut view, active)?;
        let controller = view
            .activation_controller
            .as_mut()
            .ok_or("Controller view missing")?;
        if controller.command_id.is_none() {
            controller.command_id = self.controller_key.clone();
        }
        #[cfg(windows)]
        match &self.state {
            State::StartingController(_) => {
                controller.phase = if active && view.driver_error.is_none() {
                    ActivationControllerPhase::Connecting
                } else {
                    ActivationControllerPhase::Indeterminate
                };
                controller.reason=Some("Controller dispatch staged under original custody; ACK alone proves no confirmation".into());
            }
            State::Serviced(g) | State::ServiceUnconfirmed(g) if self.controller_key.is_some() => {
                controller.resident_process = g.resident_process();
                controller.process_alive = g.process_alive();
                let saved = UpdateJournal::inspect_activation_controller(directory, id)
                    .map_err(super::super::error)?;
                let verified = saved.as_ref().zip(g.controller_observation()).is_some_and(
                    |(saved, observed)| {
                        validate_saved_controller(
                            saved,
                            g.record(),
                            self.controller_key.as_deref().unwrap_or_default(),
                            observed,
                        )
                        .is_ok()
                    },
                );
                if matches!(self.state, State::Serviced(_))
                    && verified
                    && controller.durable_phase==Some(ActivationControllerDurablePhase::Confirmed)
                    && g.controller_observation().is_some_and(|v|matches!(&v.result,ActivationControllerObservationResult::Observed{receipt,..} if controller.saved_receipt.as_ref()==Some(receipt.as_ref())))
                    && controller.process_alive == Some(true)
                    && view.driver_error.is_none()
                    && view.update.phase == UpdatePhase::AwaitingController
                {
                    controller.phase = ActivationControllerPhase::Observed;
                    if let Some(ActivationControllerObservation {
                        result:
                            ActivationControllerObservationResult::Observed {
                                projection, link, ..
                            },
                        ..
                    }) = g.controller_observation()
                    {
                        controller.observed_projection = Some(projection.as_ref().clone());
                        controller.observed_link = Some(link.as_ref().clone());
                    }
                    controller.reason=Some("Selected session was independently observed; current link may change. Job lifetime/final commit remain pending".into());
                } else {
                    controller.phase = ActivationControllerPhase::Indeterminate;
                    controller.reason = view.driver_error.clone().or(Some(
                        "Saved controller fact does not prove current original custody/readiness"
                            .into(),
                    ));
                }
            }
            State::ServiceFailed(f) if self.controller_key.is_some() => {
                controller.phase = ActivationControllerPhase::Indeterminate;
                controller.resident_process = f.resident_process();
                controller.process_alive = f.process_alive();
                controller.reason = Some(f.reason.chars().take(1000).collect());
            }
            _ => {}
        }
        Ok(view)
    }
}

#[cfg(any(windows, test))]
fn validate_saved_controller(
    saved: &crate::activation::controller::ActivationControllerRecord,
    local: &crate::activation::ActivationRecord,
    key: &str,
    observed: &ActivationControllerObservation,
) -> Result<()> {
    use crate::activation::{controller, ActivationState};
    saved.validate()?;
    let controller::ActivationControllerState::Confirmed { receipt } = &saved.state else {
        return Err("Durable controller confirmation missing".into());
    };
    let ActivationControllerObservationResult::Observed {
        receipt: actual,
        projection,
        link,
    } = &observed.result
    else {
        return Err("Normal IPC did not observe selected controller".into());
    };
    let writer = &saved
        .command
        .binding
        .service_command
        .binding
        .writer_command
        .binding;
    let source = &local.intent.source;
    if observed.controller_observation_version != 1
        || actual != receipt
        || saved.command.command_id != key
        || local.state != (ActivationState::LocalVerified {})
        || local.revision != 7
        || local.intent.source.update_id != receipt.update_id
        || local.intent.attempt_id != receipt.attempt_id
        || writer.updater_owner_id != source.owner_id
        || writer.updater_epoch != source.owner_epoch
        || writer.launch_id != source.launch_id
        || writer.worker_config_sha256 != local.intent.context.worker_config.sha256
        || writer.worker_journal_schema != local.intent.context.worker_journal_schema
        || writer.updater_journal_schema != local.intent.context.updater_journal_schema
        || local.ready.as_ref().map(|v| &v.identity)
            != Some(
                &saved
                    .command
                    .binding
                    .service_command
                    .binding
                    .expected_worker,
            )
        || local.local.as_ref() != Some(&saved.command.binding.service_receipt.compatibility)
        || local.state_owned.as_ref()
            != Some(
                &saved
                    .command
                    .binding
                    .service_command
                    .binding
                    .writer_command
                    .binding
                    .physical,
            )
    {
        return Err(
            "Controller completion changed local/physical/command/normal-observation scope".into(),
        );
    }
    controller::validate_projection_evidence(receipt, &saved.command, projection)?;
    controller::validate_link_evidence(receipt, &saved.command, link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::controller::{self, ActivationControllerRecord};
    fn fixture() -> (
        ActivationControllerRecord,
        crate::activation::ActivationRecord,
        ActivationControllerObservation,
    ) {
        let f = controller::test_fixture();
        let link = winbridge_protocol::ControllerLinkObservation {
            id: f.receipt.session.controller_id.clone(),
            attempts: f.receipt.session.connection_attempt.to_string(),
            connection: winbridge_protocol::ControllerConnection {
                url: f.receipt.session.controller_url.clone(),
                state: winbridge_protocol::ConnectionState::Connected,
                session_id: Some(f.receipt.session.session_id.clone()),
                controller_node_id: Some(f.receipt.session.controller_node_id.clone()),
                controller_generation: Some(f.receipt.session.controller_generation.clone()),
                last_confirmed_at_ms: Some(f.receipt.session.connected_at_ms),
                error: None,
            },
            last_connected_at_ms: Some(f.receipt.session.connected_at_ms),
            last_disconnected_at_ms: None,
            last_error: None,
            rpc: winbridge_protocol::NetworkRpcObservation {
                in_flight: 0,
                limit: winbridge_protocol::CONTROLLER_RPC_LIMIT,
                admitted: "0".into(),
                completed: "0".into(),
                rejected: "0".into(),
            },
        };
        let saved = ActivationControllerRecord::issued(f.command)
            .unwrap()
            .confirmed(f.receipt.clone(), f.receipt.compatibility.observed_at_ms)
            .unwrap();
        let observed = ActivationControllerObservation {
            controller_observation_version: 1,
            result: ActivationControllerObservationResult::Observed {
                receipt: Box::new(f.receipt),
                projection: Box::new(f.projection),
                link: Box::new(link),
            },
        };
        (saved, f.local, observed)
    }
    #[test]
    fn controller_completion_requires_original_scope_and_independent_full_observation() {
        let (saved, local, observed) = fixture();
        validate_saved_controller(&saved, &local, &saved.command.command_id, &observed).unwrap();
        for fault in 0..12 {
            let (saved, mut local, mut observed) = fixture();
            match fault {
                0 => local.intent.source.owner_id = uuid::Uuid::new_v4().to_string(),
                1 => local.intent.source.owner_epoch += 1,
                2 => local.intent.source.launch_id = uuid::Uuid::new_v4().to_string(),
                3 => local.intent.context.worker_config.sha256 = "0".repeat(64),
                4 => local.local = None,
                5 => local.revision = 6,
                6 => observed.controller_observation_version = 2,
                7 => {
                    observed.result = ActivationControllerObservationResult::Waiting {
                        message: "No accepted projection".into(),
                    }
                }
                8 => local.intent.context.updater_journal_schema += 1,
                9 => local.intent.context.worker_journal_schema += 1,
                10 => {
                    let ActivationControllerObservationResult::Observed { projection, .. } =
                        &mut observed.result
                    else {
                        unreachable!()
                    };
                    projection.sequence += 1;
                }
                _ => {
                    let ActivationControllerObservationResult::Observed { link, .. } =
                        &mut observed.result
                    else {
                        unreachable!()
                    };
                    link.attempts = "99".into();
                }
            }
            assert!(
                validate_saved_controller(&saved, &local, &saved.command.command_id, &observed)
                    .is_err(),
                "mutation {fault}"
            );
        }
    }
}
