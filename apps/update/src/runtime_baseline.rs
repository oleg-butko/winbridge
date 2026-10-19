//! Early immutable runtime observation. Not an install/shutdown permission.
use crate::installed_set::{InstalledSetObservation, MANIFEST_NAME};
use serde::{Deserialize, Serialize};
use std::path::Path;
use winbridge_protocol::{update::SupervisorWorkerBinding, LoadedWorkerIdentity};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeBaselineIntent {
    pub schema_version: u16,
    pub update_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub launch_id: String,
    pub worker: SupervisorWorkerBinding,
    pub config_sha256: String,
    pub request_sha256: String,
    pub parent_revision: u64,
    pub admitted_at_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeBaselineReceipt {
    pub schema_version: u16,
    pub update_id: String,
    pub intent_sha256: String,
    pub parent_revision: u64,
    pub observed_at_ms: u64,
    pub installed: InstalledSetObservation,
    pub loaded_worker: LoadedWorkerIdentity,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeBaselineSnapshot {
    pub intent: Option<RuntimeBaselineIntent>,
    pub receipt: Option<RuntimeBaselineReceipt>,
}
impl RuntimeBaselineSnapshot {
    pub(crate) fn view(&self) -> winbridge_protocol::update::RuntimeBaselineView {
        use winbridge_protocol::update::RuntimeBaselineView;
        match (&self.intent, &self.receipt) {
            (Some(intent), Some(receipt)) => RuntimeBaselineView::Verified {
                intent_revision: intent.parent_revision,
                receipt_revision: receipt.parent_revision,
                observed_at_ms: receipt.observed_at_ms,
                set_id: receipt.loaded_worker.set_id.clone(),
                worker_process: receipt.loaded_worker.process.clone(),
                worker_image_sha256: receipt.loaded_worker.image.sha256.clone(),
            },
            (Some(intent), None) => RuntimeBaselineView::Pending {
                intent_revision: intent.parent_revision,
                admitted_at_ms: intent.admitted_at_ms,
            },
            _ => RuntimeBaselineView::NotAdmitted,
        }
    }
}
pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(crate::sha256(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
/// Validate retained structure without reading any caller-selected path.
pub(crate) fn validate_observation(
    intent: &RuntimeBaselineIntent,
    receipt: &RuntimeBaselineReceipt,
    installation: &str,
    scope: &str,
) -> Result<()> {
    let o = &receipt.installed;
    crate::installed_set::valid_manifest(&o.manifest, installation, scope)?;
    let valid_id = |s: &str| !s.is_empty() && s.len() <= 128;
    let valid_hash = |s: &str| winbridge_core::update::valid_hash(s, &[64]);
    if o.schema_version != 1
        || o.kind != "installed_set_observation"
        || !o.read_only
        || !valid_id(&o.runtime_identity)
        || o.images.len() != 4
        || o.manifest_file.bytes == 0
        || o.manifest_file.bytes > 64 * 1024
        || !valid_hash(&o.manifest_file.sha256)
        || !valid_id(&o.manifest_file.file_identity)
        || o.manifest_file.path
            != Path::new(&o.runtime_directory)
                .join(MANIFEST_NAME)
                .to_str()
                .ok_or("Runtime path must be UTF-8")?
    {
        return Err("Runtime baseline known-set observation rejected".into());
    }
    let mut identities = std::collections::HashSet::new();
    identities.insert(o.manifest_file.file_identity.as_str());
    for (image, artifact) in o.images.iter().zip(&o.manifest.artifacts) {
        if image.path
            != Path::new(&o.runtime_directory)
                .join(&artifact.name)
                .to_str()
                .ok_or("Runtime image path must be UTF-8")?
            || image.bytes != artifact.bytes
            || image.sha256 != artifact.sha256
            || !image.query_supported
            || !valid_id(&image.file_identity)
            || !identities.insert(image.file_identity.as_str())
        {
            return Err("Runtime baseline image path/bytes/hash/identity inconsistent".into());
        }
    }
    let loaded = &receipt.loaded_worker;
    if loaded.schema_version != 1
        || loaded.kind != "loaded_worker_identity"
        || loaded.process != intent.worker.process
        || loaded.descriptor.build != intent.worker.build
        || loaded.runtime_identity != o.runtime_identity
        || loaded.set_id != o.manifest.set_id
        || loaded.manifest_sha256 != o.manifest_file.sha256
        || loaded.image != o.images[0]
        || loaded.descriptor != o.manifest.artifacts[0].descriptor
    {
        return Err("Runtime baseline does not bind the exact live worker/set".into());
    }
    Ok(())
}

/// Trusted supervisor driver, on its blocking thread. Repeating a completed
/// baseline only returns history. A pending same-epoch attempt can repeat reads,
/// but never rebind to another worker or resume source effects.
pub fn capture(
    j: &mut crate::journal::UpdateJournal,
    a: &crate::journal::UpdateAuthority,
    record: &winbridge_protocol::update::SupervisorRecord,
    io: tokio::runtime::Handle,
) -> Result<RuntimeBaselineSnapshot> {
    use winbridge_protocol::update::{UpdatePhase, WorkerUpdateControlCommand};
    let cfg = crate::preparation::configuration(record)?;
    let view = j
        .admit_runtime_baseline(a, record)
        .map_err(|e| e.to_string())?;
    if view.receipt.is_some() {
        return Ok(view);
    }
    let intent = view.intent.ok_or("Runtime baseline intent missing")?;
    j.check_source_authority(a).map_err(|e| e.to_string())?;
    let update = j.get(&record.spec.update_id).map_err(|e| e.to_string())?;
    if intent.owner_id != a.owner_id()
        || intent.owner_epoch != a.epoch()
        || intent.launch_id != record.spec.launch_id
        || intent.config_sha256 != record.spec.config_sha256
        || intent.worker != *record.spec.worker.as_ref().ok_or("Bound worker missing")?
        || update.phase != UpdatePhase::Requested
        || update.revision != intent.parent_revision
    {
        return Err(
            "Runtime baseline intent is historical/stale; no rebind or late adoption".into(),
        );
    }
    let pin = crate::installed_set::pin_installed_set(
        &cfg.runtime_directory,
        &update.request.expected.installation_id,
        &update.request.expected.scope_sha256,
    )?;
    let loaded = io
        .block_on(crate::supervisor::forward_worker(
            record,
            WorkerUpdateControlCommand::ObserveRuntime,
        ))?
        .into_runtime_identity()?;
    pin.revalidate()?;
    crate::preparation::configuration(record)?;
    j.check_source_authority(a).map_err(|e| e.to_string())?;
    let now: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "Runtime observation timestamp overflow")?;
    let receipt = RuntimeBaselineReceipt {
        schema_version: 1,
        update_id: intent.update_id.clone(),
        intent_sha256: digest(&intent)?,
        parent_revision: intent.parent_revision + 1,
        observed_at_ms: now.max(intent.admitted_at_ms),
        installed: pin.observation().clone(),
        loaded_worker: loaded,
    };
    validate_observation(
        &intent,
        &receipt,
        &update.request.expected.installation_id,
        &update.request.expected.scope_sha256,
    )?;
    j.complete_runtime_baseline(a, record, &receipt)
        .map_err(|e| e.to_string())
}
