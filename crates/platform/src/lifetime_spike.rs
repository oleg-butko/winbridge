//! Native OS experiment only. No installed worker, network, updater ledger or
//! new runtime capability. All helpers remain in the test transport's outer Job.
use crate::{
    execution_domain::ExecutionDomain,
    infrastructure::{self, InfrastructureChild, PinnedProcess, ProcessStamp},
    windows::Job,
    ManagedChild, OutputReader,
};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::ERROR_FILE_NOT_FOUND,
    System::{
        JobObjects::{
            JobObjectExtendedLimitInformation, OpenJobObjectW, QueryInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        SystemServices::JOB_OBJECT_QUERY,
        Threading::GetCurrentProcess,
    },
};

fn wait(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(45);
    while !condition() {
        assert!(
            Instant::now() < until,
            "Same lifetime fixture timed out; no replay"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
fn publish(root: &Path, name: &str, value: Value) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    // Readers wait for this separate marker, never for a partially written JSON.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("{name}.ready")))
        .unwrap()
        .sync_all()
        .unwrap();
}
fn read(root: &Path, name: &str) -> Value {
    wait(|| root.join(format!("{name}.ready")).is_file());
    let file = File::open(root.join(name)).unwrap();
    assert!(file.metadata().unwrap().len() <= 65536);
    serde_json::from_reader(file.take(65537)).unwrap()
}
fn own() -> ProcessStamp {
    infrastructure::stamp(std::process::id()).unwrap()
}
fn stamp(value: &Value) -> ProcessStamp {
    serde_json::from_value(value.clone()).unwrap()
}
fn limits(job: &Job) -> u32 {
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    assert_ne!(
        unsafe {
            QueryInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        },
        0
    );
    info.BasicLimitInformation.LimitFlags
}
fn member(name: &str) -> bool {
    let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let raw = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, name.as_ptr()) };
    if raw.is_null()
        && std::io::Error::last_os_error().raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
    {
        // After old keeper AND worker exit, destruction of their empty Job is
        // expected. Other errors cannot become negative membership evidence.
        return false;
    }
    assert!(!raw.is_null());
    // SAFETY: own exactly this newly opened query-only handle. No limits change.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut yes = 0;
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::JobObjects::IsProcessInJob(
                GetCurrentProcess(),
                handle.as_raw_handle(),
                &mut yes,
            )
        },
        0
    );
    yes != 0
}
fn args() -> Vec<String> {
    [
        "--exact",
        "lifetime_spike::helper",
        "--ignored",
        "--nocapture",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
fn command(root: &Path, role: &str) -> winbridge_protocol::ResolvedCommand {
    winbridge_protocol::ResolvedCommand {
        executable: std::env::current_exe().unwrap().to_str().unwrap().into(),
        arguments: args(),
        working_directory: root.join(role).to_str().unwrap().into(),
        timeout_ms: 45000,
    }
}
fn drain(mut input: OutputReader, mut log: File) -> thread::JoinHandle<std::io::Result<()>> {
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        let mut retained = 0;
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                return Ok(());
            }
            let keep = n.min(65536 - retained);
            log.write_all(&buffer[..keep])?;
            retained += keep;
        }
    })
}
struct Child {
    owner: ManagedChild,
    readers: Vec<thread::JoinHandle<std::io::Result<()>>>,
}
impl Child {
    fn start(root: &Path, role: &str, name: &str, domain: Option<&ExecutionDomain>) -> Self {
        let (mut owner, out, err) =
            ManagedChild::prepare_atomic_executable(&command(root, role), Some(name), domain)
                .unwrap();
        let log = |stream| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(format!("{role}.{stream}")))
                .unwrap()
        };
        let readers = vec![drain(out, log("stdout")), drain(err, log("stderr"))];
        owner.resume().unwrap();
        Self { owner, readers }
    }
    fn exited(&mut self) {
        let mut status = None;
        wait(|| {
            status = self.owner.try_wait().unwrap();
            status.is_some()
        });
        assert!(status.unwrap().success());
    }
    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            reader.join().unwrap().unwrap();
        }
    }
}
fn name(meta: &Value, which: &str) -> String {
    format!(
        "Global\\WinBridge-LifetimeSpike-{}-{which}",
        meta["scopeId"].as_str().unwrap()
    )
}
fn task_cancellation(root: &Path, meta: &Value) -> ExecutionDomain {
    let identity =
        ExecutionDomain::identity_for(root.to_str().unwrap(), meta["ownerId"].as_str().unwrap())
            .unwrap()
            .unwrap();
    let domain = ExecutionDomain::open(&identity).unwrap();
    assert_eq!(limits(&domain.job), JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    publish(
        root,
        "task-domain.json",
        serde_json::to_value(&identity).unwrap(),
    );
    let mut task = Child::start(root, "task", &name(meta, "task"), Some(&domain));
    let ready = read(root, "task.json");
    let task_pin = PinnedProcess::live(&stamp(&ready["process"])).unwrap();
    let leaf_pin = PinnedProcess::live(&stamp(&ready["leaf"])).unwrap();
    assert_eq!(task_pin.identity().process_id, task.owner.id());
    assert_eq!(limits(&task.owner.job), JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    task.owner.terminate_tree().unwrap();
    wait(|| task.owner.try_wait().unwrap().is_some());
    wait(|| task.owner.tree_is_empty().unwrap());
    assert!(task_pin.exited().unwrap());
    assert!(leaf_pin.exited().unwrap());
    assert_eq!(limits(&domain.job), JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    publish(
        root,
        "cancelled-task.json",
        json!({"process":task_pin.identity(),"leaf":leaf_pin.identity(),"taskJob":task.owner.identity(),"domain":identity,"domainLimits":limits(&domain.job),"taskLimits":limits(&task.owner.job),"rootExited":true,"leafExited":true,"empty":true}),
    );
    task.join_readers();
    domain
}

fn old_keeper(root: &Path, meta: &Value) {
    let mut child = Child::start(root, "old-worker", &name(meta, "old"), None);
    let old = read(root, "old-worker.json");
    assert_eq!(stamp(&old["process"]).process_id, child.owner.id());
    assert!(!member(&name(meta, "old")));
    assert_eq!(limits(&child.owner.job), JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    publish(
        root,
        "old-keeper.json",
        json!({"process":own(),"worker":old,"job":child.owner.identity(),"limits":limits(&child.owner.job)}),
    );
    if meta["mode"] == "keeper-loss" {
        read(root, "crash-old-keeper.json");
        // Deliberately bypass Rust destructors: only OS last-handle closure of
        // the original scoped Job may terminate the still waiting worker.
        std::process::exit(73);
    }
    let requested = read(root, "request-next.json");
    assert_eq!(requested["requester"], old["process"]);
    let domain =
        ExecutionDomain::open(&serde_json::from_value(old["domain"].clone()).unwrap()).unwrap();
    // Production query primitive checks the actual spawner before CreateProcess.
    // The independent test-only member() observations remain the second oracle.
    let parent = PinnedProcess::live(&own()).unwrap();
    parent.require_outside_job(&name(meta, "old")).unwrap();
    parent
        .require_outside_job(&domain.identity().job_name)
        .unwrap();
    let cmd = command(root, "new-keeper");
    let mut next = InfrastructureChild::spawn(
        Path::new(&cmd.executable),
        &cmd.arguments,
        Path::new(&cmd.working_directory),
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("new-keeper.stdout"))
            .unwrap(),
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("new-keeper.stderr"))
            .unwrap(),
        Some(&domain),
    )
    .unwrap();
    let pinned_next = PinnedProcess::live(&next.identity).unwrap();
    pinned_next.require_outside_job(&name(meta, "old")).unwrap();
    pinned_next
        .require_outside_job(&domain.identity().job_name)
        .unwrap();
    // The recipient becomes independent BEFORE a command can be accepted.
    // Lost readiness evidence must not revoke an accepted next generation.
    next.detached();
    publish(
        root,
        "start-next-keeper.json",
        json!({"process":next.identity,"requester":old["process"]}),
    );
    let ready = read(root, "new-keeper.json");
    assert_eq!(stamp(&ready["process"]), next.identity);
    assert_eq!(ready["inOldJob"], false);
    publish(root, "stop-old-worker.json", json!({"stop":true}));
    child.exited();
    // A signalled root is not an atomic Job accounting update. Observe the
    // SAME original Job until it is positively empty; never infer emptiness
    // from process exit and never substitute a new Job under its old name.
    wait(|| child.owner.tree_is_empty().unwrap());
    child.join_readers();
    publish(
        root,
        "old-keeper-done.json",
        json!({"process":own(),"oldJobEmpty":true,"next":next.identity}),
    );
}
fn new_keeper(root: &Path, meta: &Value) {
    let start = read(root, "start-next-keeper.json");
    assert_eq!(stamp(&start["process"]), own());
    assert_eq!(start["requester"], read(root, "old-worker.json")["process"]);
    assert!(!member(&name(meta, "old")));
    let old_domain = read(root, "task-domain.json");
    assert!(!member(old_domain["jobName"].as_str().unwrap()));
    let previous = read(root, "old-keeper.json");
    let previous = PinnedProcess::live(&stamp(&previous["process"])).unwrap();
    let mut child = Child::start(root, "new-worker", &name(meta, "new"), None);
    let worker = read(root, "new-worker.json");
    assert_eq!(stamp(&worker["process"]).process_id, child.owner.id());
    assert_eq!(limits(&child.owner.job), JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    publish(
        root,
        "new-keeper.json",
        json!({"process":own(),"worker":worker,"job":child.owner.identity(),"limits":limits(&child.owner.job),"inOldJob":false,"inOldDomain":false}),
    );
    wait(|| previous.exited().unwrap());
    assert!(child.owner.try_wait().unwrap().is_none());
    assert!(!member(&name(meta, "old")));
    publish(
        root,
        "survived.json",
        json!({"process":own(),"worker":worker,"oldKeeper":previous.identity(),"oldKeeperExited":true}),
    );
    read(root, "stop-new-worker.json");
    child.exited();
    wait(|| child.owner.tree_is_empty().unwrap());
    child.join_readers();
    publish(
        root,
        "new-keeper-done.json",
        json!({"process":own(),"newJobEmpty":true,"limits":limits(&child.owner.job)}),
    );
}

#[test]
#[ignore = "Private local-process helper for explicit lifetime spike only"]
fn helper() {
    let cwd = std::env::current_dir().unwrap();
    let actual_root = cwd.parent().unwrap().canonicalize().unwrap();
    let root = actual_root.as_path();
    let meta = read(root, "metadata.json");
    assert_eq!(Path::new(meta["root"].as_str().unwrap()), root);
    match cwd.file_name().unwrap().to_str().unwrap() {
        "old-keeper" => old_keeper(root, &meta),
        "old-worker" => {
            assert!(member(&name(&meta, "old")));
            let domain = task_cancellation(root, &meta);
            publish(
                root,
                "old-worker.json",
                json!({"process":own(),"domain":domain.identity()}),
            );
            if meta["mode"] == "keeper-loss" {
                // Longer than both enclosing 45s observation windows. Its own
                // timeout cannot be mistaken for OS last-handle termination.
                thread::sleep(Duration::from_secs(300));
                panic!("Keeper loss did not terminate its waiting worker");
            }
            read(root, "start-next.json");
            publish(root, "request-next.json", json!({"requester":own()}));
            read(root, "stop-old-worker.json");
        }
        "new-keeper" => new_keeper(root, &meta),
        "new-worker" => {
            assert!(member(&name(&meta, "new")));
            assert!(!member(&name(&meta, "old")));
            publish(
                root,
                "new-worker.json",
                json!({"process":own(),"inOldJob":false}),
            );
            read(root, "stop-new-worker.json");
        }
        "task" => {
            use std::os::windows::process::CommandExt;
            let cmd = command(root, "task-leaf");
            let mut child = std::process::Command::new(&cmd.executable);
            child
                .args(&cmd.arguments)
                .current_dir(&cmd.working_directory)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
            let mut leaf = child.spawn().unwrap();
            let ready = read(root, "task-leaf.json");
            assert_eq!(stamp(&ready["process"]).process_id, leaf.id());
            publish(
                root,
                "task.json",
                json!({"process":own(),"leaf":ready["process"]}),
            );
            let status = leaf.wait().unwrap();
            panic!("Task was not cancelled by its owner; grandchild returned {status}");
        }
        "task-leaf" => {
            assert!(member(&name(&meta, "task")));
            let domain = read(root, "task-domain.json");
            assert!(member(domain["jobName"].as_str().unwrap()));
            publish(root, "task-leaf.json", json!({"process":own()}));
            thread::sleep(Duration::from_secs(45));
            panic!("Task grandchild was not cancelled by its owner");
        }
        _ => panic!("Unknown lifetime fixture role"),
    }
}

fn fixture(mode: &str) -> (PathBuf, Value) {
    let input = std::env::var_os("WINBRIDGE_LIFETIME_SPIKE_INPUT")
        .expect("Explicit fixture input required");
    let input: Value = serde_json::from_reader(File::open(input).unwrap()).unwrap();
    assert_eq!(input["schemaVersion"], 1);
    let root = PathBuf::from(input["root"].as_str().unwrap()).join(mode);
    assert!(root.is_absolute());
    crate::require_safe_path(&root).unwrap();
    crate::require_local_disk(&root).unwrap();
    fs::create_dir(&root).expect("Existing fixture preserved");
    let root = root.canonicalize().unwrap();
    use sha2::{Digest, Sha256};
    let scope = format!("{:x}", Sha256::digest(root.to_str().unwrap().as_bytes()));
    let meta = json!({"schemaVersion":1,"mode":mode,"root":root,"scopeId":scope,"ownerId":format!("00000000-0000-4000-8000-{}",&scope[..12])});
    publish(&root, "metadata.json", meta.clone());
    for role in [
        "old-keeper",
        "old-worker",
        "new-keeper",
        "new-worker",
        "task",
        "task-leaf",
    ] {
        fs::create_dir(root.join(role)).unwrap();
    }
    (root, meta)
}

#[test]
#[ignore = "Native two-generation Job experiment; explicit fresh local root required"]
fn resident_keeper_launches_next_updater_outside_old_worker_job_without_breakaway() {
    let (root, meta) = fixture("two-generation");
    // This outer test owner covers every helper even after an intermediate
    // parent exits. It is retained until both original worker Jobs are empty.
    let mut external = Child::start(&root, "old-keeper", &name(&meta, "transport"), None);
    let old = read(&root, "old-keeper.json");
    assert_eq!(stamp(&old["process"]).process_id, external.owner.id());
    let old_worker = PinnedProcess::live(&stamp(&old["worker"]["process"])).unwrap();
    publish(&root, "start-next.json", json!({"request":true}));
    let next = read(&root, "new-keeper.json");
    let next_process = PinnedProcess::live(&stamp(&next["process"])).unwrap();
    let next_worker = PinnedProcess::live(&stamp(&next["worker"]["process"])).unwrap();
    external.exited();
    assert!(old_worker.exited().unwrap());
    next_process.require_live().unwrap();
    next_worker.require_live().unwrap();
    let survived = read(&root, "survived.json");
    assert_eq!(survived["process"], next["process"]);
    assert_eq!(survived["worker"], next["worker"]);
    publish(&root, "stop-new-worker.json", json!({"stop":true}));
    let done = read(&root, "new-keeper-done.json");
    wait(|| next_process.exited().unwrap());
    wait(|| next_worker.exited().unwrap());
    wait(|| external.owner.tree_is_empty().unwrap());
    external.join_readers();
    assert_eq!(done["newJobEmpty"], true);
    assert_eq!(
        limits(&external.owner.job),
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    );
    publish(
        &root,
        "proof.json",
        json!({"schemaVersion":1,"kind":"activation_lifetime_keeper_os_spike","passed":true,"platform":"windows","build":winbridge_protocol::build_identity(),"driver":own(),"transportJob":external.owner.identity(),"old":old,"next":next,"survived":survived,"oldDone":read(&root,"old-keeper-done.json"),"newDone":done,"taskCancellation":read(&root,"cancelled-task.json"),"allHelpersExited":true,"limitsChanged":false,"breakawayUsed":false,"networkStarted":false,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false}),
    );
}

#[test]
#[ignore = "Native original Job last-handle closure experiment; explicit local root required"]
fn keeper_process_loss_kills_waiting_worker_without_recovery_or_limit_changes() {
    let (root, meta) = fixture("keeper-loss");
    let mut external = Child::start(&root, "old-keeper", &name(&meta, "transport"), None);
    let old = read(&root, "old-keeper.json");
    assert_eq!(stamp(&old["process"]).process_id, external.owner.id());
    let worker = PinnedProcess::live(&stamp(&old["worker"]["process"])).unwrap();
    publish(&root, "crash-old-keeper.json", json!({"crash":73}));
    let mut status = None;
    wait(|| {
        status = external.owner.try_wait().unwrap();
        status.is_some()
    });
    assert_eq!(status.unwrap().code(), Some(73));
    wait(|| worker.exited().unwrap());
    wait(|| external.owner.tree_is_empty().unwrap());
    external.join_readers();
    assert!(!root.join("new-keeper.json").exists());
    publish(
        &root,
        "proof.json",
        json!({"schemaVersion":1,"kind":"activation_lifetime_keeper_loss_os_spike","platform":"windows","passed":true,"build":winbridge_protocol::build_identity(),"driver":own(),"transportJob":external.owner.identity(),"old":old,"keeperExitCode":73,"workerExited":true,"allHelpersExited":true,"limitsChanged":false,"breakawayUsed":false,"networkStarted":false,"automaticRecoveryCount":0,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false}),
    );
}
