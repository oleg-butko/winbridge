//! Blocking source preparation driver. The supervisor mailbox remains responsive.
use crate::{
    journal::{UpdateAuthority, UpdateJournal},
    source::{SourceApplyAdmission, SourceControl},
    worker_source::WorkerSourceControl,
    UpdateConfig,
};
use std::{fs, path::Path};
use tokio::runtime::Handle;
use winbridge_protocol::{update::*, WorkerInfo};
type Result<T> = std::result::Result<T, String>;
pub(crate) fn configuration(record: &SupervisorRecord) -> Result<UpdateConfig> {
    let path = Path::new(&record.spec.config_path);
    if crate::sha256(&crate::read(path, 65536)?) != record.spec.config_sha256 {
        return Err("Updater configuration changed; preserve source barrier".into());
    }
    crate::load_config(path)
}
struct Control {
    worker: WorkerSourceControl,
    record: SupervisorRecord,
}
impl SourceControl for Control {
    fn observe_worker(&mut self) -> Result<WorkerInfo> {
        configuration(&self.record)?;
        let info = self.worker.observe_worker()?;
        configuration(&self.record)?;
        Ok(info)
    }
}
/// Called only after a fresh Preparing commit; never resumed from saved Preparing.
pub(crate) fn source(
    journal: &mut UpdateJournal,
    owner: &UpdateAuthority,
    record: &SupervisorRecord,
    io: Handle,
) -> Result<()> {
    journal
        .check_source_authority(owner)
        .map_err(|e| e.to_string())?;
    let cfg = configuration(record)?;
    let packages = cfg
        .source_packages_directory
        .as_ref()
        .ok_or("Source package directory is not configured")?;
    let update = journal
        .get(&record.spec.update_id)
        .map_err(|e| e.to_string())?;
    if update.phase != UpdatePhase::Preparing {
        return Err("Fresh Preparing intent required".into());
    }
    for path in [&cfg.source_directory, &cfg.runtime_directory] {
        let free = fs2::available_space(path).map_err(|e| e.to_string())?;
        if free < 6 * 1024 * 1024 * 1024 {
            return Err(format!("Preparation deferred: less than 6 GiB free at {}; availableBytes={free}; no cache deleted",path.display()));
        }
    }
    let stage = cfg
        .runtime_directory
        .join("update")
        .join(&update.update_id)
        .join("archive");
    crate::archive::stage_archive(
        &packages.join(&update.request.source.archive_name),
        &update.request.source,
        &stage,
    )?;
    configuration(record)?;
    let pending = io
        .block_on(crate::supervisor::forward_worker(
            record,
            WorkerUpdateControlCommand::BeginSource,
        ))?
        .into_lease()?
        .ok_or("Source lease missing")?;
    if pending.phase != WorkerUpdatePhase::SourcePending {
        return Err("Existing worker source attempt is observation only; no source effects".into());
    }
    let held = io
        .block_on(crate::supervisor::forward_worker(
            record,
            WorkerUpdateControlCommand::Advance {
                command: WorkerUpdateCommand {
                    command_id: "preparation-source-grant-v1".into(),
                    expected_revision: pending.revision,
                    action: WorkerUpdateAction::AcquireSource,
                    source_manifest_sha256: None,
                    reason: "Supervisor: пакет проверен; запрошен source grant".into(),
                },
            },
        ))?
        .into_lease()?
        .ok_or("Source grant missing")?;
    if held.phase != WorkerUpdatePhase::SourceHeld {
        return Err("Worker did not grant source effects".into());
    }
    configuration(record)?;
    let binding = record.spec.worker.clone().ok_or("Worker binding missing")?;
    let mut control = Control {
        worker: WorkerSourceControl::new(binding, &cfg.worker_config_path, io)?,
        record: record.clone(),
    };
    crate::source::prepare_source(journal, owner, &cfg, &update.update_id, &mut control)?;
    let SourceApplyAdmission::New(permit) =
        crate::source::begin_source_apply(journal, owner, &cfg, &update.update_id, &mut control)?
    else {
        return Err("Existing source attempt cannot be dispatched again".into());
    };
    permit.apply(journal, owner, &cfg, &mut control)?;
    // Building/SourceHeld is retained; compiler/artifacts/abort are separate drivers.
    configuration(record)?;
    let _ = fs::metadata(stage).map_err(|e| e.to_string())?;
    Ok(())
}

