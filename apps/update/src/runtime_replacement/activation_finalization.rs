//! One-way retirement of verification pins, preserving the actual original
//! child/Job/channel readers. No cold constructor, adoption or Job duplication.
use super::*;
use crate::activation::{finalization as model, finalization_probe, finalization_wire};
use crate::journal::{
    ActivationFinalizationIntent, FinalizationConfirmationPermit, FinalizationDecisionAdmission,
    FinalizationDecisionPermit, FinalizationReleasePermit,
};
use winbridge_protocol::update::*;

#[path = "activation_keeper.rs"]
mod keeper;
pub use keeper::{
    ActivationKeeperAttempt, ActivationKeeperCustody, CoordinatorRetirement, KeeperPredecessor,
};

#[must_use = "Dropping closes the original managed worker Job"]
pub struct ActivationFinalizationPreparation {
    owner: Box<ActivationFinalizationGuard>,
}
/// Actual original OS custody after ALL verification/config pins are retired.
/// This is not a permanent keeper yet: the host still owns updater authority.
#[must_use = "Keep original worker custody until keeper conversion or confirmed cleanup"]
pub struct ActivationFinalizationGuard {
    physical: ProcessCustody,
    local: ActivationRecord,
    command: WorkerFinalizationCommand,
    service: WorkerServiceActivationCommand,
    shutdown: crate::shutdown::ShutdownRecord,
    record: model::ActivationFinalizationRecord,
    release: Option<Box<FinalizationReleasePermit>>,
    delivery_attempted: bool,
    interrupted_reply_prefix: Option<Vec<u8>>,
    observation: Option<ActivationFinalizationObservation>,
    observation_deadline: Option<std::time::Instant>,
}
#[must_use = "Failure retains the actual original child/Job; reconcile without redelivery"]
pub struct ActivationFinalizationFailure {
    pub reason: String,
    custody: Box<ActivationFinalizationGuard>,
}
impl std::fmt::Debug for ActivationFinalizationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivationFinalizationFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ActivationFinalizationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for ActivationFinalizationFailure {}
impl ActivationFinalizationFailure {
    pub(crate) fn custody_record(&self) -> &ActivationRecord {
        self.custody.activation()
    }
    pub fn record(&self) -> &model::ActivationFinalizationRecord {
        &self.custody.record
    }
    pub fn process_id(&self) -> Option<u32> {
        self.custody.process_id()
    }
    pub(crate) fn into_custody(self) -> Box<ActivationFinalizationGuard> {
        self.custody
    }
}

