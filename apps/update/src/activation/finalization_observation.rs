//! A late Active fact is not a new release, a startup grant or keeper custody.
use super::{finalization, Result};
use winbridge_core::update::MAX_UPDATE_COUNTER;
use winbridge_protocol::update::*;

pub fn validate_query(query: &ActivationFinalizationQuery) -> Result<()> {
    finalization::validate_command_envelope(&query.command)?;
    if query.finalization_observation_version != ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT
        || !uuid::Uuid::parse_str(&query.request_id)
            .is_ok_and(|id| id.to_string() == query.request_id)
        || [
            &query.command.command_id,
            &query.command.binding.lifetime_command.command_id,
            &query.command.binding.resident_request_id,
        ]
        .contains(&&query.request_id)
    {
        return Err("Finalization observation version/fresh correlation invalid".into());
    }
    Ok(())
}
pub fn validate_observation(
    query: &ActivationFinalizationQuery,
    shutdown: &crate::shutdown::ShutdownRecord,
    observation: &ActivationFinalizationObservation,
) -> Result<()> {
    validate_query(query)?;
    if observation.finalization_observation_version != ACTIVATION_FINALIZATION_OBSERVATION_CONTRACT
        || observation.request_id.as_deref() != Some(query.request_id.as_str())
    {
        return Err("Finalization observation reply belongs to another request/version".into());
    }
    match &observation.result {
        ActivationFinalizationObservationResult::Observed {
            receipt,
            observed_at_ms,
        } => {
            finalization::validate_receipt(receipt, &query.command, shutdown)?;
            if *observed_at_ms < receipt.compatibility.observed_at_ms
                || *observed_at_ms > MAX_UPDATE_COUNTER
            {
                return Err(
                    "Finalization current observation predates Active entry or exceeds contract"
                        .into(),
                );
            }
            Ok(())
        }
        ActivationFinalizationObservationResult::Pending { command_id, .. } => {
            if command_id != &query.command.command_id {
                return Err("Pending observation changed finalization key".into());
            }
            Err("Finalization remains pending; no Active fact or permission".into())
        }
        ActivationFinalizationObservationResult::Unavailable { message }
        | ActivationFinalizationObservationResult::Rejected { message } => Err(message.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        super::super::finalization::tests::Fixture,
        ActivationFinalizationQuery,
        ActivationFinalizationObservation,
    ) {
        let f = finalization::test_fixture();
        let query = ActivationFinalizationQuery {
            finalization_observation_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: f.command.clone(),
        };
        let observation = ActivationFinalizationObservation {
            finalization_observation_version: 1,
            request_id: Some(query.request_id.clone()),
            result: ActivationFinalizationObservationResult::Observed {
                receipt: Box::new(f.receipt.clone()),
                observed_at_ms: f.receipt.compatibility.observed_at_ms + 1,
            },
        };
        (f, query, observation)
    }
    #[test]
    fn late_active_observation_keeps_original_receipt_and_correlation() {
        let (f, query, mut observation) = fixture();
        validate_observation(&query, &f.shutdown, &observation).unwrap();
        for mutation in 0..5 {
            let mut bad = observation.clone();
            match mutation {
                0 => bad.finalization_observation_version = 8,
                1 => bad.request_id = Some(uuid::Uuid::new_v4().to_string()),
                2 => bad.request_id = None,
                _ => {
                    if let ActivationFinalizationObservationResult::Observed {
                        receipt,
                        observed_at_ms,
                    } = &mut bad.result
                    {
                        if mutation == 3 {
                            *observed_at_ms = 0;
                        } else {
                            receipt.command_id = uuid::Uuid::new_v4().to_string();
                        }
                    }
                }
            }
            assert!(validate_observation(&query, &f.shutdown, &bad).is_err());
        }
        observation.result = ActivationFinalizationObservationResult::Pending {
            command_id: query.command.command_id.clone(),
            message: "SQL/Active pending".into(),
        };
        assert!(validate_observation(&query, &f.shutdown, &observation)
            .unwrap_err()
            .contains("pending"));
    }
    #[test]
    fn observation_cannot_reuse_execution_keys_or_parse_mutating_fields() {
        let (_, query, _) = fixture();
        for id in [
            &query.command.command_id,
            &query.command.binding.lifetime_command.command_id,
            &query.command.binding.resident_request_id,
            "not-uuid",
        ] {
            let mut bad = query.clone();
            bad.request_id = id.to_string();
            assert!(validate_query(&bad).is_err());
        }
        let raw = serde_json::to_value(query).unwrap();
        for key in ["release", "force", "permit", "nonce", "active", "replay"] {
            let mut bad = raw.clone();
            bad[key] = true.into();
            assert!(serde_json::from_value::<ActivationFinalizationQuery>(bad).is_err());
        }
        let mut bad = raw;
        bad["finalizationObservationVersion"] = true.into();
        assert!(serde_json::from_value::<ActivationFinalizationQuery>(bad).is_err());
    }
}
