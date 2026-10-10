//! Shared static command handlers. Journal, scheduling and processes remain worker-owned.
use std::{fs, path::Path};
use winbridge_core::{commands::*, config::*};
use winbridge_protocol::*;

pub fn resolve(config: &WorkerConfig, request: &TaskSubmission) -> Result<ResolvedCommand> {
    resolve_at(config, request, None)
}
/// The caller must verify the generation pin before admission and launch.
/// Profile bindings always use the registered root, even when cwd is a generation.
pub fn resolve_for_generation(
    config: &WorkerConfig,
    request: &TaskSubmission,
    generation: &Path,
) -> Result<ResolvedCommand> {
    resolve_at(config, request, Some(generation))
}
fn resolve_at(
    config: &WorkerConfig,
    request: &TaskSubmission,
    generation: Option<&Path>,
) -> Result<ResolvedCommand> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(format!("protocolVersion must be {PROTOCOL_VERSION}"));
    }
    for (name, value) in [
        ("requestId", &request.request_id),
        ("idempotencyKey", &request.idempotency_key),
    ] {
        if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
            return Err(format!(
                "{name} must contain 1–160 characters without control characters"
            ));
        }
    }
    validate_timeout(request.timeout_ms)?;
    let project = config
        .projects
        .iter()
        .find(|p| p.id == request.project_id)
        .ok_or_else(|| format!("projectId {:?} is not registered", request.project_id))?;
    let recipes = config.project_recipes(project)?;
    let context = CommandContext {
        project,
        recipes: &recipes,
        timeout_ms: request.timeout_ms,
    };
    let command = handlers()
        .into_iter()
        .find(|handler| handler.supports(&request.command))
        .ok_or("No registered command handler")?
        .resolve(&context, &request.command)?;
    let CommandDefinition {
        executable,
        arguments,
        working_directory: cwd,
        timeout_ms: timeout,
    } = command;
    let project = generation
        .map(|root| ProjectConfig {
            workspace_root: root.to_path_buf(),
            ..project.clone()
        })
        .unwrap_or_else(|| project.clone());
    validate_arguments(&executable, &arguments)?;
    let working_directory = scoped_directory(&project.workspace_root, &cwd)?
        .to_string_lossy()
        .into_owned();
    // Relative executable paths belong to the registered project; bare names use PATH.
    let executable = if executable.contains(['/', '\\']) && !Path::new(&executable).is_absolute() {
        validate_relative(&executable)?;
        let root = fs::canonicalize(&project.workspace_root).map_err(|e| e.to_string())?;
        let path = fs::canonicalize(root.join(&executable))
            .map_err(|e| format!("executable {executable:?}: {e}"))?;
        if !contains_path(&root, &path) {
            return Err(
                "relative executable escapes the registered project through a symlink/junction"
                    .into(),
            );
        }
        path.to_string_lossy().into_owned()
    } else {
        executable
    };
    Ok(ResolvedCommand {
        executable,
        arguments,
        working_directory,
        timeout_ms: timeout,
    })
}

struct Direct;
struct Recipe;
const DIRECT: Direct = Direct;
const RECIPE: Recipe = Recipe;
fn handlers() -> [&'static dyn CommandHandler; 2] {
    [&DIRECT, &RECIPE]
}
pub fn metadata() -> Vec<CommandHandlerInfo> {
    handlers()
        .iter()
        .map(|handler| handler.metadata())
        .collect()
}
impl CommandHandler for Direct {
    fn metadata(&self) -> CommandHandlerInfo {
        CommandHandlerInfo { id: "direct".into(), label: "Прямая команда".into(), description: "Executable и отдельные аргументы; требуется разрешение проекта. Команда сохраняется до запуска.".into() }
    }
    fn supports(&self, source: &CommandSource) -> bool {
        matches!(source, CommandSource::Direct { .. })
    }
    fn resolve(
        &self,
        context: &CommandContext<'_>,
        source: &CommandSource,
    ) -> Result<CommandDefinition> {
        let CommandSource::Direct {
            executable,
            arguments,
            working_directory,
        } = source
        else {
            return Err("Direct handler requires direct input".into());
        };
        if !context.project.allow_direct_commands {
            return Err(format!(
                "project {} does not allow direct commands; use a registered recipe",
                context.project.id
            ));
        }
        Ok(CommandDefinition {
            executable: executable.clone(),
            arguments: arguments.clone(),
            working_directory: working_directory.clone(),
            timeout_ms: context.timeout_ms,
        })
    }
}
impl CommandHandler for Recipe {
    fn metadata(&self) -> CommandHandlerInfo {
        CommandHandlerInfo { id: "recipe".into(), label: "Зарегистрированный рецепт".into(), description: "Фиксированные параметры из конфигурации проекта или общего профиля. Итоговая команда и источник сохраняются в журнале.".into() }
    }
    fn supports(&self, source: &CommandSource) -> bool {
        matches!(source, CommandSource::Recipe { .. })
    }
    fn resolve(
        &self,
        context: &CommandContext<'_>,
        source: &CommandSource,
    ) -> Result<CommandDefinition> {
        let CommandSource::Recipe {
            recipe_id,
            arguments,
        } = source
        else {
            return Err("Recipe handler requires recipe input".into());
        };
        if !arguments.is_empty() {
            return Err("Recipe arguments must be empty: recipes have fixed parameters; use direct mode for custom arguments".into());
        }
        let recipe = context
            .recipes
            .iter()
            .find(|recipe| recipe.id == *recipe_id)
            .ok_or_else(|| {
                format!(
                    "recipeId {recipe_id:?} is not registered in project {}",
                    context.project.id
                )
            })?;
        Ok(CommandDefinition {
            executable: recipe.executable.clone(),
            arguments: recipe.arguments.clone(),
            working_directory: recipe.working_directory.clone(),
            timeout_ms: context.timeout_ms.min(recipe.timeout_ms),
        })
    }
}

/// Freeze this metadata in the same transaction as the accepted command.
pub fn recipe_origin(
    config: &WorkerConfig,
    request: &TaskSubmission,
) -> Result<Option<RecipeOrigin>> {
    let CommandSource::Recipe { recipe_id, .. } = &request.command else {
        return Ok(None);
    };
    let project = config
        .projects
        .iter()
        .find(|project| project.id == request.project_id)
        .ok_or("Project is not registered")?;
    Ok(Some(config.recipe_origin(project, recipe_id)?))
}
