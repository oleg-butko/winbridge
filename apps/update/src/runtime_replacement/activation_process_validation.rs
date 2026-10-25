//! Clean native fixture only: a release never grants execution or renews time.
use crate::activation::{ActivationRecord, ActivationState};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path, time::Duration};
type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema_version: u16,
    kind: String,
    update_id: String,
    launch_id: String,
    owner_id: String,
    owner_epoch: u64,
    attempt_id: String,
    runtime_identity: String,
    updater_config_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Release {
    schema_version: u16,
    kind: String,
    phase: String,
    attempt_id: String,
    input_sha256: String,
    record_sha256: String,
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    use std::io::Read;
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("Activation validation input exceeds limit".into());
    }
    Ok(bytes)
}
pub(super) fn hold(
    record: &ActivationRecord,
    phase: &str,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<()> {
    let source = &record.intent.source;
    let root = Path::new(&source.runtime_directory)
        .parent()
        .ok_or("Activation validation root missing")?;
    let input_path = root.join("activation-process-validation.json");
    if !input_path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    if !cfg!(windows) || winbridge_protocol::build_identity().source_dirty {
        return Err("Activation validation requires a clean native Windows build".into());
    }
    if !matches!(
        (&record.state, phase),
        (ActivationState::Creating {}, "creating") | (ActivationState::Created {}, "created")
    ) {
        return Err("Activation validation phase differs from saved effect prefix".into());
    }
    let pin = crate::artifact::pin(&input_path)?;
    let raw = bounded(&input_path)?;
    if crate::sha256(&raw) != pin.evidence.sha256 {
        return Err("Activation validation input changed".into());
    }
    let input: Input =
        serde_json::from_slice(&raw).map_err(|_| "Activation validation input rejected")?;
    if input.schema_version != 1
        || input.kind != "winbridge_activation_process_validation"
        || input.update_id != source.update_id
        || input.launch_id != source.launch_id
        || input.owner_id != source.owner_id
        || input.owner_epoch != source.owner_epoch
        || input.attempt_id != record.intent.attempt_id
        || input.runtime_identity != source.runtime_identity
        || input.updater_config_sha256 != source.updater_config_sha256
    {
        return Err("Activation validation scope differs".into());
    }
    let mut digest = b"winbridge.activation.barrier.record.v1\n".to_vec();
    digest.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(record).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
    );
    let record_sha = crate::sha256(&digest);
    fence()?;
    let marker = serde_json::json!({"schemaVersion":1,"kind":"activation_process_before_effect","phase":phase,"processId":std::process::id(),
        "build":winbridge_protocol::build_identity(),"input":input,"inputEncoded":String::from_utf8(raw).map_err(|_|"Validation input UTF-8 rejected")?,
        "inputSha256":pin.evidence.sha256,"record":record,"recordSha256":record_sha});
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(format!("activation-process-{phase}.json")))
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    let release_path = root.join(format!("activation-process-{phase}-release.json"));
    loop {
        fence()?; // The original preparation deadline is the only grant window.
        if release_path.try_exists().map_err(|e| e.to_string())? {
            let raw = bounded(&release_path)?;
            let release: Release =
                serde_json::from_slice(&raw).map_err(|_| "Activation release rejected")?;
            if release.schema_version != 1
                || release.kind != "winbridge_activation_process_release"
                || release.phase != phase
                || release.attempt_id != record.intent.attempt_id
                || release.input_sha256 != pin.evidence.sha256
                || release.record_sha256 != record_sha
            {
                return Err("Activation release changed scope; no OS effect".into());
            }
            fence()?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
