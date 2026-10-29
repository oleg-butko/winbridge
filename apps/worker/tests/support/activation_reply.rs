//! Shared by portable regressions and the actual native coordinator. Data only.
use winbridge_protocol::update::*;

pub fn validate(
    update_id: &str,
    request: &SupervisorRequest,
    response: SupervisorResponse,
) -> Result<Option<UpdatePreparationView>, String> {
    let schema = match request.contract_version {
        24 => 16,
        25 => 17,
        26 => 18,
        27 => 19,
        28 => 20,
        _ => return Err("Unknown native activation fixture contract; no fallback".into()),
    };
    if response.contract_version != request.contract_version
        || response.launch_id != request.launch_id
        || response.record.is_some()
        || response.worker_control.is_some()
    {
        return Err("Native activation fixture response scope/shape differs".into());
    }
    if let Some(reason) = response.error {
        return Err(format!("Observe the same attempt, never resend: {reason}"));
    }
    if let Some(view) = &response.preparation {
        let block = match request.contract_version {
            24 => view.activation_writer.is_some(),
            25 => view.activation_service.is_some(),
            26 => view.activation_controller.is_some(),
            27 => view.activation_lifetime.is_some(),
            28 => view.activation_resident.is_some(),
            _ => unreachable!(),
        };
        if view.schema_version != schema || view.update.update_id != update_id || !block {
            return Err("Native activation fixture view schema/update/block differs".into());
        }
    }
    Ok(response.preparation)
}
