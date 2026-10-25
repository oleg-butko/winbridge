//! Shutdown intent/delivery data. Deserializing these values never authorizes a
//! stop, a restart, a database migration or runtime replacement.
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, validate_update_key, MAX_UPDATE_COUNTER};
use winbridge_protocol::{update::*, ArtifactRole, LoadedWorkerIdentity};
mod state;
pub use state::{
    LockedShutdownState, ReleasedShutdownState, ShutdownStateObservation, ShutdownStatePins,
    ShutdownStateReleaseFailure, ShutdownStateReleasePhase,
};
mod inspection;
pub use inspection::{inspect_saved_receipt, ShutdownInspectionRequest};
mod physical;
pub use physical::Custody;
pub use physical::ShutdownExitGuard;
#[cfg(windows)]
pub(crate) use physical::{ExitedStateCustody, ReleasedExitCustody};
mod driver;
pub(crate) use driver::{execute, prepare, prepare_for_backup, Prepared};
pub(crate) fn view(
    saved: Option<&ShutdownRecord>,
    active: bool,
    failed: bool,
    custody: bool,
) -> CandidateShutdownView {
    let phase = match saved {
        None if failed => CandidateShutdownPhase::Blocked,
        None => CandidateShutdownPhase::NotAdmitted,
        Some(r) if matches!(r.delivery, ShutdownDeliveryState::Exited { .. }) => {
            CandidateShutdownPhase::Exited
        }
        Some(_) if failed || !active => CandidateShutdownPhase::Indeterminate,
        Some(r) => match r.delivery {
            ShutdownDeliveryState::Prepared => CandidateShutdownPhase::Prepared,
            ShutdownDeliveryState::Issued { .. } => CandidateShutdownPhase::AwaitingAcceptance,
            ShutdownDeliveryState::Acknowledged { .. } => {
                CandidateShutdownPhase::AwaitingExitAndLock
            }
            _ => CandidateShutdownPhase::Indeterminate,
        },
    };
    let (accepted_at_ms, exited_at_ms) = match saved.map(|r| &r.delivery) {
        Some(ShutdownDeliveryState::Acknowledged { receipt, .. }) => {
            (Some(receipt.accepted_at_ms), None)
        }
        Some(ShutdownDeliveryState::Exited {
            receipt, physical, ..
        }) => (Some(receipt.accepted_at_ms), Some(physical.observed_at_ms)),
        _ => (None, None),
    };
    CandidateShutdownView {
        phase,
        attempt_id: saved.map(|r| r.intent.attempt_id.clone()),
        command_id: saved.map(|r| r.intent.command.command_id.clone()),
        delivery_revision: saved.map(|r| r.revision),
        accepted_at_ms,
        exited_at_ms,
        physical_custody_held: custody && phase == CandidateShutdownPhase::Exited,
    }
}
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownBinding {
    pub update_id: String,
    pub launch_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    /// Root Quiesced revision, before the atomic BeginShutdown edge.
    pub parent_revision: u64,
    pub admitted_at_ms: u64,
    pub drain_sha256: String,
    pub worker: SupervisorWorkerBinding,
    pub lease: WorkerUpdateLease,
    pub loaded_worker: LoadedWorkerIdentity,
    pub worker_state: WorkerShutdownState,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownIntent {
    pub schema_version: u16,
    pub attempt_id: String,
    pub binding: ShutdownBinding,
    pub command: WorkerShutdownCommand,
}
fn digest(v: &impl Serialize) -> Result<String> {
    let json = serde_json::to_value(v).map_err(|e| e.to_string())?;
    let mut data = b"winbridge.shutdown.intent.v1\n".to_vec();
    data.extend(winbridge_protocol::canonical_evidence_json(&json).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&data))
}
fn id(s: &str) -> Result<()> {
    uuid::Uuid::parse_str(s)
        .map(|_| ())
        .map_err(|_| "Invalid shutdown identity".into())
}
fn time(n: u64) -> Result<()> {
    if n == 0 || n > MAX_UPDATE_COUNTER {
        Err("Invalid shutdown time/counter".into())
    } else {
        Ok(())
    }
}
fn text(s: &str, limit: usize) -> bool {
    !s.is_empty() && s.len() <= limit && !s.chars().any(char::is_control)
}
fn state_valid(s: &WorkerShutdownState) -> bool {
    text(&s.state_directory, 4096)
        && [&s.directory_identity, &s.journal_identity, &s.lock_identity]
            .into_iter()
            .all(|s| text(s, 128))
        && s.directory_identity != s.journal_identity
        && s.directory_identity != s.lock_identity
        && s.journal_identity != s.lock_identity
}
impl ShutdownIntent {
    /// Pure construction; the future journal must check fresh owner/drain/OS
    /// evidence and commit before returning a nonserializable delivery permit.
    pub fn from_quiesced(binding: ShutdownBinding, attempt_id: String) -> Result<Self> {
        let command = WorkerShutdownCommand {
            command_id: format!("shutdown-{attempt_id}"),
            expected_lease_revision: binding.lease.revision,
            intent_sha256: digest(&(&binding, &attempt_id))?,
            reason: "Завершить этот worker для обновления после сохранения квитанции".into(),
        };
        let intent = Self {
            schema_version: 1,
            attempt_id,
            binding,
            command,
        };
        intent.validate()?;
        Ok(intent)
    }
    pub fn validate(&self) -> Result<()> {
        let b = &self.binding;
        for value in [
            &self.attempt_id,
            &b.update_id,
            &b.launch_id,
            &b.owner_id,
            &b.worker.context.journal_incarnation,
            &b.worker.generation,
            &b.loaded_worker.set_id,
        ] {
            id(value)?;
        }
        time(b.owner_epoch)?;
        time(b.admitted_at_ms)?;
        let p = &b.worker.process;
        if self.schema_version != 1
            || b.parent_revision > MAX_UPDATE_COUNTER - 4
            || !valid_hash(&b.drain_sha256, &[64])
            || !valid_hash(&b.worker.config_sha256, &[64])
            || !winbridge_core::config::valid_id(&b.worker.context.instance_id)
            || p.process_id == 0
            || p.birth.is_empty()
            || p.birth.len() > 64
            || !p.birth.bytes().all(|c| c.is_ascii_digit())
            || !valid_hash(&p.host_id, &[64])
            || b.loaded_worker.schema_version != 1
            || b.loaded_worker.kind != "loaded_worker_identity"
            || b.loaded_worker.process != *p
            || b.loaded_worker.descriptor.build != b.worker.build
            || b.loaded_worker.descriptor.role != ArtifactRole::Worker
            || b.loaded_worker.descriptor.schema_version != 1
            || b.loaded_worker.descriptor.kind != "winbridge_artifact"
            || b.loaded_worker.descriptor.query_contract != 1
            || !text(&b.loaded_worker.runtime_identity, 128)
            || !valid_hash(&b.loaded_worker.manifest_sha256, &[64])
            || !valid_hash(&b.loaded_worker.image.sha256, &[64])
            || !text(&b.loaded_worker.image.file_identity, 128)
            || !text(&b.loaded_worker.image.path, 4096)
            || b.loaded_worker.image.bytes == 0
            || b.loaded_worker.image.bytes > 256 * 1024 * 1024
            || !b.loaded_worker.image.query_supported
            || b.lease.phase != WorkerUpdatePhase::Quiesced
            || b.lease.revision == 0
            || b.lease.revision >= MAX_UPDATE_COUNTER
            || b.lease.spec.update_id != b.update_id
            || b.lease.spec.updater_owner_id != b.owner_id
            || b.lease.spec.updater_epoch != b.owner_epoch
            || b.lease.spec.instance_id != b.worker.context.instance_id
            || b.lease.spec.journal_incarnation != b.worker.context.journal_incarnation
            || b.lease.spec.worker_generation != b.worker.generation
            || !state_valid(&b.worker_state)
        {
            return Err("Shutdown requires its exact quiesced worker/process/state scope".into());
        }
        validate_update_key(&self.command.command_id)?;
        if self.command.command_id != format!("shutdown-{}", self.attempt_id)
            || self.command.expected_lease_revision != b.lease.revision
            || self.command.intent_sha256 != digest(&(b, &self.attempt_id))?
            || self.command.reason.is_empty()
            || self.command.reason.len() > 4096
            || self
                .command
                .reason
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("Shutdown immutable command/binding digest differs".into());
        }
        Ok(())
    }
    pub fn validate_receipt(&self, r: &WorkerShutdownReceipt) -> Result<()> {
        self.validate()?;
        let b = &self.binding;
        time(r.accepted_at_ms)?;
        if r.schema_version != 1
            || r.update_id != b.update_id
            || r.launch_id != b.launch_id
            || r.updater_owner_id != b.owner_id
            || r.updater_epoch != b.owner_epoch
            || r.command != self.command
            || r.lease != b.lease
            || r.process != b.worker.process
            || r.state != b.worker_state
        {
            return Err("Shutdown receipt command/owner/lease/process/state differs".into());
        }
        Ok(())
    }
}

