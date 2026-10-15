use std::{
    collections::BTreeMap,
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use winbridge_core::config::WorkerConfig;
use winbridge_protocol::*;
use winbridge_worker::Worker;

struct Owner {
    worker: Arc<Worker>,
    ids: Vec<String>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.worker.stop();
        for id in &self.ids {
            let _ = self.worker.handle(LocalRequest::Cancel {
                task_id: id.clone(),
            });
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.worker.is_idle() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn task(worker: &Worker, id: &str) -> TaskRecord {
    let LocalResponse::Task { task } = worker.handle(LocalRequest::GetTask { task_id: id.into() })
    else {
        panic!("task lookup");
    };
    *task
}
fn observation(worker: &Worker) -> WorkSchedulingObservation {
    let LocalResponse::Info { info } = worker.handle(LocalRequest::Info) else {
        panic!("info");
    };
    interpret_work_scheduling(&info).expect("current typed scheduling observation")
}
fn started(worker: &Worker, id: &str) -> bool {
    let t = task(worker, id);
    t.status == TaskStatus::Running && t.process_id.is_some()
}
async fn until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !check() {
        assert!(Instant::now() < deadline, "work scheduling deadline");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn actual_three_project_capacity_fairness_old_backlog_cancel_and_native_ipc() {
    let mut scenarios = Vec::new();
    for limit in [Some(1), Some(2), None] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let mut projects = Vec::new();
        for id in ["a", "b", "c"] {
            let path = root.join(id);
            fs::create_dir(&path).unwrap();
            projects.push(serde_json::json!({"id":id,"workspaceRoot":path,"allowDirectCommands":true,"maxConcurrentMutations":1,"recipes":[]}));
        }
        let mut json = serde_json::json!({"schemaVersion":1,"instanceId":format!("work-slots-{}",uuid::Uuid::new_v4()),"nodeId":"worker","stateDirectory":root.join("state"),"worker":{"keepRunningOnUiExit":true},"projects":projects});
        if let Some(value) = limit {
            json["workScheduling"] = serde_json::json!({"maxConcurrentWork":value});
        }
        let config: WorkerConfig = serde_json::from_value(json).unwrap();
        let endpoint =
            winbridge_transport::endpoint(&config.state_directory, &config.instance_id).unwrap();
        let worker = Worker::open(config).unwrap();
        let mut owner = Owner {
            worker: worker.clone(),
            ids: Vec::new(),
        };
        let mut ids = BTreeMap::new();
        for project in ["a", "b", "c"] {
            for n in 0..15 {
                let response = worker.handle(LocalRequest::Submit {
                    request: TaskSubmission {
                        protocol_version: 1,
                        request_id: format!("{project}-{n}"),
                        idempotency_key: format!("{project}-{n}"),
                        project_id: project.into(),
                        timeout_ms: 15000,
                        snapshot_id: None,
                        command: CommandSource::Direct {
                            executable: env!("CARGO_BIN_EXE_winbridge-fixture").into(),
                            arguments: vec!["long".into()],
                            working_directory: ".".into(),
                        },
                    },
                });
                let LocalResponse::Task { task } = response else {
                    panic!("admission: {response:?}");
                };
                ids.insert((project, n), task.task_id.clone());
                owner.ids.push(task.task_id);
            }
        }
        let server = tokio::spawn(winbridge_worker::serve(worker.clone()));
        let expected = limit.unwrap_or(3) as u32;
        until(|| {
            observation(&worker).occupied_slots == expected
                && ["a", "b", "c"]
                    .iter()
                    .take(expected as usize)
                    .all(|p| started(&worker, &ids[&(*p, 0)]))
        })
        .await;
        let LocalResponse::Info { info } =
            winbridge_transport::call(&endpoint, &LocalRequest::Info)
                .await
                .unwrap()
        else {
            panic!("native IPC Info");
        };
        let view = interpret_work_scheduling(&info).unwrap();
        assert_eq!(view.occupied_slots, expected);
        assert_eq!(view.queued_operations, 45 - expected);
        assert_eq!(view.max_concurrent_work, limit);
        let mut cancel_ms = Vec::new();
        if let Some(cap) = limit {
            let blocked = if cap == 1 { "b" } else { "c" };
            let blocked_id = &ids[&(blocked, 0)];
            until(|| {
                task(&worker, blocked_id)
                    .reason
                    .as_ref()
                    .is_some_and(|r| r.contains("Общий лимит"))
            })
            .await;
            let request = OperationPageRequest {
                context: info.fenced_requests.as_ref().unwrap().context.clone(),
                query: OperationQuery {
                    project_id: Some(blocked.into()),
                    kind: Some(HistoryKind::Task),
                    search: blocked_id.clone(),
                    limit: 5,
                    cursor: None,
                },
            };
            let page = winbridge_transport::operation_page(&endpoint, &request)
                .await
                .unwrap();
            assert!(matches!(
                page.records[0].state,
                OperationState::Execution {
                    status: TaskStatus::Queued
                }
            ));
            assert!(page.records[0]
                .reason
                .as_ref()
                .unwrap()
                .contains("Общий лимит"));
            // This accepted task is older than the last ten admissions in the same project.
            assert_eq!(task(&worker, blocked_id).status, TaskStatus::Queued);
            for (old, next) in if cap == 1 {
                vec![("a", "b"), ("b", "c")]
            } else {
                vec![("a", "c")]
            } {
                let id = &ids[&(old, 0)];
                let start = Instant::now();
                let envelope = FencedRequest {
                    request_contract_version: 2,
                    context: request.context.clone(),
                    request: LocalRequest::Cancel {
                        task_id: id.clone(),
                    },
                };
                winbridge_transport::call_fenced(&endpoint, &envelope)
                    .await
                    .unwrap();
                until(|| task(&worker, id).status == TaskStatus::Cancelled).await;
                cancel_ms.push(start.elapsed().as_millis() as u64);
                until(|| started(&worker, &ids[&(next, 0)])).await;
                assert!(observation(&worker).occupied_slots <= u32::from(cap));
            }
        }
        for id in &owner.ids {
            worker.handle(LocalRequest::Cancel {
                task_id: id.clone(),
            });
        }
        until(|| worker.is_idle()).await;
        assert_eq!(observation(&worker).occupied_slots, 0);
        scenarios.push(serde_json::json!({"maxConcurrentWork":limit,"admissions":45,"initialSlots":view.occupied_slots,"cancelMs":cancel_ms,"cleanupSlots":0,"context":view.context}));
        worker.stop();
        server.await.unwrap().unwrap();
        drop(owner);
    }
    if let Some(path) = std::env::var_os("WINBRIDGE_WORK_SCHEDULING_REPORT") {
        let path = std::path::Path::new(&path).join("work-scheduling-proof.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"passed":true,"platform":std::env::consts::OS,"build":build_identity(),"scenarios":scenarios})).unwrap()).unwrap();
        file.sync_all().unwrap();
    }
}
