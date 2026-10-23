//! Private bounded barrier for an owned native test Job. Absent from normal builds.
use super::*;
use crate::artifact::{self, Pinned};
use std::{
    fs,
    io::Write,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema_version: u16,
    kind: String,
    update_id: String,
    launch_id: String,
    owner_id: String,
    owner_epoch: u64,
    config_sha256: String,
    runtime_identity: String,
}
pub(super) fn hold_before_receipt(
    r: &RuntimePreparationRecord,
    index: usize,
    copy: &Pinned,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<()> {
    if index != 0 {
        return Ok(());
    }
    let parent = r
        .plan
        .directory()
        .parent()
        .ok_or("Validation parent missing")?
        .to_path_buf();
    let input_path = parent.join("runtime-preparation-validation.json");
    if !input_path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    if !cfg!(windows) || winbridge_protocol::build_identity().source_dirty {
        return Err("Preparation validation requires clean native Windows build".into());
    }
    let pin = artifact::pin(&input_path)?;
    if pin.evidence.bytes > 8192 {
        return Err("Preparation validation input too large".into());
    }
    let raw = fs::read(&input_path).map_err(|e| e.to_string())?;
    if crate::sha256(&raw) != pin.evidence.sha256 {
        return Err("Validation input changed".into());
    }
    let input: Input = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if input.schema_version != 1
        || input.kind != "winbridge_runtime_preparation_validation"
        || input.update_id != r.plan.update_id
        || input.launch_id != r.plan.launch_id
        || input.owner_id != r.plan.owner_id
        || input.owner_epoch != r.plan.owner_epoch
        || input.config_sha256 != r.plan.config_sha256
        || input.runtime_identity != r.plan.runtime_identity
        || r.revision != 3
        || !matches!(r.files[0], PreparationFileState::Issued {})
    {
        return Err("Preparation validation scope rejected".into());
    }
    fence()?;
    let marker = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion":1,"kind":"runtime_preparation_before_receipt",
        "processId":std::process::id(),"build":winbridge_protocol::build_identity(),
        "inputSha256":pin.evidence.sha256,"record":r,"copiedFile":copy.evidence
    }))
    .map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(parent.join("runtime-preparation-held.json"))
        .map_err(|e| e.to_string())?;
    file.write_all(&marker)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        fence()?;
        // Native no-write/no-delete handles stay live across the barrier. Avoid
        // repeatedly hashing whole binaries while waiting for the owned kill.
        for held in [copy, &pin] {
            if winbridge_platform::file_identity(&held.file).map_err(|e| e.to_string())?
                != held.evidence.file_identity
                || held.file.metadata().map_err(|e| e.to_string())?.len() != held.evidence.bytes
            {
                return Err("Preparation validation held file changed".into());
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err("Owned preparation validation barrier deadline; preserve issued copy".into())
}
