//! Readonly image-bound support discovery. Cannot advance any update state.
use crate::artifact::{self, ImageEvidence};
use serde::Serialize;
use std::path::Path;
use winbridge_protocol::{
    update::{UpdaterBuildFlavor, UpdaterRuntimeSupport, UPDATER_RUNTIME_SUPPORT_MARKER},
    ArtifactDescriptor, ArtifactRole,
};
type Result<T> = std::result::Result<T, String>;

pub fn current() -> Result<UpdaterRuntimeSupport> {
    std::hint::black_box(UPDATER_RUNTIME_SUPPORT_MARKER);
    Ok(UpdaterRuntimeSupport {
        schema_version: 1,
        kind: "winbridge_updater_runtime_support".into(),
        query_contract: 1,
        descriptor: winbridge_protocol::artifact_descriptor(ArtifactRole::Updater),
        process: winbridge_platform::infrastructure::stamp(std::process::id())
            .map_err(|e| e.to_string())?,
        updater_journal_schema: crate::journal::UPDATE_SCHEMA_VERSION,
        build_flavor: if cfg!(feature = "update-validation") {
            UpdaterBuildFlavor::Validation
        } else {
            UpdaterBuildFlavor::Standard
        },
        final_state_backup_contract: cfg!(windows).then_some(15),
        runtime_preparation_contract: cfg!(windows).then_some(16),
        runtime_replacement_contract: cfg!(windows).then_some(17),
        // Declare these only when their trusted execution paths actually exist.
        activation_contract: None,
        controller_confirmation_contract: None,
        desktop_update_contract: None,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterSupportInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub image: ImageEvidence,
    pub support: UpdaterRuntimeSupport,
    pub cleanup_verified: bool,
    pub journal_bound: bool,
    pub candidate_verified: bool,
}

fn require_report_binding(
    support: &UpdaterRuntimeSupport,
    descriptor: &ArtifactDescriptor,
    process: &winbridge_protocol::update::SupervisorProcess,
) -> Result<()> {
    let contracts = [
        support.final_state_backup_contract,
        support.runtime_preparation_contract,
        support.runtime_replacement_contract,
        support.activation_contract,
        support.controller_confirmation_contract,
        support.desktop_update_contract,
    ];
    if support.schema_version != 1
        || support.query_contract != 1
        || support.kind != "winbridge_updater_runtime_support"
        || support.descriptor != *descriptor
        || support.process != *process
        || support.updater_journal_schema == 0
        || contracts.into_iter().flatten().any(|version| version == 0)
    {
        return Err("Updater support image/process/contract binding rejected".into());
    }
    Ok(())
}

/// Expected image/descriptor come from the trusted frozen candidate artifact set.
/// None means this exact older image lacks the new query: DO NOT try its flag.
/// The report is data; callers still need durable admission and physical custody.
pub fn inspect(
    expected: &ImageEvidence,
    descriptor: &ArtifactDescriptor,
) -> Result<Option<UpdaterSupportInspection>> {
    if descriptor.role != ArtifactRole::Updater {
        return Err(
            "Updater support query requires the pinned updater role; no process started".into(),
        );
    }
    let pin = artifact::pin(Path::new(&expected.path))?;
    if &pin.evidence != expected {
        return Err("Updater support image bytes/identity changed; no process started".into());
    }
    if !crate::worker_rehearsal::supports(&pin, UPDATER_RUNTIME_SUPPORT_MARKER)? {
        return Ok(None);
    }
    let identity = artifact::query_image(Path::new(&expected.path), ArtifactRole::Updater)?;
    if identity.image != *expected || &identity.descriptor != descriptor {
        return Err("Updater support descriptor/source changed; no support probe started".into());
    }
    let (stdout, stderr, process) =
        artifact::run_probe(&pin, &["--runtime-support".into()], 10000)?;
    let support: UpdaterRuntimeSupport =
        serde_json::from_slice(&stdout).map_err(|_| "Invalid updater support report/fields")?;
    require_report_binding(&support, descriptor, &process)?;
    if !stderr.is_empty() || artifact::inspect_image(Path::new(&expected.path))? != *expected {
        return Err("Updater support image/process/contract binding rejected".into());
    }
    Ok(Some(UpdaterSupportInspection {
        schema_version: 1,
        kind: "updater_support_inspection",
        read_only: true,
        image: pin.evidence,
        support,
        cleanup_verified: true,
        journal_bound: false,
        candidate_verified: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_binding_rejects_stale_process_wrong_build_and_zero_contract_versions() {
        let valid = current().unwrap();
        require_report_binding(&valid, &valid.descriptor, &valid.process).unwrap();
        let mut bad = valid.clone();
        bad.process.birth.push('0');
        assert!(require_report_binding(&bad, &valid.descriptor, &valid.process).is_err());
        bad = valid.clone();
        bad.descriptor.build.build_id.push('0');
        assert!(require_report_binding(&bad, &valid.descriptor, &valid.process).is_err());
        bad = valid.clone();
        bad.activation_contract = Some(0);
        assert!(require_report_binding(&bad, &valid.descriptor, &valid.process).is_err());
        bad = valid.clone();
        bad.updater_journal_schema = 0;
        assert!(require_report_binding(&bad, &valid.descriptor, &valid.process).is_err());
    }
}
