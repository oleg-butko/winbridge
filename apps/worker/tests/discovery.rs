use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;

fn config(root: &Path) -> WorkerConfig {
    fs::create_dir(root.join("project")).unwrap();
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":format!("catalog-{}",uuid::Uuid::new_v4()),"nodeId":"test","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":root.join("project"),"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()
}
async fn info(cfg: &WorkerConfig) -> WorkerInfo {
    let address = winbridge_transport::endpoint(&cfg.state_directory, &cfg.instance_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match tokio::time::timeout(
            Duration::from_secs(1),
            winbridge_transport::call(&address, &LocalRequest::Info),
        )
        .await
        {
            Ok(Ok(LocalResponse::Info { info })) => return *info,
            Ok(Ok(other)) => panic!("Unexpected Info reply: {other:?}"),
            _ => {
                assert!(
                    Instant::now() < deadline,
                    "Isolated worker Info did not become available"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_worker_ipc_advertises_build_bound_support_without_enabling_an_action() {
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    let worker = winbridge_worker::Worker::open(cfg.clone()).unwrap();
    let server = tokio::spawn(winbridge_worker::serve(Arc::clone(&worker)));
    let info = info(&cfg).await;
    let CatalogObservation::Known(catalog) = interpret_catalog(&info) else {
        panic!("IPC lost catalog")
    };
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 1));
    assert!(catalog.supports(WORKER_RPC_CONTRACT, 2));
    let schemas = interpret_contract_schemas(&info).expect("native schema metadata");
    assert_eq!(
        schemas,
        implemented_contract_schemas(info.build.clone().unwrap())
    );
    let policies = interpret_project_policies(&info).expect("native IPC policy metadata");
    assert_eq!(
        policies,
        winbridge_core::readiness::project_policy_catalog(&cfg, info.build.clone().unwrap())
            .unwrap()
    );
    if let Some(report) = std::env::var_os("WINBRIDGE_TASK_CAS_REPORT") {
        fs::write(Path::new(&report).join("policy-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"platform":std::env::consts::OS,"candidateBuild":info.build,"context":info.fenced_requests.as_ref().unwrap().context,"policies":policies,"schemas":schemas})).unwrap()).unwrap();
    }
    assert_eq!(info.active_task_count, Some(0));
    assert!(
        matches!(worker.handle(LocalRequest::ListTasks), LocalResponse::Tasks { tasks } if tasks.is_empty())
    );
    worker.stop();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit installed schema-6 reference executable; native gate runs it"]
async fn older_worker_info_is_readable_and_has_no_catalog_or_new_permission() {
    use std::{io::Read, thread};
    let executable =
        std::env::var_os("WINBRIDGE_TASK_CAS_REFERENCE_WORKER").expect("explicit reference worker");
    let temp = TempDir::new().unwrap();
    let cfg = config(temp.path());
    let config_path = temp.path().join("worker.local.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&cfg).unwrap()).unwrap();
    let command = ResolvedCommand {
        executable: Path::new(&executable).to_string_lossy().into_owned(),
        arguments: vec![
            "--config".into(),
            config_path.to_string_lossy().into_owned(),
            "serve".into(),
        ],
        working_directory: temp.path().to_string_lossy().into_owned(),
        timeout_ms: 15_000,
    };
    let (mut child, stdout, stderr) = winbridge_platform::ManagedChild::spawn(&command).unwrap();
    let drain = |reader: winbridge_platform::OutputReader| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.take(65537).read_to_end(&mut bytes).unwrap();
            assert!(bytes.len() <= 65536);
            bytes
        })
    };
    let out = drain(stdout);
    let err = drain(stderr);
    let info = info(&cfg).await;
    assert!(matches!(
        interpret_catalog(&info),
        CatalogObservation::Missing
    ));
    assert_eq!(info.instance_id, cfg.instance_id);
    assert!(info.project_policies.is_none());
    assert!(info.contract_schemas.is_none());
    assert_eq!(info.active_task_count, Some(0));
    assert!(info.fenced_requests.is_none());
    child.terminate_tree().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() || !child.tree_is_empty().unwrap() {
        assert!(
            Instant::now() < deadline,
            "Isolated reference cleanup not confirmed"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    out.join().unwrap();
    err.join().unwrap();
    if let Some(report) = std::env::var_os("WINBRIDGE_TASK_CAS_REPORT") {
        fs::write(Path::new(&report).join("discovery-reference-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"platform":if cfg!(windows){"windows"}else{"linux"},"candidateBuild":build_identity(),"referenceBuild":info.build,"catalogAbsent":true,"isolatedReferenceStopped":true})).unwrap()).unwrap();
    }
}
