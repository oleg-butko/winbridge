//! Local reusable recipe definitions. One profile per project, no implicit overrides.
use crate::config::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use winbridge_protocol::{RecipeOrigin, RecipeProfileInfo};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeProfileConfig {
    pub id: String,
    pub label: String,
    pub description: String,
    pub recipes: Vec<RecipeConfig>,
}
impl RecipeProfileConfig {
    pub fn info(&self) -> RecipeProfileInfo {
        RecipeProfileInfo {
            id: self.id.clone(),
            label: self.label.clone(),
            description: self.description.clone(),
        }
    }
}
fn expand(template: &str, project: &ProjectConfig) -> Result<String> {
    let mut remaining = template;
    let mut result = String::new();
    while let Some(start) = remaining.find("${") {
        result.push_str(&remaining[..start]);
        let rest = &remaining[start + 2..];
        let end = rest.find('}').ok_or("Unclosed recipe profile binding")?;
        match &rest[..end] {
            "workspaceRoot" => result.push_str(&project.workspace_root.to_string_lossy()),
            "projectId" => result.push_str(&project.id),
            name => {
                return Err(format!(
                    "Unknown recipe profile binding ${{{name}}}; allowed: workspaceRoot, projectId"
                ))
            }
        }
        remaining = &rest[end + 1..];
    }
    result.push_str(remaining);
    Ok(result)
}
impl WorkerConfig {
    pub fn validate_recipe_profiles(&self) -> Result<()> {
        if self.recipe_profiles.len() > 32 {
            return Err("At most 32 recipeProfiles".into());
        }
        let mut profiles = HashSet::new();
        for profile in &self.recipe_profiles {
            if !valid_id(&profile.id)
                || !profiles.insert(&profile.id)
                || profile.label.is_empty()
                || profile.label.len() > 160
                || profile.label.chars().any(char::is_control)
                || profile.description.len() > 1024
                || profile
                    .description
                    .chars()
                    .any(|c| c.is_control() && c != '\n')
                || profile.recipes.is_empty()
                || profile.recipes.len() > 128
            {
                return Err("Recipe profile requires a unique safe ID, label 1–160 bytes, description up to 1024 bytes and 1–128 recipes".into());
            }
            let mut ids = HashSet::new();
            for recipe in &profile.recipes {
                if !valid_id(&recipe.id) || !ids.insert(&recipe.id) {
                    return Err(format!(
                        "Profile {}: duplicate/invalid recipe ID",
                        profile.id
                    ));
                }
                validate_arguments(&recipe.executable, &recipe.arguments)?;
                validate_relative(&recipe.working_directory)?;
                validate_timeout(recipe.timeout_ms)?;
                // Validate bindings even in profiles that are not selected yet.
                let probe = ProjectConfig {
                    id: "validation".into(),
                    workspace_root: std::env::temp_dir(),
                    allow_direct_commands: false,
                    max_concurrent_mutations: 1,
                    recipes: Vec::new(),
                    recipe_profile: None,
                };
                expand(&recipe.executable, &probe)?;
                for argument in &recipe.arguments {
                    expand(argument, &probe)?;
                }
                if recipe.working_directory.contains("${") {
                    return Err("Profile workingDirectory must be a fixed relative path; bindings are allowed only in executable/arguments".into());
                }
            }
        }
        Ok(())
    }
    pub fn profile_for(&self, project: &ProjectConfig) -> Result<Option<&RecipeProfileConfig>> {
        project
            .recipe_profile
            .as_ref()
            .map(|id| {
                self.recipe_profiles
                    .iter()
                    .find(|profile| profile.id == *id)
                    .ok_or_else(|| {
                        format!(
                            "Project {} references unknown recipeProfile {id:?}",
                            project.id
                        )
                    })
            })
            .transpose()
    }
    pub fn project_recipes(&self, project: &ProjectConfig) -> Result<Vec<RecipeConfig>> {
        let mut recipes = Vec::new();
        if let Some(profile) = self.profile_for(project)? {
            for recipe in &profile.recipes {
                let expanded = RecipeConfig {
                    executable: expand(&recipe.executable, project)?,
                    arguments: recipe
                        .arguments
                        .iter()
                        .map(|a| expand(a, project))
                        .collect::<Result<_>>()?,
                    ..recipe.clone()
                };
                validate_arguments(&expanded.executable, &expanded.arguments)?;
                recipes.push(expanded);
            }
        }
        recipes.extend(project.recipes.clone());
        if recipes.len() > 128 {
            return Err(format!(
                "Project {}: at most 128 effective recipes",
                project.id
            ));
        }
        let mut ids = HashSet::new();
        for recipe in &recipes {
            if !ids.insert(&recipe.id) {
                return Err(format!("Project {}: recipe {} conflicts with its profile; implicit overrides are not supported", project.id, recipe.id));
            }
        }
        Ok(recipes)
    }
    pub fn recipe_origin(&self, project: &ProjectConfig, recipe_id: &str) -> Result<RecipeOrigin> {
        let profile = self
            .profile_for(project)?
            .filter(|profile| profile.recipes.iter().any(|recipe| recipe.id == recipe_id));
        Ok(RecipeOrigin {
            recipe_id: recipe_id.into(),
            profile_id: profile.map(|p| p.id.clone()),
            profile_label: profile.map(|p| p.label.clone()),
        })
    }
}
