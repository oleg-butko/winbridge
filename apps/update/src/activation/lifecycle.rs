//! Startup data FSM. These proposals are not permits or process/state ownership.
use super::{ActivationSource, Result};
use crate::{artifact::ImageEvidence, runtime_replacement::RuntimeReplacementRecord};
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::{
    update::{SupervisorProcess, WorkerShutdownState},
    LoadedWorkerIdentity, RequestContext, WorkerRuntimeCompatibility,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationContext {
    pub supervisor: SupervisorProcess,
    pub previous_worker: SupervisorProcess,
    pub previous_generation: String,
    pub worker_context: RequestContext,
    pub worker_state: WorkerShutdownState,
    pub worker_config: ImageEvidence,
    pub worker_config_schema: u16,
    pub worker_journal_schema: u32,
    pub updater_journal_schema: u32,
    pub protocol_version: u16,
    pub startup_timeout_ms: u64,
    /// Only a digest is retained; the nonce belongs to the future private channel.
    pub channel_nonce_sha256: String,
}
fn id(value: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value)
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some(value)
    {
        return Err("Activation requires canonical UUIDs".into());
    }
    Ok(())
}
fn stamp(p: &SupervisorProcess) -> Result<()> {
    if p.process_id == 0
        || p.birth
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .map(|n| n.to_string())
            .as_deref()
            != Some(p.birth.as_str())
        || !valid_hash(&p.host_id, &[64])
    {
        return Err("Activation process stamp is incomplete".into());
    }
    Ok(())
}
fn path(value: &str) -> Result<()> {
    let p = std::path::Path::new(value);
    if value.is_empty()
        || value.len() > 4096
        || value.chars().any(char::is_control)
        || !p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err("Activation path must be absolute and normalized".into());
    }
    winbridge_platform::require_local_path_prefix(p).map_err(|e| e.to_string())
}
impl ActivationContext {
    pub fn validate(&self) -> Result<()> {
        stamp(&self.supervisor)?;
        stamp(&self.previous_worker)?;
        id(&self.previous_generation)?;
        id(&self.worker_context.journal_incarnation)?;
        self.worker_state.validate()?;
        path(&self.worker_state.state_directory)?;
        path(&self.worker_config.path)?;
        let c = &self.worker_config;
        if self.supervisor.process_id == self.previous_worker.process_id
            || self.supervisor.host_id != self.previous_worker.host_id
            || self.supervisor.session_id != self.previous_worker.session_id
            || !winbridge_core::config::valid_id(&self.worker_context.instance_id)
            || c.query_supported
            || c.bytes == 0
            || c.bytes > 65536
            || c.file_identity.is_empty()
            || c.file_identity.len() > 128
            || c.file_identity.chars().any(char::is_control)
            || [
                &self.worker_state.directory_identity,
                &self.worker_state.journal_identity,
                &self.worker_state.lock_identity,
            ]
            .contains(&&c.file_identity)
            || !valid_hash(&c.sha256, &[64])
            || self.worker_config_schema == 0
            || self.worker_journal_schema == 0
            || self.updater_journal_schema == 0
            || self.protocol_version == 0
            || !(1000..=300000).contains(&self.startup_timeout_ms)
            || !valid_hash(&self.channel_nonce_sha256, &[64])
        {
            return Err("Activation context/config/schema/deadline rejected".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationIntent {
    pub schema_version: u16,
    pub attempt_id: String,
    pub admitted_at_ms: u64,
    pub source: ActivationSource,
    pub context: ActivationContext,
}
impl ActivationIntent {
    pub fn validate(&self, replacement: &RuntimeReplacementRecord) -> Result<()> {
        self.source.validate(replacement)?;
        self.context.validate()?;
        id(&self.attempt_id)?;
        if self.schema_version != 1
            || self.admitted_at_ms < replacement.intent.admitted_at_ms
            || self.admitted_at_ms > MAX_UPDATE_COUNTER - self.context.startup_timeout_ms
            || [
                &self.source.update_id,
                &self.source.replacement_attempt_id,
                &self.source.owner_id,
                &self.source.launch_id,
                &self.source.set_id,
                &self.context.previous_generation,
                &self.context.worker_context.journal_incarnation,
            ]
            .contains(&&self.attempt_id)
        {
            return Err("Activation intent identity/time differs from its source".into());
        }
        Ok(())
    }
    pub fn job_name(&self) -> Result<String> {
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let mut bytes = b"winbridge.activation.job.v1\n".to_vec();
        bytes.extend(
            winbridge_protocol::canonical_evidence_json(&value).map_err(|e| e.to_string())?,
        );
        Ok(format!(
            "Global\\WinBridge-Activation-{}",
            crate::sha256(&bytes)
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationCreated {
    pub process: SupervisorProcess,
    pub job_name: String,
    pub observed_at_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReady {
    pub attempt_id: String,
    pub identity: LoadedWorkerIdentity,
    pub worker_config_sha256: String,
    pub observed_at_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationStateOwned {
    pub attempt_id: String,
    pub process: SupervisorProcess,
    pub context: RequestContext,
    pub generation: String,
    pub state: WorkerShutdownState,
    pub observed_at_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivationState {
    Prepared {},
    Creating {},
    Created {},
    ResumeGranted {},
    ReadyForState {},
    HandoffIssued {},
    StateOwned {},
    LocalVerified {},
    /// Only a trusted creation observer may persist this no-process outcome.
    CreationFailed {
        reason: String,
    },
    Indeterminate {
        previous: Box<ActivationState>,
        reason: String,
    },
}
fn rank(s: &ActivationState) -> Result<u64> {
    Ok(match s {
        ActivationState::Prepared {} => 0,
        ActivationState::Creating {} => 1,
        ActivationState::Created {} => 2,
        ActivationState::ResumeGranted {} => 3,
        ActivationState::ReadyForState {} => 4,
        ActivationState::HandoffIssued {} => 5,
        ActivationState::StateOwned {} => 6,
        ActivationState::LocalVerified {} => 7,
        ActivationState::CreationFailed { .. } => 2,
        ActivationState::Indeterminate { previous, .. } => {
            if terminal(previous) {
                return Err("Nested/terminal activation uncertainty".into());
            }
            rank(previous)? + 1
        }
    })
}
fn terminal(s: &ActivationState) -> bool {
    matches!(
        s,
        ActivationState::LocalVerified {}
            | ActivationState::CreationFailed { .. }
            | ActivationState::Indeterminate { .. }
    )
}
fn reason(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 1000 || s.chars().any(char::is_control) {
        Err("Activation failure needs a bounded readable reason".into())
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationRecord {
    pub schema_version: u16,
    pub intent: ActivationIntent,
    pub revision: u64,
    pub updated_at_ms: u64,
    pub state: ActivationState,
    pub create_issued_at_ms: Option<u64>,
    pub created: Option<ActivationCreated>,
    pub resume_granted_at_ms: Option<u64>,
    pub ready: Option<ActivationReady>,
    pub handoff_issued_at_ms: Option<u64>,
    pub state_owned: Option<ActivationStateOwned>,
    pub local: Option<WorkerRuntimeCompatibility>,
}
impl ActivationRecord {
    pub fn prepared(intent: ActivationIntent, r: &RuntimeReplacementRecord) -> Result<Self> {
        let record = Self {
            schema_version: 1,
            updated_at_ms: intent.admitted_at_ms,
            intent,
            revision: 0,
            state: ActivationState::Prepared {},
            create_issued_at_ms: None,
            created: None,
            resume_granted_at_ms: None,
            ready: None,
            handoff_issued_at_ms: None,
            state_owned: None,
            local: None,
        };
        record.validate(r)?;
        Ok(record)
    }
    pub fn validate(&self, r: &RuntimeReplacementRecord) -> Result<()> {
        self.intent.validate(r)?;
        let base = match &self.state {
            ActivationState::Indeterminate {
                previous,
                reason: text,
            } => {
                reason(text)?;
                previous.as_ref()
            }
            ActivationState::CreationFailed { reason: text } => {
                reason(text)?;
                &ActivationState::Creating {}
            }
            state => state,
        };
        let n = rank(base)?;
        if self.schema_version != 1
            || self.revision != rank(&self.state)?
            || self.updated_at_ms < self.intent.admitted_at_ms
            || self.updated_at_ms > MAX_UPDATE_COUNTER
            || [
                self.create_issued_at_ms.is_some(),
                self.created.is_some(),
                self.resume_granted_at_ms.is_some(),
                self.ready.is_some(),
                self.handoff_issued_at_ms.is_some(),
                self.state_owned.is_some(),
                self.local.is_some(),
            ] != [n >= 1, n >= 2, n >= 3, n >= 4, n >= 5, n >= 6, n >= 7]
        {
            return Err("Activation progress skipped/rewrote a causal receipt".into());
        }
        let mut at = self.intent.admitted_at_ms;
        let deadline = at + self.intent.context.startup_timeout_ms;
        let grant = |value| -> Result<()> {
            if value > deadline {
                Err("Activation deadline expired; no new external grant".into())
            } else {
                Ok(())
            }
        };
        let mut time = |observed: u64| -> Result<()> {
            if observed < at || observed > self.updated_at_ms {
                return Err("Activation receipt time is not causal".into());
            }
            at = observed;
            Ok(())
        };
        if let Some(t) = self.create_issued_at_ms {
            grant(t)?;
            time(t)?;
        }
        let c = &self.intent.context;
        let source = &self.intent.source;
        if let Some(created) = &self.created {
            time(created.observed_at_ms)?;
            stamp(&created.process)?;
            if created.process.process_id == c.supervisor.process_id
                || created.process.process_id == c.previous_worker.process_id
                || created.process.host_id != c.supervisor.host_id
                || created.process.session_id != c.supervisor.session_id
                || created.job_name != self.intent.job_name()?
            {
                return Err("Activation created a foreign process/Job".into());
            }
        }
        if let Some(t) = self.resume_granted_at_ms {
            grant(t)?;
            time(t)?;
        }
        if let Some(ready) = &self.ready {
            time(ready.observed_at_ms)?;
            let identity = &ready.identity;
            if ready.attempt_id != self.intent.attempt_id
                || ready.worker_config_sha256 != c.worker_config.sha256
                || identity.schema_version != 1
                || identity.kind != "loaded_worker_identity"
                || Some(&identity.process) != self.created.as_ref().map(|v| &v.process)
                || identity.runtime_identity != source.runtime_identity
                || identity.set_id != source.set_id
                || identity.manifest_sha256 != source.manifest.sha256
                || identity.image != source.worker
                || identity.descriptor != source.worker_descriptor
            {
                return Err("Activation Ready is not the created loaded generation/config".into());
            }
        }
        if let Some(t) = self.handoff_issued_at_ms {
            grant(t)?;
            time(t)?;
        }
        if let Some(owned) = &self.state_owned {
            time(owned.observed_at_ms)?;
            id(&owned.generation)?;
            if owned.attempt_id != self.intent.attempt_id
                || owned.context != c.worker_context
                || owned.state != c.worker_state
                || owned.generation == c.previous_generation
                || Some(&owned.process) != self.created.as_ref().map(|v| &v.process)
            {
                return Err("Activation state handoff changed process/journal/lock scope".into());
            }
        }
        if let Some(local) = &self.local {
            time(local.observed_at_ms)?;
            if local.schema_version != 1
                || local.kind != "worker_runtime_compatibility"
                || Some(&local.identity) != self.ready.as_ref().map(|v| &v.identity)
                || local.context != c.worker_context
                || Some(&local.generation) != self.state_owned.as_ref().map(|v| &v.generation)
                || local.worker_config_sha256 != c.worker_config.sha256
                || local.worker_config_schema != c.worker_config_schema
                || local.worker_journal_schema != c.worker_journal_schema
                || local.updater_journal_schema != c.updater_journal_schema
                || local.protocol_version != c.protocol_version
            {
                return Err(
                    "Activation local health changed frozen schemas/process/generation".into(),
                );
            }
        }
        Ok(())
    }
    fn next(&self, mut next: Self, at: u64, r: &RuntimeReplacementRecord) -> Result<Self> {
        self.validate(r)?;
        if terminal(&self.state) || at < self.updated_at_ms {
            return Err("Activation terminal/stale; observe, never replay".into());
        }
        next.revision = self.revision + 1;
        next.updated_at_ms = at;
        next.validate(r)?;
        Ok(next)
    }
    pub fn issue_create(&self, at: u64, r: &RuntimeReplacementRecord) -> Result<Self> {
        if !matches!(self.state, ActivationState::Prepared {}) {
            return Err("Create already issued; never replay".into());
        }
        self.next(
            Self {
                state: ActivationState::Creating {},
                create_issued_at_ms: Some(at),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn created(
        &self,
        v: ActivationCreated,
        at: u64,
        r: &RuntimeReplacementRecord,
    ) -> Result<Self> {
        if !matches!(self.state, ActivationState::Creating {}) {
            return Err("Created needs saved creation intent".into());
        }
        self.next(
            Self {
                state: ActivationState::Created {},
                created: Some(v),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn grant_resume(&self, at: u64, r: &RuntimeReplacementRecord) -> Result<Self> {
        if !matches!(self.state, ActivationState::Created {}) {
            return Err("Resume requires actual created PID first".into());
        }
        self.next(
            Self {
                state: ActivationState::ResumeGranted {},
                resume_granted_at_ms: Some(at),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn ready(&self, v: ActivationReady, at: u64, r: &RuntimeReplacementRecord) -> Result<Self> {
        if !matches!(self.state, ActivationState::ResumeGranted {}) {
            return Err("Ready preceded resume grant".into());
        }
        self.next(
            Self {
                state: ActivationState::ReadyForState {},
                ready: Some(v),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn issue_handoff(&self, at: u64, r: &RuntimeReplacementRecord) -> Result<Self> {
        if !matches!(self.state, ActivationState::ReadyForState {}) {
            return Err("State release requires bound Ready".into());
        }
        self.next(
            Self {
                state: ActivationState::HandoffIssued {},
                handoff_issued_at_ms: Some(at),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn state_owned(
        &self,
        v: ActivationStateOwned,
        at: u64,
        r: &RuntimeReplacementRecord,
    ) -> Result<Self> {
        if !matches!(self.state, ActivationState::HandoffIssued {}) {
            return Err("State ownership needs saved handoff intent".into());
        }
        self.next(
            Self {
                state: ActivationState::StateOwned {},
                state_owned: Some(v),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn local_verified(
        &self,
        v: WorkerRuntimeCompatibility,
        at: u64,
        r: &RuntimeReplacementRecord,
    ) -> Result<Self> {
        if !matches!(self.state, ActivationState::StateOwned {}) {
            return Err("Local health preceded state ownership".into());
        }
        self.next(
            Self {
                state: ActivationState::LocalVerified {},
                local: Some(v),
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn creation_failed(
        &self,
        text: String,
        at: u64,
        r: &RuntimeReplacementRecord,
    ) -> Result<Self> {
        if !matches!(self.state, ActivationState::Creating {}) {
            return Err("No-process receipt conflicts with created PID".into());
        }
        self.next(
            Self {
                state: ActivationState::CreationFailed { reason: text },
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn indeterminate(
        &self,
        text: String,
        at: u64,
        r: &RuntimeReplacementRecord,
    ) -> Result<Self> {
        self.next(
            Self {
                state: ActivationState::Indeterminate {
                    previous: Box::new(self.state.clone()),
                    reason: text,
                },
                ..self.clone()
            },
            at,
            r,
        )
    }
    pub fn advance(&self, next: &Self, r: &RuntimeReplacementRecord) -> Result<()> {
        let at = next.updated_at_ms;
        let expected = match &next.state {
            ActivationState::Creating {} => self.issue_create(at, r),
            ActivationState::Created {} => {
                self.created(next.created.clone().ok_or("PID missing")?, at, r)
            }
            ActivationState::ResumeGranted {} => self.grant_resume(at, r),
            ActivationState::ReadyForState {} => {
                self.ready(next.ready.clone().ok_or("Ready missing")?, at, r)
            }
            ActivationState::HandoffIssued {} => self.issue_handoff(at, r),
            ActivationState::StateOwned {} => self.state_owned(
                next.state_owned.clone().ok_or("State receipt missing")?,
                at,
                r,
            ),
            ActivationState::LocalVerified {} => {
                self.local_verified(next.local.clone().ok_or("Local health missing")?, at, r)
            }
            ActivationState::CreationFailed { reason } => {
                self.creation_failed(reason.clone(), at, r)
            }
            ActivationState::Indeterminate { previous, reason } if **previous == self.state => {
                self.indeterminate(reason.clone(), at, r)
            }
            _ => Err("Activation sequence cannot be replayed/rewound".into()),
        }?;
        if *next != expected {
            return Err("Activation transition changed frozen context/evidence".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
pub(super) mod tests;
