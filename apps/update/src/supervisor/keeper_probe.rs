//! Current-child observation only. No SQL, retirement, admission or process creation.
use super::*;
use infrastructure::PinnedProcess;

fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn clock() -> Result<u64> {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_millis();
    if at > winbridge_core::update::MAX_UPDATE_COUNTER as u128 {
        return Err("Keeper probe clock out of range".into());
    }
    Ok(at as u64)
}
fn validate(q: &KeeperProbeRequest) -> Result<()> {
    let b = &q.binding;
    let process = |p: &SupervisorProcess| {
        p.process_id != 0
            && hash(&p.host_id)
            && p.birth
                .parse::<u64>()
                .is_ok_and(|v| v > 0 && v.to_string() == p.birth)
    };
    if q.keeper_probe_version != KEEPER_PROBE_CONTRACT
        || ![
            &q.request_id,
            &b.update_id,
            &b.launch_id,
            &b.generation,
            &b.context.journal_incarnation,
            &b.finalization_command_id,
        ]
        .iter()
        .all(|v| canonical_uuid(v))
        || !hash(&b.finalization_command_sha256)
        || !b.selection_sha256.as_deref().is_some_and(hash)
        || !process(&b.worker)
        || !process(&b.coordinator)
        || b.worker.process_id == b.coordinator.process_id
        || b.worker.host_id != b.coordinator.host_id
        || b.worker.session_id != b.coordinator.session_id
        || b.job_name.is_empty()
        || b.job_name.len() > 1024
        || b.job_name.chars().any(char::is_control)
        || !winbridge_core::config::valid_id(&b.context.instance_id)
    {
        return Err("Invalid keeper probe version, correlation or original binding".into());
    }
    Ok(())
}
/// The OS peer PID comes from accept(), never from the request. Physical checks
/// run only after exact current-child/binding validation. None means no custody.
pub(super) fn respond(
    original: Option<&KeeperBinding>,
    q: Option<&KeeperProbeRequest>,
    peer_pid: u32,
    physical: impl FnOnce(u32, &KeeperBinding) -> Result<()>,
) -> KeeperProbeResponse {
    std::hint::black_box(KEEPER_PROBE_MARKER);
    let result = (|| {
        let q = q.ok_or("Invalid keeper probe frame; no fallback")?;
        validate(q)?;
        let original = original.ok_or("Original keeper custody is unavailable")?;
        if peer_pid != original.worker.process_id || &q.binding != original {
            return Err(
                "Keeper probe requires the actual original Active child and binding".into(),
            );
        }
        physical(peer_pid, original)?;
        Ok(KeeperProbeResult::Observed {
            binding: Box::new(original.clone()),
            observed_at_ms: clock()?,
        })
    })()
    .unwrap_or_else(|message| KeeperProbeResult::Rejected { message });
    KeeperProbeResponse {
        keeper_probe_version: KEEPER_PROBE_CONTRACT,
        request_id: q
            .filter(|q| canonical_uuid(&q.request_id))
            .map(|q| q.request_id.clone()),
        result,
    }
}
fn validate_response(
    q: &KeeperProbeRequest,
    r: &KeeperProbeResponse,
    before: u64,
    after: u64,
) -> Result<()> {
    if r.keeper_probe_version != KEEPER_PROBE_CONTRACT
        || r.request_id.as_ref() != Some(&q.request_id)
    {
        return Err("Keeper probe response changed version or correlation".into());
    }
    match &r.result {
        KeeperProbeResult::Observed {
            binding,
            observed_at_ms,
        } if binding.as_ref() == &q.binding
            && before <= *observed_at_ms
            && *observed_at_ms <= after =>
        {
            Ok(())
        }
        KeeperProbeResult::Rejected { message } => {
            Err(format!("Original keeper refused observation: {message}"))
        }
        _ => Err("Keeper probe response changed original binding or observation clock".into()),
    }
}
/// Fresh observation with exact retained handles, not a serialisable permission.
/// An expired capture cannot be refreshed by deserialising a reply or saved row.
pub struct KeeperProbeCapture {
    binding: KeeperBinding,
    worker: PinnedProcess,
    keeper: PinnedProcess,
    deadline: Instant,
}
impl KeeperProbeCapture {
    pub fn binding(&self) -> &KeeperBinding {
        &self.binding
    }
    pub fn require_current(&self) -> Result<()> {
        if Instant::now() >= self.deadline || std::process::id() != self.binding.worker.process_id {
            return Err("Original keeper observation expired or changed worker process".into());
        }
        self.worker
            .require_job_membership(&self.binding.job_name)
            .map_err(error)?;
        self.keeper
            .require_outside_job(&self.binding.job_name)
            .map_err(error)?;
        if Instant::now() >= self.deadline {
            return Err("Original keeper observation expired during physical checks".into());
        }
        Ok(())
    }
}
/// Read-only, one exchange. The calling process must itself be the original
/// activated worker, verified against its birth/host/session and actual Job.
pub async fn observe_original_keeper(
    directory: &Path,
    binding: &KeeperBinding,
) -> Result<KeeperProbeCapture> {
    let deadline = Instant::now() + ipc::TIMEOUT;
    let before = clock()?;
    let q = KeeperProbeRequest {
        keeper_probe_version: KEEPER_PROBE_CONTRACT,
        request_id: uuid::Uuid::new_v4().to_string(),
        binding: binding.clone(),
    };
    validate(&q)?;
    if std::process::id() != binding.worker.process_id {
        return Err("Only the original activated worker can observe its keeper".into());
    }
    let capture = KeeperProbeCapture {
        binding: binding.clone(),
        worker: PinnedProcess::live(&binding.worker).map_err(error)?,
        keeper: PinnedProcess::live(&binding.coordinator).map_err(error)?,
        deadline,
    };
    capture.require_current()?;
    let endpoint = ipc::endpoint(directory, &binding.launch_id).map_err(error)?;
    let r: KeeperProbeResponse = ipc::call(&endpoint, binding.coordinator.process_id, &q)
        .await
        .map_err(error)?;
    validate_response(&q, &r, before, clock()?)?;
    capture.require_current()?;
    Ok(capture)
}
#[cfg(test)]
mod tests;
