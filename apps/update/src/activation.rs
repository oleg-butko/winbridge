//! Flat activation source binding. Metadata grants no launch, state access or custody.
//! The future ledger/driver must acquire fresh authority and the original guard.
use crate::{
    artifact::ImageEvidence,
    runtime_exchange::FileExchangeState,
    runtime_replacement::{RuntimeReplacementRecord, RuntimeReplacementState},
};
use serde::{Deserialize, Serialize};
use winbridge_core::update::MAX_UPDATE_COUNTER;
use winbridge_protocol::ArtifactDescriptor;
type Result<T> = std::result::Result<T, String>;
mod lifecycle;
pub use lifecycle::*;
pub mod channel;
mod process_view;
pub mod writer;
mod writer_record;
mod writer_view;
pub(crate) use process_view::state_view;
pub(crate) use process_view::view as process_view;
pub use writer_record::{ActivationWriterRecord, ActivationWriterState};
#[cfg(test)]
pub(crate) use writer_view::fixture as writer_view_fixture;
pub(crate) use writer_view::view as writer_view;
#[cfg(any(windows, test))]
mod owner_stack;
#[cfg(windows)]
pub(crate) use owner_stack::on_original_io;
#[cfg(windows)]
mod timing;
#[cfg(windows)]
pub(crate) use timing::ActivationTiming;

