//! Input data for a bounded, readonly query through the pinned old worker image.
use super::ShutdownIntent;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use winbridge_protocol::update::{
    SupervisorProcess, WorkerShutdownInspectionReport, WORKER_SHUTDOWN_INSPECTION_MARKER,
};
use winbridge_protocol::RuntimeImageEvidence;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShutdownInspectionRequest {
    pub schema_version: u16,
    pub intent: ShutdownIntent,
    pub worker_config: RuntimeImageEvidence,
    pub helper_image: RuntimeImageEvidence,
    pub worker_journal_schema: u32,
}
impl ShutdownInspectionRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.intent.validate()?;
        let image = &self.helper_image;
        let config = &self.worker_config;
        let valid =
            |s: &str, max| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control);
        if self.schema_version != 1
            || self.worker_journal_schema == 0
            || image.sha256 != self.intent.binding.loaded_worker.image.sha256
            || image.bytes != self.intent.binding.loaded_worker.image.bytes
            || !image.query_supported
            || !valid(&image.path, 4096)
            || !valid(&image.file_identity, 128)
            || !valid(&config.path, 4096)
            || !valid(&config.file_identity, 128)
            || config.sha256 != self.intent.binding.worker.config_sha256
            || config.bytes == 0
            || config.bytes > 65536
        {
            return Err("Shutdown inspection helper/config differs from immutable intent".into());
        }
        Ok(())
    }
}

