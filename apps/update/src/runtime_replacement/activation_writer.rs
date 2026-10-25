//! Same original child/channel; durable intent precedes the one private send.
use super::*;
use crate::activation::{ActivationWriterAdmission, ActivationWriterPermit, ActivationWriterState};

impl ActivationProcessGuard {
    /// Consumes the original StateOwned owner. Failure retains it; saved rows
    /// and duplicate admission never recreate a channel or a send permission.
    pub fn activate_journal_blocking(
        self: Box<Self>,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> std::result::Result<Box<Self>, ActivationLaunchFailure> {
        match crate::activation::on_original_io(self, move |mut owner| async move {
            match owner.activate_journal(j, a, s, key).await {
                Ok(()) => {
                    owner.emit_receiver_timing();
                    Ok(owner)
                }
                Err(reason) => {
                    owner.emit_receiver_timing();
                    owner.record_uncertainty(j, a, s);
                    Err(ActivationLaunchFailure {
                        reason,
                        custody: owner,
                    })
                }
            }
        }) {
            Ok(result) => result,
            Err((owner, reason)) => Err(ActivationLaunchFailure {
                reason: reason.into(),
                custody: owner,
            }),
        }
    }
    async fn activate_journal(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        key: &str,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "writer_admission",
            Some(self.files.preparation.deadline),
        );
        if !matches!(self.record.state, ActivationState::StateOwned {})
            || self.record.revision != 6
            || self.released.is_none()
            || self.channel.is_none()
            || self.resident_process().as_ref()
                != self.record.state_owned.as_ref().map(|r| &r.process)
        {
            return Err(
                "Journal writer requires the original resident StateOwned child/channel".into(),
            );
        }
        let worker = crate::artifact::pin(Path::new(&self.record.intent.source.worker.path))?;
        if worker.evidence != self.record.intent.source.worker
            || !crate::worker_rehearsal::supports(&worker, channel::WRITER_MARKER)?
        {
            return Err("StateOwned child lacks private writer V3; no journal command sent".into());
        }
        timing.mark("capability_checked", None, None);
        self.require_custody(j, a, s, true)?;
        timing.mark("custody_checked", None, None);
        let actual = self
            .resident_process()
            .ok_or("Writer child identity missing")?;
        let live = infrastructure::PinnedProcess::live(&actual).map_err(|e| e.to_string())?;
        let admission = j
            .admit_activation_writer(a, s, &self.files.preparation, &self.record, key, || {
                self.require_fixed_custody(true)?;
                live.require_live().map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        timing.mark("intent_committed", None, None);
        let ActivationWriterAdmission::Issued(permit) = admission else {
            return Err("Writer already admitted; observe/reconcile without another send".into());
        };
        let result = self.deliver_writer(j, a, s, &permit, &live).await;
        timing.mark("delivery_returned", None, None);
        if result.is_err() {
            if let Ok(Some(saved)) = j.activation_writer(&s.spec.update_id) {
                if matches!(saved.state, ActivationWriterState::Issued {})
                    && saved.command == permit.record().command
                {
                    if let Ok(next) = saved.indeterminate(
                        "Private writer delivery outcome unconfirmed".into(),
                        self.at()?.max(saved.updated_at_ms),
                    ) {
                        let _ = j.append_activation_writer_owned(
                            a,
                            s,
                            &self.files.preparation,
                            &self.record,
                            &next,
                        );
                    }
                }
            }
        }
        result
    }
    async fn deliver_writer(
        &mut self,
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        permit: &ActivationWriterPermit,
        live: &infrastructure::PinnedProcess,
    ) -> Result<()> {
        let mut timing = crate::activation::ActivationTiming::new(
            "writer_delivery",
            Some(self.files.preparation.deadline),
        );
        j.require_activation_writer_issued(a, s, &self.files.preparation, &self.record, permit)
            .map_err(|e| e.to_string())?;
        timing.mark("issued_checked", None, None);
        self.require_fixed_custody(true)?;
        timing.mark("custody_checked", None, None);
        live.require_live().map_err(|e| e.to_string())?;
        #[cfg(feature = "update-validation")]
        validation::writer_before_send(&self.record, &permit.record().command)?;
        let remaining = self
            .files
            .preparation
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("Original writer window expired before private send".into());
        }
        let request = channel::WriterCommand {
            contract_version: channel::WRITER_CONTRACT,
            command: permit.record().command.clone(),
        };
        let stream = self
            .channel
            .as_mut()
            .ok_or("Original writer channel missing")?;
        channel::write(stream, &request, remaining).await?;
        timing.mark("command_sent", None, None);
        // A late authenticated receipt is a fact. No further write grant is
        // issued after the original window, including on read timeout/EOF.
        let message: channel::WriterReceipt = channel::read(
            stream,
            Duration::from_millis(self.record.intent.context.startup_timeout_ms),
        )
        .await?;
        timing.mark("receipt_received", None, None);
        channel::validate_writer_receipt(&message, &permit.record().command)?;
        self.require_custody(j, a, s, false)?;
        timing.mark("receipt_custody_checked", None, None);
        let at = self.at()?.max(message.receipt.committed_at_ms);
        let next = permit.record().confirmed(message.receipt, at)?;
        j.append_activation_writer_owned(a, s, &self.files.preparation, &self.record, &next)
            .map_err(|e| e.to_string())?;
        timing.mark("confirmation_committed", None, None);
        Ok(())
    }
    fn emit_receiver_timing(&self) {
        use std::io::Write;
        // The child capture is bounded and drained independently. Export
        // only structured timing, never nonce/config or arbitrary output.
        let (_, stderr, _) = self.output();
        for line in String::from_utf8_lossy(&stderr).lines() {
            if let Some(raw) = line.strip_prefix("WinBridge activation timing ") {
                if serde_json::from_str::<serde_json::Value>(raw).is_ok() {
                    let _ = writeln!(std::io::stderr().lock(), "{line}");
                }
            }
        }
    }
}