impl ActivationProcessGuard {
    /// Fresh V7 and the same original physical owner are mandatory. Existing/
    /// uncertain root records never recreate a fresh release preparation.
    pub fn prepare_finalization_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lifetime_key: &str,
        key: &str,
    ) -> std::result::Result<ActivationFinalizationPreparation, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.admit_finalization(j, a, s, lifetime_key, key).await {
                Ok((release, service, shutdown)) => Ok(ActivationFinalizationPreparation {
                    owner: ActivationFinalizationGuard::retire_verification(
                        owner, release, service, shutdown,
                    ),
                }),
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
    async fn admit_finalization(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        lifetime_key: &str,
        key: &str,
    ) -> Result<(
        Box<FinalizationReleasePermit>,
        WorkerServiceActivationCommand,
        crate::shutdown::ShutdownRecord,
    )> {
        let mut timing = crate::activation::ActivationTiming::new("finalization_prepare", None);
        self.require_lifetime_custody(false)?;
        {
            let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
            if worker.evidence != self.record.intent.source.worker
                || !crate::worker_rehearsal::supports(
                    &worker,
                    &activation_finalization_marker_pattern(),
                )?
                || !crate::worker_rehearsal::supports(
                    &worker,
                    &finalization_observation_marker_pattern(),
                )?
            {
                return Err("Original resident lacks V8 and readonly Active receivers; no root decision or fallback".into());
            }
        }
        timing.mark("receiver_capabilities_checked", None, None);
        // Read ordinary historical inputs BEFORE taking the fresh capture.
        // The transaction compares them with its own full current snapshot.
        let root = j.get(&s.spec.update_id).map_err(|e| e.to_string())?;
        let shutdown = j
            .shutdown(&s.spec.update_id)
            .map_err(|e| e.to_string())?
            .ok_or("Original shutdown prefix missing")?;
        let graph = self
            .observe_resident_for_finalization(j, a, s, lifetime_key)
            .await?;
        let deadline = self
            .resident_observation_deadline
            .ok_or("Original V7 monotonic capture missing")?;
        timing.observe_deadline(deadline);
        timing.mark("fresh_v7_observed", None, None);
        let query = self
            .resident_query
            .as_ref()
            .ok_or("Fresh V7 request missing")?;
        let observed = self
            .resident_observation
            .as_ref()
            .ok_or("Fresh V7 reply missing")?;
        let command =
            model::prepare_command(&graph, &root, &shutdown, query, observed, key, self.at()?)?;
        let service = graph.controller.service.command.clone();
        let permit = FinalizationDecisionPermit {
            intent: ActivationFinalizationIntent {
                command,
                root_before: root,
                query: query.clone(),
                observation: observed.clone(),
            },
            observation_deadline: deadline,
        };
        let child = infrastructure::PinnedProcess::live(
            &graph
                .lifetime
                .command
                .binding
                .compatibility
                .identity
                .process,
        )
        .map_err(|e| e.to_string())?;
        let admitted = j
            .commit_activation_finalization(
                a,
                s,
                &self.files.preparation,
                &self.record,
                permit,
                || {
                    self.require_lifetime_custody(false)?;
                    child
                        .require_job_membership(&graph.lifetime.command.binding.job_name)
                        .map_err(|e| e.to_string())
                },
            )
            .map_err(|e| e.to_string())?;
        match admitted {
            FinalizationDecisionAdmission::Committed(release) => {
                timing.mark("original_root_decision_committed", None, None);
                // No fallible operation after known COMMIT before retirement:
                // the original child/Job already have a supervised lifetime.
                Ok((release, service, shutdown))
            }
            FinalizationDecisionAdmission::Existing(_) => {
                Err("Saved finalization decision is not a fresh release permit; no delivery".into())
            }
        }
    }
}

