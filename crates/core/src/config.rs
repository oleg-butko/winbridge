//! Configuration and project policy; no process execution or fixed deployment paths.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use winbridge_protocol::{ProjectInfo, RecipeInfo, ResolvedCommand};

pub type Result<T> = std::result::Result<T, String>;

pub const DESKTOP_CONFIG_FILE: &str = "winbridge-desktop.local.json";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopConfig {
    pub schema_version: u16,
    pub worker_config_path: PathBuf,
    #[serde(default)]
    pub report_directory: Option<PathBuf>,
}

impl DesktopConfig {
    pub fn load_report_directory(path: &Path) -> Result<PathBuf> {
        let config: Self = read_config(path)?;
        config.resolve_worker_config(path)?;
        let directory = config.report_directory.unwrap_or_else(|| {
            if cfg!(windows) {
                PathBuf::from(r"K:\VMs\shared\WinBridge\diagnostics")
            } else {
                path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("diagnostics")
            }
        });
        if !directory.is_absolute()
            || directory
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(
                "reportDirectory must be an absolute directory without parent traversal".into(),
            );
        }
        Ok(directory)
    }
    pub fn load(path: &Path) -> Result<PathBuf> {
        let config: Self = read_config(path)?;
        config.resolve_worker_config(path)
    }

    fn resolve_worker_config(&self, path: &Path) -> Result<PathBuf> {
        if self.schema_version != 1 {
            return Err(format!(
                "config {}: schemaVersion {} is unsupported; expected 1",
                path.display(),
                self.schema_version
            ));
        }
        if self.worker_config_path.as_os_str().is_empty() {
            return Err(format!(
                "config {}: workerConfigPath must name the worker configuration file",
                path.display()
            ));
        }
        if !self.worker_config_path.is_absolute()
            && self.worker_config_path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
        {
            return Err(format!(
                "config {}: workerConfigPath must be absolute or relative to this file, without a drive-relative/root-relative prefix",
                path.display()
            ));
        }
        if self.worker_config_path.is_absolute() {
            Ok(self.worker_config_path.clone())
        } else {
            Ok(path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&self.worker_config_path))
        }
    }
}

