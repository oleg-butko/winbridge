//! Test-only actual Main support scenario. Never requests drain or installation.
use super::*;
use winbridge_update::candidate_support::{CandidateSupportOutcome, CandidateSupportPhase};

pub(super) async fn drive(
    weak: Weak<Worker>,
    mut h: SupervisorHandle,
    reg: &Arc<Registration>,
    input: &Input,
) -> Result<()> {
    if input.fault != Fault::None || input.final_backup || input.backup_fault != BackupFault::None {
        return Err("Support validation cannot request shutdown/backup faults".into());
    }
    let id = h.record.spec.update_id.clone();
    let before = h.candidate_support(false).await?;
    if before.update.phase != UpdatePhase::CandidateVerified
        || before.driver_active
        || before
            .candidate_support
            .as_ref()
            .is_none_or(|v| v.phase != CandidateSupportViewPhase::NotAdmitted)
    {
        return Err("Support requires a fresh verified candidate".into());
    }
    // Send once. Wait for committed admission without reading its ACK, then
    // discard the original response. Closing a pipe before the server reads
    // its request could lose the request itself rather than just its ACK.
    let mut request = h.validation_private_client()?;
    request.contract_version = 18;
    request.action = SupervisorAction::CaptureCandidateSupport;
    let mut stream = winbridge_transport::supervisor::connect(
        &h.record.spec.endpoint,
        h.record
            .process
            .as_ref()
            .ok_or("Supervisor process missing")?
            .process_id,
    )
    .await
    .map_err(|e| e.to_string())?;
    winbridge_transport::write_frame(
        &mut stream,
        &serde_json::to_vec(&request).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;
    let admission_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if UpdateJournal::inspect_candidate_support(&reg.directory, &id)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            break;
        }
        if Instant::now() >= admission_deadline {
            return Err("Support request was not admitted; no automatic resend".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(stream);
    let until = Instant::now() + Duration::from_secs(90);
    let after = loop {
        let v = h.candidate_support(false).await?;
        if let Some(error) = &v.driver_error {
            return Err(error.clone());
        }
        if v.candidate_support
            .as_ref()
            .is_some_and(|s| s.phase == CandidateSupportViewPhase::Checked)
            && !v.driver_active
        {
            break v;
        }
        if Instant::now() >= until {
            return Err("Support actor did not finish accepted lost-ACK request".into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let saved = UpdateJournal::inspect_candidate_support(&reg.directory, &id)
        .map_err(|e| e.to_string())?
        .ok_or("Support record missing")?;
    saved.validate()?;
    if saved.phase != CandidateSupportPhase::Finished
        || saved.revision != 5
        || saved.result.as_ref().is_none_or(|r| {
            r.outcome != CandidateSupportOutcome::Checked
                || r.assessment.as_ref().is_none_or(|a| a.blockers.is_empty())
        })
        || after.update != before.update
    {
        return Err(
            "Support query did not retain checked blockers without a root transition".into(),
        );
    }
    let duplicate = h.candidate_support(true).await?;
    if duplicate.candidate_support != after.candidate_support
        || duplicate.driver_active
        || UpdateJournal::inspect_candidate_support(&reg.directory, &id)
            .map_err(|e| e.to_string())?
            .as_ref()
            != Some(&saved)
    {
        return Err("Duplicate changed the saved support observation".into());
    }
    let legacy = h.candidate_health(false).await?;
    if legacy.schema_version != 4 || legacy.candidate_support.is_some() {
        return Err("Support changed the legacy health view".into());
    }
    let worker = weak
        .upgrade()
        .ok_or("Old worker disappeared during support")?;
    let lease = worker
        .store()?
        .active_worker_update()?
        .ok_or("Source lease missing")?;
    let count = reg.shutdown_requests.load(Ordering::Acquire);
    if lease.phase != WorkerUpdatePhase::SourceHeld
        || lease.revision != 1
        || count != 0
        || worker.stop.load(Ordering::Acquire)
    {
        return Err("Support unexpectedly drained/stopped the old worker".into());
    }
    drop(worker);
    publish(
        &input.root,
        "07-support.json",
        &serde_json::json!({
            "schemaVersion":1,"kind":"candidate_support_main_before_cleanup","passed":true,
            "build":build_identity(),"normalMain":true,"normalServe":true,
            "worker":reg.binding,"supervisor":h.record,"before":before,"after":after,
            "record":saved,"duplicate":duplicate,"sourceLease":lease,"requestShutdownCount":count,
            "lostAckSentOnce":true,"legacyHealthFieldAbsent":true,
            "runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false,
            "productionDrainEnforced":false
        }),
    )?;
    // The parent observes the marker only after the complete proof was synced.
    publish(
        &input.root,
        "support-ready.json",
        &serde_json::json!({"attemptId":saved.intent.attempt_id}),
    )?;
    let until = Instant::now() + Duration::from_secs(45);
    while !input
        .root
        .join("support-cleanup.request")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        if Instant::now() >= until {
            return Err("Support fixture cleanup observation deadline".into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Explicit fixture cleanup, not an updater cutover/shutdown.
    let abort = h.abort_before_cutover().await?;
    let worker = weak.upgrade().ok_or("Abort stopped the old worker")?;
    let released = worker.store()?.active_worker_update()?;
    if reg.shutdown_requests.load(Ordering::Acquire) != 0
        || worker.stop.load(Ordering::Acquire)
        || released
            .as_ref()
            .is_some_and(|l| l.phase != WorkerUpdatePhase::Released)
    {
        return Err("Support abort did not release without shutting down worker".into());
    }
    let retained =
        UpdateJournal::inspect_candidate_support(&reg.directory, &id).map_err(|e| e.to_string())?;
    if retained.as_ref() != Some(&saved) {
        return Err("Abort rewrote support history".into());
    }
    publish(
        &input.root,
        "08-support-cleanup.json",
        &serde_json::json!({
            "abort":abort,"supportUnchanged":true,"requestShutdownCount":0,"fixtureCleanupStop":true
        }),
    )?;
    worker.stop();
    Ok(())
}
