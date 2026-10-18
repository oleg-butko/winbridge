//! Coordinated abort before runtime cutover. No rollback or automatic retry.
use crate::journal::{UpdateAuthority, UpdateJournal};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::runtime::Handle;
use winbridge_protocol::update::*;
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AbortPhase {
    Intent,
    Verified,
    Released,
    Completed,
    Blocked,
}
pub(crate) fn transition(a: AbortPhase, b: AbortPhase) -> bool {
    matches!(
        (a, b),
        (
            AbortPhase::Intent,
            AbortPhase::Verified | AbortPhase::Blocked
        ) | (
            AbortPhase::Verified,
            AbortPhase::Released | AbortPhase::Blocked
        ) | (
            AbortPhase::Released,
            AbortPhase::Completed | AbortPhase::Blocked
        )
    )
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbortSpec {
    pub update_id: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub original_phase: UpdatePhase,
    pub runtime_directory: String,
    pub runtime_identity: String,
    pub source_digest: Option<String>,
    pub build_digest: Option<String>,
    pub accepted_at_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbortRecord {
    pub schema_version: u16,
    pub spec: AbortSpec,
    pub revision: u64,
    pub phase: AbortPhase,
    pub cleanup_verified: bool,
    pub source_sha256: Option<String>,
    pub worker_before: Option<WorkerUpdateLease>,
    pub release_command: Option<WorkerUpdateCommand>,
    pub worker_released: Option<WorkerUpdateLease>,
    pub updated_at_ms: u64,
    pub reason: String,
}
pub struct AbortPermit {
    record: AbortRecord,
}
pub enum AbortAdmission {
    New(Box<AbortPermit>),
    Observed(Box<AbortRecord>),
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_millis()
        .try_into()
        .map_err(err)
}
pub fn admit(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    sup: &SupervisorRecord,
) -> Result<AbortAdmission> {
    j.check_source_authority(a).map_err(err)?;
    let directory = Path::new(&sup.spec.runtime_directory).join("update-state");
    if sup.phase != SupervisorPhase::Active
        || sup.spec.worker.is_none()
        || UpdateJournal::inspect_supervisor(&directory, &sup.spec.launch_id).map_err(err)? != *sup
    {
        return Err("Abort requires the exact active worker-bound supervisor".into());
    }
    match UpdateJournal::inspect_abort(&directory, &sup.spec.update_id) {
        Ok(r) => return Ok(AbortAdmission::Observed(Box::new(r))),
        Err(e) if e.code == crate::journal::JournalErrorCode::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    let saved = UpdateJournal::preparation_snapshot(&directory, &sup.spec.update_id, false, None)
        .map_err(err)?;
    if sup.owner_id.as_deref() != Some(a.owner_id())
        || sup.owner_epoch != Some(a.epoch())
        || saved.source.as_ref().is_some_and(|s| {
            s.worker_source_spec.updater_owner_id != a.owner_id()
                || s.worker_source_spec.updater_epoch != a.epoch()
                || !matches!(
                    s.phase,
                    SourceTransactionPhase::Prepared | SourceTransactionPhase::Verified
                )
        })
        || saved.build.as_ref().is_some_and(|b| {
            b.spec.owner_id != a.owner_id()
                || b.spec.owner_epoch != a.epoch()
                || b.phase != UpdateBuildPhase::Completed
                || !b.cleanup_verified
        })
    {
        return Err("Unfinished/unknown source or compiler outcome requires manual inspection; no lease release".into());
    }
    let (source_digest, build_digest) = j.abort_evidence(&sup.spec.update_id).map_err(err)?;
    let (record, fresh) = j
        .admit_abort(
            a,
            AbortSpec {
                update_id: sup.spec.update_id.clone(),
                owner_id: a.owner_id().into(),
                owner_epoch: a.epoch(),
                original_phase: saved.update.phase,
                runtime_directory: sup.spec.runtime_directory.clone(),
                runtime_identity: winbridge_platform::source_files::directory_identity(Path::new(
                    &sup.spec.runtime_directory,
                ))
                .map_err(err)?,
                source_digest,
                build_digest,
                accepted_at_ms: now()?,
            },
        )
        .map_err(err)?;
    Ok(if fresh {
        AbortAdmission::New(Box::new(AbortPermit { record }))
    } else {
        AbortAdmission::Observed(Box::new(record))
    })
}
fn guarded(j: &mut UpdateJournal, a: &UpdateAuthority, r: &AbortRecord) -> Result<()> {
    j.check_source_authority(a).map_err(err)?;
    if a.owner_id() != r.spec.owner_id
        || a.epoch() != r.spec.owner_epoch
        || j.get(&r.spec.update_id).map_err(err)?.phase != UpdatePhase::Aborting
        || winbridge_platform::source_files::directory_identity(Path::new(
            &r.spec.runtime_directory,
        ))
        .map_err(err)?
            != r.spec.runtime_identity
        || j.abort_evidence(&r.spec.update_id).map_err(err)?
            != (r.spec.source_digest.clone(), r.spec.build_digest.clone())
    {
        return Err("Abort source/build/root/owner evidence changed; preserve barriers".into());
    }
    Ok(())
}
fn compiler_empty(build: &UpdateBuildRecord) -> Result<()> {
    if build.phase != UpdateBuildPhase::Completed || !build.cleanup_verified {
        return Err("Compiler cleanup not confirmed".into());
    }
    if let Some(process) = &build.process {
        let exited = match winbridge_platform::infrastructure::matches(process) {
            Ok(value) => !value,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    || cfg!(windows) && e.raw_os_error() == Some(87) =>
            {
                true
            }
            Err(e) => return Err(format!("Compiler root observation unavailable: {e}")),
        };
        if !exited {
            return Err("Compiler root still alive".into());
        }
        if let Some(job) = &build.spec.job_name {
            let observation = winbridge_platform::process_identity::observe_process(
                &winbridge_platform::process_identity::ProcessIdentity {
                    job_name: job.clone(),
                    host_id: process.host_id.clone(),
                    session_id: process.session_id,
                    process_id: process.process_id,
                    creation_time: process.birth.parse().map_err(err)?,
                },
            );
            if !matches!(
                observation,
                winbridge_platform::process_identity::ProcessObservation::Empty(_)
            ) {
                return Err("Compiler physical Job is not confirmed empty".into());
            }
        }
    }
    for (name, output) in [("stdout", &build.stdout), ("stderr", &build.stderr)] {
        if let Some(output) = output {
            let path = Path::new(&build.spec.runtime_directory)
                .join("update")
                .join(&build.spec.update_id)
                .join("compiler")
                .join(&build.spec.attempt_id)
                .join(format!("{name}.bin"));
            winbridge_platform::require_safe_path(&path).map_err(err)?;
            let bytes = crate::read(&path, crate::build::MAX_OUTPUT_BYTES)?;
            if bytes.len() as u64 != output.retained_bytes || crate::sha256(&bytes) != output.sha256
            {
                return Err("Compiler retained output proof changed".into());
            }
        }
    }
    Ok(())
}
impl AbortPermit {
    pub fn record(&self) -> &AbortRecord {
        &self.record
    }
    pub fn execute(
        self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        sup: &SupervisorRecord,
        io: Handle,
    ) -> Result<AbortRecord> {
        let mut r = self.record;
        let outcome = (|| {
            guarded(j, a, &r)?;
            let directory = Path::new(&r.spec.runtime_directory).join("update-state");
            if UpdateJournal::inspect_abort(&directory, &r.spec.update_id).map_err(err)? != r
                || r.phase != AbortPhase::Intent
            {
                return Err("Abort already attempted; observe only".into());
            }
            let view =
                UpdateJournal::preparation_snapshot(&directory, &r.spec.update_id, false, None)
                    .map_err(err)?;
            if let Some(build) = &view.build {
                compiler_empty(build)?;
            }
            let before = io
                .block_on(crate::supervisor::forward_worker(
                    sup,
                    WorkerUpdateControlCommand::Observe,
                ))?
                .into_lease()?;
            if (view.source.is_some() || view.build.is_some()) && before.is_none() {
                return Err("Expected worker source lease is missing; preserve barriers".into());
            }
            let mut source_sha = None;
            if let Some(source) = &view.source {
                if winbridge_platform::source_files::directory_identity(Path::new(
                    &source.source_directory,
                ))
                .map_err(err)?
                    != source.source_directory_identity
                {
                    return Err("Source root changed before abort".into());
                }
                let sha = if source.phase == SourceTransactionPhase::Verified {
                    &view.update.request.source.manifest_sha256
                } else if source.phase == SourceTransactionPhase::Prepared {
                    &view.update.request.expected.baseline_manifest_sha256
                } else {
                    return Err("Source outcome cannot be released".into());
                };
                crate::verify_source(Path::new(&source.source_directory), sha)?;
                source_sha = Some(sha.clone());
            } else if let Some(lease) = &before {
                if lease.phase != WorkerUpdatePhase::SourcePending {
                    crate::verify_source(
                        Path::new(&lease.spec.source_directory),
                        &view.update.request.expected.baseline_manifest_sha256,
                    )?;
                    source_sha = Some(
                        view.update
                            .request
                            .expected
                            .baseline_manifest_sha256
                            .clone(),
                    );
                }
            }
            if before
                .as_ref()
                .is_some_and(|l| l.phase == WorkerUpdatePhase::SourcePending)
            {
                source_sha = None;
            }
            let command = before
                .as_ref()
                .filter(|l| l.phase != WorkerUpdatePhase::Released)
                .map(|l| WorkerUpdateCommand {
                    command_id: format!("abort-release-{}", r.spec.update_id),
                    expected_revision: l.revision,
                    action: WorkerUpdateAction::Release,
                    source_manifest_sha256: source_sha.clone(),
                    reason:
                        "Подтверждена отмена до установки; освобождение только своей worker lease"
                            .into(),
                });
            guarded(j, a, &r)?;
            let mut ready = r.clone();
            ready.phase = AbortPhase::Verified;
            ready.cleanup_verified = true;
            ready.source_sha256 = source_sha;
            ready.worker_before = before.clone();
            ready.release_command = command.clone();
            ready.reason = "Source и compiler cleanup проверены до worker release".into();
            r = j.advance_abort(a, &r, ready).map_err(err)?;
            guarded(j, a, &r)?;
            let released = if let Some(command) = command {
                match io.block_on(crate::supervisor::forward_worker(
                    sup,
                    WorkerUpdateControlCommand::Advance { command },
                )) {
                    Ok(reply) => reply.into_lease()?,
                    Err(original) => {
                        let observed = io
                            .block_on(crate::supervisor::forward_worker(
                                sup,
                                WorkerUpdateControlCommand::Observe,
                            ))?
                            .into_lease()?;
                        if observed
                            .as_ref()
                            .is_none_or(|l| l.phase != WorkerUpdatePhase::Released)
                        {
                            return Err(format!("Worker release outcome unknown: {original}"));
                        }
                        observed
                    }
                }
            } else {
                before
            };
            let mut acknowledged = r.clone();
            acknowledged.phase = AbortPhase::Released;
            acknowledged.worker_released = released;
            acknowledged.reason =
                "Worker release подтверждён; final metadata ещё не сохранены".into();
            r = j.advance_abort(a, &r, acknowledged).map_err(err)?;
            j.finish_abort(
                a,
                &r,
                true,
                "Операция отменена до замены runtime; worker lease освобождена",
            )
            .map_err(err)
        })();
        match outcome {
            Ok(r) => Ok(r),
            Err(error) => {
                let reason=format!("Abort остановлен: {error}; сохранить journals/guard, требуется ручная проверка");
                match j.finish_abort(a, &r, false, &reason) {
                    Ok(_) => Err(error),
                    Err(e) => Err(format!("{error}; abort failure receipt unconfirmed: {e}")),
                }
            }
        }
    }
}