impl ActivationFinalizationGuard {
    fn retire_verification(
        original: Box<ActivationProcessGuard>,
        release: Box<FinalizationReleasePermit>,
        service: WorkerServiceActivationCommand,
        shutdown: crate::shutdown::ShutdownRecord,
    ) -> Box<Self> {
        let command = release.record().command.clone();
        let record = release.record().clone();
        let ActivationProcessGuard {
            physical,
            record: local,
            files,
            released,
            _controller_config_pin,
            ..
        } = *original;
        // ProcessCustody is moved whole: no Job detach/clone, no private V6
        // read, and both output readers keep draining. All other fields are
        // verification/config/journal authority owners or plain historical data.
        drop((_controller_config_pin, released, files));
        Box::new(Self {
            physical,
            local,
            command,
            service,
            shutdown,
            record,
            release: Some(release),
            delivery_attempted: false,
            interrupted_reply_prefix: None,
            observation: None,
            observation_deadline: None,
        })
    }
    pub fn record(&self) -> &model::ActivationFinalizationRecord {
        &self.record
    }
    pub fn activation(&self) -> &ActivationRecord {
        &self.local
    }
    pub fn process_id(&self) -> Option<u32> {
        self.physical.child.as_ref().map(ManagedChild::id)
    }
    pub fn delivery_attempted(&self) -> bool {
        self.delivery_attempted
    }
    pub fn interrupted_reply_prefix(&self) -> Option<&[u8]> {
        self.interrupted_reply_prefix.as_deref()
    }
    pub fn observation(&self) -> Option<&ActivationFinalizationObservation> {
        self.observation.as_ref()
    }
    pub(crate) fn diagnostics(&self) -> (String, String, bool, bool) {
        self.physical.diagnostics()
    }
    pub(crate) fn process_alive(&self) -> Option<bool> {
        let process = &self
            .command
            .binding
            .lifetime_command
            .binding
            .compatibility
            .identity
            .process;
        match infrastructure::PinnedProcess::live(process) {
            Ok(_) => Some(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
            Err(_) => None,
        }
    }
    pub(crate) fn resident_process(&self) -> Option<SupervisorProcess> {
        let actual = self.physical.child.as_ref()?.identity()?;
        Some(SupervisorProcess {
            process_id: actual.process_id,
            birth: actual.creation_time.to_string(),
            host_id: actual.host_id.clone(),
            session_id: actual.session_id,
        })
    }
    pub(crate) fn physical_custody_held(&self) -> bool {
        self.require_physical().is_ok()
    }
    fn require_physical(&self) -> Result<()> {
        let b = &self.command.binding.lifetime_command.binding;
        let expected = &b.compatibility.identity.process;
        let child = self
            .physical
            .child
            .as_ref()
            .and_then(ManagedChild::identity)
            .ok_or("Original finalization child/Job missing")?;
        if b.keeper.process_id != std::process::id()
            || child.process_id != expected.process_id
            || child.creation_time.to_string() != expected.birth
            || child.host_id != expected.host_id
            || child.session_id != expected.session_id
            || child.job_name != b.job_name
            || self.local.state_owned.as_ref().map(|v| &v.process) != Some(expected)
        {
            return Err("Finalization changed original physical keeper/child/Job".into());
        }
        infrastructure::PinnedProcess::live(&b.keeper)
            .and_then(|p| p.require_live())
            .map_err(|e| e.to_string())?;
        infrastructure::PinnedProcess::live(expected)
            .and_then(|p| p.require_job_membership(&b.job_name))
            .map_err(|e| e.to_string())
    }
    fn record_unknown(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        reason: &str,
    ) {
        let reason: String = reason
            .chars()
            .filter(|c| !c.is_control())
            .take(1000)
            .collect();
        match j.mark_activation_finalization_unknown(a, s, &self.command, reason) {
            Ok(record) => self.record = record,
            Err(error) => eprintln!(
                "Finalization outcome remains unconfirmed; diagnostic append failed: {error}"
            ),
        }
    }
    async fn deliver(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new("finalization_delivery", None);
        self.require_physical()?;
        let permit = self
            .release
            .as_ref()
            .ok_or("Original known release permit already consumed; no resend")?;
        let saved = j
            .require_activation_finalization_release(a, s, permit)
            .map_err(|e| e.to_string())?;
        if saved.shutdown != self.shutdown
            || saved.lifetime.controller.service.command != self.service
        {
            return Err("Finalization changed original service/shutdown scope".into());
        }
        self.require_physical()?;
        drop(self.release.take()); // Only the consuming preparation reaches this.
        drop(self.physical.channel.take()); // Retire partial V6; never read/reuse it.
        self.delivery_attempted = true;
        let request = ActivationFinalizationRequest {
            finalization_version: ACTIVATION_FINALIZATION_CONTRACT,
            command: self.command.clone(),
        };
        let physical = &self.service.binding.writer_command.binding.physical;
        let endpoint = winbridge_transport::endpoint(
            Path::new(&physical.state.state_directory),
            &physical.context.instance_id,
        )
        .map_err(|e| e.to_string())?;
        timing.mark("original_pins_and_v6_retired", None, None);
        let pid = physical.process.process_id;
        let response = self.delivery_response(&endpoint, pid, &request).await;
        let delivered = response.and_then(|value| {
            finalization_wire::validate_response(&request, &self.shutdown, &value)
        });
        if let Err(reason) = delivered {
            let diagnostic: String = reason
                .chars()
                .filter(|c| !c.is_control())
                .take(1000)
                .collect();
            eprintln!(
                "Original V8 reply unconfirmed: {diagnostic}; observe Active without resending"
            );
            self.record_unknown(j, a, s, &format!("V8 result unconfirmed: {reason}"));
        }
        timing.mark("single_delivery_finished_or_unknown", None, None);
        // Success also needs an independent read. Never resend to recover ACK.
        self.observe_and_confirm(j, a, s).await
    }
    async fn delivery_response(
        &mut self,
        endpoint: &str,
        pid: u32,
        request: &ActivationFinalizationRequest,
    ) -> Result<ActivationFinalizationResponse> {
        #[cfg(feature = "update-validation")]
        if std::env::var("WINBRIDGE_FINALIZATION_READY_LOSS_VALIDATE")
            .ok()
            .as_deref()
            == Some("prefix")
        {
            use tokio::io::AsyncReadExt;
            // The production10s timeout can fire before this deliberately
            // interrupted reply reaches its header. This validation-only wait
            // observes one already accepted send; it renews no V7/startup grant.
            return tokio::time::timeout(
                std::time::Duration::from_millis(ACTIVATION_FINALIZATION_REPLY_WAIT_MS),
                async {
                    let mut stream = winbridge_transport::supervisor::connect(endpoint, pid)
                        .await
                        .map_err(|e| e.to_string())?;
                    winbridge_transport::write_frame(
                        &mut stream,
                        &serde_json::to_vec(request).map_err(|e| e.to_string())?,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    let mut prefix = [0; 4];
                    stream
                        .read_exact(&mut prefix)
                        .await
                        .map_err(|e| e.to_string())?;
                    let length = u32::from_le_bytes(prefix);
                    if length == 0 || length > 1_048_576 {
                        return Err("Invalid interrupted V8 frame prefix".into());
                    }
                    self.interrupted_reply_prefix = Some(prefix.to_vec());
                    // Actual original PID is authenticated. Discard the unread
                    // body and use a new readonly connection, never resend V8.
                    drop(stream);
                    Err(
                        "Injected loss after V8 frame prefix; original result unknown, no resend"
                            .into(),
                    )
                },
            )
            .await
            .map_err(|_| {
                "Interrupted V8 observation timed out; delivery outcome unknown".to_string()
            })?;
        }
        winbridge_transport::supervisor::call_with_timeout(
            endpoint,
            pid,
            request,
            winbridge_transport::IPC_TIMEOUT,
        )
        .await
        .map_err(|e| e.to_string())
    }
    async fn observe_and_confirm(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<()> {
        self.observation = None;
        self.observation_deadline = None;
        self.require_physical()?;
        let saved = j
            .activation_finalization(&s.spec.update_id)
            .map_err(|e| e.to_string())?
            .ok_or("Original finalization ledger missing")?;
        if saved.command != self.command {
            return Err("Finalization observation changed original decision".into());
        }
        let query = ActivationFinalizationQuery {
            finalization_observation_version: ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: self.command.clone(),
        };
        let deadline = std::time::Instant::now() + winbridge_transport::IPC_TIMEOUT;
        let observed =
            finalization_probe::observe(&query, &self.shutdown, &self.service, deadline, || {
                self.require_physical()
            })
            .await?;
        self.require_physical()?;
        let permit = FinalizationConfirmationPermit {
            query,
            observation: observed.clone(),
            observation_deadline: deadline,
        };
        let confirmed = j
            .confirm_activation_finalization(a, s, permit, || self.require_physical())
            .map_err(|e| e.to_string())?;
        self.record = confirmed;
        self.observation = Some(observed);
        self.observation_deadline = Some(deadline);
        Ok(())
    }
    /// Readonly recovery of the original child, followed only by fact append.
    /// There is no path from this method back to a V8 delivery or root decision.
    pub fn observe_blocking(
        mut self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<Self>, ActivationFinalizationFailure> {
        self.observation = None;
        self.observation_deadline = None;
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.observe_and_confirm(j, a, s).await {
                Ok(()) => Ok(owner),
                Err(reason) => Err(ActivationFinalizationFailure {
                    reason,
                    custody: owner,
                }),
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationFinalizationFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }
}
impl ActivationFinalizationPreparation {
    pub fn record(&self) -> &model::ActivationFinalizationRecord {
        self.owner.record()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.owner.process_id()
    }
    pub fn deliver_blocking(
        self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<Box<ActivationFinalizationGuard>, ActivationFinalizationFailure> {
        match crate::activation::on_original_io(self.owner, move |mut owner| async move {
            match owner.deliver(j, a, s).await {
                Ok(()) => Ok(owner),
                Err(reason) => {
                    owner.record_unknown(j, a, s, &reason);
                    Err(ActivationFinalizationFailure {
                        reason,
                        custody: owner,
                    })
                }
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationFinalizationFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }
}
