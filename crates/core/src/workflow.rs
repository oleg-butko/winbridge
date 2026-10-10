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
        || request.readiness.stdout_contains.len() > 1024
        || request.readiness.stdout_contains.contains('\0')
    {
        return Err("Workflow timeout 100-1800000 ms; readiness timeout 100-180000 ms and nonempty stdout token up to 1024 bytes".into());
    }
    match &request.readiness.netwatch {
        None if request.readiness.stdout_contains.is_empty() => {
            return Err(
                "Readiness requires a stdout token or isolated Netwatch health probe".into(),
            )
        }
        Some(probe) => {
            if !request.readiness.stdout_contains.is_empty()
                || !valid_id(&probe.instance_id)
                || !probe.instance_id.starts_with("winbridge-")
            {
                return Err("Netwatch probe requires an isolated winbridge-* instance ID and no stdout token".into());
            }
            validate_log_path(&probe.runtime_directory)?;
        }
        None => {}
    }
    if request.artifacts.len() > 8 {
        return Err("At most 8 declared build artifacts".into());
    }
    let mut paths = std::collections::HashSet::new();
    for artifact in &request.artifacts {
        validate_log_path(&artifact.path)?;
        if artifact.name.is_empty()
            || artifact.name.len() > 160
            || artifact.name.chars().any(char::is_control)
            || !paths.insert(artifact.path.replace('\\', "/").to_ascii_lowercase())
        {
            return Err(
                "Artifact requires a unique relative path and a name of 1-160 bytes".into(),
            );
        }
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
    let recipes = config.project_recipes(project)?;
    for command in [&request.build, &request.start] {
        match command {
            CommandSource::Recipe {
                recipe_id,
                arguments,
            } => {
                if !arguments.is_empty() || !recipes.iter().any(|r| r.id == *recipe_id) {
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

/// Freeze recipes before ACK, including profiles. No generation filesystem is required.
pub fn recipe_pins(
    config: &WorkerConfig,
    request: &WorkflowSubmission,
) -> Result<Vec<winbridge_protocol::WorkflowRecipePin>, String> {
    let project = config
        .projects
        .iter()
        .find(|project| project.id == request.project_id)
        .ok_or("Workflow project is not registered")?;
    let recipes = config.project_recipes(project)?;
    let mut pins = Vec::new();
    for (step_name, source) in [("build", &request.build), ("start", &request.start)] {
        if let CommandSource::Recipe { recipe_id, .. } = source {
            let recipe = recipes
                .iter()
                .find(|recipe| recipe.id == *recipe_id)
                .ok_or("Workflow recipe is not registered")?;
            pins.push(winbridge_protocol::WorkflowRecipePin {
                step_name: step_name.into(),
                recipe_id: recipe_id.clone(),
                origin: config.recipe_origin(project, recipe_id)?,
                command: winbridge_protocol::ResolvedCommand {
                    executable: recipe.executable.clone(),
                    arguments: recipe.arguments.clone(),
                    working_directory: recipe.working_directory.clone(),
                    timeout_ms: recipe.timeout_ms,
                },
            });
        }
    }
    Ok(pins)
}
pub fn validate_recipe_pins(
    config: &WorkerConfig,
    workflow: &winbridge_protocol::WorkflowRecord,
) -> Result<(), String> {
    let Some(pins) = &workflow.recipe_pins else {
        if matches!(workflow.submission.build, CommandSource::Recipe { .. })
            || matches!(workflow.submission.start, CommandSource::Recipe { .. })
        {
            return Err("Legacy workflow has no frozen recipe definitions; review the current configuration and submit a new explicit workflow. No further child operation was launched".into());
        }
        return Ok(());
    };
    let current = recipe_pins(config, &workflow.submission)?;
    if pins.len() != current.len()
        || pins.iter().zip(&current).any(|(old, new)| {
            old.step_name != new.step_name
                || old.recipe_id != new.recipe_id
                || old.command != new.command
        })
    {
        return Err("Workflow recipe changed since acceptance; no further import/build/start was launched. Review the current profile and submit a new explicit workflow".into());
    }
    Ok(())
}