/// Fresh metadata preparation retains authentication material only in memory.
/// It grants no launch or instance-lock transfer; there is no JSON constructor.
pub struct ActivationPreparation {
    pub(crate) record: ActivationRecord,
    pub(crate) channel_nonce: String,
    #[cfg(windows)]
    pub(crate) deadline: std::time::Instant,
}
impl ActivationPreparation {
    pub fn record(&self) -> &ActivationRecord {
        &self.record
    }
    /// For the future trusted private-channel driver only; never publish/log it.
    pub fn channel_nonce(&self) -> &str {
        &self.channel_nonce
    }
    #[cfg(windows)]
    pub(crate) fn require_live_window(&self) -> Result<()> {
        if std::time::Instant::now() >= self.deadline {
            return Err(
                "Activation preparation deadline expired; no new execution or adoption".into(),
            );
        }
        Ok(())
    }
}
pub enum ActivationPreparationAdmission {
    Prepared(Box<ActivationPreparation>),
    Existing(Box<ActivationRecord>),
}
/// A single readonly updater snapshot. The receipt is the frozen expected
/// worker-journal data, not a permission to acquire state or start a writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationStateSnapshot {
    pub record: ActivationRecord,
    pub shutdown_receipt: winbridge_protocol::update::WorkerShutdownReceipt,
}
pub struct ActivationWriterPermit {
    pub(crate) record: ActivationWriterRecord,
}
impl ActivationWriterPermit {
    pub fn record(&self) -> &ActivationWriterRecord {
        &self.record
    }
}
pub enum ActivationWriterAdmission {
    Issued(Box<ActivationWriterPermit>),
    Existing(Box<ActivationWriterRecord>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationWriterSnapshot {
    pub activation: ActivationRecord,
    pub writer: ActivationWriterRecord,
    pub shutdown_receipt: winbridge_protocol::update::WorkerShutdownReceipt,
}
#[cfg(windows)]
pub use crate::runtime_replacement::{
    ActivationFilesFailure, ActivationFilesGuard, ActivationLaunchFailure, ActivationProcessGuard,
};

pub(crate) fn view(
    value: Option<&ActivationRecord>,
) -> winbridge_protocol::update::ActivationPreparationView {
    use winbridge_protocol::update::{
        ActivationPreparationPhase as Phase, ActivationPreparationView,
    };
    let phase = match value.map(|v| &v.state) {
        None => Phase::NotPrepared,
        Some(ActivationState::Prepared {}) => Phase::Prepared,
        Some(ActivationState::Indeterminate { .. } | ActivationState::CreationFailed { .. }) => {
            Phase::Indeterminate
        }
        Some(_) => Phase::Progressed,
    };
    let reason = value.and_then(|v| match &v.state {
        ActivationState::Indeterminate { reason, .. }
        | ActivationState::CreationFailed { reason } => Some(reason.clone()),
        _ => None,
    });
    ActivationPreparationView {
        phase,
        attempt_id: value.map(|v| v.intent.attempt_id.clone()),
        revision: value.map(|v| v.revision),
        set_id: value.map(|v| v.intent.source.set_id.clone()),
        replacement_sha256: value.map(|v| v.intent.source.replacement_sha256.clone()),
        worker: value.map(|v| v.intent.source.worker.clone()),
        worker_config_sha256: value.map(|v| v.intent.context.worker_config.sha256.clone()),
        prepared_at_ms: value.map(|v| v.intent.admitted_at_ms),
        physical_custody_held: false,
        reason,
    }
}

#[cfg(test)]
pub(crate) use lifecycle::tests::{fixture as activation_fixture, stages as activation_stages};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationSource {
    pub schema_version: u16,
    pub update_id: String,
    pub replacement_attempt_id: String,
    pub replacement_revision: u64,
    pub replacement_sha256: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub launch_id: String,
    /// Expected VerifyingLocal edge, not evidence that the root reached it.
    pub parent_revision: u64,
    pub installation_id: String,
    pub scope_sha256: String,
    pub set_id: String,
    pub runtime_directory: String,
    pub runtime_identity: String,
    pub updater_config_sha256: String,
    pub shutdown_sha256: String,
    pub backup_sha256: String,
    pub manifest: ImageEvidence,
    pub worker: ImageEvidence,
    pub worker_descriptor: ArtifactDescriptor,
}

fn digest(record: &RuntimeReplacementRecord) -> Result<String> {
    let value = serde_json::to_value(record).map_err(|e| e.to_string())?;
    let mut bytes = b"winbridge.activation.replacement.v1\n".to_vec();
    bytes.extend(winbridge_protocol::canonical_evidence_json(&value).map_err(|e| e.to_string())?);
    Ok(crate::sha256(&bytes))
}
impl ActivationSource {
    /// Pure capture from terminal data. Does not observe files, current root,
    /// live owner or OS resources, and deliberately returns no execution permit.
    pub fn from_files_moved(record: &RuntimeReplacementRecord) -> Result<Self> {
        record.validate()?;
        if !matches!(record.state, RuntimeReplacementState::FilesMoved { .. }) {
            return Err("Activation source requires the complete manifest-last replacement".into());
        }
        let installed = |index: usize| match &record.batch.files[index].state {
            FileExchangeState::Installed { installed, .. } => Ok(installed.clone()),
            _ => Err("Activation source has an incomplete installed file".to_string()),
        };
        let plan = &record.intent.preparation.plan;
        Ok(Self {
            schema_version: 1,
            update_id: plan.update_id.clone(),
            replacement_attempt_id: record.intent.attempt_id.clone(),
            replacement_revision: record.revision,
            replacement_sha256: digest(record)?,
            owner_id: plan.owner_id.clone(),
            owner_epoch: plan.owner_epoch,
            launch_id: plan.launch_id.clone(),
            parent_revision: plan
                .parent_revision
                .checked_add(1)
                .filter(|n| *n <= MAX_UPDATE_COUNTER)
                .ok_or("Activation parent revision exhausted")?,
            installation_id: plan.manifest.installation_id.clone(),
            scope_sha256: plan.manifest.scope_sha256.clone(),
            set_id: plan.manifest.set_id.clone(),
            runtime_directory: plan.runtime_directory.clone(),
            runtime_identity: plan.runtime_identity.clone(),
            updater_config_sha256: plan.config_sha256.clone(),
            shutdown_sha256: plan.proofs.shutdown_sha256.clone(),
            backup_sha256: plan.proofs.backup_sha256.clone(),
            manifest: installed(4)?,
            worker: installed(0)?,
            worker_descriptor: plan.manifest.artifacts[0].descriptor.clone(),
        })
    }
    /// Compare with the retained immutable replacement, not a newer executable's
    /// interpretation of capability support. Historical binding stays historical.
    pub fn validate(&self, record: &RuntimeReplacementRecord) -> Result<()> {
        if *self != Self::from_files_moved(record)? {
            return Err("Activation source differs from exact replacement/owner/generation".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
