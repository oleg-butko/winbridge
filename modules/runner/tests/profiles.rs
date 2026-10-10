use std::{fs, path::Path};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;

fn config(root: &Path) -> WorkerConfig {
    for name in ["first project ✓", "second"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"instanceId":"profiles","nodeId":"test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},
        "recipeProfiles":[{"id":"shared","label":"Общие команды","description":"Два проекта","recipes":[{"id":"build","executable":"cargo","arguments":["build","--target-dir","${workspaceRoot}/cache","${projectId}","Привет ✓"],"workingDirectory":".","timeoutMs":10000}]}],
        "projects":[{"id":"first","workspaceRoot":root.join("first project ✓"),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[],"recipeProfile":"shared"},{"id":"second","workspaceRoot":root.join("second"),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[],"recipeProfile":"shared"}]
    })).unwrap()
}
fn request() -> TaskSubmission {
    serde_json::from_value(serde_json::json!({"protocolVersion":1,"requestId":"test","projectId":"first","idempotencyKey":"test","timeoutMs":20000,"command":{"kind":"recipe","recipeId":"build","arguments":[]}})).unwrap()
}
#[test]
fn bindings_remain_registered_paths_in_snapshot_commands_and_use_single_argv() {
    let temp = TempDir::new().unwrap();
    let config = config(temp.path());
    config.validate().unwrap();
    let root = &config.projects[0].workspace_root;
    let generation = root.join("snapshots/verified");
    fs::create_dir_all(&generation).unwrap();
    let command =
        winbridge_runner::resolve_for_generation(&config, &request(), &generation).unwrap();
    assert_eq!(command.arguments[2], format!("{}/cache", root.display()));
    assert_eq!(command.arguments[3], "first");
    assert_eq!(command.arguments[4], "Привет ✓");
    assert_eq!(command.timeout_ms, 10000);
    assert_eq!(
        Path::new(&command.working_directory),
        fs::canonicalize(generation).unwrap()
    );
    let public = config.public_projects().unwrap();
    assert_eq!(public[0].recipe_profile.as_ref().unwrap().id, "shared");
    assert_eq!(public[1].recipes[0].command.arguments[3], "second");
}
#[test]
fn invalid_profiles_and_policy_are_rejected_before_execution() {
    let temp = TempDir::new().unwrap();
    let config = config(temp.path());
    let mut direct = request();
    direct.command = CommandSource::Direct {
        executable: "cargo".into(),
        arguments: vec![],
        working_directory: ".".into(),
    };
    assert!(winbridge_runner::resolve(&config, &direct)
        .unwrap_err()
        .contains("does not allow"));
    let mut extra = request();
    if let CommandSource::Recipe { arguments, .. } = &mut extra.command {
        arguments.push("override".into());
    }
    assert!(winbridge_runner::resolve(&config, &extra).is_err());
    let mut cases = Vec::new();
    let mut bad = config.clone();
    bad.projects[0].recipes = bad.recipe_profiles[0].recipes.clone();
    cases.push(bad);
    let mut bad = config.clone();
    bad.projects[0].recipe_profile = Some("missing".into());
    cases.push(bad);
    let mut bad = config.clone();
    bad.recipe_profiles[0].recipes[0].arguments = vec!["${HOME}".into()];
    cases.push(bad);
    let mut bad = config.clone();
    bad.recipe_profiles[0].recipes[0].arguments = vec!["${projectId".into()];
    cases.push(bad);
    let mut bad = config.clone();
    bad.recipe_profiles[0].recipes[0].working_directory = "${workspaceRoot}".into();
    cases.push(bad);
    let mut bad = config.clone();
    bad.recipe_profiles.push(bad.recipe_profiles[0].clone());
    cases.push(bad);
    let mut bad = config.clone();
    bad.recipe_profiles[0].recipes[0].arguments =
        vec!["x".repeat(32768), "${workspaceRoot}".into()];
    cases.push(bad);
    for bad in cases {
        assert!(bad.validate().is_err());
    }
}
#[test]
fn profile_reuse_cannot_overflow_info_frame_and_local_recipes_keep_literal_args() {
    let temp = TempDir::new().unwrap();
    let mut config = config(temp.path());
    let template = config.recipe_profiles[0].recipes[0].clone();
    config.recipe_profiles[0].recipes = (0..32)
        .map(|n| {
            let mut r = template.clone();
            r.id = format!("recipe-{n}");
            r.arguments = vec!["x".repeat(16000)];
            r
        })
        .collect();
    assert!(config.validate().unwrap_err().contains("512 KiB"));
    let mut config = self::config(temp.path());
    config.recipe_profiles.clear();
    config.projects[0].recipe_profile = None;
    config.projects[1].recipe_profile = None;
    let mut local = template;
    local.arguments = vec!["${HOME}".into()];
    config.projects[0].recipes.push(local);
    config.validate().unwrap();
    assert_eq!(
        winbridge_runner::resolve(&config, &request())
            .unwrap()
            .arguments[0],
        "${HOME}"
    );
}
#[test]
fn bindings_do_not_recursively_interpret_substituted_values() {
    let temp = TempDir::new().unwrap();
    let mut config = config(temp.path());
    let root = temp.path().join("literal-${projectId}");
    fs::create_dir(&root).unwrap();
    config.projects[0].workspace_root = root.clone();
    config.validate().unwrap();
    assert_eq!(
        winbridge_runner::resolve(&config, &request())
            .unwrap()
            .arguments[2],
        format!("{}/cache", root.display())
    );
}