fn validate_report(
    request: &ShutdownInspectionRequest,
    sha: &str,
    process: &SupervisorProcess,
    report: &WorkerShutdownInspectionReport,
) -> Result<(), String> {
    request.validate()?;
    if report.schema_version != 1
        || report.kind != "worker_shutdown_inspection"
        || report.request_sha256 != sha
        || report.helper_process != *process
        || report.descriptor != request.intent.binding.loaded_worker.descriptor
        || report.helper_image != request.helper_image
        || report.worker_config != request.worker_config
        || report.state != request.intent.binding.worker_state
        || report.worker_journal_schema != request.worker_journal_schema
        || !report.journal_opened_read_only
        || report.worker_constructed
        || report.scheduler_started
        || report.recovery_marker_changed
        || report.runtime_applied
    {
        return Err(
            "Cold shutdown receipt report does not match its exact readonly helper/request/scope"
                .into(),
        );
    }
    if let Some(receipt) = &report.receipt {
        request.intent.validate_receipt(receipt)?;
    }
    Ok(())
}
/// Trusted readonly query through a SHA/identity-pinned old image. It grants no
/// stop, replacement, startup or migration authority and never resends a stop.
pub fn inspect_saved_receipt(
    input_path: &Path,
    input_sha256: &str,
    request: &ShutdownInspectionRequest,
) -> Result<WorkerShutdownInspectionReport, String> {
    request.validate()?;
    if !winbridge_core::update::valid_hash(input_sha256, &[64]) {
        return Err("Invalid cold inspection input SHA".into());
    }
    let input = crate::artifact::pin(input_path)?;
    if input.evidence.bytes > 65536 || input.evidence.sha256 != input_sha256 {
        return Err("Cold inspection input file differs".into());
    }
    let mut reader = input.file.try_clone().map_err(|e| e.to_string())?;
    reader.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut raw = vec![];
    reader
        .take(65537)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    let saved: ShutdownInspectionRequest =
        serde_json::from_slice(&raw).map_err(|_| "Invalid cold inspection input")?;
    if saved != *request || crate::sha256(&raw) != input_sha256 {
        return Err("Cold inspection file/request changed".into());
    }
    let image = crate::artifact::pin(Path::new(&request.helper_image.path))?;
    if image.evidence != request.helper_image
        || !crate::worker_rehearsal::supports(&image, WORKER_SHUTDOWN_INSPECTION_MARKER)?
    {
        return Err("Unsupported cold shutdown inspection; exact supported old image required, helper not started".into());
    }
    let config = crate::artifact::pin(Path::new(&request.worker_config.path))?;
    let state = super::ShutdownStateObservation::capture(Path::new(
        &request.intent.binding.worker_state.state_directory,
    ))
    .map_err(|e| e.to_string())?;
    if config.evidence != request.worker_config
        || state.observation() != &request.intent.binding.worker_state
    {
        return Err("Cold inspection config/state changed before query".into());
    }
    let (stdout, stderr, process) = crate::artifact::run_probe(
        &image,
        &[
            "--inspect-update-shutdown".into(),
            input.evidence.path.clone(),
            input_sha256.into(),
        ],
        10000,
    )?;
    if !stderr.is_empty() {
        return Err("Cold shutdown inspection emitted stderr; no receipt confirmed".into());
    }
    let report: WorkerShutdownInspectionReport =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid cold shutdown inspection output")?;
    validate_report(request, input_sha256, &process, &report)?;
    state.revalidate().map_err(|e| e.to_string())?;
    for expected in [&input.evidence, &image.evidence, &config.evidence] {
        if crate::artifact::inspect_image(Path::new(&expected.path))? != *expected {
            return Err("Cold inspection pinned input/image/config changed".into());
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_report_is_exact_readonly_history_not_process_exit_or_owner_authority() {
        // Typed metadata only. Actual helper/Job/CLI behavior is tested by the
        // worker integration test and the forthcoming Windows combined gate.
        let (intent, receipt, _) = crate::shutdown::tests::fixture();
        let request = ShutdownInspectionRequest {
            worker_config: RuntimeImageEvidence {
                path: "synthetic-config".into(),
                bytes: 10,
                sha256: intent.binding.worker.config_sha256.clone(),
                file_identity: "config".into(),
                query_supported: false,
            },
            helper_image: intent.binding.loaded_worker.image.clone(),
            worker_journal_schema: 16,
            schema_version: 1,
            intent,
        };
        let process = SupervisorProcess {
            process_id: 87,
            birth: "1234".into(),
            host_id: "b".repeat(64),
            session_id: 1,
        };
        let report = WorkerShutdownInspectionReport {
            schema_version: 1,
            kind: "worker_shutdown_inspection".into(),
            request_sha256: "c".repeat(64),
            descriptor: request.intent.binding.loaded_worker.descriptor.clone(),
            helper_process: process.clone(),
            helper_image: request.helper_image.clone(),
            worker_config: request.worker_config.clone(),
            state: request.intent.binding.worker_state.clone(),
            receipt: Some(Box::new(receipt)),
            worker_journal_schema: 16,
            journal_opened_read_only: true,
            worker_constructed: false,
            scheduler_started: false,
            recovery_marker_changed: false,
            runtime_applied: false,
        };
        validate_report(&request, &report.request_sha256, &process, &report).unwrap();
        let mut missing = report.clone();
        missing.receipt = None;
        validate_report(&request, &report.request_sha256, &process, &missing).unwrap();
        for (pointer, replacement) in [
            ("/requestSha256", serde_json::json!("d".repeat(64))),
            ("/helperProcess/birth", serde_json::json!("1235")),
            (
                "/helperImage/fileIdentity",
                serde_json::json!("other-image"),
            ),
            (
                "/workerConfig/fileIdentity",
                serde_json::json!("other-config"),
            ),
            ("/state/journalIdentity", serde_json::json!("other-journal")),
            ("/receipt/process/birth", serde_json::json!("other-worker")),
            ("/workerJournalSchema", serde_json::json!(17)),
            ("/journalOpenedReadOnly", serde_json::json!(false)),
            ("/workerConstructed", serde_json::json!(true)),
            ("/schedulerStarted", serde_json::json!(true)),
            ("/recoveryMarkerChanged", serde_json::json!(true)),
            ("/runtimeApplied", serde_json::json!(true)),
        ] {
            let mut raw = serde_json::to_value(&report).unwrap();
            *raw.pointer_mut(pointer).unwrap() = replacement;
            let changed = serde_json::from_value(raw).unwrap();
            assert!(
                validate_report(&request, &report.request_sha256, &process, &changed).is_err(),
                "{pointer}"
            );
        }
        let mut raw = serde_json::to_value(&report).unwrap();
        raw["processExited"] = true.into();
        assert!(serde_json::from_value::<WorkerShutdownInspectionReport>(raw).is_err());
    }
}
