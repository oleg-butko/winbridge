//! Journal-only delegation data. This is NOT a wire request or a spawn permit.
//! Old keepers must never deserialize an entire future updater Root/SQLite model.
//! The live factory/one-shot journal admission and minimal wire bridge follow.
use serde::{Deserialize, Serialize};
use winbridge_platform::execution_domain::DomainIdentity;
use winbridge_protocol::{update::*, BuildIdentity};

type Result<T> = std::result::Result<T, String>;
const MAX: u64 = winbridge_core::update::MAX_UPDATE_COUNTER;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapFile {
    pub sha256: String,
    pub bytes: u64,
    pub file_identity: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapIntent {
    pub schema_version: u16,
    pub previous: KeeperBinding,
    pub update: UpdateRecord,
    pub launch_id: String,
    pub parent_owner_id: String,
    pub parent_epoch: u64,
    pub updater_build: BuildIdentity,
    pub image: BootstrapFile,
    pub config: BootstrapFile,
    pub stdout_identity: String,
    pub stderr_identity: String,
    pub token_sha256: String,
    pub excluded_domain: DomainIdentity,
    pub prepared_at_ms: u64,
}
fn need(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hash(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s)
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn process(p: &SupervisorProcess) -> bool {
    p.process_id != 0
        && hash(&p.host_id, 64)
        && p.birth
            .parse::<u64>()
            .is_ok_and(|v| v > 0 && v.to_string() == p.birth)
}
impl BootstrapFile {
    fn validate(&self) -> Result<()> {
        need(
            hash(&self.sha256, 64)
                && self.bytes > 0
                && self.bytes <= MAX
                && text(&self.file_identity, 512),
            "Invalid bootstrap file evidence",
        )
    }
}
impl BootstrapIntent {
    /// Data validation only. A successful result grants no process/Job/lock.
    pub fn validate(&self) -> Result<()> {
        need(
            serde_json::to_vec(self).map_err(|e| e.to_string())?.len() <= 32768,
            "Bootstrap intent exceeds record limit",
        )?;
        let ids = [
            &self.previous.update_id,
            &self.previous.launch_id,
            &self.update.update_id,
            &self.launch_id,
        ];
        need(
            ids.iter().all(|id| uuid(id))
                && (0..ids.len()).all(|i| ((i + 1)..ids.len()).all(|j| ids[i] != ids[j])),
            "Bootstrap update/launch identities alias or are invalid",
        )?;
        need(
            self.schema_version == 1
                && self.update.phase == UpdatePhase::Requested
                && self.update.revision == 0
                && self.update.request.contract_version == 1
                && self.update.reason.is_none()
                && self.update.updated_at_ms == self.update.accepted_at_ms,
            "Bootstrap needs a new Requested root",
        )?;
        need(
            process(&self.previous.worker)
                && process(&self.previous.coordinator)
                && self.previous.worker.process_id != self.previous.coordinator.process_id,
            "Bootstrap logical worker/physical keeper invalid or alias",
        )?;
        need(
            uuid(&self.previous.generation)
                && uuid(&self.previous.finalization_command_id)
                && hash(&self.previous.finalization_command_sha256, 64)
                && self
                    .previous
                    .selection_sha256
                    .as_deref()
                    .is_some_and(|v| hash(v, 64))
                && text(&self.previous.job_name, 1024),
            "Bootstrap has no complete historical keeper identity",
        )?;
        need(
            winbridge_core::config::valid_id(&self.previous.context.instance_id)
                && uuid(&self.previous.context.journal_incarnation),
            "Bootstrap worker context invalid",
        )?;
        need(
            uuid(&self.parent_owner_id)
                && self.parent_epoch > 0
                && self.parent_epoch <= MAX
                && self.prepared_at_ms >= self.update.updated_at_ms
                && self.update.updated_at_ms >= self.update.accepted_at_ms
                && self.prepared_at_ms <= MAX,
            "Bootstrap owner/time invalid",
        )?;
        let source = &self.update.request.source;
        need(
            hash(&source.source_commit, 40)
                && hash(&source.manifest_sha256, 64)
                && hash(&source.archive_sha256, 64)
                && text(&source.archive_name, 255)
                && !source.archive_name.contains(['/', '\\', ':'])
                && source.archive_name != "."
                && source.archive_name != "..",
            "Bootstrap source pin invalid",
        )?;
        let expected = &self.update.request.expected;
        need(
            uuid(&expected.installation_id)
                && hash(&expected.scope_sha256, 64)
                && hash(&expected.worker_config_sha256, 64)
                && hash(&expected.baseline_manifest_sha256, 64)
                && text(&expected.worker_build_id, 128)
                && text(&self.update.request.idempotency_key, 256),
            "Bootstrap installation preconditions invalid",
        )?;
        need(
            !self.updater_build.source_dirty
                && self.updater_build.source_commit.as_ref() == Some(&source.source_commit)
                && self.updater_build.source_manifest_sha256.as_ref()
                    == Some(&source.manifest_sha256)
                && text(&self.updater_build.version, 128)
                && text(&self.updater_build.build_id, 128),
            "Bootstrap updater is not from the requested clean source",
        )?;
        self.image.validate()?;
        self.config.validate()?;
        // config is the UPDATER config, not the loaded worker config. Their hashes
        // are intentionally not equated; live validation pins each fixed file.
        let files = [
            &self.image.file_identity,
            &self.config.file_identity,
            &self.stdout_identity,
            &self.stderr_identity,
        ];
        need(
            (0..files.len()).all(|i| ((i + 1)..files.len()).all(|j| files[i] != files[j])),
            "Bootstrap files alias",
        )?;
        need(
            hash(&self.token_sha256, 64)
                && text(&self.stdout_identity, 512)
                && text(&self.stderr_identity, 512)
                && self.stdout_identity != self.stderr_identity
                && self.image.file_identity != self.config.file_identity,
            "Bootstrap control/output identities invalid or alias",
        )?;
        self.excluded_domain.validate().map_err(|e| e.to_string())?;
        need(
            self.excluded_domain.host_id == self.previous.worker.host_id
                && self.excluded_domain.session_id == self.previous.worker.session_id,
            "Bootstrap task domain belongs to another host/session",
        )
    }
    /// Historical correlation only; never adopt a saved selection as live custody.
    pub fn validate_against(&self, selection: &crate::journal::KeeperSelection) -> Result<()> {
        self.validate()?;
        selection.validate().map_err(|e| e.to_string())?;
        let crate::activation::finalization::ActivationFinalizationState::Confirmed { receipt } =
            &selection.finalization.state
        else {
            return Err("Bootstrap parent not confirmed".into());
        };
        let b = &selection.finalization.command.binding;
        let old = &b.lifetime_command.binding;
        let expected = keeper_binding_from_receipt(
            old.update_id.clone(),
            old.launch_id.clone(),
            old.keeper.clone(),
            old.job_name.clone(),
            receipt,
            Some(selection.sha256().map_err(|e| e.to_string())?),
        );
        let current = &self.update.request.expected;
        need(
            self.update.accepted_at_ms >= selection.selected.updated_at_ms
                && self.previous == expected
                && current.installation_id == b.installation_id
                && current.scope_sha256 == b.scope_sha256
                && current.worker_build_id
                    == receipt.compatibility.identity.descriptor.build.build_id
                && current.worker_config_sha256 == receipt.compatibility.worker_config_sha256
                && current.baseline_manifest_sha256 == b.source_manifest_sha256
                && selection
                    .selected
                    .owner_epoch
                    .is_some_and(|v| self.parent_epoch > v)
                && selection.selected.owner_id.as_ref() != Some(&self.parent_owner_id),
            "Bootstrap changed historical keeper/current worker/installation/owner",
        )
    }
    /// A canonical key for immutable data, not a launch authorization.
    pub fn sha256(&self) -> Result<String> {
        self.validate()?;
        let mut bytes = b"winbridge.keeper.bootstrap-intent.v1\n".to_vec();
        bytes.extend(
            winbridge_protocol::canonical_evidence_json(
                &serde_json::to_value(self).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
        );
        Ok(crate::sha256(&bytes))
    }
    /// Names only: runtime scope and safe filesystem pins belong to a live owner.
    pub fn helper_components(&self) -> Result<[&str; 4]> {
        self.validate()?;
        Ok([
            "update",
            &self.update.update_id,
            "supervisor",
            "winbridge-update.exe",
        ])
    }
}
#[cfg(test)]
#[path = "bootstrap/tests.rs"]
pub(crate) mod tests;

#[cfg(any(windows, test))]
mod files;
#[cfg(any(windows, test))]
pub use files::{FreshUpdater, PreparedBootstrapFiles};

#[cfg(windows)]
mod admission;
#[cfg(windows)]
pub use admission::{BootstrapPreparation, BootstrapPreparationFailure, PreparedBootstrap};
