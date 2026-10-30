//! Main31 only: retire the actual coordinator, retain the original live Job.
use super::*;
use winbridge_protocol::*;
fn call(
    rt: &tokio::runtime::Runtime,
    sup: &SupervisorRecord,
    q: &KeeperControlRequest,
) -> KeeperStatus {
    let reply: KeeperControlResponse = rt
        .block_on(winbridge_transport::supervisor::call_with_timeout(
            &sup.spec.endpoint,
            sup.process.as_ref().unwrap().process_id,
            q,
            winbridge_transport::supervisor::CONTROL_TIMEOUT,
        ))
        .unwrap();
    assert_eq!(reply.keeper_control_version, 1);
    assert_eq!(reply.update_id, q.update_id);
    assert_eq!(reply.launch_id, q.launch_id);
    assert_eq!(reply.request_id.as_deref(), Some(q.request_id.as_str()));
    match reply.result {
        KeeperControlResult::Status { status } => *status,
        other => panic!("Original retirement did not return physical status: {other:?}"),
    }
}
fn cpu_ticks(pid: u32) -> u64 {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn GetProcessTimes(
            handle: *mut std::ffi::c_void,
            created: *mut u64,
            exited: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
    }
    let p = unsafe { OpenProcess(0x1000, 0, pid) }; // PROCESS_QUERY_LIMITED_INFORMATION
    assert!(!p.is_null(), "Keeper process must still exist");
    let (mut created, mut exited, mut kernel, mut user) = (0, 0, 0, 0);
    let ok = unsafe { GetProcessTimes(p, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(p);
    }
    assert_ne!(ok, 0);
    kernel + user
}
pub(in super::super) fn execute(
    root: &FixtureLayout,
    sup: &SupervisorRecord,
    finalization: &serde_json::Value,
) -> serde_json::Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let directory = root.runtime().join("update-state");
    let control: SupervisorRequest =
        serde_json::from_slice(&fs::read(root.join("supervisor-control.private")).unwrap())
            .unwrap();
    let mut q = KeeperControlRequest {
        keeper_control_version: 1,
        update_id: sup.spec.update_id.clone(),
        launch_id: sup.spec.launch_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        expected_coordinator_revision: sup.revision,
        token: control.token,
        command: KeeperControlCommand::RetireCoordinator,
    };
    let before = UpdateJournal::read_snapshot(&directory, Some(&q.update_id)).unwrap();
    let first = call(&rt, sup, &q);
    assert_eq!(
        first.phase,
        KeeperPhase::Keeper,
        "Writer/authority must retire before claiming Keeper: {first:?}"
    );
    assert!(first.role_selection_known && first.writer_closed && first.authority_released);
    assert_eq!(first.original_custody_held, Some(true));
    assert_eq!(first.worker_alive, Some(true));
    assert_eq!(first.job_tree_empty, Some(false));
    let binding = first.binding.as_ref().unwrap();
    assert_eq!(&binding.coordinator, sup.process.as_ref().unwrap());
    assert_eq!(
        serde_json::to_value(&binding.worker).unwrap(),
        finalization["record"]["state"]["receipt"]["compatibility"]["identity"]["process"]
    );
    let role = UpdateJournal::inspect_supervisor(&directory, &q.launch_id).unwrap();
    assert_eq!(role.phase, SupervisorPhase::Keeper);
    assert_eq!(role.revision, sup.revision + 1);
    let sql = Connection::open_with_flags(
        directory.join("updates.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let selection: String = sql
        .query_row(
            "SELECT selection FROM keeper_selections WHERE launch_id=?1",
            [&q.launch_id],
            |r| r.get(0),
        )
        .unwrap();
    let selection: winbridge_update::journal::KeeperSelection =
        serde_json::from_str(&selection).unwrap();
    selection.validate().unwrap();
    assert_eq!(
        binding.selection_sha256.as_deref(),
        Some(selection.sha256().unwrap().as_str())
    );
    drop(sql);
    let cfg =
        winbridge_update::load_config(&root.runtime().join("winbridge-update.local.json")).unwrap();
    let scope = winbridge_update::installation_scope(&cfg).unwrap();
    let mut next = UpdateJournal::open(&directory, &scope).unwrap();
    let owner = next
        .acquire_owner()
        .expect("OS ownership lock must actually be free while the original Job remains live");
    assert!(owner.epoch() > before.0.owner_epoch);
    let acquired = next.info().unwrap();
    q.request_id = uuid::Uuid::new_v4().to_string();
    // Duplicate Retire must be observation, even after a different owner epoch.
    let duplicate = call(&rt, sup, &q);
    assert_eq!(duplicate.binding, first.binding);
    assert!(duplicate.writer_closed && duplicate.authority_released);
    assert_eq!(
        UpdateJournal::inspect_supervisor(&directory, &q.launch_id).unwrap(),
        role
    );
    q.command = KeeperControlCommand::Observe;
    q.request_id = uuid::Uuid::new_v4().to_string();
    let second = call(&rt, sup, &q);
    let idle_start = cpu_ticks(binding.coordinator.process_id);
    let timer = Instant::now();
    std::thread::sleep(Duration::from_secs(3));
    let idle_end = cpu_ticks(binding.coordinator.process_id);
    let elapsed_ms = timer.elapsed().as_millis() as u64;
    let cpu = idle_end - idle_start;
    assert!(
        cpu <= elapsed_ms * 1000,
        "Keeper is busy polling in idle: {cpu} ticks/{elapsed_ms}ms"
    );
    let controller = UpdateJournal::inspect_activation_controller(&directory, &q.update_id)
        .unwrap()
        .unwrap();
    let command = &controller.command.binding.service_command;
    let response: LocalResponse = service::normal(&rt, command, &LocalRequest::Info);
    let LocalResponse::Info { info } = response else {
        panic!("Same worker lost ordinary IPC after authority release")
    };
    assert_eq!(info.generation, binding.generation);
    assert_eq!(
        info.fenced_requests.as_ref().unwrap().context,
        binding.context
    );
    assert!(info.maintenance_available);
    let config: winbridge_core::config::WorkerConfig =
        serde_json::from_slice(&fs::read(&cfg.worker_config_path).unwrap()).unwrap();
    let request = FencedRequest {
        request_contract_version: 2,
        context: binding.context.clone(),
        request: LocalRequest::Submit {
            request: TaskSubmission {
                protocol_version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                project_id: config.projects[0].id.clone(),
                idempotency_key: "main31-keeper-active-once".into(),
                timeout_ms: 10000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "cmd.exe".into(),
                    arguments: vec![
                        "/d".into(),
                        "/c".into(),
                        "echo KEEPER>>main31-keeper-count.txt".into(),
                    ],
                    working_directory: ".".into(),
                },
            },
        },
    };
    let accepted: FencedResponse = service::normal(&rt, command, &request);
    let LocalResponse::Task {
        task: accepted_task,
    } = &accepted.response
    else {
        panic!("Post-retirement task rejected: {:?}", accepted.response)
    };
    let until = Instant::now() + Duration::from_secs(20);
    let task = loop {
        let reply: LocalResponse = service::normal(
            &rt,
            command,
            &LocalRequest::GetTask {
                task_id: accepted_task.task_id.clone(),
            },
        );
        if let LocalResponse::Task { task } = reply {
            if task.status == TaskStatus::Succeeded {
                break task;
            }
            assert!(
                !matches!(
                    task.status,
                    TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Indeterminate
                ),
                "Post-retirement task failed: {task:?}"
            );
        }
        assert!(
            Instant::now() < until,
            "Post-retirement task did not execute"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let duplicate_task: FencedResponse = service::normal(&rt, command, &request);
    let LocalResponse::Task { task: dedup } = &duplicate_task.response else {
        panic!("Post-retirement duplicate rejected")
    };
    assert_eq!(dedup.task_id, task.task_id);
    let effect = fs::read_to_string(
        config.projects[0]
            .workspace_root
            .join("main31-keeper-count.txt"),
    )
    .unwrap();
    assert_eq!(effect.lines().collect::<Vec<_>>(), vec!["KEEPER"]);
    assert_eq!(next.get(&q.update_id).unwrap(), before.1.unwrap());
    let after = UpdateJournal::inspect_activation_finalization(&directory, &q.update_id)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(after).unwrap(), finalization["record"]);
    // This owner is observation-only in Main31; next-generation bootstrap is separate.
    drop(owner);
    drop(next);
    json!({"before":before.0,"first":first,"duplicate":duplicate,"second":second,"role":role,"selection":selection,"newOwner":acquired,"infoAfter":info,"idleCpuTicks100ns":cpu,"idleWallMs":elapsed_ms,"actualNewOwnerAcquired":true,"sameWorkerAndJobRetained":true,"nextGenerationStarted":false,"accepted":accepted,"duplicateTask":duplicate_task,"task":task,"taskEffect":effect})
}
