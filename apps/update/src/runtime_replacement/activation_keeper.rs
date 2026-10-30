//! Consume actual original custody; close writer BEFORE all authority copies.
//! Nothing here can adopt a saved selection or construct a process/Job.
use super::*;
use crate::journal::{KeeperSelectionAdmission, KeeperSelectionCommit, KeeperSelectionPermit};
use std::sync::Arc;

#[must_use = "Retain the original worker until coordinator retirement is known"]
pub struct ActivationKeeperAttempt {
    guard: Box<ActivationFinalizationGuard>,
    known: Option<Box<KeeperSelectionCommit>>,
    reason: Option<String>,
}
#[must_use = "A failed retirement retains the actual Job, journal and authority"]
pub struct CoordinatorRetirement {
    // Declaration order requests original Job cleanup before dropping writer
    // or lock if the host itself is abandoned. Drop never claims empty Job.
    guard: Box<ActivationFinalizationGuard>,
    predecessor: KeeperPredecessor,
    journal: Option<UpdateJournal>,
    authority: Option<Arc<UpdateAuthority>>,
    control_authority: Option<Arc<UpdateAuthority>>,
    known: Option<Box<KeeperSelectionCommit>>,
    reason: Option<String>,
}
/// No SQLite, authority Arc or config/file verification pin survives here.
#[must_use = "Keeper loss closes its original managed Job"]
pub struct ActivationKeeperCustody {
    physical: ProcessCustody,
    binding: KeeperBinding,
    worker: Arc<infrastructure::PinnedProcess>,
    predecessor: KeeperPredecessor,
}
/// Query-only predecessor evidence captured by this coordinator while the
/// original keeper is still alive. Saved rows alone never prove its exit.
pub struct KeeperPredecessor(Option<infrastructure::PinnedProcess>);
impl KeeperPredecessor {
    #[cfg(windows)]
    pub(crate) fn capture(
        directory: &std::path::Path,
        record: &SupervisorRecord,
        incoming: Option<&crate::journal::BootstrapReservation>,
    ) -> Result<Self> {
        let Some(incoming) = incoming else {
            return Ok(Self(None));
        };
        if incoming.supervisor.spec != record.spec
            || incoming.intent.previous.worker != record.spec.parent
        {
            return Err("Incoming bootstrap changed the coordinator/logical parent".into());
        }
        // Keep the exact object open before the old keeper may retire. Never
        // reopen its PID later and never turn NotFound into proof of exit.
        let pin = infrastructure::PinnedProcess::live(&incoming.intent.previous.coordinator)
            .map_err(|e| e.to_string())?;
        pin.require_outside_job(&incoming.intent.previous.job_name)
            .map_err(|e| e.to_string())?;
        if UpdateJournal::inspect_bootstrap(directory, &record.spec.launch_id)
            .map_err(|e| e.to_string())?
            .as_ref()
            != Some(incoming)
        {
            return Err("Incoming immutable bootstrap changed during capture".into());
        }
        Ok(Self(Some(pin)))
    }
    #[cfg(windows)]
    fn require_exited(&self, expected: Option<&SupervisorProcess>) -> Result<()> {
        match (&self.0, expected) {
            (None, None) => Ok(()),
            (Some(pin), Some(expected)) if pin.identity() == expected => {
                if pin.exited().map_err(|e| e.to_string())? {
                    Ok(())
                } else {
                    Err("Original predecessor is still alive; another keeper is forbidden".into())
                }
            }
            _ => Err("Original retained predecessor differs; no adoption from saved PID".into()),
        }
    }
}
impl ActivationFinalizationGuard {
    pub fn prepare_keeper_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> std::result::Result<ActivationKeeperAttempt, ActivationFinalizationFailure> {
        match crate::activation::on_original_io(self, move |mut guard| async move {
            let prepared = async {
                if !matches!(
                    guard.record.state,
                    model::ActivationFinalizationState::Confirmed { .. }
                ) || !guard.delivery_attempted
                    || guard.release.is_some()
                {
                    return Err(
                        "Keeper requires an already confirmed original Active owner".to_string()
                    );
                }
                guard.observe_and_confirm(j, a, s).await?;
                guard.require_physical()?;
                let observation = guard
                    .observation
                    .clone()
                    .ok_or("Original Active observation missing")?;
                let query = ActivationFinalizationQuery {
                    finalization_observation_version: 1,
                    request_id: observation
                        .request_id
                        .clone()
                        .ok_or("Original correlation missing")?,
                    command: guard.command.clone(),
                };
                Ok(KeeperSelectionPermit {
                    capture: FinalizationConfirmationPermit {
                        query,
                        observation,
                        observation_deadline: guard
                            .observation_deadline
                            .ok_or("Original Active capture deadline missing")?,
                    },
                })
            }
            .await;
            let permit = match prepared {
                Ok(p) => p,
                Err(reason) => {
                    return Err(ActivationFinalizationFailure {
                        reason,
                        custody: guard,
                    })
                }
            };
            // From this point a COMMIT error may already have changed the
            // durable role. Return a dedicated owner, never the old loop.
            let (known,reason)=match j.select_keeper_role(a,s,permit,||guard.require_physical()) {
                Ok(KeeperSelectionAdmission::Committed(v))=>(Some(v),None),
                Ok(KeeperSelectionAdmission::Existing(_))=>(None,Some("Existing keeper data are not a fresh retirement permit".into())),
                Err(e)=>(None,Some(format!("Keeper selection unconfirmed; retain writer/authority/custody, no replay: {e}"))),
            };
            #[cfg(feature = "update-validation")]
            if known.is_some()
                && std::env::var("WINBRIDGE_KEEPER_RETIREMENT_VALIDATE")
                    .ok()
                    .as_deref()
                    == Some("hold_known_role")
            {
                // Force the real SQL-commit/JoinHandle-finished race. No grant,
                // capture refresh, retry or production delay is introduced.
                eprintln!("WinBridge keeper validation: hold known role before driver completion");
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            }
            Ok(ActivationKeeperAttempt {
                guard,
                known,
                reason,
            })
        }) {
            Ok(value) => value,
            Err((custody, reason)) => Err(ActivationFinalizationFailure {
                reason: reason.into(),
                custody,
            }),
        }
    }
}
impl ActivationKeeperAttempt {
    pub fn bind_coordinator(
        self,
        journal: UpdateJournal,
        authority: Arc<UpdateAuthority>,
        control_authority: Arc<UpdateAuthority>,
        predecessor: KeeperPredecessor,
    ) -> Box<CoordinatorRetirement> {
        Box::new(CoordinatorRetirement {
            guard: self.guard,
            predecessor,
            journal: Some(journal),
            authority: Some(authority),
            control_authority: Some(control_authority),
            known: self.known,
            reason: self.reason,
        })
    }
}
impl CoordinatorRetirement {
    pub fn try_close(
        mut self: Box<Self>,
    ) -> std::result::Result<Box<ActivationKeeperCustody>, Box<Self>> {
        let checked = (|| -> Result<(KeeperBinding, Arc<infrastructure::PinnedProcess>)> {
            let known = self
                .known
                .as_ref()
                .ok_or("Keeper selection was not a known COMMIT; no automatic role adoption")?;
            let authority = self
                .authority
                .as_ref()
                .ok_or("Original updater authority missing")?;
            let control = self
                .control_authority
                .as_ref()
                .ok_or("Original control authority missing")?;
            if !Arc::ptr_eq(authority, control) || Arc::strong_count(authority) != 2 {
                return Err(format!(
                    "Updater authority still has unexpected owners: {}; retain original Job",
                    Arc::strong_count(authority)
                ));
            }
            self.guard.require_physical()?;
            if known.selection().finalization != self.guard.record {
                return Err("Keeper selection changed original Active fact".into());
            }
            self.journal
                .as_ref()
                .ok_or("Original writer missing")?
                .require_known_keeper_selection(authority, known)
                .map_err(|e| e.to_string())?;
            let binding = binding(
                &self.guard,
                Some(known.selection().sha256().map_err(|e| e.to_string())?),
            )?;
            let worker = Arc::new(
                infrastructure::PinnedProcess::live(&binding.worker).map_err(|e| e.to_string())?,
            );
            worker
                .require_job_membership(&binding.job_name)
                .map_err(|e| e.to_string())?;
            Ok((binding, worker))
        })();
        let (binding, worker) = match checked {
            Ok(v) => v,
            Err(reason) => {
                self.reason = Some(reason);
                return Err(self);
            }
        };
        let journal = self.journal.take().expect("Checked original writer");
        match journal.close_owned(self.authority.as_ref().expect("Checked original authority")) {
            Ok(()) => {}
            Err(failure) => {
                let (journal, error) = failure.into_parts();
                self.journal = Some(journal);
                self.reason = Some(format!(
                    "Writer close failed; original authority and Job retained: {error}"
                ));
                return Err(self);
            }
        }
        // All fallible work is finished. Exactly these two copies existed;
        // writer closed while both still held the OS lock. No detached task
        // or guard may retain an uncounted clone across this transition.
        drop(self.authority.take());
        drop(self.control_authority.take());
        let CoordinatorRetirement {
            guard, predecessor, ..
        } = *self;
        let ActivationFinalizationGuard { physical, .. } = *guard;
        Ok(Box::new(ActivationKeeperCustody {
            physical,
            binding,
            worker,
            predecessor,
        }))
    }
    pub fn status(&self) -> KeeperStatus {
        let (out, err, cut, done) = self.guard.diagnostics();
        KeeperStatus {
            phase: KeeperPhase::Unconfirmed,
            binding: binding(
                &self.guard,
                self.known
                    .as_ref()
                    .and_then(|v| v.selection().sha256().ok()),
            )
            .ok(),
            role_selection_known: self.known.is_some(),
            writer_closed: self.journal.is_none(),
            authority_released: self.authority.is_none() && self.control_authority.is_none(),
            original_custody_held: Some(self.guard.physical_custody_held()),
            worker_alive: self.guard.process_alive(),
            job_tree_empty: self
                .guard
                .physical
                .child
                .as_ref()
                .and_then(|c| c.tree_is_empty().ok()),
            stdout_tail: out,
            stderr_tail: err,
            output_incomplete: Some(cut),
            readers_finished: Some(done),
            reason: self.reason.as_ref().map(|v| v.chars().take(1000).collect()),
        }
    }
}
fn binding(
    guard: &ActivationFinalizationGuard,
    selection_sha256: Option<String>,
) -> Result<KeeperBinding> {
    let model::ActivationFinalizationState::Confirmed { receipt } = &guard.record.state else {
        return Err("Keeper has no confirmed original Active fact".into());
    };
    let b = &guard.command.binding.lifetime_command.binding;
    Ok(keeper_binding_from_receipt(
        b.update_id.clone(),
        b.launch_id.clone(),
        b.keeper.clone(),
        b.job_name.clone(),
        receipt,
        selection_sha256,
    ))
}
impl ActivationKeeperCustody {
    /// Native OS/IPC unit fixture only. This does NOT prove the SQL retirement
    /// path; production can construct custody only by consuming its real guard.
    #[cfg(all(test, windows))]
    pub(crate) fn probe_test_custody(child: ManagedChild, binding: KeeperBinding) -> Result<Self> {
        let mut physical = ProcessCustody::new();
        physical.child = Some(child);
        let worker = Arc::new(
            infrastructure::PinnedProcess::live(&binding.worker).map_err(|e| e.to_string())?,
        );
        let value = Self {
            physical,
            binding,
            worker,
            predecessor: KeeperPredecessor(None),
        };
        value.require_physical()?;
        Ok(value)
    }
    /// Native exit-loop fixture only: real original Job/readers, synthetic role
    /// history. This does not execute/attest the SQL retirement path.
    #[cfg(all(test, windows))]
    pub(crate) fn exit_test_custody(
        child: ManagedChild,
        binding: KeeperBinding,
        readers: [winbridge_platform::OutputReader; 2],
    ) -> Result<Self> {
        let mut custody = Self::probe_test_custody(child, binding)?;
        for (i, reader) in readers.into_iter().enumerate() {
            custody
                .physical
                .readers
                .push(drain(reader, Arc::clone(&custody.physical.captures[i]))?);
        }
        Ok(custody)
    }
    #[cfg(all(test, windows))]
    pub(crate) fn physical_test_child(&mut self) -> &mut ManagedChild {
        self.physical
            .child
            .as_mut()
            .expect("Native fixture retains its original child")
    }
    /// Authorize the newly activated ORIGINAL child, not the old supervisor's
    /// logical parent. No JSON PID, saved row or previous token grants this.
    #[cfg(windows)]
    pub(crate) fn require_bootstrap_peer(&self, pid: u32, expected: &KeeperBinding) -> Result<()> {
        if pid != self.binding.worker.process_id || expected != &self.binding {
            return Err("Keeper probe requires its actual original Active child".into());
        }
        self.require_physical()?;
        infrastructure::PinnedProcess::live(&self.binding.coordinator)
            .and_then(|p| p.require_outside_job(&self.binding.job_name))
            .map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    pub(crate) fn require_predecessor_exited(
        &self,
        expected: Option<&SupervisorProcess>,
    ) -> Result<()> {
        self.predecessor.require_exited(expected)
    }
    #[cfg(windows)]
    pub(crate) fn bootstrap_binding(&self) -> &KeeperBinding {
        &self.binding
    }
    fn require_identity(&self) -> Result<()> {
        let actual = self
            .physical
            .child
            .as_ref()
            .and_then(ManagedChild::identity)
            .ok_or("Original keeper child/Job missing")?;
        let expected = &self.binding.worker;
        if self.binding.coordinator.process_id != std::process::id()
            || actual.process_id != expected.process_id
            || actual.creation_time.to_string() != expected.birth
            || actual.host_id != expected.host_id
            || actual.session_id != expected.session_id
            || actual.job_name != self.binding.job_name
            || self.worker.identity() != expected
        {
            return Err("Keeper original physical identity changed".into());
        }
        Ok(())
    }
    fn require_physical(&self) -> Result<()> {
        self.require_identity()?;
        self.worker
            .require_job_membership(&self.binding.job_name)
            .map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    pub(crate) fn worker_exit_pin(&self) -> Arc<infrastructure::PinnedProcess> {
        Arc::clone(&self.worker)
    }
    /// Original object/owned Job/readers only. No saved PID, SQL or new authority.
    #[cfg(windows)]
    pub(crate) fn retire_if_drained(&mut self) -> Result<bool> {
        self.require_identity()?;
        if !self.worker.exited().map_err(|e| e.to_string())? {
            return Ok(false);
        }
        let child = self
            .physical
            .child
            .as_mut()
            .ok_or("Original child missing")?;
        if child.try_wait().map_err(|e| e.to_string())?.is_none()
            || !child.tree_is_empty().map_err(|e| e.to_string())?
        {
            return Ok(false);
        }
        if !self.physical.join_finished_readers() {
            return Ok(false);
        }
        // Nothing was respawned, and no await lies between this final positive
        // observation and closing the original owned Job in Drop.
        self.physical
            .child
            .as_ref()
            .ok_or("Original Job missing")?
            .tree_is_empty()
            .map_err(|e| e.to_string())
    }
    pub fn status(&self) -> KeeperStatus {
        let alive = self.worker.exited().ok().map(|exited| !exited);
        let (out, err, cut, done) = self.physical.diagnostics();
        KeeperStatus {
            phase: if alive == Some(false) {
                KeeperPhase::WorkerExited
            } else {
                KeeperPhase::Keeper
            },
            binding: Some(self.binding.clone()),
            role_selection_known: true,
            writer_closed: true,
            authority_released: true,
            original_custody_held: Some(self.require_identity().is_ok()),
            worker_alive: alive,
            job_tree_empty: self
                .physical
                .child
                .as_ref()
                .and_then(|c| c.tree_is_empty().ok()),
            stdout_tail: out,
            stderr_tail: err,
            output_incomplete: Some(cut),
            readers_finished: Some(done),
            reason: None,
        }
    }
}

#[cfg(all(test, windows))]
mod predecessor_tests {
    use super::*;
    const HELPER:&str="runtime_replacement::physical::activation_files::activation_process::finalization::keeper::predecessor_tests::predecessor_wait_helper";
    #[test]
    #[ignore = "Owned native child, started only by the exact-handle parent test"]
    fn predecessor_wait_helper() {
        std::fs::write("predecessor-ready.pending", std::process::id().to_string()).unwrap();
        std::fs::rename("predecessor-ready.pending", "predecessor-ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    #[test]
    fn retained_predecessor_handle_rejects_live_missing_and_changed_exit_claims() {
        let root = tempfile::tempdir().unwrap();
        let command = winbridge_protocol::ResolvedCommand {
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            arguments: vec![
                "--exact".into(),
                HELPER.into(),
                "--ignored".into(),
                "--test-threads=1".into(),
            ],
            working_directory: root.path().to_string_lossy().into_owned(),
            timeout_ms: 15000,
        };
        let job = format!("Local\\WinBridge-Ancestor-{}", uuid::Uuid::new_v4());
        let (mut child, stdout, stderr) = ManagedChild::prepare(&command, Some(&job)).unwrap();
        let expected = infrastructure::stamp(child.id()).unwrap();
        let original = KeeperPredecessor(None);
        assert!(original.require_exited(None).is_ok());
        assert!(original.require_exited(Some(&expected)).is_err());
        let held = KeeperPredecessor(Some(
            infrastructure::PinnedProcess::live(&expected).unwrap(),
        ));
        child.resume().unwrap();
        let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !root.path().join("predecessor-ready").exists() {
            assert!(
                std::time::Instant::now() < ready_deadline,
                "Exact native helper was not selected"
            );
            assert!(
                child.try_wait().unwrap().is_none(),
                "Exact helper exited before readiness"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join("predecessor-ready")).unwrap(),
            expected.process_id.to_string()
        );
        assert!(held.require_exited(Some(&expected)).is_err());
        assert!(held.require_exited(None).is_err());
        let mut foreign = expected.clone();
        foreign.birth.push('0');
        assert!(held.require_exited(Some(&foreign)).is_err());
        child.terminate_tree().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() || !child.tree_is_empty().unwrap() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        held.require_exited(Some(&expected)).unwrap();
        assert!(held.require_exited(Some(&foreign)).is_err());
        // Reopening a dead/reused PID is deliberately not the authority used by
        // require_exited: the original kernel object stays held across exit.
        assert!(held.0.as_ref().unwrap().require_live().is_err());
        drop((stdout, stderr));
    }
}