impl crate::build::BuildControl for Control {}
// PowerShell's authorization manager rejects verbatim paths even for a local
// drive. Preserve the checked file/root; change only its command-line spelling.
fn tool_path(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        return Err("Build tool paths must be absolute; drive-relative paths are forbidden".into());
    }
    winbridge_platform::require_local_path_prefix(path).map_err(|e| e.to_string())?;
    let text = path.to_str().ok_or("Build paths must be UTF-8")?;
    #[cfg(windows)]
    if let Some(plain) = text.strip_prefix(r"\\?\") {
        winbridge_platform::require_local_path_prefix(Path::new(plain))
            .map_err(|e| e.to_string())?;
        return Ok(plain.to_owned());
    }
    Ok(text.to_owned())
}
/// Resolve a fixed WinBridge build, never command JSON from an agent.
pub(crate) fn build_command(
    journal: &mut UpdateJournal,
    record: &SupervisorRecord,
) -> Result<(UpdateConfig, winbridge_protocol::ResolvedCommand)> {
    let cfg = configuration(record)?;
    let policy = cfg
        .build
        .as_ref()
        .ok_or("Configure build policy with updater config schema 3")?;
    let source = cfg
        .source_directory
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let runtime = cfg
        .runtime_directory
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let script = source.join("scripts/build-windows-update.ps1");
    let (manifest, sha) = winbridge_workspace_sync::source_tree::read_manifest(&source)?;
    let update = journal
        .get(&record.spec.update_id)
        .map_err(|e| e.to_string())?;
    if sha != update.request.source.manifest_sha256
        || !manifest
            .files
            .iter()
            .any(|f| f.path == "scripts/build-windows-update.ps1")
    {
        return Err("The fixed build script must belong to the pinned source manifest".into());
    }
    winbridge_platform::require_safe_path(&script).map_err(|e| e.to_string())?;
    if crate::supervisor::image_sha256(Path::new(&record.spec.image_path))?
        != record.spec.image_sha256
    {
        return Err("Pinned supervisor verifier image changed; no build".into());
    }
    let cmd = winbridge_protocol::ResolvedCommand {
        executable: tool_path(
            &policy
                .powershell_executable
                .canonicalize()
                .map_err(|e| e.to_string())?,
        )?,
        arguments: vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-File".into(),
            tool_path(&script)?,
            "-SourceDirectory".into(),
            tool_path(&source)?,
            "-ManifestSha256".into(),
            sha,
            "-StagingDirectory".into(),
            tool_path(
                &runtime
                    .join("update")
                    .join(&update.update_id)
                    .join("candidate"),
            )?,
            "-VerifierExecutable".into(),
            tool_path(Path::new(&record.spec.image_path))?,
        ],
        working_directory: source.to_str().ok_or("Build cwd must be UTF-8")?.to_owned(),
        timeout_ms: policy.timeout_ms,
    };
    Ok((cfg, cmd))
}
pub(crate) fn compiler(
    journal: &mut UpdateJournal,
    owner: &UpdateAuthority,
    record: &SupervisorRecord,
    permit: crate::build::BuildPermit,
    io: Handle,
) -> Result<()> {
    let cfg = configuration(record)?;
    let mut control = Control {
        worker: WorkerSourceControl::new(
            record.spec.worker.clone().ok_or("Worker binding missing")?,
            &cfg.worker_config_path,
            io,
        )?,
        record: record.clone(),
    };
    let result = permit.execute(journal, owner, &cfg, &mut control)?;
    if result.phase != UpdateBuildPhase::Completed
        || result.outcome != Some(UpdateBuildOutcome::Succeeded)
        || !result.cleanup_verified
    {
        return Err(format!(
            "Compiler did not complete successfully: {}; preserve source barrier",
            result.reason
        ));
    }
    // This is compiler success only; no candidate/lease/install promotion.
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn powershell_paths_preserve_local_drive_and_reject_unc_device_paths() {
        assert_eq!(
            tool_path(Path::new(r"\\?\K:\source with space\build.ps1")).unwrap(),
            r"K:\source with space\build.ps1"
        );
        assert_eq!(
            tool_path(Path::new(r"K:\source\build.ps1")).unwrap(),
            r"K:\source\build.ps1"
        );
        for p in [
            r"\\server\share\script.ps1",
            r"\\?\UNC\server\share\script.ps1",
            r"\\.\pipe\device",
            r"K:relative",
        ] {
            assert!(tool_path(Path::new(p)).is_err(), "{p}");
        }
    }
}
