//! A one-use driver owns every helper until capture and process-tree cleanup finish.
use super::*;
use crate::{
    artifact::{self, Pinned, ProbeObserver},
    journal::{UpdateAuthority, UpdateJournal},
    UpdateConfig,
};
use std::path::Path;
type Result<T> = std::result::Result<T, String>;
fn current(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    h: &HealthRecord,
) -> Result<()> {
    j.check_source_authority(a).map_err(error)?;
    let u = j.get(&h.intent.update_id).map_err(error)?;
    if h.result.is_some()
        || h.intent.owner_id != a.owner_id()
        || h.intent.owner_epoch != a.epoch()
        || h.intent.launch_id != s.spec.launch_id
        || s.phase != SupervisorPhase::Active
        || u.phase != UpdatePhase::ValidatingCandidate
        || u.revision != h.intent.parent_revision
        || j.health(&h.intent.update_id).map_err(error)?.as_ref() != Some(h)
    {
        return Err("Health owner/root/attempt changed; no further process effects".into());
    }
    crate::preparation::configuration(s)?;
    Ok(())
}
fn source_guard(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    cfg: &UpdateConfig,
    id: &str,
    control: &mut impl HealthControl,
) -> Result<()> {
    let source =
        crate::build::saved_source_guard_for(j, a, cfg, id, UpdatePhase::ValidatingCandidate)?;
    let update = j.get(id).map_err(error)?;
    crate::source::live_lease(control, &update, Some(&source))?;
    let artifacts = j.candidate_artifacts(id).map_err(error)?;
    let build =
        UpdateJournal::inspect_build_report(&cfg.runtime_directory.join("update-state"), id)
            .map_err(error)?;
    if artifacts.source_digest != digest(&source)?
        || artifacts.build_digest != digest(&build.build)?
    {
        return Err("Health source/compiler receipt changed".into());
    }
    crate::verify_source(
        &cfg.source_directory,
        &update.request.source.manifest_sha256,
    )?;
    crate::abort::compiler_empty(&build.build)
}
fn compatibility(
    j: &UpdateJournal,
    s: &SupervisorRecord,
    control: &mut impl HealthControl,
) -> Result<WorkerRuntimeCompatibility> {
    let c = control.compatibility()?;
    c.validate_binding(
        s.spec
            .worker
            .as_ref()
            .ok_or("Health worker binding missing")?,
    )?;
    let baseline = j
        .runtime_baseline(&s.spec.update_id)
        .map_err(error)?
        .receipt
        .ok_or("Health baseline missing")?;
    if c.identity != baseline.loaded_worker
        || c.updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
    {
        return Err("Authenticated live runtime/baseline/updater schema differs".into());
    }
    Ok(c)
}
struct Observer<'a> {
    j: &'a mut UpdateJournal,
    a: &'a UpdateAuthority,
    s: &'a SupervisorRecord,
    h: &'a mut HealthRecord,
    kind: HealthProbeKind,
}
impl Observer<'_> {
    fn save(&mut self, next: HealthRecord, cleanup: bool) -> Result<()> {
        *self.h = self
            .j
            .update_health(self.a, self.s, self.h, next, cleanup)
            .map_err(error)?;
        Ok(())
    }
}
impl ProbeObserver for Observer<'_> {
    fn before_create(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.h)?;
        let mut next = self.h.clone();
        let ctx = next
            .context
            .as_ref()
            .ok_or("Health context missing before creation")?;
        next.probes.push(HealthProbe {
            kind: self.kind,
            job_name: job(&next.intent, self.kind),
            command: command(&next.intent, ctx, self.kind),
            process: None,
            resume_granted: false,
            resume_acknowledged: false,
            cleanup: None,
        });
        self.save(next, false)
    }
    fn before_resume(&mut self, p: &SupervisorProcess, name: &str) -> Result<()> {
        current(self.j, self.a, self.s, self.h)?;
        let mut next = self.h.clone();
        let probe = next
            .probes
            .last_mut()
            .ok_or("Probe intent missing before resume")?;
        if probe.job_name != name || probe.process.is_some() {
            return Err("Probe process intent changed".into());
        }
        probe.process = Some(p.clone());
        probe.resume_granted = true;
        self.save(next, false)
    }
    fn resumed(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.h)?;
        let mut next = self.h.clone();
        next.probes
            .last_mut()
            .ok_or("Probe missing after resume")?
            .resume_acknowledged = true;
        self.save(next, false)
    }
    fn poll(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.h)
    }
    fn finished(&mut self, c: &ProbeCleanup) -> Result<()> {
        let mut next = self.h.clone();
        // A denied before-create callback did not persist a process intent.
        let Some(p) = next.probes.last_mut().filter(|p| p.kind == self.kind) else {
            return if c.no_process_created {
                Ok(())
            } else {
                Err("Unjournaled process cleanup outcome".into())
            };
        };
        if p.process.is_none() {
            p.process = c.process.clone();
        }
        p.cleanup = Some(c.clone());
        self.save(next, true)
    }
}
fn probe(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    h: &mut HealthRecord,
    pin: &Pinned,
    kind: HealthProbeKind,
) -> Result<Vec<u8>> {
    let cmd = command(
        &h.intent,
        h.context.as_ref().ok_or("Health context missing")?,
        kind,
    );
    let name = job(&h.intent, kind);
    let (stdout, stderr, _) = artifact::run_probe_observed(
        pin,
        &cmd.arguments,
        cmd.timeout_ms,
        &name,
        &mut Observer { j, a, s, h, kind },
    )?;
    if !stderr.is_empty() {
        return Err("Health helper wrote stderr; output is not an approval receipt".into());
    }
    Ok(stdout)
}
fn config_valid(h: &HealthRecord, r: &WorkerConfigCheckReport) -> Result<()> {
    let ctx = h.context.as_ref().ok_or("Health context missing")?;
    if r.schema_version != 1
        || r.kind != "worker_config_check"
        || r.descriptor != ctx.artifacts.inspection.artifacts[0].descriptor
        || Some(&r.process) != h.probes[0].process.as_ref()
        || r.config != ctx.worker_config
        || r.worker_config_schema != 1
        || r.instance_id != ctx.compatibility.context.instance_id
        || r.worker_journal_opened
        || r.scheduler_started
        || r.candidate_verified
        || r.runtime_applied
    {
        return Err(
            "Candidate config report differs from pinned process/config/isolation contract".into(),
        );
    }
    Ok(())
}
/// Consumes only a fresh in-memory permit. Failure is terminal or uncertain; never replayed.
pub fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    permit: HealthPermit,
    control: &mut impl HealthControl,
) -> Result<HealthRecord> {
    let mut h = j
        .health(&permit.intent.update_id)
        .map_err(error)?
        .ok_or("Health intent disappeared")?;
    if h.intent != permit.intent
        || h.context.is_some()
        || !h.probes.is_empty()
        || h.result.is_some()
    {
        return Err("Health permit already used or intent changed; inspect only".into());
    }
    let mut config = None;
    let mut rehearsal = None;
    let outcome: Result<()> = (|| {
        current(j, a, s, &h)?;
        let cfg = crate::preparation::configuration(s)?;
        source_guard(j, a, &cfg, &h.intent.update_id, control)?;
        let (inspection, pins) =
            crate::candidate_artifacts::inspect_pinned(&cfg, &h.intent.update_id)?;
        let artifacts = j.candidate_artifacts(&h.intent.update_id).map_err(error)?;
        if inspection != artifacts.inspection {
            return Err("Pinned artifact set differs from admitted receipt".into());
        }
        let worker = pins.first().ok_or("Candidate worker pin missing")?;
        for marker in [
            winbridge_protocol::WORKER_CONFIG_CHECK_MARKER,
            winbridge_protocol::WORKER_REHEARSAL_MARKER,
        ] {
            if !crate::worker_rehearsal::supports(worker, marker)? {
                return Err("Candidate lacks isolated health capability; no helper started".into());
            }
        }
        let config_pin = artifact::pin(Path::new(&h.intent.worker_config_path))?;
        let compat = compatibility(j, s, control)?;
        if config_pin.evidence.sha256 != compat.worker_config_sha256
            || config_pin.evidence.bytes > 65536
            || config_pin.evidence.bytes == 0
        {
            return Err(
                "Pinned worker configuration differs from authenticated live config".into(),
            );
        }
        let next = HealthRecord {
            context: Some(HealthContext {
                compatibility: compat,
                worker_config: config_pin.evidence.clone(),
                artifacts,
            }),
            ..h.clone()
        };
        h = j.update_health(a, s, &h, next, false).map_err(error)?;
        let path = Path::new(&h.intent.directory);
        winbridge_platform::require_safe_path(path).map_err(error)?;
        winbridge_platform::require_local_disk(path).map_err(error)?;
        std::fs::create_dir(path).map_err(error)?;
        winbridge_platform::source_files::sync_parent(path).map_err(error)?;
        let root_identity =
            winbridge_platform::source_files::directory_identity(path).map_err(error)?;
        source_guard(j, a, &cfg, &h.intent.update_id, control)?;
        let report: WorkerConfigCheckReport =
            serde_json::from_slice(&probe(j, a, s, &mut h, worker, HealthProbeKind::Config)?)
                .map_err(|_| "Invalid health config report")?;
        config_valid(&h, &report)?;
        config = Some(report);
        source_guard(j, a, &cfg, &h.intent.update_id, control)?;
        let report: WorkerRehearsalReport =
            serde_json::from_slice(&probe(j, a, s, &mut h, worker, HealthProbeKind::Rehearsal)?)
                .map_err(|_| "Invalid health rehearsal report")?;
        let expected_directory = Path::new(&h.intent.directory).join("worker");
        if Path::new(&report.directory) != expected_directory {
            return Err("Rehearsal reported a path outside its fixed new generation".into());
        }
        if winbridge_platform::source_files::directory_identity(&expected_directory)
            .map_err(error)?
            != report.directory_identity
            || winbridge_platform::source_files::directory_identity(Path::new(&h.intent.directory))
                .map_err(error)?
                != root_identity
        {
            return Err("Health rehearsal/root directory changed".into());
        }
        rehearsal = Some(report);
        source_guard(j, a, &cfg, &h.intent.update_id, control)?;
        let final_compat = compatibility(j, s, control)?;
        current(j, a, s, &h)?;
        let (final_inspection, _final_pins) =
            crate::candidate_artifacts::inspect_pinned(&cfg, &h.intent.update_id)?;
        if final_inspection != h.context.as_ref().unwrap().artifacts.inspection
            || artifact::inspect_image(Path::new(&h.intent.worker_config_path))?
                != config_pin.evidence
        {
            return Err("Candidate/config changed during health check".into());
        }
        let next=HealthRecord{result:Some(HealthResult{outcome:HealthOutcome::Verified,completed_at_ms:now()?,reason:"Конфигурация, IPC нового пустого worker и завершение процессов проверены; runtime не заменён".into(),config:config.clone(),rehearsal:rehearsal.clone(),final_compatibility:Some(final_compat),parent_revision:None}),..h.clone()};
        validate_success(&next)?;
        h = j.accept_health(a, s, &h, next).map_err(error)?;
        // Keep candidate/config pins through the atomic acceptance transaction.
        drop(config_pin);
        drop(pins);
        Ok(())
    })();
    match outcome {
        Ok(()) => Ok(h),
        Err(e) => {
            let certain = h.probes.iter().all(|p| {
                p.cleanup.as_ref().is_some_and(|c| {
                    c.tree_empty
                        && c.capture_complete
                        && (c.creation_confirmed || c.no_process_created)
                })
            });
            let cancelled =
                j.get(&h.intent.update_id).map_err(error)?.phase == UpdatePhase::Aborting;
            let next = HealthRecord {
                result: Some(HealthResult {
                    outcome: if !certain {
                        HealthOutcome::Indeterminate
                    } else if cancelled {
                        HealthOutcome::Cancelled
                    } else {
                        HealthOutcome::Rejected
                    },
                    completed_at_ms: now()?,
                    reason: e.chars().take(1000).collect(),
                    config,
                    rehearsal,
                    final_compatibility: None,
                    parent_revision: None,
                }),
                ..h.clone()
            };
            match j.update_health(a, s, &h, next, true) {
                Ok(_) => Err(e),
                Err(save) => Err(format!(
                    "{e}; health terminal receipt unconfirmed: {save}; preserve barrier"
                )),
            }
        }
    }
}
