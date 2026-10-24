//! Durable production prerequisite decision. Data never reconstructs a drain permit.
use crate::candidate_support::{digest, CandidateSupportOutcome, CandidateSupportRecord};
use serde::{Deserialize, Serialize};
use winbridge_core::update::{valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::*;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompatibleDrainDecision {
    Blocked {
        outcome: CandidateSupportOutcome,
        assessment: Option<UpdaterCompatibilityAssessment>,
    },
    Admitted {
        drain_attempt_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibleDrainRecord {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub support_attempt_id: String,
    pub support_revision: u64,
    pub support_sha256: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub parent_revision: u64,
    pub decided_at_ms: u64,
    pub decision: CompatibleDrainDecision,
}
pub enum CompatibleDrainAdmission {
    Blocked(Box<CompatibleDrainRecord>),
    Fresh(Box<crate::drain::DrainPermit>),
    Observed(Box<CompatibleDrainRecord>),
}
pub(crate) fn support_digest(s: &CandidateSupportRecord) -> Result<String> {
    let value = serde_json::to_value(s).map_err(|e| e.to_string())?;
    let mut bytes = b"winbridge.compatible-drain.support.v1\n".to_vec();
    bytes.extend(winbridge_protocol::canonical_evidence_json(&value).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&bytes))
}
pub(crate) fn blocked(s: &CandidateSupportRecord) -> Result<Option<CompatibleDrainDecision>> {
    s.validate()?;
    let r = s
        .result
        .as_ref()
        .ok_or("Support is still pending; wait, never freeze a pending result")?;
    if r.outcome == CandidateSupportOutcome::Checked
        && r.assessment.as_ref().is_some_and(|a| a.blockers.is_empty())
    {
        Ok(None)
    } else {
        Ok(Some(CompatibleDrainDecision::Blocked {
            outcome: r.outcome,
            assessment: r.assessment.clone(),
        }))
    }
}
impl CompatibleDrainRecord {
    pub(crate) fn new(
        s: &CandidateSupportRecord,
        decision: CompatibleDrainDecision,
        at: u64,
    ) -> Result<Self> {
        let r = Self {
            schema_version: 1,
            update_id: s.intent.update_id.clone(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
            support_attempt_id: s.intent.attempt_id.clone(),
            support_revision: s.revision,
            support_sha256: support_digest(s)?,
            owner_id: s.intent.owner_id.clone(),
            owner_epoch: s.intent.owner_epoch,
            parent_revision: s.intent.parent_revision,
            decided_at_ms: at,
            decision,
        };
        r.validate(s)?;
        Ok(r)
    }
    pub fn validate(&self, s: &CandidateSupportRecord) -> Result<()> {
        s.validate()?;
        let result = s
            .result
            .as_ref()
            .ok_or("A pending query cannot decide production drain")?;
        for id in [
            &self.update_id,
            &self.attempt_id,
            &self.support_attempt_id,
            &self.owner_id,
        ] {
            uuid::Uuid::parse_str(id).map_err(|_| "Invalid compatible-drain identity")?;
        }
        if self.schema_version != 1
            || self.attempt_id == self.update_id
            || self.attempt_id == self.support_attempt_id
            || self.update_id != s.intent.update_id
            || self.support_attempt_id != s.intent.attempt_id
            || self.support_revision != s.revision
            || self.support_sha256 != support_digest(s)?
            || !valid_hash(&self.support_sha256, &[64])
            || self.owner_id != s.intent.owner_id
            || self.owner_epoch != s.intent.owner_epoch
            || self.parent_revision != s.intent.parent_revision
            || self.decided_at_ms < result.completed_at_ms
            || self.decided_at_ms > MAX_UPDATE_COUNTER
        {
            return Err(
                "Compatible-drain decision differs from its immutable support/owner/root".into(),
            );
        }
        match &self.decision {
            CompatibleDrainDecision::Blocked { .. }
                if blocked(s)?.as_ref() == Some(&self.decision) =>
            {
                Ok(())
            }
            CompatibleDrainDecision::Admitted { drain_attempt_id } if blocked(s)?.is_none() => {
                uuid::Uuid::parse_str(drain_attempt_id)
                    .map_err(|_| "Invalid linked drain attempt")?;
                if [
                    self.update_id.as_str(),
                    self.attempt_id.as_str(),
                    self.support_attempt_id.as_str(),
                ]
                .contains(&drain_attempt_id.as_str())
                {
                    return Err("Drain attempt reused another operation ID".into());
                }
                crate::candidate_support::quiescent(s)
            }
            _ => Err("Production drain decision invented eligibility or rewrote blockers".into()),
        }
    }
}

pub(crate) fn revalidate_executor(s: &SupervisorRecord, r: &CandidateSupportRecord) -> Result<()> {
    let current = crate::updater_support::current()?;
    if current.process != s.process.clone().ok_or("Supervisor process missing")?
        || current.descriptor != r.intent.executor
        || current.driver_contracts() != r.intent.drivers
        || r.intent.expected_updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
    {
        return Err(
            "Support executor/context/schema is historical; no fresh production admission".into(),
        );
    }
    Ok(())
}
/// Full preflight for a potentially allowed decision. Pins live through admission;
/// the consuming drain driver acquires and revalidates them again before any RPC.
pub(crate) fn preflight(
    j: &mut crate::journal::UpdateJournal,
    a: &crate::journal::UpdateAuthority,
    s: &SupervisorRecord,
    r: &CandidateSupportRecord,
    c: &mut impl crate::health::HealthControl,
) -> Result<Vec<crate::artifact::Pinned>> {
    revalidate_executor(s, r)?;
    crate::candidate_support::physically_quiescent(r)?;
    let cfg = crate::preparation::configuration(s)?;
    let source = crate::build::saved_source_guard_for(
        j,
        a,
        &cfg,
        &s.spec.update_id,
        UpdatePhase::CandidateVerified,
    )?;
    crate::source::live_lease(
        c,
        &j.get(&s.spec.update_id).map_err(|e| e.to_string())?,
        Some(&source),
    )?;
    if digest(&source)? != r.intent.source_sha256 {
        return Err("Support source changed".into());
    }
    crate::verify_source(
        &cfg.source_directory,
        &source.worker_source_spec.target_manifest_sha256,
    )?;
    let h = j
        .health(&s.spec.update_id)
        .map_err(|e| e.to_string())?
        .ok_or("Health missing")?;
    crate::health::validate_fresh_success(&h)?;
    crate::health::physically_quiescent(&h)?;
    if crate::drain::health_digest(&h)? != r.intent.health_sha256 {
        return Err("Support health changed".into());
    }
    let report = crate::journal::UpdateJournal::inspect_build_report(
        &cfg.runtime_directory.join("update-state"),
        &s.spec.update_id,
    )
    .map_err(|e| e.to_string())?;
    crate::abort::compiler_empty(&report.build)?;
    let (set, mut pins) = crate::candidate_artifacts::inspect_pinned(&cfg, &s.spec.update_id)?;
    let saved = j
        .candidate_artifacts(&s.spec.update_id)
        .map_err(|e| e.to_string())?;
    let config = crate::artifact::pin(std::path::Path::new(&r.intent.worker_config.path))?;
    let executor = crate::artifact::pin(std::path::Path::new(&s.spec.image_path))?;
    if set != saved.inspection
        || digest(&saved)? != r.intent.artifacts_sha256
        || config.evidence != r.intent.worker_config
        || executor.evidence.sha256 != r.intent.supervisor_image_sha256
        || executor.evidence.file_identity != s.spec.image_identity
    {
        return Err("Support artifacts/config/executor pins changed".into());
    }
    let live = c.compatibility()?;
    live.validate_binding(s.spec.worker.as_ref().ok_or("Worker binding missing")?)?;
    let mut observed = live;
    observed.observed_at_ms = h
        .context
        .as_ref()
        .ok_or("Health context missing")?
        .compatibility
        .observed_at_ms;
    if observed != h.context.as_ref().unwrap().compatibility {
        return Err("Worker changed after support".into());
    }
    pins.push(config);
    pins.push(executor);
    Ok(pins)
}

pub(crate) fn view(
    r: Option<&CompatibleDrainRecord>,
    support: Option<&CandidateSupportRecord>,
    drain: &CandidateDrainView,
) -> CompatibleDrainView {
    let Some(r) = r else {
        return CompatibleDrainView {
            phase: CompatibleDrainPhase::NotAdmitted,
            attempt_id: None,
            support_attempt_id: None,
            support_revision: None,
            drain_attempt_id: None,
            decided_at_ms: None,
            assessment: None,
            reason: None,
        };
    };
    let result = support.and_then(|s| s.result.as_ref());
    let (phase, drain_attempt_id) = match &r.decision {
        CompatibleDrainDecision::Blocked { .. } => (CompatibleDrainPhase::Blocked, None),
        CompatibleDrainDecision::Admitted { drain_attempt_id } => {
            let phase = match drain.phase {
                CandidateDrainPhase::Quiesced => CompatibleDrainPhase::Quiesced,
                CandidateDrainPhase::Aborted => CompatibleDrainPhase::Aborted,
                CandidateDrainPhase::Indeterminate => CompatibleDrainPhase::Indeterminate,
                _ => CompatibleDrainPhase::Draining,
            };
            (phase, Some(drain_attempt_id.clone()))
        }
    };
    CompatibleDrainView {
        phase,
        attempt_id: Some(r.attempt_id.clone()),
        support_attempt_id: Some(r.support_attempt_id.clone()),
        support_revision: Some(r.support_revision),
        drain_attempt_id,
        decided_at_ms: Some(r.decided_at_ms),
        assessment: result.and_then(|r| r.assessment.clone()),
        reason: result.map(|r| r.reason.clone()),
    }
}

#[cfg(test)]
pub(crate) mod tests;
