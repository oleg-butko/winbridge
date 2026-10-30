//! Real closed-loop OS fixture with synthetic role history and Ready claim.
//! No bootstrap creation, updater SQL handoff or installed runtime is exercised.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Command, Stdio},
};
use winbridge_platform::{infrastructure::PinnedProcess, ManagedChild};
const KEEPER: &str = "supervisor::keeper_control::exit_tests::keeper_helper";
const WORKER: &str = "supervisor::keeper_control::exit_tests::worker_helper";
const PASSIVE: &str = "supervisor::keeper_control::exit_tests::passive_helper";
const TOKEN: &str = "9999999999999999999999999999999999999999999999999999999999999999";
fn args(test: &str) -> Vec<String> {
    vec![
        "--exact".into(),
        test.into(),
        "--ignored".into(),
        "--test-threads=1".into(),
        "--nocapture".into(),
    ]
}
fn publish(path: &Path, value: &impl Serialize) {
    let pending = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)
        .unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(!path.exists());
    fs::rename(pending, path).unwrap();
}
async fn exists(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "Missing fixture marker {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
async fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    exists(path).await;
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len() <= 65536);
    serde_json::from_slice(&bytes).unwrap()
}
async fn exited(pin: &PinnedProcess) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !pin.exited().unwrap() {
        assert!(
            Instant::now() < deadline,
            "Exact helper exit unconfirmed: {:?}",
            pin.identity()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerInput {
    binding: KeeperBinding,
    launch_id: String,
    ticket_sha256: String,
    successor: SupervisorProcess,
    confirm: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Roles {
    root: PathBuf,
    binding: KeeperBinding,
    leaf: SupervisorProcess,
    successor: SupervisorProcess,
    jobs: [String; 2],
    record: SupervisorRecord,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    root: PathBuf,
    proof: PathBuf,
}
#[tokio::test]
#[ignore = "Owned local OS helper, launched only by the explicit native keeper-exit case"]
async fn passive_helper() {
    let directory = std::env::current_dir().unwrap();
    let role = directory.file_name().unwrap().to_str().unwrap();
    assert!(matches!(role, "leaf" | "successor"));
    let root = directory.parent().unwrap();
    let identity = infrastructure::stamp(std::process::id()).unwrap();
    publish(&root.join(format!("{role}-ready.json")), &identity);
    exists(&root.join(format!("stop-{role}"))).await;
    publish(&root.join(format!("{role}-stopped.json")), &identity);
}
#[tokio::test]
#[ignore = "Owned original worker helper, never a production Active worker"]
// Windows-only fixture deliberately leaves the leaf in its ORIGINAL owned Job.
// Its exact kernel exit is confirmed by the outer fixture after this root exits.
#[allow(clippy::zombie_processes)]
async fn worker_helper() {
    let root = std::env::current_dir().unwrap();
    let input: WorkerInput = read(&root.join("worker-input.json")).await;
    assert_eq!(
        input.binding.worker,
        infrastructure::stamp(std::process::id()).unwrap()
    );
    fs::create_dir(root.join("leaf")).unwrap();
    let _leaf = Command::new(std::env::current_exe().unwrap())
        .args(args(PASSIVE))
        .current_dir(root.join("leaf"))
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .spawn()
        .unwrap();
    let _: SupervisorProcess = read(&root.join("leaf-ready.json")).await;
    exists(&root.join("server-start")).await;
    if input.confirm {
        let q = KeeperLaunchRequest {
            keeper_launch_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: input.binding.clone(),
            command: KeeperLaunchCommand::ConfirmReady {
                launch_id: input.launch_id,
                ticket_sha256: input.ticket_sha256,
                process: input.successor,
            },
        };
        let endpoint = ipc::endpoint(&root, &input.binding.launch_id).unwrap();
        let response: KeeperLaunchResponse =
            ipc::call(&endpoint, input.binding.coordinator.process_id, &q)
                .await
                .unwrap();
        assert!(
            matches!(response.result,KeeperLaunchResult::Status {ref status} if status.phase==KeeperLaunchPhase::ReadyConfirmed)
        );
    }
    publish(&root.join("worker-stage-ready"), &input.confirm);
    exists(&root.join("stop-worker")).await;
    // std::Child Drop leaves the inherited leaf alive in the ORIGINAL Job.
}
#[tokio::test]
#[ignore = "Owned closed-keeper helper; synthetic history, real production loop"]
async fn keeper_helper() {
    let root = std::env::current_dir().unwrap();
    let confirm: bool = read(&root.join("confirm.json")).await;
    let exe = std::env::current_exe().unwrap();
    let keeper = infrastructure::stamp(std::process::id()).unwrap();
    let mut model = crate::bootstrap::tests::fixture();
    let job = format!("Local\\WinBridge-Exit-{}", uuid::Uuid::new_v4());
    let command = winbridge_protocol::ResolvedCommand {
        executable: exe.to_str().unwrap().into(),
        arguments: args(WORKER),
        working_directory: root.to_str().unwrap().into(),
        timeout_ms: 20000,
    };
    let (mut worker, stdout, stderr) = ManagedChild::prepare(&command, Some(&job)).unwrap();
    let binding = &mut model.previous;
    binding.update_id = uuid::Uuid::new_v4().to_string();
    binding.launch_id = uuid::Uuid::new_v4().to_string();
    binding.generation = uuid::Uuid::new_v4().to_string();
    binding.coordinator = keeper.clone();
    binding.worker = infrastructure::stamp(worker.id()).unwrap();
    binding.job_name = job.clone();
    let domain_id =
        ExecutionDomain::identity_for("keeper-exit-fixture", &uuid::Uuid::new_v4().to_string())
            .unwrap()
            .unwrap();
    let domain = ExecutionDomain::open(&domain_id).unwrap();
    let jobs = [job, domain.identity().job_name.clone()];
    fs::create_dir(root.join("successor")).unwrap();
    let parent = PinnedProcess::live(&keeper).unwrap();
    let child = InfrastructureChild::spawn_excluding_jobs(
        &parent,
        [&jobs[0], &jobs[1]],
        &exe,
        &args(PASSIVE),
        &root.join("successor"),
        [
            File::create(root.join("successor.stdout")).unwrap(),
            File::create(root.join("successor.stderr")).unwrap(),
        ],
    )
    .unwrap();
    let successor = child.identity.clone();
    let ready: SupervisorProcess = read(&root.join("successor-ready.json")).await;
    assert_eq!(ready, successor);
    let file = |f: &crate::bootstrap::BootstrapFile| KeeperLaunchFile {
        sha256: f.sha256.clone(),
        bytes: f.bytes,
        file_identity: f.file_identity.clone(),
    };
    let ticket = KeeperLaunchTicket {
        update_id: uuid::Uuid::new_v4().to_string(),
        launch_id: uuid::Uuid::new_v4().to_string(),
        image: file(&model.image),
        config: file(&model.config),
        stdout_identity: model.stdout_identity,
        stderr_identity: model.stderr_identity,
        token: "7".repeat(64),
        excluded_job: jobs[1].clone(),
        predecessor: None,
        prepared_at_ms: now().unwrap(),
    };
    let actor = keeper_launch::Actor::exit_test_actor(child, binding, &ticket).unwrap();
    let endpoint = ipc::endpoint(&root, &binding.launch_id).unwrap();
    let listener = ipc::Listener::bind(&endpoint).unwrap();
    let record = SupervisorRecord {
        schema_version: 1,
        spec: SupervisorSpec {
            launch_id: binding.launch_id.clone(),
            update_id: binding.update_id.clone(),
            parent: binding.worker.clone(),
            parent_owner_id: uuid::Uuid::new_v4().to_string(),
            parent_epoch: 1,
            token_sha256: sha256(TOKEN.as_bytes()),
            config_path: "synthetic config".into(),
            config_sha256: "a".repeat(64),
            runtime_directory: root.to_str().unwrap().into(),
            image_path: "synthetic image".into(),
            image_sha256: "b".repeat(64),
            image_identity: "synthetic".into(),
            excluded_domain: Some(jobs[1].clone()),
            endpoint,
            accepted_at_ms: now().unwrap(),
            worker: None,
        },
        phase: SupervisorPhase::Active,
        revision: 3,
        process: Some(keeper),
        owner_id: Some(uuid::Uuid::new_v4().to_string()),
        owner_epoch: Some(2),
        updated_at_ms: now().unwrap(),
        reason: "Synthetic closed keeper history; no SQL retirement exercised".into(),
    };
    publish(
        &root.join("worker-input.json"),
        &WorkerInput {
            binding: binding.clone(),
            launch_id: ticket.launch_id.clone(),
            ticket_sha256: keeper_launch::ticket_sha(&ticket).unwrap(),
            successor: successor.clone(),
            confirm,
        },
    );
    worker.resume().unwrap();
    let leaf: SupervisorProcess = read(&root.join("leaf-ready.json")).await;
    PinnedProcess::live(&leaf)
        .unwrap()
        .require_job_membership(&binding.job_name)
        .unwrap();
    let custody = crate::activation::ActivationKeeperCustody::exit_test_custody(
        worker,
        binding.clone(),
        [stdout, stderr],
    )
    .unwrap();
    publish(
        &root.join("roles.json"),
        &Roles {
            root: root.clone(),
            binding: binding.clone(),
            leaf,
            successor,
            jobs,
            record: record.clone(),
            confirm,
        },
    );
    publish(&root.join("server-start"), &true);
    run_state(
        listener,
        record,
        State::Keeper(Box::new(custody)),
        actor,
        None,
    )
    .await
    .unwrap();
    drop(domain);
}
async fn status(roles: &Roles) -> KeeperStatus {
    let q = KeeperControlRequest {
        keeper_control_version: 1,
        update_id: roles.binding.update_id.clone(),
        launch_id: roles.binding.launch_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        expected_coordinator_revision: roles.record.revision,
        token: TOKEN.into(),
        command: KeeperControlCommand::Observe,
    };
    let r: KeeperControlResponse = ipc::call(
        &roles.record.spec.endpoint,
        roles.binding.coordinator.process_id,
        &q,
    )
    .await
    .unwrap();
    assert_eq!(r.request_id.as_ref(), Some(&q.request_id));
    match r.result {
        KeeperControlResult::Status { status } => *status,
        _ => panic!("Original status rejected"),
    }
}
fn ticks(child: &std::process::Child) -> u64 {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
    let (mut born, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
        unsafe { std::mem::zeroed() };
    assert_ne!(
        unsafe {
            GetProcessTimes(
                child.as_raw_handle(),
                &mut born,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        },
        0
    );
    let value = |v: FILETIME| ((v.dwHighDateTime as u64) << 32) | v.dwLowDateTime as u64;
    value(kernel) + value(user)
}
async fn idle(child: &std::process::Child) -> serde_json::Value {
    let before = ticks(child);
    let started = Instant::now();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let elapsed = started.elapsed().as_nanos();
    let delta = ticks(child) - before;
    assert!(
        (delta as u128) * 100 <= elapsed / 4,
        "Closed keeper consumed >25% of one CPU during an idle sample"
    );
    serde_json::json!({"elapsedNs":elapsed,"cpuTicks100ns":delta})
}
#[tokio::test]
#[ignore = "Explicit native closed-loop/root/Job/readers/idle fixture, no updater SQL or network"]
async fn keeper_exit_waits_for_known_successor_original_job_and_readers() {
    let input: Input = serde_json::from_slice(
        &fs::read(
            std::env::var_os("WINBRIDGE_KEEPER_EXIT_NATIVE_INPUT")
                .expect("Explicit native input required"),
        )
        .unwrap(),
    )
    .unwrap();
    winbridge_platform::require_local_path_prefix(&input.root).unwrap();
    winbridge_platform::require_safe_path(&input.root).unwrap();
    winbridge_platform::require_local_disk(&input.root).unwrap();
    let output = input.proof.parent().unwrap();
    let mut cases = Vec::new();
    for (mode, confirm) in [("confirmed", true), ("unconfirmed", false)] {
        let root = input.root.join(mode);
        fs::create_dir(&root).unwrap();
        publish(&root.join("confirm.json"), &confirm);
        let mut keeper = Command::new(std::env::current_exe().unwrap())
            .args(args(KEEPER))
            .current_dir(&root)
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
            .stdout(Stdio::from(
                File::create(root.join("keeper.stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                File::create(root.join("keeper.stderr")).unwrap(),
            ))
            .spawn()
            .unwrap();
        let keeper_pin = PinnedProcess::live(&infrastructure::stamp(keeper.id()).unwrap()).unwrap();
        let roles: Roles = read(&root.join("roles.json")).await;
        assert_eq!(keeper_pin.identity(), &roles.binding.coordinator);
        assert_eq!(
            roles.root.canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        assert_eq!(roles.confirm, confirm);
        let worker = PinnedProcess::live(&roles.binding.worker).unwrap();
        let leaf = PinnedProcess::live(&roles.leaf).unwrap();
        let successor = PinnedProcess::live(&roles.successor).unwrap();
        worker.require_job_membership(&roles.jobs[0]).unwrap();
        leaf.require_job_membership(&roles.jobs[0]).unwrap();
        successor.require_outside_job(&roles.jobs[0]).unwrap();
        successor.require_outside_job(&roles.jobs[1]).unwrap();
        let confirmed: bool = read(&root.join("worker-stage-ready")).await;
        assert_eq!(confirmed, confirm);
        let before = status(&roles).await;
        assert_eq!(before.worker_alive, Some(true));
        assert_eq!(before.original_custody_held, Some(true));
        let mut idle_samples = vec![idle(&keeper).await];
        assert!(keeper.try_wait().unwrap().is_none());
        publish(&root.join("stop-worker"), &true);
        exited(&worker).await;
        leaf.require_live().unwrap();
        let after_root = status(&roles).await;
        assert_eq!(after_root.worker_alive, Some(false));
        assert_eq!(after_root.job_tree_empty, Some(false));
        assert_eq!(after_root.readers_finished, Some(false));
        assert_eq!(after_root.original_custody_held, Some(true));
        assert!(
            keeper.try_wait().unwrap().is_none(),
            "Root exit is not empty Job/readers"
        );
        publish(&root.join("stop-leaf"), &true);
        exited(&leaf).await;
        let unconfirmed_after_drain = if !confirm {
            let deadline = Instant::now() + Duration::from_secs(5);
            let drained = loop {
                let s = status(&roles).await;
                if s.job_tree_empty == Some(true) && s.readers_finished == Some(true) {
                    break s;
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(20)).await;
            };
            assert_eq!(drained.worker_alive, Some(false));
            assert_eq!(drained.original_custody_held, Some(true));
            idle_samples.push(idle(&keeper).await);
            assert!(
                keeper.try_wait().unwrap().is_none(),
                "No known successor: retain original keeper even after drain"
            );
            keeper.kill().unwrap();
            Some(drained)
        } else {
            None
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let exit = loop {
            if let Some(s) = keeper.try_wait().unwrap() {
                break s;
            }
            assert!(
                Instant::now() < deadline,
                "Keeper failed to exit after known successor and complete drain"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(exit.success(), confirm);
        assert!(keeper_pin.exited().unwrap());
        successor.require_live().unwrap();
        publish(&root.join("stop-successor"), &true);
        exited(&successor).await;
        let stopped: SupervisorProcess = read(&root.join("successor-stopped.json")).await;
        assert_eq!(stopped, roles.successor);
        let stdout_name = format!("{mode}-helper.stdout.log");
        let stderr_name = format!("{mode}-helper.stderr.log");
        for (name, path) in [
            (&stdout_name, root.join("keeper.stdout")),
            (&stderr_name, root.join("keeper.stderr")),
        ] {
            let bytes = fs::read(path).unwrap();
            assert!(bytes.len() <= 65536);
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join(name))
                .unwrap();
            f.write_all(&bytes).unwrap();
        }
        let stderr = fs::read_to_string(root.join("keeper.stderr")).unwrap();
        let events: Vec<serde_json::Value> = stderr
            .lines()
            .filter_map(|s| {
                s.strip_prefix("WinBridge keeper ")
                    .and_then(|j| serde_json::from_str(j).ok())
            })
            .filter(|v: &serde_json::Value| v["event"] == "original_keeper_exit")
            .collect();
        assert_eq!(events.len(), usize::from(confirm));
        if confirm {
            let event = &events[0];
            assert_eq!(event["keeperPid"], keeper_pin.identity().process_id);
            assert_eq!(event["workerPid"], roles.binding.worker.process_id);
            assert_eq!(event["jobTreeEmpty"], true);
            assert_eq!(event["readersFinished"], true);
            assert_eq!(event["workerAlive"], false);
        }
        cases.push(serde_json::json!({"mode":mode,"root":root,"keeper":keeper_pin.identity(),"worker":roles.binding.worker,"leaf":roles.leaf,"successor":roles.successor,"jobs":roles.jobs,"binding":roles.binding,"beforeRootExit":before,"afterRootExit":after_root,"unconfirmedAfterDrain":unconfirmed_after_drain,"idleSamples":idle_samples,"keeperExitSeen":true,"keeperExitedNaturally":confirm,"successorOutsideBothJobsBeforeExit":true,"successorSurvivedKeeper":true,"successorExitSeen":true,"workerExitSeen":true,"leafExitSeen":true,"stdoutLog":stdout_name,"stderrLog":stderr_name}));
    }
    let proof = serde_json::json!({"schemaVersion":1,"kind":"keeper_exit_native_proof","platform":"windows","passed":true,"build":winbridge_protocol::build_identity(),"scope":"real closed keeper loop/root exit/Job drain/independent successor; synthetic role history and Ready claim, no updater workflow","syntheticRoleHistory":true,"syntheticReadyClaim":true,"cases":cases,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false,"networkUsed":false,"bootstrapCreationProved":false,"updaterWorkflowProved":false});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input.proof)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&proof).unwrap())
        .unwrap();
}
