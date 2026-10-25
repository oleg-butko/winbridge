//! Saved delivery facts do not prove a current child or normal worker service.
use super::{ActivationWriterRecord, ActivationWriterState};
use winbridge_protocol::update::{
    ActivationWriterDurablePhase as Durable, ActivationWriterPhase as Phase, ActivationWriterView,
};
pub(crate) fn view(value: Option<&ActivationWriterRecord>) -> ActivationWriterView {
    ActivationWriterView {
        phase:if value.is_some(){Phase::NotResident}else{Phase::NotStarted},
        attempt_id:value.map(|v|v.command.binding.attempt_id.clone()),
        command_id:value.map(|v|v.command.command_id.clone()),
        revision:value.map(|v|v.revision),
        durable_phase:value.map(|v|match v.state {ActivationWriterState::Issued{}=>Durable::Issued,ActivationWriterState::Confirmed{..}=>Durable::Confirmed,ActivationWriterState::Indeterminate{..}=>Durable::Indeterminate}),
        saved_receipt:value.and_then(|v|match &v.state {ActivationWriterState::Confirmed{receipt}=>Some(receipt.clone()),_=>None}),
        resident_process:None,process_alive:None,
        reason:value.map(|v|match &v.state {ActivationWriterState::Indeterminate{reason}=>reason.clone(),_=>"Saved writer delivery is history; no resident owner or normal-service readiness is inferred".into()}),
    }
}

#[cfg(test)]
pub(crate) fn fixture() -> (super::ActivationRecord, ActivationWriterRecord) {
    use winbridge_protocol::update::{StateBackupAuthority, WorkerJournalActivationReceipt};
    let (r, initial) = super::activation_fixture();
    let physical = super::activation_stages(&initial, &r)[6].clone();
    let old = StateBackupAuthority {
        incarnation: physical
            .intent
            .context
            .worker_context
            .journal_incarnation
            .clone(),
        epoch: 7,
        fingerprint_sha256: "a".repeat(64),
    };
    let command = super::writer::prepare_command(
        &physical,
        &old,
        &uuid::Uuid::new_v4().to_string(),
        physical.updated_at_ms + 1,
    )
    .unwrap();
    let receipt = WorkerJournalActivationReceipt {
        schema_version: 1,
        command_id: command.command_id.clone(),
        update_id: command.binding.update_id.clone(),
        attempt_id: command.binding.attempt_id.clone(),
        command_sha256: super::writer::command_sha256(&command).unwrap(),
        authority: StateBackupAuthority {
            incarnation: old.incarnation,
            epoch: 8,
            fingerprint_sha256: "b".repeat(64),
        },
        committed_at_ms: command.issued_at_ms + 1,
    };
    let record = ActivationWriterRecord::issued(command).unwrap();
    let record = record.confirmed(receipt, record.updated_at_ms + 1).unwrap();
    (physical, record)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_writer_history_never_claims_a_resident_process_or_normal_service() {
        let (_, confirmed) = fixture();
        let issued = ActivationWriterRecord::issued(confirmed.command.clone()).unwrap();
        let unknown = issued
            .indeterminate("Unknown private delivery".into(), issued.updated_at_ms + 1)
            .unwrap();
        assert_eq!(view(None).phase, Phase::NotStarted);
        for record in [issued, unknown, confirmed.clone()] {
            let value = view(Some(&record));
            assert_eq!(value.phase, Phase::NotResident);
            assert!(value.resident_process.is_none() && value.process_alive.is_none());
            assert_eq!(
                value.command_id.as_deref(),
                Some(record.command.command_id.as_str())
            );
        }
        let saved = view(Some(&confirmed));
        assert_eq!(saved.durable_phase, Some(Durable::Confirmed));
        assert!(saved.saved_receipt.is_some());
    }
}
