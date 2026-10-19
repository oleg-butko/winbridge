//! Known installed bytes and the loaded worker binding. No install/write authority.
use crate::artifact::{ImageEvidence, Pinned};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winbridge_platform::source_files::directory_identity;
pub use winbridge_protocol::LoadedWorkerIdentity;
use winbridge_protocol::{ArtifactDescriptor, ArtifactRole};
type Result<T> = std::result::Result<T, String>;
pub const MANIFEST_NAME: &str = "winbridge-installed-set.json";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const FILES: [(&str, ArtifactRole); 4] = [
    ("winbridge-worker.exe", ArtifactRole::Worker),
    ("winbridge-fixture.exe", ArtifactRole::Fixture),
    ("winbridge-desktop.exe", ArtifactRole::Desktop),
    ("winbridge-update.exe", ArtifactRole::Updater),
];
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledArtifact {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub descriptor: ArtifactDescriptor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSetManifest {
    pub schema_version: u16,
    pub kind: String,
    pub set_id: String,
    pub installation_id: String,
    pub scope_sha256: String,
    pub artifacts: Vec<InstalledArtifact>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSetObservation {
    pub schema_version: u16,
    pub kind: String,
    pub read_only: bool,
    pub runtime_directory: String,
    pub runtime_identity: String,
    pub manifest_file: ImageEvidence,
    pub manifest: InstalledSetManifest,
    pub images: Vec<ImageEvidence>,
}
/// Keep all five Windows handles open until the caller explicitly drops this guard.
/// They deny write/delete; an observation alone never grants later replacement.
pub struct PinnedInstalledSet {
    observation: InstalledSetObservation,
    _manifest: Pinned,
    _images: Vec<Pinned>,
}
pub(crate) fn valid_manifest(
    m: &InstalledSetManifest,
    installation: &str,
    scope: &str,
) -> Result<()> {
    if m.schema_version != 1
        || m.kind != "winbridge_installed_set"
        || uuid::Uuid::parse_str(&m.set_id).is_err()
        || uuid::Uuid::parse_str(installation).is_err()
        || m.installation_id != installation
        || m.scope_sha256 != scope
        || !winbridge_core::update::valid_hash(scope, &[64])
        || m.artifacts.len() != 4
    {
        return Err("Installed-set manifest version/installation/scope rejected; bootstrap or inspect, never adopt current files".into());
    }
    let worker = &m.artifacts[0].descriptor.build;
    for (a, (name, role)) in m.artifacts.iter().zip(FILES) {
        let d = &a.descriptor;
        let b = &d.build;
        if a.name != name
            || a.bytes == 0
            || a.bytes > 256 * 1024 * 1024
            || !winbridge_core::update::valid_hash(&a.sha256, &[64])
            || d.schema_version != 1
            || d.kind != "winbridge_artifact"
            || d.query_contract != 1
            || d.role != role
            || d.target_os != std::env::consts::OS
            || d.target_arch != std::env::consts::ARCH
            || b.source_dirty
            || b.version.is_empty()
            || b.version.len() > 128
            || b.version != worker.version
            || b.build_id.is_empty()
            || b.build_id.len() > 128
            || b.source_commit
                .as_ref()
                .is_none_or(|s| !winbridge_core::update::valid_hash(s, &[40, 64]))
            || b.source_manifest_sha256
                .as_ref()
                .is_none_or(|s| !winbridge_core::update::valid_hash(s, &[64]))
            || b.source_commit != worker.source_commit
            || b.source_manifest_sha256 != worker.source_manifest_sha256
        {
            return Err(format!(
                "Installed-set role/source/build/file evidence rejected: {name}"
            ));
        }
    }
    Ok(())
}
pub fn pin_installed_set(
    runtime: &Path,
    installation: &str,
    scope: &str,
) -> Result<PinnedInstalledSet> {
    let identity = directory_identity(runtime).map_err(err)?;
    let runtime = runtime.canonicalize().map_err(err)?;
    let path = runtime.join(MANIFEST_NAME);
    if !path.try_exists().map_err(err)? {
        return Err(format!("Bootstrap required: known installed-set manifest missing: {}; current binaries are not adopted automatically",path.display()));
    }
    let manifest_pin = crate::artifact::pin(&path)?;
    if manifest_pin.evidence.bytes > MAX_MANIFEST_BYTES {
        return Err("Installed-set manifest exceeds byte limit".into());
    }
    let bytes = crate::read(&path, MAX_MANIFEST_BYTES)?;
    if crate::sha256(&bytes) != manifest_pin.evidence.sha256 {
        return Err("Installed-set manifest changed while reading".into());
    }
    let manifest: InstalledSetManifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Installed-set manifest JSON rejected: {e}"))?;
    valid_manifest(&manifest, installation, scope)?;
    let mut pins = vec![];
    for a in &manifest.artifacts {
        let pin = crate::artifact::pin(&runtime.join(&a.name))?;
        if pin.evidence.bytes != a.bytes
            || pin.evidence.sha256 != a.sha256
            || !pin.evidence.query_supported
        {
            return Err(format!("Installed runtime differs from its known manifest: {}; no update/automatic adoption",a.name));
        }
        pins.push(pin);
    }
    let observation = InstalledSetObservation {
        schema_version: 1,
        kind: "installed_set_observation".into(),
        read_only: true,
        runtime_directory: runtime.to_str().ok_or("Runtime path must be UTF-8")?.into(),
        runtime_identity: identity,
        manifest_file: manifest_pin.evidence.clone(),
        manifest,
        images: pins.iter().map(|p| p.evidence.clone()).collect(),
    };
    let result = PinnedInstalledSet {
        observation,
        _manifest: manifest_pin,
        _images: pins,
    };
    result.revalidate()?;
    Ok(result)
}
impl PinnedInstalledSet {
    pub fn observation(&self) -> &InstalledSetObservation {
        &self.observation
    }
    pub fn revalidate(&self) -> Result<()> {
        let o = &self.observation;
        if directory_identity(Path::new(&o.runtime_directory)).map_err(err)? != o.runtime_identity
            || crate::artifact::inspect_image(Path::new(&o.manifest_file.path))? != o.manifest_file
        {
            return Err("Installed runtime directory/manifest identity changed".into());
        }
        for image in &o.images {
            if crate::artifact::inspect_image(Path::new(&image.path))? != *image {
                return Err("Installed runtime image identity/content changed".into());
            }
        }
        Ok(())
    }
    /// Derive the descriptor from this process's compiled code, never a caller DTO.
    /// Caller retains this guard for the whole worker lifetime. Control exposure
    /// and bootstrap manifest publication are separate, not enabled by this API.
    pub fn bind_running_worker(&self) -> Result<LoadedWorkerIdentity> {
        self.revalidate()?;
        let exe = std::env::current_exe()
            .map_err(err)?
            .canonicalize()
            .map_err(err)?;
        let expected = PathBuf::from(&self.observation.images[0].path);
        let descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Worker);
        let same_path = if cfg!(windows) {
            exe.to_str()
                .zip(expected.to_str())
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        } else {
            exe == expected
        };
        if !same_path || descriptor != self.observation.manifest.artifacts[0].descriptor {
            return Err("Loaded worker path/build does not match the known installed set; no runtime authority".into());
        }
        let actual = crate::artifact::inspect_image(&exe)?;
        let known = &self.observation.images[0];
        if actual.file_identity != known.file_identity
            || actual.bytes != known.bytes
            || actual.sha256 != known.sha256
        {
            return Err("Running worker executable identity differs from its pinned set".into());
        }
        Ok(LoadedWorkerIdentity {
            schema_version: 1,
            kind: "loaded_worker_identity".into(),
            process: winbridge_platform::infrastructure::stamp(std::process::id()).map_err(err)?,
            runtime_identity: self.observation.runtime_identity.clone(),
            set_id: self.observation.manifest.set_id.clone(),
            manifest_sha256: self.observation.manifest_file.sha256.clone(),
            descriptor,
            image: self.observation.images[0].clone(),
        })
    }
}
#[cfg(test)]
#[path = "installed_set_tests.rs"]
mod tests;
