//! Exact-scope native fault only. No body/receipt is read or validated here.
use super::*;
use winbridge_protocol::update::WorkerLifetimeActivationCommand;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema_version: u16,
    kind: String,
    record_sha256: String,
}
pub(crate) struct ReadyLoss {
    root: PathBuf,
    pin: crate::artifact::Pinned,
    raw: Vec<u8>,
    input: Input,
}
pub(crate) fn prepare(
    record: &ActivationRecord,
    command: &WorkerLifetimeActivationCommand,
) -> Result<Option<ReadyLoss>> {
    let source = &record.intent.source;
    let root = Path::new(&source.runtime_directory)
        .join("update")
        .join(&source.update_id);
    let path = root.join("lifetime-ready-loss-validation.json");
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    if !cfg!(windows) || winbridge_protocol::build_identity().source_dirty {
        return Err("Lifetime Ready loss requires a clean native validation build".into());
    }
    let pin = crate::artifact::pin(&path)?;
    let raw = bounded(&path)?;
    if crate::sha256(&raw) != pin.evidence.sha256 {
        return Err("Lifetime Ready loss input changed".into());
    }
    let input: Input = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    require_scope(&input, record, command)?;
    Ok(Some(ReadyLoss {
        root,
        pin,
        raw,
        input,
    }))
}

fn require_scope(
    input: &Input,
    record: &ActivationRecord,
    command: &WorkerLifetimeActivationCommand,
) -> Result<()> {
    crate::activation::lifetime::validate_command_envelope(command)?;
    if input.schema_version != 1
        || input.kind != "winbridge_lifetime_ready_body_loss_validation"
        || input.record_sha256 != record_sha(record)?
        || record.revision != 7
        || record.state != (ActivationState::LocalVerified {})
        || command.binding.update_id != record.intent.source.update_id
        || command.binding.attempt_id != record.intent.attempt_id
        || command.binding.keeper != record.intent.context.supervisor
        || record.state_owned.as_ref().map(|v| &v.process)
            != Some(&command.binding.compatibility.identity.process)
        || command.binding.job_name != record.intent.job_name()?
    {
        return Err("Lifetime Ready loss changed original local7/child/keeper scope".into());
    }
    Ok(())
}

impl ReadyLoss {
    pub(crate) async fn interrupt(
        self,
        record: &ActivationRecord,
        command: &WorkerLifetimeActivationCommand,
        stream: &mut winbridge_transport::supervisor::Stream,
        deadline: std::time::Instant,
    ) -> Result<()> {
        let length =
            crate::activation::lifetime_ready_loss::read_header_only(stream, deadline).await?;
        require_scope(&self.input, record, command)?;
        let observed_at_ms = crate::journal::now_ms().map_err(|e| e.to_string())?;
        if observed_at_ms < command.issued_at_ms || observed_at_ms >= command.deadline_at_ms {
            return Err("Lifetime Ready loss outside the original window".into());
        }
        let marker = serde_json::json!({"schemaVersion":1,"kind":"activation_lifetime_ready_body_loss",
            "processId":std::process::id(),"build":winbridge_protocol::build_identity(),
            "input":self.input,"inputEncoded":String::from_utf8(self.raw).map_err(|e|e.to_string())?,
            "inputSha256":self.pin.evidence.sha256,"record":record,"recordSha256":record_sha(record)?,
            "command":command,"commandSha256":crate::activation::lifetime::command_sha256(command)?,
            "readyFrameBytes":length,"headerBytesRead":4,"bodyBytesRead":0,"readyValidated":false,
            "normalProbeBeforeFailure":false,"observedAtMs":observed_at_ms});
        write_marker(
            &self.root,
            "activation-lifetime-ready-body-loss.json",
            &marker,
        )?;
        Err("Validation: private lifetime Ready body intentionally unread; observe same command without replay".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifetime_ready_loss_fixture_requires_exact_local7_record_and_command_scope() {
        let (graph, command, _) = crate::activation::lifetime::test_fixture();
        let record = graph.local;
        let input = Input {
            schema_version: 1,
            kind: "winbridge_lifetime_ready_body_loss_validation".into(),
            record_sha256: record_sha(&record).unwrap(),
        };
        require_scope(&input, &record, &command).unwrap();
        for change in 0..5 {
            let mut bad = record.clone();
            match change {
                0 => bad.intent.source.update_id = uuid::Uuid::new_v4().to_string(),
                1 => bad.intent.attempt_id = uuid::Uuid::new_v4().to_string(),
                2 => bad.intent.context.supervisor.birth.push('0'),
                3 => bad.revision = 6,
                _ => bad.intent.context.channel_nonce_sha256 = "0".repeat(64),
            }
            assert!(require_scope(&input, &bad, &command).is_err());
        }
        let mut rebound = command;
        rebound.binding.attempt_id = uuid::Uuid::new_v4().to_string();
        assert!(require_scope(&input, &record, &rebound).is_err());
    }
    #[test]
    fn lifetime_ready_loss_input_rejects_unknown_grants_and_cannot_relabel_a_trace() {
        let raw = serde_json::json!({"schemaVersion":1,"kind":"winbridge_lifetime_ready_body_loss_validation","recordSha256":"a".repeat(64)});
        for key in ["canCommit", "permit", "deadlineAtMs", "stateTraceOnly"] {
            let mut bad = raw.clone();
            bad[key] = true.into();
            assert!(serde_json::from_value::<Input>(bad).is_err());
        }
        let (graph, command, _) = crate::activation::lifetime::test_fixture();
        let mut input: Input = serde_json::from_value(raw).unwrap();
        input.record_sha256 = record_sha(&graph.local).unwrap();
        input.kind = "winbridge_activation_process_validation".into();
        assert!(require_scope(&input, &graph.local, &command).is_err());
    }
}
