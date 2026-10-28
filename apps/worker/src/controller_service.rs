//! One network-manager start on the same resident local service and Store.
use super::*;
use winbridge_protocol::update::*;
use winbridge_update::activation::controller;

pub(super) enum ControllerStartup {
    NotStarted,
    Attempted,
    Running(crate::network::NetworkTask),
}
impl RunningLocalService {
    /// Trusted private receiver only. Even a failed validation consumes this
    /// attempt: the caller retains the original owner for explicit cleanup.
    pub fn start_controller(
        &mut self,
        command: WorkerControllerActivationCommand,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        if !matches!(self.controller, ControllerStartup::NotStarted) {
            return Err("Controller startup already attempted; observe without replay".into());
        }
        self.controller = ControllerStartup::Attempted;
        if self.exit.is_some() || self.task.as_ref().is_none_or(|task| task.is_finished()) {
            return Err("Local IPC owner has already stopped; no controller startup".into());
        }
        controller::validate_command_envelope(&command)?;
        self.service.check_ready()?;
        let worker = &self.service.worker;
        if Instant::now() >= self.service.deadline
            || crate::store::now_ms() < command.issued_at_ms
            || crate::store::now_ms() >= command.deadline_at_ms
            || command.binding.service_command != self.service.command
            || controller::topology_from_config(
                &worker.config,
                Some(&command.binding.topology.confirmation_endpoint_id),
            )? != command.binding.topology
        {
            return Err("Controller startup changed original local service/config/window".into());
        }
        tokio::runtime::Handle::try_current()
            .map_err(|_| "Controller startup requires original live IO runtime")?;
        checkpoint()?;
        let scope = worker
            .local_activation
            .as_ref()
            .ok_or("Resident local-service binding missing")?;
        {
            let mut sealed = scope
                .controller
                .lock()
                .map_err(|_| "Controller binding lock poisoned")?;
            if sealed.is_some() {
                return Err("Controller binding already sealed; no retry".into());
            }
            *sealed = Some(command);
        }
        self.service.check_ready()?;
        checkpoint()?;
        if self.task.as_ref().is_none_or(|task| task.is_finished()) {
            return Err("Local IPC stopped before controller dispatch".into());
        }
        // Reuse the exact Arc/Store/instance lock/generation/authority. The
        // normal manager supervises all configured connections independently.
        self.controller = ControllerStartup::Running(crate::network::start(Arc::clone(worker)));
        Ok(())
    }

    pub(super) async fn close_controller(&mut self) {
        if let ControllerStartup::Running(network) =
            std::mem::replace(&mut self.controller, ControllerStartup::Attempted)
        {
            network.close().await;
        }
    }
}

pub(super) fn rejected(message: String) -> ActivationControllerObservation {
    ActivationControllerObservation {
        controller_observation_version: 1,
        result: ActivationControllerObservationResult::Rejected { message },
    }
}

impl Worker {
    pub(crate) fn handle_controller_observation(
        &self,
        query: ActivationControllerQuery,
    ) -> ActivationControllerObservation {
        match self.observe_controller(query) {
            Ok(result) => ActivationControllerObservation {
                controller_observation_version: 1,
                result,
            },
            Err(message) => rejected(message),
        }
    }
    fn observe_controller(
        &self,
        query: ActivationControllerQuery,
    ) -> Result<ActivationControllerObservationResult> {
        if self.stop.load(Ordering::Acquire) {
            return Err("Local service stopping; controller readiness unconfirmed".into());
        }
        let scope = self
            .local_activation
            .as_ref()
            .ok_or("No resident staged local service")?;
        if query.controller_observation_version != 1
            || scope
                .controller
                .lock()
                .map_err(|_| "Controller binding lock poisoned")?
                .as_ref()
                != Some(&query.command)
        {
            return Err(
                "Controller observation contract/resident binding differs; no rebind".into(),
            );
        }
        self.store()?.check_local_service_identity(
            &scope.command,
            &scope.shutdown,
            scope.deadline,
        )?;
        let command = &query.command;
        match self.activation_controller_frame(
            &command.binding.topology.confirmation_endpoint_id,
            &command.binding.service_receipt.compatibility.context,
        ) {
            Ok(_) => {}
            Err(message) => return Ok(ActivationControllerObservationResult::Waiting { message }),
        };
        let mut actual = self.observe_service(ActivationServiceQuery {
            service_observation_version: 1,
            command: scope.command.clone(),
        })?;
        // Recapture after potentially expensive file/runtime verification.
        // A previous ticket must not survive that work as current evidence.
        let (projection, link) = match self.activation_controller_frame(
            &command.binding.topology.confirmation_endpoint_id,
            &actual.compatibility.context,
        ) {
            Ok(value) => value,
            Err(message) => return Ok(ActivationControllerObservationResult::Waiting { message }),
        };
        actual.compatibility.observed_at_ms = crate::store::now_ms();
        let c = &link.connection;
        let receipt = WorkerControllerActivationReceipt {
            schema_version: 1,
            command_id: command.command_id.clone(),
            update_id: actual.update_id,
            attempt_id: actual.attempt_id,
            command_sha256: controller::command_sha256(command)?,
            authority: actual.authority,
            session: WorkerControllerSessionEvidence {
                controller_id: link.id.clone(),
                controller_url: c.url.clone(),
                controller_node_id: c.controller_node_id.clone().ok_or("Welcome node missing")?,
                controller_generation: projection.controller_generation.clone(),
                session_id: projection.session_id.clone(),
                controller_build: projection.controller_build.clone(),
                worker_context: actual.compatibility.context.clone(),
                worker_generation: actual.compatibility.generation.clone(),
                worker_build: actual.compatibility.identity.descriptor.build.clone(),
                connection_attempt: link
                    .attempts
                    .parse()
                    .map_err(|_| "Invalid controller attempt")?,
                projection_sequence: projection.sequence,
                projection_sha256: controller::projection_sha256(&projection)?,
                connected_at_ms: link
                    .last_connected_at_ms
                    .ok_or("Selected link connectedAt missing")?,
            },
            compatibility: actual.compatibility,
        };
        controller::validate_projection_evidence(&receipt, command, &projection)?;
        controller::validate_link_evidence(&receipt, command, &link)?;
        // This remains readonly and never creates an update/root/dispatch grant.
        self._instance_lock.revalidate()?;
        self.store()?.check_local_service_identity(
            &scope.command,
            &scope.shutdown,
            scope.deadline,
        )?;
        Ok(ActivationControllerObservationResult::Observed {
            receipt: Box::new(receipt),
            projection: Box::new(projection),
            link: Box::new(link),
        })
    }
}
