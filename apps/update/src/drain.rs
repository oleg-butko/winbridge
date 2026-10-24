//! Drain delivery data and its trusted, separately admitted blocking driver.
//! Pure transitions alone grant no RPC, process or file execution authority.
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, validate_update_key, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::*;
mod driver;
pub(crate) use driver::{execute, Control, Telemetry};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainBinding {
    pub update_id: String,
    pub launch_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    /// Root revision BEFORE the atomic BeginDrain admission edge.
    pub parent_revision: u64,
    pub accepted_at_ms: u64,
    pub source_sha256: String,
    pub health_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainIntent {
    pub schema_version: u16,
    pub attempt_id: String,
    pub binding: DrainBinding,
    pub worker_spec_sha256: String,
    pub worker_revision: u64,
    pub commands: [WorkerUpdateCommand; 3],
}

const ACTIONS: [WorkerUpdateAction; 3] = [
    WorkerUpdateAction::BeginDrain,
    WorkerUpdateAction::CloseDispatch,
    WorkerUpdateAction::ConfirmQuiesced,
];

fn evidence_digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut bytes = domain.to_vec();
    bytes.extend(winbridge_protocol::canonical_evidence_json(&value).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&bytes))
}
pub(crate) fn spec_digest(value: &WorkerUpdateSpec) -> Result<String> {
    evidence_digest(b"winbridge.drain.worker-spec.v1\n", value)
}
pub(crate) fn source_digest(value: &SourceTransactionRecord) -> Result<String> {
    evidence_digest(b"winbridge.drain.source.v1\n", value)
}
pub(crate) fn health_digest(value: &crate::health::HealthRecord) -> Result<String> {
    evidence_digest(b"winbridge.drain.health.v1\n", value)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainSnapshot {
    pub intent: Option<DrainIntent>,
    pub commands: Vec<DrainCommandRecord>,
    pub quiesced_revision: Option<u64>,
}
/// Exists only after fresh durable admission; never reconstructed from JSON.
pub struct DrainPermit {
    pub(crate) intent: DrainIntent,
    pub(crate) production: Option<crate::compatible_drain::CompatibleDrainRecord>,
}
pub enum DrainAdmission {
    Fresh(Box<DrainPermit>),
    Observed(Box<DrainSnapshot>),
}
pub struct DrainDeliveryPermit {
    pub(crate) record: DrainCommandRecord,
}
impl DrainDeliveryPermit {
    pub fn into_record(self) -> DrainCommandRecord {
        self.record
    }
}
pub enum DrainIssue {
    Fresh(Box<DrainDeliveryPermit>),
    Observed(Box<DrainCommandRecord>),
}

pub(crate) fn view(
    saved: &DrainSnapshot,
    root: UpdatePhase,
    active: bool,
    released_after_abort: bool,
) -> CandidateDrainView {
    let confirmed = saved
        .commands
        .iter()
        .filter(|r| matches!(r.delivery, DrainDeliveryState::Confirmed { .. }))
        .count() as u8;
    let phase = if saved.intent.is_none() {
        CandidateDrainPhase::NotAdmitted
    } else if matches!(
        root,
        UpdatePhase::Aborting | UpdatePhase::FailedBeforeCutover | UpdatePhase::Deferred
    ) {
        if root == UpdatePhase::FailedBeforeCutover && released_after_abort {
            CandidateDrainPhase::Aborted
        } else {
            // Root Aborting/Deferred alone proves neither callback retirement
            // nor owned lease release. Keep the unresolved outcome explicit.
            CandidateDrainPhase::Indeterminate
        }
    } else if saved.quiesced_revision.is_some() {
        CandidateDrainPhase::Quiesced
    } else if !active
        || saved
            .commands
            .iter()
            .any(|r| matches!(r.delivery, DrainDeliveryState::Uncertain { .. }))
    {
        CandidateDrainPhase::Indeterminate
    } else if matches!(
        saved.commands.get(2).map(|r| &r.delivery),
        Some(DrainDeliveryState::Issued { .. })
    ) {
        CandidateDrainPhase::ConfirmingQuiescence
    } else if matches!(
        saved.commands.get(1).map(|r| &r.delivery),
        Some(DrainDeliveryState::Issued { .. })
    ) || confirmed >= 2
    {
        CandidateDrainPhase::ClosingDispatch
    } else if confirmed == 1 {
        CandidateDrainPhase::WaitingAccepted
    } else {
        CandidateDrainPhase::Starting
    };
    CandidateDrainView {
        phase,
        attempt_id: saved.intent.as_ref().map(|i| i.attempt_id.clone()),
        confirmed_commands: confirmed,
        quiesced_revision: saved.quiesced_revision,
        worker_observation: None,
    }
}

impl DrainIntent {
    /// Input must later come from trusted journal/worker observations. Merely
    /// constructing this value cannot authorize BeginDrain or a worker command.
    pub fn from_lease(
        binding: DrainBinding,
        lease: &WorkerUpdateLease,
        attempt_id: String,
    ) -> Result<Self> {
        if lease.phase != WorkerUpdatePhase::SourceHeld
            || lease.spec.update_id != binding.update_id
            || lease.spec.updater_owner_id != binding.owner_id
            || lease.spec.updater_epoch != binding.owner_epoch
        {
            return Err("Drain needs its matching held source lease".into());
        }
        let reasons = [
            "Не принимать новые задачи; завершить принятые",
            "Закрыть запуск новых операций после завершения очереди",
            "Подтвердить завершение владельцев и физических процессов",
        ];
        let commands = std::array::from_fn(|i| WorkerUpdateCommand {
            command_id: format!("drain-{attempt_id}-{i}"),
            expected_revision: lease.revision.saturating_add(i as u64),
            action: ACTIONS[i],
            source_manifest_sha256: None,
            reason: reasons[i].into(),
        });
        let intent = Self {
            schema_version: 1,
            attempt_id,
            binding,
            worker_spec_sha256: spec_digest(&lease.spec)?,
            worker_revision: lease.revision,
            commands,
        };
        intent.validate()?;
        Ok(intent)
    }
    pub fn validate(&self) -> Result<()> {
        for id in [
            &self.attempt_id,
            &self.binding.update_id,
            &self.binding.launch_id,
            &self.binding.owner_id,
        ] {
            uuid::Uuid::parse_str(id).map_err(|_| "Invalid drain identity")?;
        }
        if self.schema_version != 1
            || self.binding.owner_epoch == 0
            || self.binding.owner_epoch > MAX_UPDATE_COUNTER
            || self.binding.parent_revision > MAX_UPDATE_COUNTER - 2
            || self.binding.accepted_at_ms == 0
            || self.binding.accepted_at_ms > MAX_UPDATE_COUNTER
            || self.worker_revision == 0
            // Three commands plus a final owned Release must fit.
            || self.worker_revision > MAX_UPDATE_COUNTER - 4
            || [
                &self.worker_spec_sha256,
                &self.binding.source_sha256,
                &self.binding.health_sha256,
            ]
            .into_iter()
            .any(|s| !valid_hash(s, &[64]))
        {
            return Err("Invalid drain binding/counters/digests".into());
        }
        for (i, command) in self.commands.iter().enumerate() {
            validate_update_key(&command.command_id)?;
            if command.command_id != format!("drain-{}-{i}", self.attempt_id)
                || command.action != ACTIONS[i]
                || command.expected_revision != self.worker_revision + i as u64
                || command.source_manifest_sha256.is_some()
                || command.reason.is_empty()
                || command.reason.len() > 4096
                || command
                    .reason
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                return Err("Drain command sequence/key/payload inconsistent".into());
            }
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
pub enum DrainDeliveryState {
    Prepared,
    Issued {
        at_ms: u64,
    },
    Uncertain {
        at_ms: u64,
        reason: String,
    },
    Confirmed {
        at_ms: u64,
        receipt: Box<WorkerUpdateLease>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainCommandRecord {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub ordinal: u8,
    pub revision: u64,
    pub command: WorkerUpdateCommand,
    pub delivery: DrainDeliveryState,
}
impl DrainCommandRecord {
    pub fn prepare(intent: &DrainIntent, ordinal: u8) -> Result<Self> {
        intent.validate()?;
        let command = intent
            .commands
            .get(ordinal as usize)
            .ok_or("Invalid drain ordinal")?
            .clone();
        Ok(Self {
            schema_version: 1,
            update_id: intent.binding.update_id.clone(),
            attempt_id: intent.attempt_id.clone(),
            ordinal,
            revision: 0,
            command,
            delivery: DrainDeliveryState::Prepared,
        })
    }
    pub fn validate(&self, intent: &DrainIntent) -> Result<()> {
        intent.validate()?;
        if self.schema_version != 1
            || self.update_id != intent.binding.update_id
            || self.attempt_id != intent.attempt_id
            || intent.commands.get(self.ordinal as usize) != Some(&self.command)
            || self.revision > MAX_UPDATE_COUNTER
        {
            return Err("Drain command record binding inconsistent".into());
        }
        let expected_revision = match &self.delivery {
            DrainDeliveryState::Prepared => 0,
            DrainDeliveryState::Issued { at_ms } => {
                time(*at_ms)?;
                1
            }
            DrainDeliveryState::Uncertain { at_ms, reason } => {
                time(*at_ms)?;
                if reason.is_empty() || reason.len() > 4096 {
                    return Err("Invalid uncertain drain reason".into());
                }
                2
            }
            DrainDeliveryState::Confirmed { at_ms, receipt } => {
                time(*at_ms)?;
                validate_receipt(intent, self.ordinal, receipt)?;
                // A receipt may follow Issued directly or readonly reconciliation
                // of Uncertain. Sequence, not wall-clock ordering, fences writes.
                if self.revision != 2 && self.revision != 3 {
                    return Err("Invalid confirmed drain revision".into());
                }
                self.revision
            }
        };
        if self.revision != expected_revision {
            return Err("Drain delivery/revision inconsistent".into());
        }
        Ok(())
    }
    /// The future journal must commit this transition before returning a fresh,
    /// one-use in-memory delivery permit. Reading Issued cannot recreate it.
    pub fn issue(&self, intent: &DrainIntent, at_ms: u64) -> Result<Self> {
        self.validate(intent)?;
        time(at_ms)?;
        if self.delivery != DrainDeliveryState::Prepared {
            return Err("Drain delivery already issued; observe only".into());
        }
        Ok(Self {
            revision: 1,
            delivery: DrainDeliveryState::Issued { at_ms },
            ..self.clone()
        })
    }
    pub fn uncertain(&self, intent: &DrainIntent, at_ms: u64, reason: String) -> Result<Self> {
        self.validate(intent)?;
        time(at_ms)?;
        if !matches!(self.delivery, DrainDeliveryState::Issued { .. }) {
            return Err("Only issued drain delivery can become uncertain".into());
        }
        let next = Self {
            revision: 2,
            delivery: DrainDeliveryState::Uncertain { at_ms, reason },
            ..self.clone()
        };
        next.validate(intent)?;
        Ok(next)
    }
    pub fn confirm(
        &self,
        intent: &DrainIntent,
        at_ms: u64,
        receipt: WorkerUpdateLease,
    ) -> Result<Self> {
        self.validate(intent)?;
        time(at_ms)?;
        validate_receipt(intent, self.ordinal, &receipt)?;
        if let DrainDeliveryState::Confirmed { receipt: old, .. } = &self.delivery {
            return if old.as_ref() == &receipt {
                Ok(self.clone())
            } else {
                Err("Immutable drain receipt changed".into())
            };
        }
        if !matches!(
            self.delivery,
            DrainDeliveryState::Issued { .. } | DrainDeliveryState::Uncertain { .. }
        ) {
            return Err("Receipt without a committed delivery intent".into());
        }
        let next = Self {
            revision: self.revision + 1,
            delivery: DrainDeliveryState::Confirmed {
                at_ms,
                receipt: Box::new(receipt),
            },
            ..self.clone()
        };
        next.validate(intent)?;
        Ok(next)
    }
}
fn time(n: u64) -> Result<()> {
    if n == 0 || n > MAX_UPDATE_COUNTER {
        Err("Invalid drain observation time".into())
    } else {
        Ok(())
    }
}
fn validate_receipt(intent: &DrainIntent, ordinal: u8, receipt: &WorkerUpdateLease) -> Result<()> {
    let command = intent
        .commands
        .get(ordinal as usize)
        .ok_or("Invalid drain receipt ordinal")?;
    let phase = [
        WorkerUpdatePhase::Draining,
        WorkerUpdatePhase::Quiescing,
        WorkerUpdatePhase::Quiesced,
    ][ordinal as usize];
    if spec_digest(&receipt.spec)? != intent.worker_spec_sha256
        || receipt.spec.update_id != intent.binding.update_id
        || receipt.spec.updater_owner_id != intent.binding.owner_id
        || receipt.spec.updater_epoch != intent.binding.owner_epoch
        || receipt.revision != command.expected_revision + 1
        || receipt.phase != phase
        || receipt.reason != command.reason
    {
        return Err("Drain receipt spec/owner/epoch/phase/revision/payload differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_intent_and_old_quiescence_never_imply_completed_release() {
        let (intent, lease) = fixture();
        let commands = (0..3)
            .map(|n| {
                DrainCommandRecord::prepare(&intent, n)
                    .unwrap()
                    .issue(&intent, 101)
                    .unwrap()
                    .confirm(&intent, 102, receipt(&intent, &lease, n as usize))
                    .unwrap()
            })
            .collect();
        let saved = DrainSnapshot {
            intent: Some(intent),
            commands,
            quiesced_revision: Some(12),
        };
        for phase in [
            UpdatePhase::Aborting,
            UpdatePhase::Deferred,
            UpdatePhase::FailedBeforeCutover,
        ] {
            assert_eq!(
                view(&saved, phase, true, false).phase,
                CandidateDrainPhase::Indeterminate
            );
        }
        assert_eq!(
            view(&saved, UpdatePhase::Aborting, true, true).phase,
            CandidateDrainPhase::Indeterminate
        );
        assert_eq!(
            view(&saved, UpdatePhase::FailedBeforeCutover, false, true).phase,
            CandidateDrainPhase::Aborted
        );
    }
    pub(super) fn fixture() -> (DrainIntent, WorkerUpdateLease) {
        let binding = DrainBinding {
            update_id: uuid::Uuid::new_v4().to_string(),
            launch_id: uuid::Uuid::new_v4().to_string(),
            owner_id: uuid::Uuid::new_v4().to_string(),
            owner_epoch: 2,
            parent_revision: 10,
            accepted_at_ms: 100,
            source_sha256: "a".repeat(64),
            health_sha256: "b".repeat(64),
        };
        let lease = WorkerUpdateLease {
            spec: WorkerUpdateSpec {
                update_id: binding.update_id.clone(),
                instance_id: "fixture".into(),
                journal_incarnation: uuid::Uuid::new_v4().to_string(),
                worker_generation: uuid::Uuid::new_v4().to_string(),
                updater_owner_id: binding.owner_id.clone(),
                updater_epoch: binding.owner_epoch,
                source_directory: "synthetic-local-source".into(),
                baseline_manifest_sha256: "c".repeat(64),
                target_manifest_sha256: "d".repeat(64),
                projects: vec!["fixture".into()],
            },
            revision: 1,
            phase: WorkerUpdatePhase::SourceHeld,
            accepted_at_ms: 1,
            updated_at_ms: 2,
            reason: "Held".into(),
        };
        (
            DrainIntent::from_lease(binding, &lease, uuid::Uuid::new_v4().to_string()).unwrap(),
            lease,
        )
    }
    pub(super) fn receipt(
        intent: &DrainIntent,
        before: &WorkerUpdateLease,
        ordinal: usize,
    ) -> WorkerUpdateLease {
        WorkerUpdateLease {
            revision: intent.commands[ordinal].expected_revision + 1,
            phase: [
                WorkerUpdatePhase::Draining,
                WorkerUpdatePhase::Quiescing,
                WorkerUpdatePhase::Quiesced,
            ][ordinal],
            updated_at_ms: 3,
            reason: intent.commands[ordinal].reason.clone(),
            ..before.clone()
        }
    }
    #[test]
    fn command_plan_freezes_keys_scope_sequence_and_never_accepts_overflow_or_release() {
        let (intent, lease) = fixture();
        intent.validate().unwrap();
        assert_eq!(
            intent.commands.each_ref().map(|c| c.expected_revision),
            [1, 2, 3]
        );
        assert_ne!(intent.commands[0].command_id, intent.commands[1].command_id);
        let mut wrong = intent.clone();
        wrong.commands[1].action = WorkerUpdateAction::Release;
        assert!(wrong.validate().is_err());
        let mut wrong = intent.clone();
        wrong.commands[0].source_manifest_sha256 = Some("e".repeat(64));
        assert!(wrong.validate().is_err());
        let mut wrong = lease.clone();
        wrong.revision = MAX_UPDATE_COUNTER - 1;
        assert!(
            DrainIntent::from_lease(intent.binding.clone(), &wrong, intent.attempt_id.clone())
                .is_err()
        );
        let mut last = lease.clone();
        last.revision = MAX_UPDATE_COUNTER - 4;
        DrainIntent::from_lease(intent.binding.clone(), &last, intent.attempt_id.clone()).unwrap();
        last.revision += 1;
        assert!(
            DrainIntent::from_lease(intent.binding.clone(), &last, intent.attempt_id.clone())
                .is_err()
        );
        let mut wrong = lease;
        wrong.spec.updater_epoch += 1;
        assert!(DrainIntent::from_lease(intent.binding, &wrong, intent.attempt_id).is_err());
    }
    #[test]
    fn persisted_issued_or_uncertain_state_never_recreates_delivery_and_reconciliation_is_idempotent(
    ) {
        let (intent, before) = fixture();
        let record = DrainCommandRecord::prepare(&intent, 0).unwrap();
        let ack = receipt(&intent, &before, 0);
        assert!(record.confirm(&intent, 5, ack.clone()).is_err());
        let issued = record.issue(&intent, 10).unwrap();
        let reopened: DrainCommandRecord =
            serde_json::from_slice(&serde_json::to_vec(&issued).unwrap()).unwrap();
        assert!(reopened.issue(&intent, 11).is_err());
        let unknown = reopened.uncertain(&intent, 12, "ACK lost".into()).unwrap();
        assert!(unknown.issue(&intent, 13).is_err());
        assert!(unknown.uncertain(&intent, 13, "No replay".into()).is_err());
        // Wall clock may move backwards; the persisted revision defines order.
        let done = unknown.confirm(&intent, 1, ack.clone()).unwrap();
        assert_eq!(done.revision, 3);
        assert_eq!(done.confirm(&intent, 999, ack).unwrap(), done);
        assert!(done.issue(&intent, 999).is_err());
        assert!(done.uncertain(&intent, 999, "Late error".into()).is_err());
    }
    #[test]
    fn typed_receipts_cannot_be_rebound_to_other_command_owner_phase_or_worker_generation() {
        let (intent, before) = fixture();
        for ordinal in 0..3 {
            let issued = DrainCommandRecord::prepare(&intent, ordinal)
                .unwrap()
                .issue(&intent, 10)
                .unwrap();
            let ack = receipt(&intent, &before, ordinal as usize);
            issued.confirm(&intent, 11, ack.clone()).unwrap();
            for field in [
                "owner",
                "epoch",
                "generation",
                "phase",
                "revision",
                "reason",
            ] {
                let mut wrong = ack.clone();
                match field {
                    "owner" => wrong.spec.updater_owner_id = uuid::Uuid::new_v4().to_string(),
                    "epoch" => wrong.spec.updater_epoch += 1,
                    "generation" => wrong.spec.worker_generation = uuid::Uuid::new_v4().to_string(),
                    "phase" => wrong.phase = WorkerUpdatePhase::Released,
                    "revision" => wrong.revision += 1,
                    "reason" => wrong.reason = "Another command".into(),
                    _ => unreachable!(),
                }
                assert!(issued.confirm(&intent, 11, wrong).is_err(), "{field}");
            }
            let mut wrong = issued;
            wrong.command.command_id = "different".into();
            assert!(wrong.validate(&intent).is_err());
        }
        assert!(DrainCommandRecord::prepare(&intent, 255).is_err());
        let mut value = serde_json::to_value(&intent).unwrap();
        value["executionPermit"] = true.into();
        assert!(serde_json::from_value::<DrainIntent>(value).is_err());
    }
}
