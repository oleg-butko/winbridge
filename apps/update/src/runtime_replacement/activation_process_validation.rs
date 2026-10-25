//! Clean native fixture only: a release never grants execution or renews time.
use crate::activation::{ActivationRecord, ActivationState};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use winbridge_protocol::update::WorkerShutdownState;
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
    #[serde(default, skip_serializing_if = "is_false")]
    state_trace_only: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}
struct ValidationInput {
    root: PathBuf,
    pin: crate::artifact::Pinned,
    raw: Vec<u8>,
    input: Input,
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
fn capture(record: &ActivationRecord) -> Result<Option<ValidationInput>> {
    let source = &record.intent.source;
    let root = Path::new(&source.runtime_directory)
        .parent()
        .ok_or("Activation validation root missing")?;
    let input_path = root.join("activation-process-validation.json");
    if !input_path.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    if !cfg!(windows) || winbridge_protocol::build_identity().source_dirty {
        return Err("Activation validation requires a clean native Windows build".into());
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
    Ok(Some(ValidationInput {
        root: root.to_path_buf(),
        pin,
        raw,
        input,
    }))
}
fn record_sha(record: &ActivationRecord) -> Result<String> {
    let mut digest = b"winbridge.activation.barrier.record.v1\n".to_vec();
    digest.extend(
        winbridge_protocol::canonical_evidence_json(
            &serde_json::to_value(record).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(crate::sha256(&digest))
}
fn write_marker(root: &Path, name: &str, marker: &serde_json::Value) -> Result<()> {
    let path = root.join(name);
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(marker).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
pub(super) fn hold(
    record: &ActivationRecord,
    phase: &str,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<()> {
    let Some(ValidationInput {
        root,
        pin,
        raw,
        input,
    }) = capture(record)?
    else {
        return Ok(());
    };
    if !matches!(
        (&record.state, phase),
        (ActivationState::Creating {}, "creating") | (ActivationState::Created {}, "created")
    ) {
        return Err("Activation validation phase differs from saved effect prefix".into());
    }
    // A different explicit fixture mode uses nonblocking lock-boundary traces.
    // It does not claim that the old Creating/Created barriers were repeated.
    if input.state_trace_only {
        return fence();
    }
    let record_sha = record_sha(record)?;
    fence()?;
    let marker = serde_json::json!({"schemaVersion":1,"kind":"activation_process_before_effect","phase":phase,"processId":std::process::id(),
        "build":winbridge_protocol::build_identity(),"input":input,"inputEncoded":String::from_utf8(raw).map_err(|_|"Validation input UTF-8 rejected")?,
        "inputSha256":pin.evidence.sha256,"record":record,"recordSha256":record_sha});
    write_marker(&root, &format!("activation-process-{phase}.json"), &marker)?;
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

#[derive(Clone, Copy)]
pub(super) enum StateBoundary {
    Held,
    Released,
}
impl StateBoundary {
    fn phase(self) -> &'static str {
        match self {
            Self::Held => "before_release",
            Self::Released => "after_release",
        }
    }
    fn lock_available(self) -> bool {
        matches!(self, Self::Released)
    }
}
fn lock_busy(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
        || error.raw_os_error().is_some()
            && error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}
/// A fresh, short-lived kernel probe. The successful probe is dropped before
/// the successor receives its command. It neither creates state nor adopts it.
fn probe_lock(
    expected: &WorkerShutdownState,
    boundary: StateBoundary,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<WorkerShutdownState> {
    let pins = crate::shutdown::ShutdownStatePins::capture(Path::new(&expected.state_directory))
        .map_err(|e| e.to_string())?;
    if pins.observation() != expected {
        return Err("State boundary probe identities differ".into());
    }
    let observed = pins.observation().clone();
    fence()?;
    match (pins.try_lock(), boundary) {
        (Ok(probe), StateBoundary::Released) => {
            probe.revalidate().map_err(|e| e.to_string())?;
            // Explicitly retire this separate handle before the receiver command.
            drop(probe);
        }
        (Err(error), StateBoundary::Held) if lock_busy(&error) => {}
        (Ok(probe), StateBoundary::Held) => {
            drop(probe);
            return Err("Original state lock unexpectedly available before release".into());
        }
        (Err(error), _) => return Err(format!("State boundary lock probe failed: {error}")),
    }
    fence()?;
    Ok(observed)
}
pub(super) fn state_boundary(
    record: &ActivationRecord,
    boundary: StateBoundary,
    mut fence: impl FnMut() -> Result<()>,
) -> Result<()> {
    let Some(ValidationInput {
        root,
        pin,
        raw,
        input,
    }) = capture(record)?
    else {
        return Ok(());
    };
    if !input.state_trace_only
        || record.state != (ActivationState::HandoffIssued {})
        || record.revision != 5
    {
        return Err("State boundary needs the explicit trace-only HandoffIssued fixture".into());
    }
    let state = probe_lock(&record.intent.context.worker_state, boundary, &mut fence)?;
    let observed_at_ms = crate::journal::now_ms().map_err(|e| e.to_string())?;
    let handoff = crate::activation::channel::state_handoff(record)?;
    let marker = serde_json::json!({"schemaVersion":1,"kind":"activation_state_lock_boundary",
        "phase":boundary.phase(),"processId":std::process::id(),"build":winbridge_protocol::build_identity(),
        "input":input,"inputEncoded":String::from_utf8(raw).map_err(|_|"Validation input UTF-8 rejected")?,
        "inputSha256":pin.evidence.sha256,"record":record,"recordSha256":record_sha(record)?,
        "observedState":state,"lockAvailable":boundary.lock_available(),"observedAtMs":observed_at_ms,
        "handoffRecordSha256":handoff.record_sha256});
    fence()?;
    write_marker(
        &root,
        &format!("activation-state-{}.json", boundary.phase()),
        &marker,
    )?;
    fence()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, WorkerShutdownState) {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("journal.sqlite"), b"not opened as SQLite").unwrap();
        fs::write(temp.path().join("worker.lock"), b"").unwrap();
        let pins = crate::shutdown::ShutdownStatePins::capture(temp.path()).unwrap();
        let state = pins.observation().clone();
        (temp, state)
    }
    #[test]
    fn state_boundary_probe_observes_the_exact_lock_without_retaining_a_new_owner() {
        let (_temp, state) = fixture();
        let owner = crate::shutdown::ShutdownStatePins::capture(Path::new(&state.state_directory))
            .unwrap()
            .try_lock()
            .unwrap();
        assert_eq!(
            probe_lock(&state, StateBoundary::Held, || Ok(())).unwrap(),
            state
        );
        assert!(probe_lock(&state, StateBoundary::Released, || Ok(())).is_err());
        drop(owner);
        assert!(probe_lock(&state, StateBoundary::Held, || Ok(())).is_err());
        assert_eq!(
            probe_lock(&state, StateBoundary::Released, || Ok(())).unwrap(),
            state
        );
        // The successful probe cannot keep the successor's lock occupied.
        let successor =
            crate::shutdown::ShutdownStatePins::capture(Path::new(&state.state_directory))
                .unwrap()
                .try_lock()
                .unwrap();
        assert!(probe_lock(&state, StateBoundary::Released, || Ok(())).is_err());
        drop(successor);
    }
    #[test]
    fn state_boundary_probe_rejects_rebinding_stale_fence_and_non_busy_errors() {
        let (_temp, state) = fixture();
        let mut rebound = state.clone();
        rebound.journal_identity.push('x');
        assert!(probe_lock(&rebound, StateBoundary::Released, || Ok(())).is_err());
        assert!(probe_lock(
            &state,
            StateBoundary::Released,
            || Err("Stale owner".into())
        )
        .is_err());
        assert!(!lock_busy(&io::Error::new(
            io::ErrorKind::PermissionDenied,
            "denied"
        )));
        assert!(!lock_busy(&io::Error::other("looks busy")));
        assert!(!lock_busy(&io::Error::from_raw_os_error(2)));
        assert!(lock_busy(&fs2::lock_contended_error()));
        assert!(lock_busy(&io::Error::from(io::ErrorKind::WouldBlock)));
        // A rejected fence did not leave a probe owner behind.
        assert!(probe_lock(&state, StateBoundary::Released, || Ok(())).is_ok());
    }
    #[test]
    fn state_trace_flag_preserves_the_frozen_old_input_and_rejects_fabricated_permissions() {
        let (replacement, initial) = crate::activation::activation_fixture();
        let stages = crate::activation::activation_stages(&initial, &replacement);
        let r = &stages[5];
        let old = serde_json::json!({"schemaVersion":1,"kind":"winbridge_activation_process_validation",
            "updateId":r.intent.source.update_id,"launchId":r.intent.source.launch_id,
            "ownerId":r.intent.source.owner_id,"ownerEpoch":r.intent.source.owner_epoch,
            "attemptId":r.intent.attempt_id,"runtimeIdentity":r.intent.source.runtime_identity,
            "updaterConfigSha256":r.intent.source.updater_config_sha256});
        let input: Input = serde_json::from_value(old.clone()).unwrap();
        assert!(!input.state_trace_only);
        assert_eq!(serde_json::to_value(input).unwrap(), old);
        let mut new = old;
        new["stateTraceOnly"] = true.into();
        let input: Input = serde_json::from_value(new.clone()).unwrap();
        assert!(input.state_trace_only);
        assert_eq!(serde_json::to_value(input).unwrap(), new);
        new["writerGranted"] = true.into();
        assert!(serde_json::from_value::<Input>(new).is_err());
    }
}
