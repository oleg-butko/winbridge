//! A bounded owned-Job crash barrier, absent from normal builds. Never replay IO.
use super::*;
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
    preparation_attempt_id: String,
}
pub(super) fn hold_before_receipt(
    saved: &RuntimeReplacementRecord,
    after: &RuntimeReplacementRecord,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<()> {
    if saved.revision != 3
        || !matches!(
            saved.batch.files[0].state,
            FileExchangeState::RetireIssued {}
        )
    {
        return Ok(());
    }
    let plan = &saved.intent.preparation.plan;
    let parent = plan
        .directory()
        .parent()
        .ok_or("Replacement validation parent missing")?
        .to_owned();
    let input_path = parent.join("runtime-replacement-validation.json");
    if !input_path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    if !cfg!(windows) || winbridge_protocol::build_identity().source_dirty {
        return Err("Replacement validation requires clean native Windows build".into());
    }
    saved.advance(after)?;
    if !matches!(
        after.batch.files[0].state,
        FileExchangeState::Retired { .. }
    ) {
        return Err("Replacement validation requires actual first retired file".into());
    }
    let pin = crate::artifact::pin(&input_path)?;
    if pin.evidence.bytes > 8192 {
        return Err("Replacement validation input too large".into());
    }
    let raw = fs::read(&input_path).map_err(|e| e.to_string())?;
    if crate::sha256(&raw) != pin.evidence.sha256 {
        return Err("Replacement validation input changed".into());
    }
    let input: Input = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if input.schema_version != 1
        || input.kind != "winbridge_runtime_replacement_validation"
        || input.update_id != plan.update_id
        || input.launch_id != plan.launch_id
        || input.owner_id != plan.owner_id
        || input.owner_epoch != plan.owner_epoch
        || input.config_sha256 != plan.config_sha256
        || input.runtime_identity != plan.runtime_identity
        || input.preparation_attempt_id != plan.attempt_id
    {
        return Err("Replacement validation scope rejected".into());
    }
    fence()?;
    let marker = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion":1,"kind":"runtime_replacement_before_receipt","processId":std::process::id(),
        "build":winbridge_protocol::build_identity(),"inputSha256":pin.evidence.sha256,
        "record":saved,"afterEffect":after
    })).map_err(|e|e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(parent.join("runtime-replacement-held.json"))
        .map_err(|e| e.to_string())?;
    file.write_all(&marker)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        fence()?;
        // The original guard and every owned file stay on the caller stack.
        if winbridge_platform::file_identity(&pin.file).map_err(|e| e.to_string())?
            != pin.evidence.file_identity
            || pin.file.metadata().map_err(|e| e.to_string())?.len() != pin.evidence.bytes
        {
            return Err("Replacement validation input identity changed".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err("Owned replacement validation barrier deadline; preserve issued move".into())
}
