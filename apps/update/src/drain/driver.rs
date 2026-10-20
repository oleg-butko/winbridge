//! One owned driver; logical observations never substitute for command receipts.
use super::*;
use crate::journal::{UpdateAuthority, UpdateJournal};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use winbridge_protocol::WorkerRuntimeCompatibility;

#[derive(Clone, Default)]
pub(crate) struct Telemetry(Arc<Mutex<Option<WorkerDrainObservation>>>);
impl Telemetry {
    pub(crate) fn sample(&self) -> Result<Option<WorkerDrainObservation>> {
        self.0
            .lock()
            .map(|v| v.clone())
            .map_err(|_| "Drain telemetry lock poisoned".into())
    }
    fn save(&self, value: WorkerDrainObservation) -> Result<()> {
        *self.0.lock().map_err(|_| "Drain telemetry lock poisoned")? = Some(value);
        Ok(())
    }
}
pub(crate) trait Control {
    fn compatibility(&mut self) -> Result<WorkerRuntimeCompatibility>;
    fn advance(&mut self, command: WorkerUpdateCommand) -> Result<WorkerUpdateLease>;
    fn receipt(&mut self, command: &WorkerUpdateCommand) -> Result<Option<WorkerUpdateLease>>;
    fn observation(&mut self) -> Result<WorkerDrainObservation>;
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn current(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: &DrainPermit,
) -> Result<()> {
    j.require_drain_current(a, s, p).map_err(error)
}
fn compatibility(j: &UpdateJournal, s: &SupervisorRecord, c: &mut impl Control) -> Result<()> {
    let live = c.compatibility()?;
    live.validate_binding(
        s.spec
            .worker
            .as_ref()
            .ok_or("Drain worker binding missing")?,
    )?;
    let baseline = j
        .runtime_baseline(&s.spec.update_id)
        .map_err(error)?
        .receipt
        .ok_or("Drain baseline missing")?;
    let h = j
        .health(&s.spec.update_id)
        .map_err(error)?
        .ok_or("Drain health missing")?;
    let saved = &h
        .context
        .as_ref()
        .ok_or("Drain health context missing")?
        .compatibility;
    // Fresh timestamps differ; all compatibility and loaded-runtime fields must agree.
    let mut observed = live.clone();
    observed.observed_at_ms = saved.observed_at_ms;
    if live.identity != baseline.loaded_worker
        || observed != *saved
        || live.updater_journal_schema != crate::journal::UPDATE_SCHEMA_VERSION
    {
        return Err("Drain live runtime/config/schema changed since health; preserve lease".into());
    }
    Ok(())
}
fn evidence(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: &DrainPermit,
) -> Result<()> {
    current(j, a, s, p)?;
    let cfg = crate::preparation::configuration(s)?;
    let source =
        crate::build::saved_source_guard_for(j, a, &cfg, &s.spec.update_id, UpdatePhase::Draining)?;
    if source_digest(&source)? != p.intent.binding.source_sha256 {
        return Err("Drain source evidence changed".into());
    }
    crate::verify_source(
        &cfg.source_directory,
        &source.worker_source_spec.target_manifest_sha256,
    )?;
    let h = j
        .health(&s.spec.update_id)
        .map_err(error)?
        .ok_or("Drain health missing")?;
    crate::health::validate_fresh_success(&h)?;
    crate::health::physically_quiescent(&h)?;
    // Build inspection below is journal-bound and includes retained output proof.
    let view = UpdateJournal::preparation_snapshot(
        &cfg.runtime_directory.join("update-state"),
        &s.spec.update_id,
        true,
        None,
    )
    .map_err(error)?;
    crate::abort::compiler_empty(
        view.build
            .as_ref()
            .ok_or("Drain compiler receipt missing")?,
    )?;
    current(j, a, s, p)
}

/// Receipt lookup follows a failed exchange exactly once. The sequential worker
/// control server processes it after the original callback; absence still grants
/// no retry. All failures retain the issued intent and the worker barrier.
fn exchange(
    control: &mut impl Control,
    command: &WorkerUpdateCommand,
) -> Result<WorkerUpdateLease> {
    match control.advance(command.clone()) {
        Ok(receipt) => Ok(receipt),
        Err(original) => match control.receipt(command) {
            Ok(Some(receipt)) => Ok(receipt),
            Ok(None) => Err(format!(
                "{original}; exact command receipt not observed; no resend"
            )),
            Err(query) => Err(format!(
                "{original}; exact receipt lookup failed: {query}; no resend"
            )),
        },
    }
}
fn deliver(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: &DrainPermit,
    ordinal: u8,
    c: &mut impl Control,
) -> Result<()> {
    current(j, a, s, p)?;
    crate::preparation::configuration(s)?;
    let DrainIssue::Fresh(permit) = j.issue_drain_command(a, s, p, ordinal).map_err(error)? else {
        return Err("Existing drain delivery is observation only; no resend".into());
    };
    let record = permit.into_record();
    let result = exchange(c, &record.command).and_then(|receipt| {
        j.confirm_drain_command(a, s, p, ordinal, receipt)
            .map_err(error)
    });
    if let Err(reason) = result {
        // Even rejected/malformed ACKs are not permission to repeat a mutation.
        return match j.mark_drain_uncertain(a, s, p, &record, reason.chars().take(1000).collect()) {
            Ok(_) => Err(reason),
            Err(save) => Err(format!(
                "{reason}; uncertain receipt not committed: {save}; inspect journal"
            )),
        };
    }
    Ok(())
}
pub(crate) fn execute(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    p: DrainPermit,
    c: &mut impl Control,
    telemetry: Telemetry,
) -> Result<()> {
    current(j, a, s, &p)?;
    let cfg = crate::preparation::configuration(s)?;
    evidence(j, a, s, &p)?;
    compatibility(j, s, c)?;
    let h = j
        .health(&s.spec.update_id)
        .map_err(error)?
        .ok_or("Drain health missing")?;
    let ctx = h.context.as_ref().ok_or("Drain health context missing")?;
    let (inspection, _pins) = crate::candidate_artifacts::inspect_pinned(&cfg, &s.spec.update_id)?;
    let config = crate::artifact::pin(Path::new(&h.intent.worker_config_path))?;
    if inspection != ctx.artifacts.inspection || config.evidence != ctx.worker_config {
        return Err("Drain pinned candidate/config differs from health receipt".into());
    }
    // All four candidate handles and the config stay pinned throughout waiting.
    deliver(j, a, s, &p, 0, c)?;
    loop {
        current(j, a, s, &p)?;
        crate::preparation::configuration(s)?;
        let observation = c.observation()?;
        let snapshot = j.drain(&s.spec.update_id).map_err(error)?;
        validate_wait_observation(&p.intent, &snapshot.commands[0], &observation)?;
        let clear = observation.accepted_operations == 0
            && observation.logical_owners == 0
            && observation.in_flight_effects == 0;
        telemetry.save(observation)?;
        current(j, a, s, &p)?;
        if clear {
            break;
        }
        // User pause and ready applications remain blockers; no TTL or force-kill.
        std::thread::sleep(Duration::from_millis(250));
    }
    for ordinal in 1..3 {
        evidence(j, a, s, &p)?;
        compatibility(j, s, c)?;
        let (fresh, _fresh_pins) =
            crate::candidate_artifacts::inspect_pinned(&cfg, &s.spec.update_id)?;
        if fresh != inspection
            || crate::artifact::inspect_image(Path::new(&h.intent.worker_config_path))?
                != config.evidence
        {
            return Err("Candidate/config changed during drain; preserve barrier".into());
        }
        deliver(j, a, s, &p, ordinal, c)?;
    }
    // Quiesced is not permission to stop or replace the worker. Keep the lease.
    Ok(())
}

fn validate_wait_observation(
    intent: &DrainIntent,
    first: &DrainCommandRecord,
    v: &WorkerDrainObservation,
) -> Result<()> {
    let DrainDeliveryState::Confirmed { receipt, .. } = &first.delivery else {
        return Err("Drain wait has no BeginDrain receipt".into());
    };
    if v.schema_version != 1
        || v.context.instance_id != receipt.spec.instance_id
        || v.context.journal_incarnation != receipt.spec.journal_incarnation
        || v.worker_generation != receipt.spec.worker_generation
        || v.lease.as_deref() != Some(receipt.as_ref())
        || spec_digest(&receipt.spec)? != intent.worker_spec_sha256
    {
        return Err("Drain wait observation scope/lease/revision changed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Peer {
        calls: Vec<&'static str>,
        reply: Result<Option<WorkerUpdateLease>>,
    }
    impl Control for Peer {
        fn compatibility(&mut self) -> Result<WorkerRuntimeCompatibility> {
            unreachable!()
        }
        fn advance(&mut self, _: WorkerUpdateCommand) -> Result<WorkerUpdateLease> {
            self.calls.push("advance");
            Err("ACK lost".into())
        }
        fn receipt(&mut self, _: &WorkerUpdateCommand) -> Result<Option<WorkerUpdateLease>> {
            self.calls.push("receipt");
            self.reply.clone()
        }
        fn observation(&mut self) -> Result<WorkerDrainObservation> {
            unreachable!()
        }
    }
    #[test]
    fn lost_ack_queries_exact_receipt_once_and_never_resends_when_missing_or_failed() {
        let command = WorkerUpdateCommand {
            command_id: "frozen-drain-key".into(),
            expected_revision: 1,
            action: WorkerUpdateAction::BeginDrain,
            source_manifest_sha256: None,
            reason: "test".into(),
        };
        for reply in [Ok(None), Err("offline".into())] {
            let mut peer = Peer {
                calls: vec![],
                reply,
            };
            assert!(exchange(&mut peer, &command)
                .unwrap_err()
                .contains("no resend"));
            assert_eq!(peer.calls, vec!["advance", "receipt"]);
        }
    }
    #[test]
    fn lost_ack_accepts_only_lookup_result_and_wait_rejects_stale_or_other_lease() {
        let (intent, before) = crate::drain::tests::fixture();
        let receipt = crate::drain::tests::receipt(&intent, &before, 0);
        let mut peer = Peer {
            calls: vec![],
            reply: Ok(Some(receipt.clone())),
        };
        assert_eq!(exchange(&mut peer, &intent.commands[0]).unwrap(), receipt);
        assert_eq!(peer.calls, vec!["advance", "receipt"]);
        let done = DrainCommandRecord::prepare(&intent, 0)
            .unwrap()
            .issue(&intent, 101)
            .unwrap()
            .confirm(&intent, 102, receipt.clone())
            .unwrap();
        let mut v = WorkerDrainObservation {
            schema_version: 1,
            context: winbridge_protocol::RequestContext {
                instance_id: receipt.spec.instance_id.clone(),
                journal_incarnation: receipt.spec.journal_incarnation.clone(),
            },
            worker_generation: receipt.spec.worker_generation.clone(),
            lease: Some(Box::new(receipt)),
            accepted_operations: 1,
            logical_owners: 1,
            in_flight_effects: 1,
            queue_revision: "0".into(),
            queue_mode: winbridge_protocol::QueueMode::DispatchPaused,
            observed_at_ms: 102,
        };
        validate_wait_observation(&intent, &done, &v).unwrap();
        v.lease.as_mut().unwrap().revision += 1;
        assert!(validate_wait_observation(&intent, &done, &v).is_err());
        v.lease = Some(Box::new(before));
        assert!(validate_wait_observation(&intent, &done, &v).is_err());
        v.lease = None;
        assert!(validate_wait_observation(&intent, &done, &v).is_err());
    }
}
