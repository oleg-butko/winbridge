//! Pure workflow admission policy. Files and processes are resolved by the worker.
use crate::config::{valid_id, validate_arguments, validate_relative, WorkerConfig};
use winbridge_protocol::{CommandSource, WorkflowSubmission, PROTOCOL_VERSION};
pub fn validate_log_path(path: &str) -> Result<(), String> {
    validate_relative(path)?;
    let first = path.split(['/', '\\']).next().unwrap_or("");
    if path == "."
        || path.len() > 1024
        || first.eq_ignore_ascii_case("snapshots")
        || first.eq_ignore_ascii_case(".winbridge")
    {
        return Err(
            "Application log must be a relative project file outside snapshots/staging".into(),
        );
    }
    Ok(())
}
pub fn validate(config: &WorkerConfig, request: &WorkflowSubmission) -> Result<(), String> {
    if request.protocol_version != PROTOCOL_VERSION
        || !valid_id(&request.project_id)
        || !valid_id(&request.snapshot_id)
        || request.manifest_sha256.len() != 64
        || !request
            .manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(
            "Workflow requires protocol 1, safe project/snapshot IDs and manifest SHA-256".into(),
        );
    }
    for value in [&request.name, &request.request_id, &request.idempotency_key] {
        if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
            return Err(
                "Workflow name/request/key must contain 1-160 bytes without controls".into(),
            );
        }
    }
    if !(100..=1800000).contains(&request.timeout_ms)
        || !(100..=180000).contains(&request.readiness.timeout_ms)
        || request.readiness.stdout_contains.is_empty()
        || request.readiness.stdout_contains.len() > 1024
        || request.readiness.stdout_contains.contains('\0')
    {
        return Err("Workflow timeout 100-1800000 ms; readiness timeout 100-180000 ms and nonempty stdout token up to 1024 bytes".into());
    }
    validate_log_path(&request.log_path)?;
    if config.delivery.is_none() {
        return Err("Workflow requires configured snapshot delivery".into());
    }
    let project = config
        .projects
        .iter()
        .find(|p| p.id == request.project_id)
        .ok_or("Workflow project is not registered")?;
    for command in [&request.build, &request.start] {
        match command {
            CommandSource::Recipe {
                recipe_id,
                arguments,
            } => {
                if !arguments.is_empty() || !project.recipes.iter().any(|r| r.id == *recipe_id) {
                    return Err("Workflow recipe must exist and use its fixed arguments".into());
                }
            }
            CommandSource::Direct {
                executable,
                arguments,
                working_directory,
            } => {
                if !project.allow_direct_commands {
                    return Err("Direct workflow commands are disabled for this project".into());
                }
                validate_arguments(executable, arguments)?;
                validate_relative(working_directory)?;
            }
        }
    }
    Ok(())
}