/// Retained observations only. A consuming driver must obtain these from exact
/// process observation and the actual instance-lock guard; JSON is not a lock.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownExitEvidence {
    pub process: SupervisorProcess,
    pub state: WorkerShutdownState,
    pub runtime_identity: String,
    pub exact_process_exited: bool,
    pub instance_lock_acquired: bool,
    pub observed_at_ms: u64,
}
impl ShutdownExitEvidence {
    pub fn validate(&self, intent: &ShutdownIntent) -> Result<()> {
        intent.validate()?;
        time(self.observed_at_ms)?;
        if self.process != intent.binding.worker.process
            || self.state != intent.binding.worker_state
            || self.runtime_identity != intent.binding.loaded_worker.runtime_identity
            || !self.exact_process_exited
            || !self.instance_lock_acquired
        {
            return Err("Exact worker exit/state/instance lock remains unconfirmed".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ShutdownDeliveryState {
    Prepared,
    Issued {
        at_ms: u64,
    },
    Uncertain {
        at_ms: u64,
        reason: String,
    },
    Acknowledged {
        at_ms: u64,
        receipt: Box<WorkerShutdownReceipt>,
    },
    Exited {
        at_ms: u64,
        receipt: Box<WorkerShutdownReceipt>,
        physical: Box<ShutdownExitEvidence>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownRecord {
    pub schema_version: u16,
    pub intent: ShutdownIntent,
    pub revision: u64,
    pub delivery: ShutdownDeliveryState,
}

/// Fresh admission and delivery capabilities are deliberately absent from
/// Serde. Reading an Issued/Uncertain journal row cannot recreate either one.
pub struct ShutdownPermit {
    pub(crate) intent: ShutdownIntent,
}
pub enum ShutdownAdmission {
    Fresh(Box<ShutdownPermit>),
    Observed(Box<ShutdownRecord>),
}
pub struct ShutdownDeliveryPermit {
    pub(crate) record: ShutdownRecord,
}
impl ShutdownDeliveryPermit {
    pub fn into_record(self) -> ShutdownRecord {
        self.record
    }
}
pub enum ShutdownIssue {
    Fresh(Box<ShutdownDeliveryPermit>),
    Observed(Box<ShutdownRecord>),
}

pub(crate) fn drain_digest(value: &crate::drain::DrainSnapshot) -> Result<String> {
    let json = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut data = b"winbridge.shutdown.drain.v1\n".to_vec();
    data.extend(winbridge_protocol::canonical_evidence_json(&json).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&data))
}
impl ShutdownRecord {
    pub fn prepare(intent: ShutdownIntent) -> Result<Self> {
        intent.validate()?;
        Ok(Self {
            schema_version: 1,
            intent,
            revision: 0,
            delivery: ShutdownDeliveryState::Prepared,
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        if self.schema_version != 1 {
            return Err("Unsupported shutdown record".into());
        }
        let valid = match &self.delivery {
            ShutdownDeliveryState::Prepared => self.revision == 0,
            ShutdownDeliveryState::Issued { at_ms } => {
                time(*at_ms)?;
                self.revision == 1
            }
            ShutdownDeliveryState::Uncertain { at_ms, reason } => {
                time(*at_ms)?;
                self.revision == 2 && !reason.is_empty() && reason.len() <= 4096
            }
            ShutdownDeliveryState::Acknowledged { at_ms, receipt } => {
                time(*at_ms)?;
                self.intent.validate_receipt(receipt)?;
                matches!(self.revision, 2 | 3)
            }
            ShutdownDeliveryState::Exited {
                at_ms,
                receipt,
                physical,
            } => {
                time(*at_ms)?;
                self.intent.validate_receipt(receipt)?;
                physical.validate(&self.intent)?;
                matches!(self.revision, 3 | 4)
            }
        };
        if valid {
            Ok(())
        } else {
            Err("Shutdown delivery/revision inconsistent".into())
        }
    }
    pub fn issue(&self, at_ms: u64) -> Result<Self> {
        self.validate()?;
        time(at_ms)?;
        if self.delivery != ShutdownDeliveryState::Prepared {
            return Err("Shutdown issued already; observe only, never resend".into());
        }
        Ok(Self {
            revision: 1,
            delivery: ShutdownDeliveryState::Issued { at_ms },
            ..self.clone()
        })
    }
    pub fn uncertain(&self, at_ms: u64, reason: String) -> Result<Self> {
        self.validate()?;
        time(at_ms)?;
        if !matches!(self.delivery, ShutdownDeliveryState::Issued { .. }) {
            return Err("Only issued shutdown can become uncertain".into());
        }
        let next = Self {
            revision: 2,
            delivery: ShutdownDeliveryState::Uncertain { at_ms, reason },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn acknowledge(&self, at_ms: u64, receipt: WorkerShutdownReceipt) -> Result<Self> {
        self.validate()?;
        time(at_ms)?;
        self.intent.validate_receipt(&receipt)?;
        match &self.delivery {
            ShutdownDeliveryState::Acknowledged { receipt: old, .. }
            | ShutdownDeliveryState::Exited { receipt: old, .. } => {
                return if old.as_ref() == &receipt {
                    Ok(self.clone())
                } else {
                    Err("Immutable shutdown ACK changed".into())
                }
            }
            ShutdownDeliveryState::Issued { .. } | ShutdownDeliveryState::Uncertain { .. } => {}
            _ => return Err("Shutdown ACK without issued intent".into()),
        }
        let next = Self {
            revision: self.revision + 1,
            delivery: ShutdownDeliveryState::Acknowledged {
                at_ms,
                receipt: Box::new(receipt),
            },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn confirm_exit(&self, at_ms: u64, physical: ShutdownExitEvidence) -> Result<Self> {
        self.validate()?;
        time(at_ms)?;
        physical.validate(&self.intent)?;
        let receipt = match &self.delivery {
            ShutdownDeliveryState::Acknowledged { receipt, .. } => receipt.clone(),
            ShutdownDeliveryState::Exited { physical: old, .. } => {
                return if old.as_ref() == &physical {
                    Ok(self.clone())
                } else {
                    Err("Immutable shutdown exit proof changed".into())
                }
            }
            _ => {
                return Err(
                    "Exit observation cannot substitute for an accepted shutdown receipt".into(),
                )
            }
        };
        let next = Self {
            revision: self.revision + 1,
            delivery: ShutdownDeliveryState::Exited {
                at_ms,
                receipt,
                physical: Box::new(physical),
            },
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    // Synthetic typed data only; flags below never constitute an OS proof.
    pub(crate) fn fixture() -> (ShutdownIntent, WorkerShutdownReceipt, ShutdownExitEvidence) {
        let h = crate::health::tests::verified();
        let c = h.context.unwrap().compatibility;
        let worker = SupervisorWorkerBinding {
            context: c.context.clone(),
            generation: c.generation.clone(),
            build: c.identity.descriptor.build.clone(),
            process: c.identity.process.clone(),
            config_sha256: c.worker_config_sha256.clone(),
            endpoint: "fixture-only-endpoint".into(),
        };
        let update_id = uuid::Uuid::new_v4().to_string();
        let owner_id = uuid::Uuid::new_v4().to_string();
        let lease = WorkerUpdateLease {
            spec: WorkerUpdateSpec {
                update_id: update_id.clone(),
                instance_id: c.context.instance_id.clone(),
                journal_incarnation: c.context.journal_incarnation.clone(),
                worker_generation: c.generation.clone(),
                updater_owner_id: owner_id.clone(),
                updater_epoch: 2,
                source_directory: "synthetic-source".into(),
                baseline_manifest_sha256: "b".repeat(64),
                target_manifest_sha256: "c".repeat(64),
                projects: vec!["source".into()],
            },
            revision: 4,
            phase: WorkerUpdatePhase::Quiesced,
            accepted_at_ms: 1,
            updated_at_ms: 2,
            reason: "Synthetic quiesced data".into(),
        };
        let state = WorkerShutdownState {
            state_directory: "synthetic-state".into(),
            directory_identity: "state-directory".into(),
            journal_identity: "journal-file".into(),
            lock_identity: "instance-lock".into(),
        };
        let binding = ShutdownBinding {
            update_id,
            launch_id: uuid::Uuid::new_v4().to_string(),
            owner_id,
            owner_epoch: 2,
            parent_revision: 14,
            admitted_at_ms: 100,
            drain_sha256: "a".repeat(64),
            worker,
            lease,
            loaded_worker: c.identity,
            worker_state: state,
        };
        let intent =
            ShutdownIntent::from_quiesced(binding, uuid::Uuid::new_v4().to_string()).unwrap();
        let b = &intent.binding;
        let receipt = WorkerShutdownReceipt {
            schema_version: 1,
            update_id: b.update_id.clone(),
            launch_id: b.launch_id.clone(),
            updater_owner_id: b.owner_id.clone(),
            updater_epoch: b.owner_epoch,
            command: intent.command.clone(),
            lease: b.lease.clone(),
            process: b.worker.process.clone(),
            state: b.worker_state.clone(),
            accepted_at_ms: 200,
        };
        let physical = ShutdownExitEvidence {
            process: b.worker.process.clone(),
            state: b.worker_state.clone(),
            runtime_identity: b.loaded_worker.runtime_identity.clone(),
            exact_process_exited: true,
            instance_lock_acquired: true,
            observed_at_ms: 300,
        };
        (intent, receipt, physical)
    }
    #[test]
    fn acceptance_is_not_exit_and_uncertain_deserialization_never_reissues_stop() {
        let (intent, receipt, physical) = fixture();
        let prepared = ShutdownRecord::prepare(intent).unwrap();
        assert!(prepared.acknowledge(201, receipt.clone()).is_err());
        let issued = prepared.issue(101).unwrap();
        assert!(issued.confirm_exit(301, physical.clone()).is_err());
        let unknown = issued.uncertain(102, "ACK lost".into()).unwrap();
        let reopened: ShutdownRecord =
            serde_json::from_slice(&serde_json::to_vec(&unknown).unwrap()).unwrap();
        assert!(reopened.issue(103).is_err());
        let ack = reopened.acknowledge(201, receipt.clone()).unwrap();
        assert!(matches!(
            ack.delivery,
            ShutdownDeliveryState::Acknowledged { .. }
        ));
        assert_eq!(ack.acknowledge(199, receipt.clone()).unwrap(), ack);
        let mut bad = physical.clone();
        bad.exact_process_exited = false;
        assert!(ack.confirm_exit(301, bad).is_err());
        let mut bad = physical.clone();
        bad.instance_lock_acquired = false;
        assert!(ack.confirm_exit(301, bad).is_err());
        let exited = ack.confirm_exit(301, physical.clone()).unwrap();
        assert_eq!(exited.revision, 4);
        assert_eq!(exited.confirm_exit(302, physical).unwrap(), exited);
        assert_eq!(exited.acknowledge(303, receipt).unwrap(), exited);
        assert!(exited.issue(304).is_err());
    }
    #[test]
    fn receipt_rejects_changed_command_owner_epoch_process_and_state_file_identity() {
        let (intent, receipt, physical) = fixture();
        for field in [
            "command",
            "owner",
            "epoch",
            "process",
            "directory",
            "journal",
            "lock",
            "lease",
        ] {
            let mut r = receipt.clone();
            match field {
                "command" => r.command.command_id.push('x'),
                "owner" => r.updater_owner_id = uuid::Uuid::new_v4().to_string(),
                "epoch" => r.updater_epoch += 1,
                "process" => r.process.birth.push('1'),
                "directory" => r.state.directory_identity.push('x'),
                "journal" => r.state.journal_identity.push('x'),
                "lock" => r.state.lock_identity.push('x'),
                "lease" => r.lease.revision += 1,
                _ => unreachable!(),
            }
            assert!(intent.validate_receipt(&r).is_err(), "{field}");
        }
        let mut p = physical;
        p.state.journal_identity.push('x');
        assert!(p.validate(&intent).is_err());
        let mut raw = serde_json::to_value(receipt).unwrap();
        raw["processExited"] = true.into();
        assert!(serde_json::from_value::<WorkerShutdownReceipt>(raw).is_err());
    }
    #[test]
    fn projection_separates_unrelated_work_acceptance_historical_exit_and_live_custody() {
        let empty = view(None, true, false, true);
        assert_eq!(empty.phase, CandidateShutdownPhase::NotAdmitted);
        assert!(!empty.physical_custody_held);
        let (intent, receipt, physical) = fixture();
        let prepared = ShutdownRecord::prepare(intent).unwrap();
        assert_eq!(
            view(Some(&prepared), false, false, false).phase,
            CandidateShutdownPhase::Indeterminate
        );
        let ack = prepared
            .issue(101)
            .unwrap()
            .acknowledge(201, receipt)
            .unwrap();
        let waiting = view(Some(&ack), true, false, true);
        assert_eq!(waiting.phase, CandidateShutdownPhase::AwaitingExitAndLock);
        assert!(waiting.accepted_at_ms.is_some() && waiting.exited_at_ms.is_none());
        assert!(!waiting.physical_custody_held);
        let exited = ack.confirm_exit(301, physical).unwrap();
        let history = view(Some(&exited), false, false, false);
        assert_eq!(history.phase, CandidateShutdownPhase::Exited);
        assert!(history.exited_at_ms.is_some() && !history.physical_custody_held);
        assert!(view(Some(&exited), false, false, true).physical_custody_held);
    }
    #[test]
    fn frozen_binding_hash_and_quiesced_prerequisite_reject_retargeting_or_counter_overflow() {
        let (intent, _, _) = fixture();
        let mut changed = intent.clone();
        changed.binding.worker_state.lock_identity.push('x');
        assert!(changed.validate().is_err());
        let mut b = intent.binding.clone();
        b.lease.phase = WorkerUpdatePhase::SourceHeld;
        assert!(ShutdownIntent::from_quiesced(b, intent.attempt_id.clone()).is_err());
        let mut b = intent.binding.clone();
        b.parent_revision = MAX_UPDATE_COUNTER - 3;
        assert!(ShutdownIntent::from_quiesced(b, intent.attempt_id.clone()).is_err());
        let mut b = intent.binding.clone();
        b.parent_revision = MAX_UPDATE_COUNTER - 4;
        ShutdownIntent::from_quiesced(b, intent.attempt_id).unwrap();
    }
}