pub(crate) fn read_config<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("config {}: {e}", path.display()))?;
    if bytes.len() > 1024 * 1024 {
        return Err(format!("config {} exceeds 1 MiB", path.display()));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("config {}: {e}", path.display()))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerConfig {
    pub schema_version: u16,
    pub instance_id: String,
    pub node_id: String,
    pub state_directory: PathBuf,
    pub worker: WorkerSettings,
    pub delivery: Option<DeliverySettings>,
    pub projects: Vec<ProjectConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_profiles: Vec<crate::profiles::RecipeProfileConfig>,
    #[serde(default = "default_output_limit")]
    pub max_output_bytes_per_task: u64,
    #[serde(default)]
    pub output_retention: OutputRetention,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputRetention {
    pub automatic: bool,
    pub older_than_days: u16,
}
impl Default for OutputRetention {
    fn default() -> Self {
        Self {
            automatic: false,
            older_than_days: 30,
        }
    }
}
fn default_output_limit() -> u64 {
    16 * 1024 * 1024
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerSettings {
    pub controller_url: Option<String>,
    pub keep_running_on_ui_exit: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliverySettings {
    pub adapter: String,
    pub root: PathBuf,
    pub namespace: String,
    #[serde(default)]
    pub peer_root: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectConfig {
    pub id: String,
    pub workspace_root: PathBuf,
    pub allow_direct_commands: bool,
    pub max_concurrent_mutations: u16,
    pub recipes: Vec<RecipeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeConfig {
    pub id: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub timeout_ms: u64,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl DeliverySettings {
    pub fn validate(&self) -> Result<()> {
        if self.adapter != "shared-folder" || !self.root.is_absolute() || !valid_id(&self.namespace)
        {
            return Err(
                "delivery requires shared-folder, absolute root and a safe namespace ID".into(),
            );
        }
        validate_relative(&self.namespace)
    }
}

impl WorkerConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = read_config(path)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !(1..=3650).contains(&self.output_retention.older_than_days) {
            return Err("outputRetention.olderThanDays must be 1–3650 days".into());
        }
        if let Some(delivery) = &self.delivery {
            delivery.validate()?;
        }
        if self.schema_version != 1 {
            return Err(format!(
                "schemaVersion {} is unsupported; expected 1",
                self.schema_version
            ));
        }
        if !valid_id(&self.instance_id) || !valid_id(&self.node_id) {
            return Err("instanceId and nodeId must use 1–80 letters, digits, '-' or '_'".into());
        }
        if !self.state_directory.is_absolute() {
            return Err("stateDirectory must be an absolute path on a local disk".into());
        }
        if !self.worker.keep_running_on_ui_exit {
            return Err("worker.keepRunningOnUiExit must be true: tasks belong to worker".into());
        }
        if let Some(url) = &self.worker.controller_url {
            crate::controller_config::validate_controller_url(url)?;
        }
        if !(4096..=256 * 1024 * 1024).contains(&self.max_output_bytes_per_task) {
            return Err("maxOutputBytesPerTask must be 4096–268435456 bytes".into());
        }
        if self.projects.is_empty() || self.projects.len() > 32 {
            return Err("projects must contain 1–32 registered projects".into());
        }
        self.validate_recipe_profiles()?;
        let mut ids = HashSet::new();
        let mut roots: Vec<PathBuf> = Vec::new();
        for project in &self.projects {
            if !valid_id(&project.id) || !ids.insert(&project.id) {
                return Err(format!(
                    "projects.id {:?}: invalid or duplicate ID",
                    project.id
                ));
            }
            if project.max_concurrent_mutations != 1 {
                return Err(format!(
                    "project {}: maxConcurrentMutations must be 1 in stage 1",
                    project.id
                ));
            }
            if !project.workspace_root.is_absolute() {
                return Err(format!(
                    "project {}: workspaceRoot must be absolute",
                    project.id
                ));
            }
            let root = fs::canonicalize(&project.workspace_root).map_err(|e| {
                format!(
                    "project {}: workspaceRoot {}: {e}; create/select an existing local directory",
                    project.id,
                    project.workspace_root.display()
                )
            })?;
            if !root.is_dir() {
                return Err(format!(
                    "project {}: workspaceRoot is not a directory",
                    project.id
                ));
            }
            if roots
                .iter()
                .any(|r| contains_path(r, &root) || contains_path(&root, r))
            {
                return Err("registered workspace roots must not overlap".into());
            }
            roots.push(root);
            let mut recipe_ids = HashSet::new();
            for recipe in &self.project_recipes(project)? {
                if !valid_id(&recipe.id) || !recipe_ids.insert(&recipe.id) {
                    return Err(format!(
                        "project {}: invalid/duplicate recipe ID {:?}",
                        project.id, recipe.id
                    ));
                }
                validate_arguments(&recipe.executable, &recipe.arguments)?;
                validate_timeout(recipe.timeout_ms)?;
                scoped_directory(&project.workspace_root, &recipe.working_directory)?;
            }
            if let Some(delivery) = &self.delivery {
                if delivery.adapter != "shared-folder" || !valid_id(&delivery.namespace) {
                    return Err(
                        "delivery requires adapter shared-folder and a valid namespace".into(),
                    );
                }
                if paths_overlap(&project.workspace_root, &delivery.root)
                    || paths_overlap(&self.state_directory, &delivery.root)
                {
                    return Err("workspaceRoot and stateDirectory must be outside the shared delivery folder".into());
                }
            }
        }
        self.public_projects()?;
        Ok(())
    }

    pub fn public_projects(&self) -> Result<Vec<ProjectInfo>> {
        let projects = self.projects.iter().map(|p| -> Result<ProjectInfo> {
            Ok(ProjectInfo {
                id: p.id.clone(),
                workspace_root: p.workspace_root.to_string_lossy().into_owned(),
                allow_direct_commands: p.allow_direct_commands,
                recipe_profile: self.profile_for(p)?.map(|profile| profile.info()),
                recipes: self
                    .project_recipes(p)?
                    .iter()
                    .map(|r| {
                        Ok(RecipeInfo {
                            id: r.id.clone(),
                            origin: Some(self.recipe_origin(p, &r.id)?),
                            command: ResolvedCommand {
                                executable: r.executable.clone(),
                                arguments: r.arguments.clone(),
                                working_directory: r.working_directory.clone(),
                                timeout_ms: r.timeout_ms,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        });
        // Reuse can expand a small config into a much larger read model. Keep
        // Info below the transport frame budget, including JSON escaping.
        let mut result = Vec::new();
        let mut bytes = 0;
        for project in projects {
            let project = project?;
            bytes += serde_json::to_vec(&project)
                .map_err(|e| e.to_string())?
                .len();
            if bytes > 512 * 1024 {
                return Err("Expanded project/recipe metadata exceeds 512 KiB; reduce recipes/arguments before saving".into());
            }
            result.push(project);
        }
        Ok(result)
    }
}

fn paths_overlap(a: &Path, b: &Path) -> bool {
    let normalized = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let (a, b) = (normalized(a), normalized(b));
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}
pub fn validate_timeout(ms: u64) -> Result<()> {
    if !(100..=24 * 60 * 60 * 1000).contains(&ms) {
        return Err("timeoutMs must be between 100 and 86400000 (24 hours)".into());
    }
    Ok(())
}
pub fn validate_arguments(executable: &str, arguments: &[String]) -> Result<()> {
    if executable.is_empty() || executable.len() > 32768 || executable.contains('\0') {
        return Err("executable must be nonempty and contain no NUL".into());
    }
    if arguments.len() > 256
        || arguments.iter().any(|a| a.contains('\0'))
        || arguments.iter().map(String::len).sum::<usize>() > 32768
    {
        return Err("arguments exceed 256 items/32768 bytes or contain NUL".into());
    }
    if [".cmd", ".bat"]
        .iter()
        .any(|suffix| executable.to_lowercase().ends_with(suffix))
    {
        return Err(
            "batch files require an explicit cmd.exe executable; shell execution must be visible"
                .into(),
        );
    }
    Ok(())
}

pub fn validate_relative(relative: &str) -> Result<()> {
    if relative.is_empty() || relative.contains(['\0', ':']) || relative.starts_with(['/', '\\']) {
        return Err("workingDirectory must be a relative project path; drive/UNC/device/ADS paths are forbidden".into());
    }
    for segment in relative.split(['/', '\\']) {
        if segment == "." {
            continue;
        }
        if segment.is_empty()
            || segment == ".."
            || segment.ends_with([' ', '.'])
            || segment
                .chars()
                .any(|c| c.is_control() || "<>\"|?*".contains(c))
        {
            return Err(format!("invalid Windows path component {segment:?}: traversal, empty components, trailing dots/spaces and reserved characters are forbidden"));
        }
        let stem = segment.split('.').next().unwrap_or_default().to_uppercase();
        if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"].contains(&stem.as_str())
            || (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.chars().count() == 4
                && "123456789¹²³".contains(stem.chars().last().unwrap())
        {
            return Err(format!("reserved Windows name {segment:?}"));
        }
    }
    Ok(())
}
pub fn scoped_directory(root: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative(relative)?;
    let root =
        fs::canonicalize(root).map_err(|e| format!("workspaceRoot {}: {e}", root.display()))?;
    let path = fs::canonicalize(root.join(relative.replace('\\', "/"))).map_err(|e| {
        format!(
            "workingDirectory {relative:?}: {e}; choose an existing directory inside the project"
        )
    })?;
    if !contains_path(&root, &path) || !path.is_dir() {
        return Err(format!("workingDirectory {relative:?} escapes workspaceRoot or is not a directory (check symlinks/junctions)"));
    }
    Ok(path)
}

pub fn contains_path(root: &Path, path: &Path) -> bool {
    #[cfg(windows)]
    {
        let normalized = |p: &Path| {
            p.to_string_lossy()
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_lowercase()
        };
        let root = normalized(root);
        let path = normalized(path);
        path == root || path.starts_with(&format!("{root}/"))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

#[cfg(test)]
mod desktop_config_tests {
    use super::*;

    #[test]
    fn report_directory_preserves_custom_path_and_accepts_legacy_config() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(DESKTOP_CONFIG_FILE);
        let mut json =
            serde_json::json!({"schemaVersion":1,"workerConfigPath":"worker.local.json"});
        fs::write(&path, json.to_string()).unwrap();
        let expected = if cfg!(windows) {
            PathBuf::from(r"K:\VMs\shared\WinBridge\diagnostics")
        } else {
            temp.path().join("diagnostics")
        };
        assert_eq!(
            DesktopConfig::load_report_directory(&path).unwrap(),
            expected
        );
        let selected = temp.path().join("selected reports");
        json["reportDirectory"] = serde_json::json!(selected);
        fs::write(&path, json.to_string()).unwrap();
        assert_eq!(
            DesktopConfig::load_report_directory(&path).unwrap(),
            selected
        );
        for invalid in [
            PathBuf::from("relative/reports"),
            temp.path().join("reports/../outside"),
        ] {
            json["reportDirectory"] = serde_json::json!(invalid);
            fs::write(&path, json.to_string()).unwrap();
            assert!(DesktopConfig::load_report_directory(&path).is_err());
        }
    }

    #[test]
    fn relative_worker_config_is_resolved_beside_desktop_config() {
        let location = std::env::temp_dir()
            .join("WinBridge folder with spaces")
            .join(DESKTOP_CONFIG_FILE);
        let config: DesktopConfig =
            serde_json::from_str(include_str!("../../../examples/desktop.config.json")).unwrap();
        assert_eq!(
            config.resolve_worker_config(&location).unwrap(),
            location.parent().unwrap().join("worker.local.json")
        );
    }

    #[test]
    fn absolute_worker_config_keeps_its_location() {
        let worker_config = std::env::temp_dir().join("separate worker/worker.local.json");
        let config = DesktopConfig {
            schema_version: 1,
            worker_config_path: worker_config.clone(),
            report_directory: None,
        };
        assert_eq!(
            config
                .resolve_worker_config(Path::new(DESKTOP_CONFIG_FILE))
                .unwrap(),
            worker_config
        );
    }

    #[test]
    fn desktop_config_rejects_unknown_fields_bad_version_and_empty_path() {
        assert!(serde_json::from_str::<DesktopConfig>(
            r#"{"schemaVersion":1,"workerConfigPath":"worker.local.json","extra":true}"#
        )
        .is_err());
        for json in [
            r#"{"schemaVersion":2,"workerConfigPath":"worker.local.json"}"#,
            r#"{"schemaVersion":1,"workerConfigPath":""}"#,
        ] {
            let config: DesktopConfig = serde_json::from_str(json).unwrap();
            let error = config
                .resolve_worker_config(Path::new(DESKTOP_CONFIG_FILE))
                .unwrap_err();
            assert!(error.contains(DESKTOP_CONFIG_FILE));
        }
    }

    #[cfg(windows)]
    #[test]
    fn desktop_config_rejects_ambiguous_windows_paths() {
        for path in [
            r"C:worker.local.json",
            r"\worker.local.json",
            "/worker.local.json",
        ] {
            let config = DesktopConfig {
                schema_version: 1,
                worker_config_path: PathBuf::from(path),
                report_directory: None,
            };
            assert!(config
                .resolve_worker_config(Path::new(r"C:\desktop\winbridge-desktop.local.json"))
                .is_err());
        }
    }
}
