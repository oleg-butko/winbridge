//! One fresh permit, exact process-before-resume receipts, no cold replay.
use super::*;
use crate::{
    artifact::{self, ProbeObserver},
    health::HealthControl,
    journal::{UpdateAuthority, UpdateJournal},
};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(error)?
        .as_millis()
        .try_into()
        .map_err(error)
}
fn current(
    j: &UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    r: &CandidateSupportRecord,
) -> Result<()> {
    j.require_candidate_support_current(a, s, r)
        .map_err(error)?;
    let local = crate::updater_support::current()?;
    if s.process.as_ref() != Some(&local.process)
        || local.descriptor != r.intent.executor
        || local.driver_contracts() != r.intent.drivers
    {
        return Err("Support executor image/contracts differ from admitted snapshot".into());
    }
    crate::preparation::configuration(s)?;
    Ok(())
}
fn source_guard(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    r: &CandidateSupportRecord,
    c: &mut impl HealthControl,
) -> Result<WorkerRuntimeCompatibility> {
    current(j, a, s, r)?;
    let cfg = crate::preparation::configuration(s)?;
    let source = crate::build::saved_source_guard_for(
        j,
        a,
        &cfg,
        &r.intent.update_id,
        UpdatePhase::CandidateVerified,
    )?;
    let root = j.get(&r.intent.update_id).map_err(error)?;
    crate::source::live_lease(c, &root, Some(&source))?;
    if digest(&source)? != r.intent.source_sha256 {
        return Err("Support source receipt changed".into());
    }
    crate::verify_source(&cfg.source_directory, &root.request.source.manifest_sha256)?;
    let build = UpdateJournal::inspect_build_report(
        &cfg.runtime_directory.join("update-state"),
        &r.intent.update_id,
    )
    .map_err(error)?;
    crate::abort::compiler_empty(&build.build)?;
    let h = j
        .health(&r.intent.update_id)
        .map_err(error)?
        .ok_or("Support health missing")?;
    crate::health::validate_fresh_success(&h)?;
    crate::health::physically_quiescent(&h)?;
    if crate::drain::health_digest(&h)? != r.intent.health_sha256 {
        return Err("Support health changed".into());
    }
    let live = c.compatibility()?;
    live.validate_binding(
        s.spec
            .worker
            .as_ref()
            .ok_or("Support worker binding missing")?,
    )?;
    let mut normalized = live.clone();
    normalized.observed_at_ms = h
        .context
        .as_ref()
        .ok_or("Support health context missing")?
        .compatibility
        .observed_at_ms;
    if normalized != h.context.as_ref().unwrap().compatibility
        || live.observed_at_ms < r.intent.admitted_at_ms
    {
        return Err("Support live worker/config/schema differs from health".into());
    }
    Ok(live)
}
struct Observer<'a> {
    j: &'a mut UpdateJournal,
    a: &'a UpdateAuthority,
    s: &'a SupervisorRecord,
    r: &'a mut CandidateSupportRecord,
}
impl Observer<'_> {
    fn save(&mut self, mut next: CandidateSupportRecord, cleanup: bool) -> Result<()> {
        next.revision = self.r.revision + 1;
        *self.r = self
            .j
            .update_candidate_support(self.a, self.s, self.r, next, cleanup)
            .map_err(error)?;
        Ok(())
    }
}
impl ProbeObserver for Observer<'_> {
    fn before_create(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.r)?;
        let mut next = self.r.clone();
        next.phase = CandidateSupportPhase::Creating;
        next.probe = Some(CandidateSupportProbe {
            job_name: next.intent.job_name()?,
            command: next.intent.command()?,
            process: None,
            resume_granted: false,
            resume_acknowledged: false,
            cleanup: None,
        });
        self.save(next, false)
    }
    fn before_resume(&mut self, process: &SupervisorProcess, job: &str) -> Result<()> {
        current(self.j, self.a, self.s, self.r)?;
        let mut next = self.r.clone();
        next.phase = CandidateSupportPhase::ResumeGranted;
        let p = next
            .probe
            .as_mut()
            .ok_or("Support creation intent missing")?;
        if p.job_name != job || p.process.is_some() {
            return Err("Support Job/PID changed".into());
        }
        p.process = Some(process.clone());
        p.resume_granted = true;
        self.save(next, false)
    }
    fn resumed(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.r)?;
        let mut next = self.r.clone();
        next.phase = CandidateSupportPhase::Running;
        next.probe
            .as_mut()
            .ok_or("Support resume intent missing")?
            .resume_acknowledged = true;
        self.save(next, false)
    }
    fn poll(&mut self) -> Result<()> {
        current(self.j, self.a, self.s, self.r)
    }
    fn finished(&mut self, cleanup: &ProbeCleanup) -> Result<()> {
        let mut next = self.r.clone();
        let Some(p) = next.probe.as_mut() else {
            return if cleanup.no_process_created {
                Ok(())
            } else {
                Err("Support cleanup lacks saved intent".into())
            };
        };
        if p.process.is_none() {
            p.process = cleanup.process.clone();
        }
        p.cleanup = Some(cleanup.clone());
        next.phase = CandidateSupportPhase::Captured;
        self.save(next, true)
    }
}

