//! V8 is a delivery of one committed decision, not a new launch/adoption grant.
use super::{finalization, Result};
use winbridge_protocol::update::*;

pub fn validate_request(request: &ActivationFinalizationRequest) -> Result<()> {
    if request.finalization_version != ACTIVATION_FINALIZATION_CONTRACT {
        return Err("Unsupported finalization version; no fallback".into());
    }
    finalization::validate_command_envelope(&request.command)
}
pub fn validate_response(
    request: &ActivationFinalizationRequest,
    shutdown: &crate::shutdown::ShutdownRecord,
    response: &ActivationFinalizationResponse,
) -> Result<()> {
    validate_request(request)?;
    if response.finalization_version != ACTIVATION_FINALIZATION_CONTRACT
        || response.command_id.as_deref() != Some(request.command.command_id.as_str())
    {
        return Err("Finalization response changed version/decision key".into());
    }
    match &response.result {
        ActivationFinalizationResult::Active { receipt } => {
            finalization::validate_receipt(receipt, &request.command, shutdown)
        }
        ActivationFinalizationResult::Indeterminate { message }
        | ActivationFinalizationResult::Rejected { message } => Err(message.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_active_receipt_can_complete_a_delivery_and_versions_do_not_fall_back() {
        let f = finalization::test_fixture();
        let request = ActivationFinalizationRequest {
            finalization_version: 8,
            command: f.command,
        };
        let response = ActivationFinalizationResponse {
            finalization_version: 8,
            command_id: Some(request.command.command_id.clone()),
            result: ActivationFinalizationResult::Active {
                receipt: Box::new(f.receipt),
            },
        };
        validate_response(&request, &f.shutdown, &response).unwrap();
        for fault in 0..5 {
            let mut bad = response.clone();
            match fault {
                0 => bad.finalization_version = 7,
                1 => bad.command_id = Some(uuid::Uuid::new_v4().to_string()),
                2 => {
                    bad.result = ActivationFinalizationResult::Indeterminate {
                        message: "Unknown COMMIT".into(),
                    }
                }
                3 => {
                    bad.result = ActivationFinalizationResult::Rejected {
                        message: "No original owner".into(),
                    }
                }
                _ => {
                    if let ActivationFinalizationResult::Active { receipt } = &mut bad.result {
                        receipt.released_lease.revision += 1;
                    }
                }
            }
            assert!(validate_response(&request, &f.shutdown, &bad).is_err());
        }
        let mut bad = request.clone();
        bad.finalization_version = 7;
        assert!(validate_request(&bad).is_err());
        for field in [
            "force",
            "adopt",
            "permit",
            "cancel",
            "finalizationObservationVersion",
        ] {
            let mut raw = serde_json::to_value(&request).unwrap();
            raw[field] = true.into();
            assert!(serde_json::from_value::<ActivationFinalizationRequest>(raw).is_err());
        }
    }
}
