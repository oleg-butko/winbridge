//! Trusted previous-runtime archiver. Never replaces or starts runtime files.
use crate::{
    artifact::{ImageEvidence, Pinned},
    journal::{UpdateAuthority, UpdateJournal},
    runtime_baseline::{digest, RuntimeBaselineReceipt},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winbridge_platform::source_files::{directory_identity, require_same_volume, sync_parent};
use winbridge_protocol::{update::*, LoadedWorkerIdentity};
type Result<T> = std::result::Result<T, String>;
pub(crate) const DIRECTORY: &str = "previous-runtime";
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn text(p: &Path) -> Result<String> {
    Ok(p.to_str().ok_or("Preimages path must be UTF-8")?.into())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreimagesIntent {
    pub schema_version: u16,
    pub update_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub launch_id: String,
    pub baseline_sha256: String,
    pub parent_directory: String,
    pub parent_identity: String,
    pub directory: String,
    pub parent_revision: u64,
    pub admitted_at_ms: u64,
    pub available_bytes_before: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreimagesReceipt {
    pub schema_version: u16,
    pub update_id: String,
    pub intent_sha256: String,
    pub parent_revision: u64,
    pub completed_at_ms: u64,
    pub directory_identity: String,
    pub files: Vec<ImageEvidence>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePreimagesSnapshot {
    pub intent: Option<RuntimePreimagesIntent>,
    pub receipt: Option<RuntimePreimagesReceipt>,
}
/// One-use authority. It cannot be cloned/deserialized or recreated from a pending intent.
pub struct RuntimePreimagesPermit {
    pub(crate) intent: RuntimePreimagesIntent,
    baseline: RuntimeBaselineReceipt,
}
pub enum RuntimePreimagesAdmission {
    Existing(Box<RuntimePreimagesSnapshot>),
    Fresh(Box<RuntimePreimagesPermit>),
}
pub(crate) fn parent_path(runtime: &str, id: &str) -> PathBuf {
    Path::new(runtime).join("update").join(id)
}
pub(crate) fn validate_receipt(
    intent: &RuntimePreimagesIntent,
    receipt: &RuntimePreimagesReceipt,
    baseline: &RuntimeBaselineReceipt,
) -> Result<()> {
    let source =
        std::iter::once(&baseline.installed.manifest_file).chain(&baseline.installed.images);
    let mut identities: std::collections::HashSet<_> =
        source.clone().map(|i| i.file_identity.as_str()).collect();
    if receipt.schema_version != 1
        || receipt.update_id != intent.update_id
        || receipt.intent_sha256 != digest(intent)?
        || receipt.parent_revision != intent.parent_revision + 1
        || receipt.completed_at_ms < intent.admitted_at_ms
        || receipt.files.len() != 5
        || receipt.directory_identity.is_empty()
        || receipt.directory_identity.len() > 128
        || receipt.directory_identity == intent.parent_identity
        || receipt.directory_identity == baseline.installed.runtime_identity
    {
        return Err("Runtime preimages receipt/intent/directory binding rejected".into());
    }
    for (file, original) in receipt.files.iter().zip(source) {
        let name = Path::new(&original.path)
            .file_name()
            .ok_or("Preimage source name missing")?;
        if file.path != text(&Path::new(&intent.directory).join(name))?
            || file.bytes != original.bytes
            || file.sha256 != original.sha256
            || file.query_supported != original.query_supported
            || file.file_identity.is_empty()
            || file.file_identity.len() > 128
            || !identities.insert(file.file_identity.as_str())
        {
            return Err(
                "Preimages must contain five independent copies of the exact baseline".into(),
            );
        }
    }
    Ok(())
}
fn current(
    j: &UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    i: &RuntimePreimagesIntent,
) -> Result<()> {
    j.require_runtime_preimages_current(a, s, i).map_err(err)?;
    let baseline = j
        .runtime_baseline(&i.update_id)
        .map_err(err)?
        .receipt
        .ok_or("Verified runtime baseline required")?;
    if digest(&baseline)? != i.baseline_sha256
        || directory_identity(Path::new(&i.parent_directory)).map_err(err)? != i.parent_identity
        || directory_identity(Path::new(&baseline.installed.runtime_directory)).map_err(err)?
            != baseline.installed.runtime_identity
    {
        return Err("Runtime preimages baseline/parent directory changed".into());
    }
    Ok(())
}
fn observe(s: &SupervisorRecord, io: &tokio::runtime::Handle) -> Result<LoadedWorkerIdentity> {
    io.block_on(crate::supervisor::forward_worker(
        s,
        WorkerUpdateControlCommand::ObserveRuntime,
    ))?
    .into_runtime_identity()
}
/// Fresh admission before ACK/dispatch. Repeating completed work only returns history;
/// repeating pending work never authorizes writes or adopts files already present.
pub fn admit(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
) -> Result<RuntimePreimagesAdmission> {
    let cfg = || crate::preparation::configuration(s).map(|_| ());
    admit_with(j, a, s, cfg)
}
fn admit_with(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    mut config: impl FnMut() -> Result<()>,
) -> Result<RuntimePreimagesAdmission> {
    j.check_source_authority(a).map_err(err)?;
    let saved = j.runtime_preimages(&s.spec.update_id).map_err(err)?;
    if saved.receipt.is_some() {
        return Ok(RuntimePreimagesAdmission::Existing(Box::new(saved)));
    }
    if saved.intent.is_some() {
        return Err("Runtime preimages already admitted; inspect only, no replay/adoption".into());
    }
    config()?;
    let baseline = j.runtime_baseline(&s.spec.update_id).map_err(err)?;
    let initial = baseline.intent.ok_or("Runtime baseline intent missing")?;
    let baseline = baseline
        .receipt
        .ok_or("Verified runtime baseline required")?;
    if initial.owner_id != a.owner_id()
        || initial.owner_epoch != a.epoch()
        || initial.launch_id != s.spec.launch_id
    {
        return Err("Baseline is historical; no new preimages authority".into());
    }
    let installed = &baseline.installed;
    config()?;
    let parent = parent_path(&s.spec.runtime_directory, &s.spec.update_id);
    let parent_identity = directory_identity(&parent).map_err(err)?;
    require_same_volume(Path::new(&s.spec.runtime_directory), &parent).map_err(err)?;
    let destination = parent.join(DIRECTORY);
    winbridge_platform::require_safe_path(&destination).map_err(err)?;
    if destination.try_exists().map_err(err)? {
        return Err(
            "Previous runtime destination already exists; not adopted or overwritten".into(),
        );
    }
    let available = fs2::available_space(&parent).map_err(err)?;
    let bytes = std::iter::once(&installed.manifest_file)
        .chain(&installed.images)
        .map(|i| i.bytes)
        .sum::<u64>();
    if available < 6 * 1024 * 1024 * 1024 + bytes {
        return Err(format!(
            "Preimages deferred: preserve 6 GiB plus copy bytes; availableBytes={available}"
        ));
    }
    let update = j.get(&s.spec.update_id).map_err(err)?;
    let intent = RuntimePreimagesIntent {
        schema_version: 1,
        update_id: s.spec.update_id.clone(),
        owner_id: a.owner_id().into(),
        owner_epoch: a.epoch(),
        launch_id: s.spec.launch_id.clone(),
        baseline_sha256: digest(&baseline)?,
        parent_directory: text(&parent)?,
        parent_identity,
        directory: text(&destination)?,
        parent_revision: update.revision + 1,
        admitted_at_ms: crate::journal::now_ms()
            .map_err(err)?
            .max(update.updated_at_ms),
        available_bytes_before: available,
    };
    config()?;
    j.admit_runtime_preimages(a, s, &intent).map_err(err)?;
    Ok(RuntimePreimagesAdmission::Fresh(Box::new(
        RuntimePreimagesPermit { intent, baseline },
    )))
}
pub fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: RuntimePreimagesPermit,
    io: tokio::runtime::Handle,
) -> Result<RuntimePreimagesSnapshot> {
    execute_with(
        j,
        a,
        s,
        permit,
        || crate::preparation::configuration(s).map(|_| ()),
        || observe(s, &io),
    )
}
fn execute_with(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: RuntimePreimagesPermit,
    mut config: impl FnMut() -> Result<()>,
    mut live: impl FnMut() -> Result<LoadedWorkerIdentity>,
) -> Result<RuntimePreimagesSnapshot> {
    let intent = &permit.intent;
    let check = || current(j, a, s, intent);
    check()?;
    config()?;
    let installed = &permit.baseline.installed;
    let pin = crate::installed_set::pin_installed_set(
        Path::new(&installed.runtime_directory),
        &installed.manifest.installation_id,
        &installed.manifest.scope_sha256,
    )?;
    if pin.observation() != installed {
        return Err("Old runtime differs from baseline; preserve intent".into());
    }
    if live()? != permit.baseline.loaded_worker {
        return Err("Loaded worker changed before preimages".into());
    }
    pin.revalidate()?;
    check()?;
    config()?;
    let directory = Path::new(&intent.directory);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory).map_err(err)?;
    sync_parent(directory).map_err(err)?;
    let identity = directory_identity(directory).map_err(err)?;
    let copies: Vec<Pinned> = pin.copy_new_files(directory, || {
        check()?;
        config()?;
        if directory_identity(directory).map_err(err)? != identity {
            return Err("Preimages directory replaced during copy".into());
        }
        Ok(())
    })?;
    let mut names: Vec<_> = std::fs::read_dir(directory)
        .map_err(err)?
        .map(|e| e.map(|e| e.file_name()).map_err(err))
        .collect::<Result<_>>()?;
    let mut expected: Vec<_> = copies
        .iter()
        .map(|p| Path::new(&p.evidence.path).file_name().unwrap().to_owned())
        .collect();
    names.sort();
    expected.sort();
    if names != expected {
        return Err("Preimages generation contains unexpected files; preserve all".into());
    }
    check()?;
    config()?;
    if live()? != permit.baseline.loaded_worker {
        return Err("Loaded worker changed during preimages".into());
    }
    pin.revalidate()?;
    check()?;
    config()?;
    if directory_identity(directory).map_err(err)? != identity {
        return Err("Preimages directory changed before receipt".into());
    }
    let mut final_names: Vec<_> = std::fs::read_dir(directory)
        .map_err(err)?
        .map(|e| e.map(|e| e.file_name()).map_err(err))
        .collect::<Result<_>>()?;
    final_names.sort();
    if final_names != expected {
        return Err("Preimages generation changed during final worker observation".into());
    }
    for copy in &copies {
        if crate::artifact::inspect_image(Path::new(&copy.evidence.path))? != copy.evidence {
            return Err("Preimages copy changed before receipt; preserve all".into());
        }
    }
    check()?;
    config()?;
    let receipt = RuntimePreimagesReceipt {
        schema_version: 1,
        update_id: intent.update_id.clone(),
        intent_sha256: digest(intent)?,
        parent_revision: intent.parent_revision + 1,
        completed_at_ms: crate::journal::now_ms()
            .map_err(err)?
            .max(intent.admitted_at_ms),
        directory_identity: identity,
        files: copies.iter().map(|p| p.evidence.clone()).collect(),
    };
    validate_receipt(intent, &receipt, &permit.baseline)?;
    // `copies` and the original pin remain in scope until this transaction commits.
    j.complete_runtime_preimages(a, s, &receipt).map_err(err)
}

#[cfg(test)]
pub(crate) fn admit_fixture(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    _identity: LoadedWorkerIdentity,
) -> Result<RuntimePreimagesAdmission> {
    admit_with(j, a, s, || Ok(()))
}
#[cfg(test)]
pub(crate) fn execute_fixture(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: RuntimePreimagesPermit,
    mut check: impl FnMut() -> Result<()>,
) -> Result<RuntimePreimagesSnapshot> {
    let identity = p.baseline.loaded_worker.clone();
    execute_with(j, a, s, p, &mut check, || Ok(identity.clone()))
}