pub fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: CandidateSupportPermit,
    c: &mut impl HealthControl,
) -> Result<CandidateSupportRecord> {
    let mut r = j
        .candidate_support(&p.intent.update_id)
        .map_err(error)?
        .ok_or("Support intent disappeared")?;
    if r.intent != p.intent || r.phase != CandidateSupportPhase::Admitted || r.revision != 0 {
        return Err("Support permit already used; inspect only".into());
    }
    let outcome: Result<CandidateSupportResult> = (|| {
        source_guard(j, a, s, &r, c)?;
        let cfg = crate::preparation::configuration(s)?;
        let (set, pins) = crate::candidate_artifacts::inspect_pinned(&cfg, &r.intent.update_id)?;
        let artifacts = j.candidate_artifacts(&r.intent.update_id).map_err(error)?;
        if set != artifacts.inspection || digest(&artifacts)? != r.intent.artifacts_sha256 {
            return Err("Support pinned artifacts differ from frozen set".into());
        }
        let pin = pins
            .get(3)
            .filter(|p| p.evidence == r.intent.image)
            .ok_or("Support updater pin missing")?;
        let config = artifact::pin(std::path::Path::new(&r.intent.worker_config.path))?;
        if config.evidence != r.intent.worker_config {
            return Err("Support worker config changed".into());
        }
        let executor = artifact::pin(std::path::Path::new(&s.spec.image_path))?;
        if executor.evidence.sha256 != r.intent.supervisor_image_sha256
            || executor.evidence.file_identity != s.spec.image_identity
        {
            return Err("Support owner executable changed".into());
        }
        if !crate::worker_rehearsal::supports(pin, UPDATER_RUNTIME_SUPPORT_MARKER)? {
            // Keep the exact set pinned through the terminal no-process receipt.
            let result=CandidateSupportResult{outcome:CandidateSupportOutcome::Unsupported,completed_at_ms:now()?,reason:"Кандидат updater не поддерживает проверку контрактов; процесс не запускался, drain запрещён".into(),report:None,assessment:None,final_worker:None};
            current(j, a, s, &r)?;
            let mut next = r.clone();
            next.phase = CandidateSupportPhase::Finished;
            next.result = Some(result.clone());
            next.revision += 1;
            r = j
                .update_candidate_support(a, s, &r, next, false)
                .map_err(error)?;
            return Ok(result);
        }
        let command = r.intent.command()?;
        let job = r.intent.job_name()?;
        let (stdout, stderr, process) = artifact::run_probe_observed(
            pin,
            &command.arguments,
            command.timeout_ms,
            &job,
            &mut Observer { j, a, s, r: &mut r },
        )?;
        if !stderr.is_empty() {
            return Err("Support query wrote unexpected stderr".into());
        }
        let report: UpdaterRuntimeSupport =
            serde_json::from_slice(&stdout).map_err(|_| "Invalid support report/fields")?;
        crate::updater_support::require_report_binding(&report, &r.intent.descriptor, &process)?;
        let live = source_guard(j, a, s, &r, c)?;
        let (final_set, _final_pins) =
            crate::candidate_artifacts::inspect_pinned(&cfg, &r.intent.update_id)?;
        if final_set != set
            || artifact::inspect_image(std::path::Path::new(&config.evidence.path))?
                != config.evidence
        {
            return Err("Support artifacts/config changed during probe".into());
        }
        let assessment = assess_updater_compatibility_with_drivers(
            &report,
            r.intent.expected_updater_journal_schema,
            &r.intent.drivers,
        );
        let result = CandidateSupportResult {
            outcome: CandidateSupportOutcome::Checked,
            completed_at_ms: now()?,
            reason: if assessment.blockers.is_empty() {
                "Контракты updater совпадают; требуется отдельный допуск drain"
            } else {
                "Сведения updater проверены; есть причины, запрещающие drain"
            }
            .into(),
            report: Some(report),
            assessment: Some(assessment),
            final_worker: Some(live),
        };
        let mut next = r.clone();
        next.phase = CandidateSupportPhase::Finished;
        next.result = Some(result.clone());
        next.revision += 1;
        current(j, a, s, &r)?;
        r = j
            .update_candidate_support(a, s, &r, next, false)
            .map_err(error)?;
        Ok(result)
    })();
    match outcome {
        Ok(_) => Ok(r),
        Err(e) => {
            if r.result.is_some() {
                return Err(e);
            }
            let certain = r.probe.as_ref().is_none_or(super::certain);
            let cancelled =
                j.get(&r.intent.update_id).map_err(error)?.phase == UpdatePhase::Aborting;
            let result = CandidateSupportResult {
                outcome: if !certain {
                    CandidateSupportOutcome::Indeterminate
                } else if cancelled {
                    CandidateSupportOutcome::Cancelled
                } else {
                    CandidateSupportOutcome::Rejected
                },
                completed_at_ms: now()?,
                reason: e.chars().filter(|c| !c.is_control()).take(1000).collect(),
                report: None,
                assessment: None,
                final_worker: None,
            };
            let mut next = r.clone();
            next.phase = CandidateSupportPhase::Finished;
            next.result = Some(result);
            next.revision += 1;
            match j.update_candidate_support(a, s, &r, next, true) {
                Ok(_) => Err(e),
                Err(save) => Err(format!(
                    "{e}; support terminal receipt unconfirmed: {save}; preserve source barrier"
                )),
            }
        }
    }
}
