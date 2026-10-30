//! Minimal local keeper control. Closed keepers never read an updater schema.
use super::*;

pub(super) fn validate(
    r: &SupervisorRecord,
    q: &KeeperControlRequest,
    pid: u32,
    selected: bool,
) -> Result<()> {
    authorize(r, &q.token, pid, SupervisorAction::Observe)?;
    if q.keeper_control_version != KEEPER_CONTROL_CONTRACT
        || q.update_id != r.spec.update_id
        || q.launch_id != r.spec.launch_id
        || !uuid::Uuid::parse_str(&q.request_id).is_ok_and(|v| v.to_string() == q.request_id)
        || !(q.expected_coordinator_revision == r.revision
            || selected && r.revision.checked_add(1) == Some(q.expected_coordinator_revision))
    {
        return Err("Keeper control scope/version/correlation/revision changed".into());
    }
    Ok(())
}
pub(super) fn response(
    r: &SupervisorRecord,
    q: Option<&KeeperControlRequest>,
    result: KeeperControlResult,
) -> KeeperControlResponse {
    KeeperControlResponse {
        keeper_control_version: KEEPER_CONTROL_CONTRACT,
        update_id: r.spec.update_id.clone(),
        launch_id: r.spec.launch_id.clone(),
        request_id: q.and_then(|q| {
            uuid::Uuid::parse_str(&q.request_id)
                .ok()
                .filter(|v| v.to_string() == q.request_id)
                .map(|_| q.request_id.clone())
        }),
        result,
    }
}
pub(super) fn not_selected(reason: Option<String>) -> KeeperStatus {
    KeeperStatus {
        phase: KeeperPhase::NotSelected,
        binding: None,
        role_selection_known: false,
        writer_closed: false,
        authority_released: false,
        original_custody_held: None,
        worker_alive: None,
        job_tree_empty: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        output_incomplete: None,
        readers_finished: None,
        reason,
    }
}
#[cfg(windows)]
enum State {
    Retiring(Box<crate::activation::CoordinatorRetirement>),
    Keeper(Box<crate::activation::ActivationKeeperCustody>),
}
#[cfg(windows)]
impl State {
    fn status(&self) -> KeeperStatus {
        match self {
            Self::Retiring(v) => v.status(),
            Self::Keeper(v) => v.status(),
        }
    }
    fn retry(self) -> Self {
        match self {
            Self::Retiring(v) => match v.try_close() {
                Ok(v) => Self::Keeper(v),
                Err(v) => Self::Retiring(v),
            },
            v => v,
        }
    }
}
#[cfg(windows)]
#[derive(Default)]
struct RootExitWatch {
    armed: bool,
    exited: bool,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    failure: Option<String>,
    last_drain: Option<(Option<bool>, Option<bool>)>,
}
#[cfg(windows)]
impl RootExitWatch {
    fn fail(&mut self, reason: String) {
        let reason: String = reason.chars().take(1000).collect();
        eprintln!(
            "WinBridge keeper {}",
            serde_json::json!({"event":"original_keeper_exit_observation_unknown","keeperPid":std::process::id(),"reason":reason})
        );
        self.failure = Some(reason);
    }
    fn arm(&mut self, state: &State) {
        if self.armed {
            return;
        }
        if let State::Keeper(custody) = state {
            let pin = custody.worker_exit_pin();
            // Dropping original custody closes its Job, so this exact-object
            // wait also completes if the host itself exits on IPC failure.
            self.task = Some(tokio::task::spawn_blocking(move || pin.wait_for_exit()));
            self.armed = true;
        }
    }
    fn status(&self, state: &State) -> KeeperStatus {
        let mut status = state.status();
        if self.failure.is_some() {
            status.reason = self.failure.clone();
        }
        status
    }
}
#[cfg(windows)]
pub(super) async fn run(
    listener: ipc::Listener,
    r: SupervisorRecord,
    resources: Box<crate::activation::CoordinatorRetirement>,
    initial: Option<(ipc::Stream, KeeperControlRequest)>,
) -> Result<()> {
    run_state(
        listener,
        r,
        State::Retiring(resources).retry(),
        keeper_launch::Actor::default(),
        initial,
    )
    .await
}
#[cfg(windows)]
async fn run_state(
    mut listener: ipc::Listener,
    r: SupervisorRecord,
    mut state: State,
    mut launch: keeper_launch::Actor,
    initial: Option<(ipc::Stream, KeeperControlRequest)>,
) -> Result<()> {
    std::hint::black_box(KEEPER_CONTROL_MARKER);
    let mut root = RootExitWatch::default();
    let first = state.status();
    eprintln!(
        "WinBridge keeper {}",
        serde_json::json!({"event":"coordinator_retirement","keeperPid":std::process::id(),"phase":first.phase,"writerClosed":first.writer_closed,"authorityReleased":first.authority_released,"roleSelectionKnown":first.role_selection_known,"reason":first.reason})
    );
    if let Some((mut stream, q)) = initial {
        let _ = ipc::write(
            &mut stream,
            &response(
                &r,
                Some(&q),
                KeeperControlResult::Status {
                    status: Box::new(first),
                },
            ),
        )
        .await;
    }
    loop {
        root.arm(&state);
        let draining = root.exited && launch.successor_confirmed() && root.failure.is_none();
        if draining {
            if let State::Keeper(custody) = &mut state {
                match custody.retire_if_drained() {
                    Ok(true) => {
                        let status = custody.status();
                        eprintln!(
                            "WinBridge keeper {}",
                            serde_json::json!({"event":"original_keeper_exit","keeperPid":std::process::id(),"workerPid":status.binding.as_ref().map(|b|b.worker.process_id),"successorReadyKnown":true,"workerAlive":status.worker_alive,"jobTreeEmpty":status.job_tree_empty,"readersFinished":status.readers_finished,"outputIncomplete":status.output_incomplete})
                        );
                        return Ok(()); // Only this original owned Job is closed.
                    }
                    Ok(false) => {
                        let status = custody.status();
                        let seen = (status.job_tree_empty, status.readers_finished);
                        if root.last_drain != Some(seen) {
                            root.last_drain = Some(seen);
                            eprintln!(
                                "WinBridge keeper {}",
                                serde_json::json!({"event":"original_keeper_drain_observed","keeperPid":std::process::id(),"jobTreeEmpty":seen.0,"readersFinished":seen.1})
                            );
                        }
                    }
                    Err(e) => root.fail(format!(
                        "Original keeper drain observation unknown; retain custody: {e}"
                    )),
                }
            }
        }
        let drain_wait = root.exited && launch.successor_confirmed() && root.failure.is_none();
        let connection = tokio::select! {
            connection=listener.accept()=>connection,
            waited=async {
                match root.task.as_mut() {
                    Some(task)=>task.await,
                    None=>std::future::pending().await,
                }
            }=>{
                root.task.take(); // Never poll a completed JoinHandle again.
                match waited {
                    Ok(Ok(()))=>{
                        root.exited=true;
                        eprintln!("WinBridge keeper {}",serde_json::json!({"event":"original_worker_exit_observed","keeperPid":std::process::id(),"successorReadyKnown":launch.successor_confirmed()}));
                    },
                    Ok(Err(e))=>root.fail(format!("Original root wait unknown; retain custody: {e}")),
                    Err(e)=>root.fail(format!("Original root wait task failed; retain custody: {e}")),
                }
                continue;
            },
            _=tokio::time::sleep(Duration::from_millis(250)), if drain_wait=>continue,
        };
        let (mut stream, pid) = match connection {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "Original keeper IPC stopped; original Job cleanup requested: {e}"
                ))
            }
        };
        let bytes = match ipc::read_bytes(&mut stream).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let routing: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if routing.get("keeperLaunchVersion").is_some() {
            let q = serde_json::from_slice::<KeeperLaunchRequest>(&bytes).ok();
            let reply = match (&state, q.as_ref()) {
                (State::Keeper(c), Some(q)) => launch.handle(&r, c, q, pid),
                _ => keeper_launch::rejected(
                    q.as_ref(),
                    "Original keeper retirement incomplete or invalid launch frame",
                ),
            };
            let _ = ipc::write(&mut stream, &reply).await;
            continue;
        }
        if routing.get("keeperProbeVersion").is_some() {
            let q = serde_json::from_slice::<KeeperProbeRequest>(&bytes).ok();
            let reply = match &state {
                State::Keeper(v) => keeper_probe::respond(
                    Some(v.bootstrap_binding()),
                    q.as_ref(),
                    pid,
                    |pid, binding| v.require_bootstrap_peer(pid, binding),
                ),
                State::Retiring(_) => keeper_probe::respond(None, q.as_ref(), pid, |_, _| {
                    Err("Original keeper retirement incomplete".into())
                }),
            };
            let _ = ipc::write(&mut stream, &reply).await;
            continue;
        }
        let q: KeeperControlRequest = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                let _ = ipc::write(
                    &mut stream,
                    &response(
                        &r,
                        None,
                        KeeperControlResult::Rejected {
                            message: "Invalid keeper request; no fallback".into(),
                        },
                    ),
                )
                .await;
                continue;
            }
        };
        let result = match validate(&r, &q, pid, true) {
            Err(message) => KeeperControlResult::Rejected { message },
            Ok(()) => {
                // A duplicate Retire is observation only. Only a known original
                // retirement can retry closing its SAME writer, never SQL/V8.
                if q.command == KeeperControlCommand::RetryWriterClose {
                    state = state.retry();
                }
                KeeperControlResult::Status {
                    status: Box::new(root.status(&state)),
                }
            }
        };
        let _ = ipc::write(&mut stream, &response(&r, Some(&q), result)).await;
    }
}
impl SupervisorHandle {
    /// Call once to retire, then Observe the same original PID after uncertainty.
    pub async fn keeper_control(
        &self,
        command: KeeperControlCommand,
    ) -> Result<KeeperControlResult> {
        if !infrastructure::matches(&self.child.identity).map_err(error)? {
            return Err("Original keeper process identity changed".into());
        }
        let q = KeeperControlRequest {
            keeper_control_version: 1,
            update_id: self.record.spec.update_id.clone(),
            launch_id: self.record.spec.launch_id.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            expected_coordinator_revision: self.record.revision,
            token: self.token.clone(),
            command,
        };
        let reply: KeeperControlResponse = ipc::call_with_timeout(
            &self.record.spec.endpoint,
            self.child.identity.process_id,
            &q,
            ipc::CONTROL_TIMEOUT,
        )
        .await
        .map_err(error)?;
        if reply.keeper_control_version != 1
            || reply.update_id != q.update_id
            || reply.launch_id != q.launch_id
            || reply.request_id.as_deref() != Some(q.request_id.as_str())
        {
            return Err(
                "Keeper response changed scope/correlation; no fallback or new retirement".into(),
            );
        }
        Ok(reply.result)
    }
}

#[cfg(all(test, windows))]
mod exit_tests;
