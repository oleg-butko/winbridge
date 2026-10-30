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
pub(super) async fn run(
    mut listener: ipc::Listener,
    r: SupervisorRecord,
    resources: Box<crate::activation::CoordinatorRetirement>,
    initial: Option<(ipc::Stream, KeeperControlRequest)>,
) -> Result<()> {
    std::hint::black_box(KEEPER_CONTROL_MARKER);
    let mut state = State::Retiring(resources).retry();
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
        let (mut stream, pid) = match listener.accept().await {
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
        let q: KeeperControlRequest = match ipc::read(&mut stream).await {
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
                    status: Box::new(state.status()),
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
