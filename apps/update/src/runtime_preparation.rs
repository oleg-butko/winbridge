//! Create-only preparation data, before any runtime rename. JSON is not authority.
use crate::{
    artifact::ImageEvidence,
    installed_set::InstalledSetManifest,
    runtime_exchange::{FileExchangeBatch, FileExchangeRecord, FileExchangeSpec, RuntimeFile},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use winbridge_core::update::valid_hash;
type Result<T> = std::result::Result<T, String>;
pub const FILE_ORDER: [RuntimeFile; 5] = [
    RuntimeFile::Worker,
    RuntimeFile::Fixture,
    RuntimeFile::Desktop,
    RuntimeFile::Updater,
    RuntimeFile::Manifest,
];
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s)
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        == Some(s)
}
fn image(e: &ImageEvidence, query: bool) -> bool {
    text(&e.path, 4096)
        && text(&e.file_identity, 128)
        && valid_hash(&e.sha256, &[64])
        && e.bytes > 0
        && e.bytes <= if query { 256 * 1024 * 1024 } else { 64 * 1024 }
        && e.query_supported == query
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparationProofs {
    pub request_sha256: String,
    pub supervisor_spec_sha256: String,
    pub baseline_sha256: String,
    pub preimages_sha256: String,
    pub artifacts_sha256: String,
    pub health_sha256: String,
    pub shutdown_sha256: String,
    pub backup_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparationFile {
    pub file: RuntimeFile,
    pub old: ImageEvidence,
    /// None only for the new manifest, whose exact bytes are derived from plan.manifest.
    pub source: Option<ImageEvidence>,
    pub destination: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreparationPlan {
    pub schema_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub parent_revision: u64,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub launch_id: String,
    pub config_sha256: String,
    pub runtime_directory: String,
    pub runtime_identity: String,
    pub parent_identity: String,
    pub admitted_at_ms: u64,
    pub available_bytes_before: u64,
    pub proofs: PreparationProofs,
    pub manifest: InstalledSetManifest,
    pub files: Vec<PreparationFile>,
}
impl RuntimePreparationPlan {
    pub fn directory(&self) -> std::path::PathBuf {
        Path::new(&self.runtime_directory)
            .join("update")
            .join(&self.update_id)
            .join("prepared-runtime")
    }
    /// A fixed canonical encoding is the actual manifest payload, not a display rendering.
    pub fn manifest_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&self.manifest).map_err(|e| e.to_string())
    }
    pub fn required_bytes(&self) -> Result<u64> {
        self.files.iter().try_fold(0_u64, |n, f| {
            n.checked_add(f.bytes)
                .ok_or("Preparation size overflow".into())
        })
    }
    pub fn validate(&self) -> Result<()> {
        let root = Path::new(&self.runtime_directory);
        if self.schema_version != 1
            || !uuid(&self.update_id)
            || !uuid(&self.attempt_id)
            || !uuid(&self.owner_id)
            || !uuid(&self.launch_id)
            || self.parent_revision == 0
            || self.parent_revision > winbridge_core::update::MAX_UPDATE_COUNTER
            || self.owner_epoch == 0
            || self.owner_epoch > winbridge_core::update::MAX_UPDATE_COUNTER
            || self.admitted_at_ms == 0
            || self.admitted_at_ms > winbridge_core::update::MAX_UPDATE_COUNTER
            || !text(&self.runtime_directory, 4000)
            || !root.is_absolute()
            || root.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
            || !text(&self.runtime_identity, 128)
            || !text(&self.parent_identity, 128)
            || self.runtime_identity == self.parent_identity
            || !valid_hash(&self.config_sha256, &[64])
            || self.files.len() != FILE_ORDER.len()
        {
            return Err("Preparation scope/authority/version/counters rejected".into());
        }
        winbridge_platform::require_local_path_prefix(root).map_err(|e| e.to_string())?;
        for hash in [
            &self.proofs.request_sha256,
            &self.proofs.supervisor_spec_sha256,
            &self.proofs.baseline_sha256,
            &self.proofs.preimages_sha256,
            &self.proofs.artifacts_sha256,
            &self.proofs.health_sha256,
            &self.proofs.shutdown_sha256,
            &self.proofs.backup_sha256,
        ] {
            if !valid_hash(hash, &[64]) {
                return Err("Preparation causal proof digest rejected".into());
            }
        }
        crate::installed_set::valid_manifest(
            &self.manifest,
            &self.manifest.installation_id,
            &self.manifest.scope_sha256,
        )?;
        if !uuid(&self.manifest.set_id) {
            return Err("Prepared manifest requires canonical set ID".into());
        }
        let mut ids = std::collections::HashSet::new();
        for (index, (file, role)) in self.files.iter().zip(FILE_ORDER).enumerate() {
            let query = role != RuntimeFile::Manifest;
            if file.file != role
                || Path::new(&file.old.path) != root.join(role.name())
                || Path::new(&file.destination) != self.directory().join(role.name())
                || !image(&file.old, query)
                || !ids.insert(file.old.file_identity.as_str())
            {
                return Err("Preparation fixed file/independent old identity differs".into());
            }
            if query {
                let source = file.source.as_ref().ok_or("Candidate source missing")?;
                let artifact = &self.manifest.artifacts[index];
                if !image(source, true)
                    || !ids.insert(source.file_identity.as_str())
                    || Path::new(&source.path)
                        != root
                            .join("update")
                            .join(&self.update_id)
                            .join("candidate")
                            .join("payload")
                            .join(role.name())
                    || file.bytes != source.bytes
                    || file.sha256 != source.sha256
                    || file.bytes != artifact.bytes
                    || file.sha256 != artifact.sha256
                {
                    return Err("Preparation exact candidate payload/manifest differs".into());
                }
            } else {
                let bytes = self.manifest_bytes()?;
                if file.source.is_some()
                    || bytes.is_empty()
                    || bytes.len() > 64 * 1024
                    || file.bytes != bytes.len() as u64
                    || file.sha256 != crate::sha256(&bytes)
                {
                    return Err("Preparation manifest bytes/hash differ".into());
                }
            }
        }
        if self.available_bytes_before
            < self
                .required_bytes()?
                .checked_add(6 * 1024 * 1024 * 1024)
                .ok_or("Preparation reserve overflow")?
        {
            return Err("Preparation needs 6 GiB free plus all independent copies".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationFileState {
    Pending {},
    Issued {},
    Verified { image: ImageEvidence },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreparationPhase {
    Planned,
    DirectoryIssued,
    Preparing,
    Prepared,
    Indeterminate,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreparationRecord {
    pub schema_version: u16,
    pub plan: RuntimePreparationPlan,
    pub revision: u64,
    pub phase: PreparationPhase,
    pub directory_identity: Option<String>,
    pub files: Vec<PreparationFileState>,
    pub reason: Option<String>,
}
impl RuntimePreparationRecord {
    pub fn planned(plan: RuntimePreparationPlan) -> Result<Self> {
        let r = Self {
            schema_version: 1,
            plan,
            revision: 0,
            phase: PreparationPhase::Planned,
            directory_identity: None,
            files: vec![PreparationFileState::Pending {}; 5],
            reason: None,
        };
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.schema_version != 1 || self.files.len() != 5 {
            return Err("Preparation record shape rejected".into());
        }
        let mut steps = 0;
        let mut unfinished = false;
        let mut ids: std::collections::HashSet<_> = self
            .plan
            .files
            .iter()
            .flat_map(|f| {
                std::iter::once(f.old.file_identity.as_str())
                    .chain(f.source.as_ref().map(|s| s.file_identity.as_str()))
            })
            .collect();
        for (file, state) in self.plan.files.iter().zip(&self.files) {
            if unfinished && !matches!(state, PreparationFileState::Pending {}) {
                return Err("Preparation files must advance in fixed causal order".into());
            }
            match state {
                PreparationFileState::Pending {} => unfinished = true,
                PreparationFileState::Issued {} => {
                    steps += 1;
                    unfinished = true;
                }
                PreparationFileState::Verified { image: actual } => {
                    if !image(actual, file.file != RuntimeFile::Manifest)
                        || actual.path != file.destination
                        || actual.bytes != file.bytes
                        || actual.sha256 != file.sha256
                        || !ids.insert(actual.file_identity.as_str())
                    {
                        return Err("Prepared receipt must be an independent exact copy".into());
                    }
                    steps += 2;
                }
            }
        }
        if let Some(id) = &self.directory_identity {
            if !text(id, 128)
                || id == &self.plan.parent_identity
                || id == &self.plan.runtime_identity
                || ids.contains(id.as_str())
            {
                return Err("Prepared directory identity aliases source/parent".into());
            }
        }
        let normal_revision = if self.directory_identity.is_some() {
            2 + steps
        } else if self.revision == 0 {
            0
        } else {
            1
        };
        let shape = match self.phase {
            PreparationPhase::Planned => {
                self.directory_identity.is_none() && self.revision == 0 && steps == 0
            }
            PreparationPhase::DirectoryIssued => {
                self.directory_identity.is_none() && self.revision == 1 && steps == 0
            }
            PreparationPhase::Preparing => self.directory_identity.is_some() && unfinished,
            PreparationPhase::Prepared => self.directory_identity.is_some() && !unfinished,
            PreparationPhase::Indeterminate => {
                self.reason.as_ref().is_some_and(|r| text(r, 4096))
                    && self.revision > 0
                    && (self.directory_identity.is_some() || steps == 0)
                    && unfinished
            }
        };
        // An uncertain pre-directory record preserves whether its directory intent was issued.
        let expected =
            if self.phase == PreparationPhase::Indeterminate && self.directory_identity.is_none() {
                self.revision == 1 || self.revision == 2
            } else {
                self.revision
                    == normal_revision + u64::from(self.phase == PreparationPhase::Indeterminate)
            };
        if !shape
            || !expected
            || (self.phase != PreparationPhase::Indeterminate && self.reason.is_some())
        {
            return Err("Preparation phase/revision/prefix mismatch".into());
        }
        Ok(())
    }
    fn next(&self, mut next: Self) -> Result<Self> {
        self.validate()?;
        if matches!(
            self.phase,
            PreparationPhase::Prepared | PreparationPhase::Indeterminate
        ) {
            return Err("Preparation terminal; observe only".into());
        }
        next.revision = self.revision + 1;
        next.validate()?;
        Ok(next)
    }
    pub fn issue_directory(&self) -> Result<Self> {
        if self.phase != PreparationPhase::Planned {
            return Err("Directory already issued; never replay".into());
        }
        self.next(Self {
            phase: PreparationPhase::DirectoryIssued,
            ..self.clone()
        })
    }
    pub fn directory_created(&self, identity: String) -> Result<Self> {
        if self.phase != PreparationPhase::DirectoryIssued {
            return Err("Directory receipt requires intent".into());
        }
        self.next(Self {
            phase: PreparationPhase::Preparing,
            directory_identity: Some(identity),
            ..self.clone()
        })
    }
    pub fn issue_file(&self, index: usize) -> Result<Self> {
        if self.phase != PreparationPhase::Preparing
            || self
                .files
                .iter()
                .position(|f| !matches!(f, PreparationFileState::Verified { .. }))
                != Some(index)
            || !matches!(self.files[index], PreparationFileState::Pending {})
        {
            return Err("File already issued or out of order; no replay".into());
        }
        let mut next = self.clone();
        next.files[index] = PreparationFileState::Issued {};
        self.next(next)
    }
    pub fn file_created(&self, index: usize, image: ImageEvidence) -> Result<Self> {
        if self.phase != PreparationPhase::Preparing
            || !matches!(self.files.get(index), Some(PreparationFileState::Issued {}))
        {
            return Err("Copy receipt requires original saved intent".into());
        }
        let mut next = self.clone();
        next.files[index] = PreparationFileState::Verified { image };
        if next
            .files
            .iter()
            .all(|s| matches!(s, PreparationFileState::Verified { .. }))
        {
            next.phase = PreparationPhase::Prepared;
        }
        self.next(next)
    }
    pub fn indeterminate(&self, reason: String) -> Result<Self> {
        self.next(Self {
            phase: PreparationPhase::Indeterminate,
            reason: Some(reason),
            ..self.clone()
        })
    }
    /// Data only: no OS custody, rename handles or dispatch permission.
    pub fn exchange_batch(&self) -> Result<FileExchangeBatch> {
        self.validate()?;
        if self.phase != PreparationPhase::Prepared {
            return Err("All independent copies required before exchange plan".into());
        }
        let files = self
            .plan
            .files
            .iter()
            .zip(&self.files)
            .map(|(f, state)| {
                let PreparationFileState::Verified { image } = state else {
                    unreachable!("validated Prepared");
                };
                FileExchangeRecord::prepared(FileExchangeSpec {
                    update_id: self.plan.update_id.clone(),
                    runtime_directory: self.plan.runtime_directory.clone(),
                    file: f.file,
                    old: f.old.clone(),
                    prepared: image.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let batch = FileExchangeBatch {
            schema_version: 1,
            files,
        };
        batch.validate()?;
        Ok(batch)
    }
    pub(crate) fn validate_step(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        let expected = if next.phase == PreparationPhase::Indeterminate {
            self.indeterminate(next.reason.clone().ok_or("Uncertainty reason missing")?)?
        } else if next.phase == PreparationPhase::DirectoryIssued {
            self.issue_directory()?
        } else if self.phase == PreparationPhase::DirectoryIssued {
            self.directory_created(
                next.directory_identity
                    .clone()
                    .ok_or("Directory identity missing")?,
            )?
        } else {
            let index = self
                .files
                .iter()
                .zip(&next.files)
                .position(|(a, b)| a != b)
                .ok_or("No causal copy step")?;
            match &next.files[index] {
                PreparationFileState::Issued {} => self.issue_file(index)?,
                PreparationFileState::Verified { image } => {
                    self.file_created(index, image.clone())?
                }
                _ => return Err("Preparation cannot return to Pending".into()),
            }
        };
        if expected != *next {
            return Err("Preparation must preserve plan and append exactly one causal step".into());
        }
        Ok(())
    }
}
/// Fresh DB admission permits preparation only, never replacement or activation.
pub struct RuntimePreparationPermit {
    pub(crate) plan: RuntimePreparationPlan,
}
impl RuntimePreparationPermit {
    pub fn plan(&self) -> &RuntimePreparationPlan {
        &self.plan
    }
}
pub enum RuntimePreparationAdmission {
    Fresh(Box<RuntimePreparationPermit>),
    Existing(Box<RuntimePreparationRecord>),
}

#[cfg(test)]
pub(crate) mod tests;
