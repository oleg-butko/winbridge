//! Per-file replacement data FSM. No filesystem IO or execution authority.
//! A future ledger/driver must bind this to live custody and verified sets.
use crate::artifact::ImageEvidence;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFile {
    Worker,
    Fixture,
    Desktop,
    Updater,
    Manifest,
}
impl RuntimeFile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Worker => "winbridge-worker.exe",
            Self::Fixture => "winbridge-fixture.exe",
            Self::Desktop => "winbridge-desktop.exe",
            Self::Updater => "winbridge-update.exe",
            Self::Manifest => crate::installed_set::MANIFEST_NAME,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileExchangeSpec {
    pub update_id: String,
    pub runtime_directory: String,
    pub file: RuntimeFile,
    pub old: ImageEvidence,
    pub prepared: ImageEvidence,
}
impl FileExchangeSpec {
    fn run(&self) -> PathBuf {
        Path::new(&self.runtime_directory)
            .join("update")
            .join(&self.update_id)
    }
    pub fn retained_path(&self) -> PathBuf {
        self.run().join("displaced-runtime").join(self.file.name())
    }
    pub fn installed_path(&self) -> PathBuf {
        Path::new(&self.runtime_directory).join(self.file.name())
    }
    pub fn validate(&self) -> Result<()> {
        let root = Path::new(&self.runtime_directory);
        if uuid::Uuid::parse_str(&self.update_id)
            .ok()
            .map(|id| id.to_string())
            .as_deref()
            != Some(self.update_id.as_str())
            || !root.is_absolute()
            || self.runtime_directory.len() > 32768
            || self.runtime_directory.chars().any(char::is_control)
            || root.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
            || Path::new(&self.old.path) != self.installed_path()
            || Path::new(&self.prepared.path)
                != self.run().join("prepared-runtime").join(self.file.name())
            || self.old.file_identity == self.prepared.file_identity
        {
            return Err("File exchange fixed scope/independent identities differ".into());
        }
        winbridge_platform::require_local_path_prefix(root).map_err(|e| e.to_string())?;
        let maximum = if self.file == RuntimeFile::Manifest {
            64 * 1024
        } else {
            256 * 1024 * 1024
        };
        for image in [&self.old, &self.prepared] {
            if image.bytes == 0
                || image.bytes > maximum
                || !winbridge_core::update::valid_hash(&image.sha256, &[64])
                || image.file_identity.is_empty()
                || image.file_identity.len() > 128
                || image.file_identity.chars().any(char::is_control)
                || image.query_supported != (self.file != RuntimeFile::Manifest)
            {
                return Err("File exchange image/hash/type evidence rejected".into());
            }
        }
        Ok(())
    }
    fn moved(&self, actual: &ImageEvidence, source: &ImageEvidence, target: &Path) -> Result<()> {
        let mut expected = source.clone();
        expected.path = target
            .to_str()
            .ok_or("Exchange destination must be UTF-8")?
            .into();
        if *actual != expected {
            return Err("Exchange receipt must preserve exact moved file ID/hash/size/type".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileExchangeState {
    Prepared {},
    RetireIssued {},
    Retired {
        retained: ImageEvidence,
    },
    PlaceIssued {
        retained: ImageEvidence,
    },
    Installed {
        retained: ImageEvidence,
        installed: ImageEvidence,
    },
    Indeterminate {
        previous: Box<FileExchangeState>,
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileExchangeRecord {
    pub schema_version: u16,
    pub spec: FileExchangeSpec,
    pub revision: u64,
    pub state: FileExchangeState,
}
impl FileExchangeRecord {
    pub fn prepared(spec: FileExchangeSpec) -> Result<Self> {
        let record = Self {
            schema_version: 1,
            spec,
            revision: 0,
            state: FileExchangeState::Prepared {},
        };
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> Result<()> {
        self.spec.validate()?;
        let validate_state = |state: &FileExchangeState| -> Result<u64> {
            Ok(match state {
                FileExchangeState::Prepared {} => 0,
                FileExchangeState::RetireIssued {} => 1,
                FileExchangeState::Retired { retained }
                | FileExchangeState::PlaceIssued { retained } => {
                    self.spec
                        .moved(retained, &self.spec.old, &self.spec.retained_path())?;
                    if matches!(state, FileExchangeState::Retired { .. }) {
                        2
                    } else {
                        3
                    }
                }
                FileExchangeState::Installed {
                    retained,
                    installed,
                } => {
                    self.spec
                        .moved(retained, &self.spec.old, &self.spec.retained_path())?;
                    self.spec
                        .moved(installed, &self.spec.prepared, &self.spec.installed_path())?;
                    4
                }
                FileExchangeState::Indeterminate { .. } => {
                    return Err("Nested uncertainty or changed terminal outcome".into())
                }
            })
        };
        let expected = match &self.state {
            FileExchangeState::Indeterminate { previous, reason } => {
                if matches!(previous.as_ref(), FileExchangeState::Installed { .. })
                    || reason.is_empty()
                    || reason.len() > 4096
                    || reason.chars().any(char::is_control)
                {
                    return Err("Exchange uncertainty must retain a nonterminal observation and readable reason".into());
                }
                validate_state(previous)? + 1
            }
            state => validate_state(state)?,
        };
        if self.schema_version != 1 || self.revision != expected {
            return Err("Exchange version/revision does not match causal progress".into());
        }
        Ok(())
    }
    fn next(&self, state: FileExchangeState) -> Result<Self> {
        self.validate()?;
        let next = Self {
            revision: self.revision + 1,
            state,
            ..self.clone()
        };
        next.validate()?;
        Ok(next)
    }
    pub fn issue_retire(&self) -> Result<Self> {
        if !matches!(self.state, FileExchangeState::Prepared {}) {
            return Err("Retire already issued; observe, never replay".into());
        }
        self.next(FileExchangeState::RetireIssued {})
    }
    pub fn retired(&self, retained: ImageEvidence) -> Result<Self> {
        if !matches!(self.state, FileExchangeState::RetireIssued {}) {
            return Err("Retire receipt requires original saved intent".into());
        }
        self.next(FileExchangeState::Retired { retained })
    }
    pub fn issue_place(&self) -> Result<Self> {
        let FileExchangeState::Retired { retained } = &self.state else {
            return Err("Place requires saved retire receipt; never replay".into());
        };
        self.next(FileExchangeState::PlaceIssued {
            retained: retained.clone(),
        })
    }
    pub fn installed(&self, installed: ImageEvidence) -> Result<Self> {
        let FileExchangeState::PlaceIssued { retained } = &self.state else {
            return Err("Install receipt requires original saved place intent".into());
        };
        self.next(FileExchangeState::Installed {
            retained: retained.clone(),
            installed,
        })
    }
    pub fn indeterminate(&self, reason: String) -> Result<Self> {
        if matches!(
            self.state,
            FileExchangeState::Installed { .. } | FileExchangeState::Indeterminate { .. }
        ) {
            return Err("Terminal file outcome immutable".into());
        }
        self.next(FileExchangeState::Indeterminate {
            previous: Box::new(self.state.clone()),
            reason,
        })
    }
}

/// Logical set progress only. It grants neither file handles nor a fresh permit.
/// The future transaction ledger must additionally verify baseline/backup/candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileExchangeBatch {
    pub schema_version: u16,
    pub files: Vec<FileExchangeRecord>,
}
impl FileExchangeBatch {
    pub fn validate(&self) -> Result<()> {
        const ORDER: [RuntimeFile; 5] = [
            RuntimeFile::Worker,
            RuntimeFile::Fixture,
            RuntimeFile::Desktop,
            RuntimeFile::Updater,
            RuntimeFile::Manifest,
        ];
        if self.schema_version != 1 || self.files.len() != ORDER.len() {
            return Err("Exchange set must contain four binaries followed by manifest".into());
        }
        let first = &self.files[0].spec;
        let mut identities = std::collections::HashSet::new();
        let mut unfinished = false;
        for (record, role) in self.files.iter().zip(ORDER) {
            record.validate()?;
            let spec = &record.spec;
            if spec.file != role
                || spec.update_id != first.update_id
                || spec.runtime_directory != first.runtime_directory
                || !identities.insert(spec.old.file_identity.as_str())
                || !identities.insert(spec.prepared.file_identity.as_str())
            {
                return Err("Exchange set role/scope/independent file identity differs".into());
            }
            if unfinished && !matches!(record.state, FileExchangeState::Prepared {}) {
                return Err(
                    "Later runtime file or manifest progressed before earlier receipt".into(),
                );
            }
            unfinished |= !matches!(record.state, FileExchangeState::Installed { .. });
        }
        Ok(())
    }
    /// Index is an observation of the next incomplete file, not dispatch permission.
    pub fn next_file(&self) -> Result<Option<usize>> {
        self.validate()?;
        Ok(self
            .files
            .iter()
            .position(|r| !matches!(r.state, FileExchangeState::Installed { .. })))
    }
    pub fn advance(&self, index: usize, next: FileExchangeRecord) -> Result<Self> {
        if self.next_file()? != Some(index) {
            return Err("Only the first incomplete exchange may advance".into());
        }
        let old = &self.files[index];
        if next.spec != old.spec || next.revision != old.revision + 1 {
            return Err("Exchange set requires immutable spec and one causal revision".into());
        }
        let expected = match &next.state {
            FileExchangeState::RetireIssued {} => old.issue_retire()?,
            FileExchangeState::Retired { retained } => old.retired(retained.clone())?,
            FileExchangeState::PlaceIssued { .. } => old.issue_place()?,
            FileExchangeState::Installed { installed, .. } => old.installed(installed.clone())?,
            FileExchangeState::Indeterminate { reason, .. } => old.indeterminate(reason.clone())?,
            _ => return Err("Exchange cannot return to Prepared".into()),
        };
        if expected != next {
            return Err("Exchange transition changed saved move receipts/history".into());
        }
        let mut batch = self.clone();
        batch.files[index] = next;
        batch.validate()?;
        Ok(batch)
    }
}

#[cfg(test)]
mod tests;
