//! Final journal export data. Declaring this contract does not advertise or
//! authorize an export, migration, restore, replacement or worker startup.
use super::{SupervisorProcess, WorkerShutdownReceipt, WorkerShutdownState, WorkerUpdatePhase};
use crate::{ArtifactDescriptor, ArtifactRole, RuntimeImageEvidence};
use serde::{Deserialize, Serialize};

/// Worker must explicitly implement/advertise this mode before stop admission.
/// No executable embeds this marker merely by exporting these Rust types.
pub const WORKER_STATE_BACKUP_MARKER: &[u8] = b"\0WinBridge.WorkerStateBackup.contract-1\0";
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateBackupAuthority {
    pub incarnation: String,
    pub epoch: u64,
    /// Fingerprint of the complete private authority header; no execution token.
    pub fingerprint_sha256: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateBackupFileEvidence {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub file_identity: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerStateBackupReport {
    pub schema_version: u16,
    pub kind: String,
    pub input_sha256: String,
    pub update_id: String,
    pub attempt_id: String,
    pub helper_descriptor: ArtifactDescriptor,
    pub helper_process: SupervisorProcess,
    pub helper_image: RuntimeImageEvidence,
    pub worker_config: RuntimeImageEvidence,
    pub state: WorkerShutdownState,
    pub worker_journal_schema: u32,
    pub source_authority: StateBackupAuthority,
    pub backup_authority: StateBackupAuthority,
    pub shutdown_receipt: WorkerShutdownReceipt,
    pub backup: StateBackupFileEvidence,
    pub page_size: u32,
    pub page_count: u64,
    pub completed_at_ms: u64,
    pub integrity_verified: bool,
    pub backup_created_new: bool,
    pub journal_opened_read_only: bool,
    pub worker_constructed: bool,
    pub scheduler_started: bool,
    pub recovery_marker_changed: bool,
    pub runtime_applied: bool,
}
const MAX: u64 = 9_007_199_254_740_991;
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}
fn process(p: &SupervisorProcess) -> bool {
    p.process_id != 0 && text(&p.birth, 128) && text(&p.host_id, 128)
}
impl WorkerStateBackupReport {
    /// Envelope/causal consistency only. The trusted updater must additionally
    /// join its immutable attempt, actual helper process/cleanup and file pins.
    /// This method cannot create a physical exit guard or execution permission.
    pub fn validate_envelope(&self) -> Result<(), String> {
        let r = &self.shutdown_receipt;
        let a = &self.source_authority;
        let image = &self.helper_image;
        let cfg = &self.worker_config;
        if self.schema_version != 1
            || self.kind != "worker_state_backup"
            || !hash(&self.input_sha256)
            || !uuid(&self.attempt_id)
            || self.update_id != r.update_id
            || !uuid(&self.update_id)
            || r.schema_version != 1
            || r.lease.phase != WorkerUpdatePhase::Quiesced
            || r.command.expected_lease_revision != r.lease.revision
            || r.lease.revision == 0
            || r.lease.revision > MAX
            || !text(&r.command.command_id, 128)
            || !text(&r.launch_id, 128)
            || !text(&r.updater_owner_id, 128)
            || !hash(&r.command.intent_sha256)
            || r.lease.spec.update_id != r.update_id
            || r.lease.spec.updater_owner_id != r.updater_owner_id
            || r.lease.spec.updater_epoch != r.updater_epoch
            || r.updater_epoch == 0
            || r.updater_epoch > MAX
            || r.accepted_at_ms == 0
            || r.accepted_at_ms > MAX
            || self.state != r.state
            || a.incarnation != r.lease.spec.journal_incarnation
            || !uuid(&a.incarnation)
            || a.epoch == 0
            || a.epoch > MAX
            || !hash(&a.fingerprint_sha256)
            || self.backup_authority != *a
            || self.worker_journal_schema == 0
            || !process(&self.helper_process)
            || !process(&r.process)
            || self.helper_process == r.process
            || self.helper_process.host_id != r.process.host_id
            || self.helper_process.session_id != r.process.session_id
            || self.helper_descriptor.role != ArtifactRole::Worker
            || self.helper_descriptor.kind != "winbridge_artifact"
            || self.helper_descriptor.schema_version != 1
            || self.helper_descriptor.query_contract != 1
            || self.helper_descriptor.target_os != "windows"
            || image.bytes == 0
            || image.bytes > 256 * 1024 * 1024
            || !image.query_supported
            || !hash(&image.sha256)
            || !text(&image.path, 4096)
            || !text(&image.file_identity, 128)
            || cfg.bytes == 0
            || cfg.bytes > 65536
            || cfg.query_supported
            || !hash(&cfg.sha256)
            || !text(&cfg.path, 4096)
            || !text(&cfg.file_identity, 128)
            || !text(&self.state.state_directory, 4096)
            || [
                &self.state.directory_identity,
                &self.state.journal_identity,
                &self.state.lock_identity,
            ]
            .into_iter()
            .any(|v| !text(v, 128))
            || self.state.directory_identity == self.state.journal_identity
            || self.state.journal_identity == self.state.lock_identity
            || self.state.directory_identity == self.state.lock_identity
        {
            return Err(
                "Final state backup helper/source/authority/shutdown envelope differs".into(),
            );
        }
        let b = &self.backup;
        if !text(&b.path, 4096)
            || !text(&b.file_identity, 128)
            || !hash(&b.sha256)
            || [
                &self.state.directory_identity,
                &self.state.journal_identity,
                &self.state.lock_identity,
                &image.file_identity,
                &cfg.file_identity,
            ]
            .into_iter()
            .any(|id| id == &b.file_identity)
            || b.bytes == 0
            || b.bytes > MAX
            || !self.page_size.is_power_of_two()
            || !(512..=65536).contains(&self.page_size)
            || self.page_count == 0
            || self.page_count.checked_mul(u64::from(self.page_size)) != Some(b.bytes)
            || self.completed_at_ms < r.accepted_at_ms
            || self.completed_at_ms > MAX
            || !self.integrity_verified
            || !self.backup_created_new
            || !self.journal_opened_read_only
            || self.worker_constructed
            || self.scheduler_started
            || self.recovery_marker_changed
            || self.runtime_applied
        {
            return Err("Final state backup must be a complete new consistent copy without source/runtime effects".into());
        }
        Ok(())
    }
}
