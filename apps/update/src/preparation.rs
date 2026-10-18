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
