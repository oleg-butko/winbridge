//! Explicit resumable verification. Plans and terminal records are immutable;
//! effects use caller-pinned worker scope and deterministic idempotency keys.
use crate::{readiness_store::ReceiptStore, Controller};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winbridge_protocol::*;
use winbridge_transport::websocket::RpcSender;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    request: VerifyPairRequest,
    adapter: BuildIdentity,
    adapter_kind: ControllerAdapter,
    provider_request: FencedRequest,
    pair_request: FencedRequest,
    report_directory: PathBuf,
}
type VerificationObservation = (BuildIdentity, ControllerAdapter, VerificationView);
#[derive(Clone)]
pub(crate) struct RecentPlans {
    paths: Vec<PathBuf>,
    truncated: bool,
}
enum VerificationFailure {
    Failed(String),
    Indeterminate(String),
}
impl From<String> for VerificationFailure {
    fn from(detail: String) -> Self {
        Self::Indeterminate(detail)
    }
}
impl From<&str> for VerificationFailure {
    fn from(detail: &str) -> Self {
        Self::Indeterminate(detail.into())
    }
}
// Dropping the supervisor (including unwinding) must release its in-memory key.
struct VerificationRun {
    owner: Arc<Controller>,
    key: String,
}
impl Drop for VerificationRun {
    fn drop(&mut self) {
        self.owner
            .verification_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}
fn durable_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let payload = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let checksum = evidence_sha256(&canonical_evidence_json(&payload).map_err(|e| e.to_string())?);
    let bytes = serde_json::to_vec(&json!({"schemaVersion":1,"sha256":checksum,"payload":payload}))
        .map_err(|e| e.to_string())?;
    if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
        return Err("Verification record exceeds 1 MiB".into());
    }
    let parent = path.parent().ok_or("Missing verification directory")?;
    crate::activity::private_directory(parent)?;
    let pending = parent.join(format!(".pending-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        match fs::hard_link(&pending, path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        };
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        let observed: Value = durable_read(path)?;
        if observed != payload {
            return Err("Immutable verification record collision; original retained".into());
        }
        Ok(())
    })();
    let _ = fs::remove_file(pending);
    result
}
fn durable_read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Verification record is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(winbridge_transport::MAX_FRAME_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
        return Err("Verification record exceeds 1 MiB".into());
    }
    let envelope: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if envelope["schemaVersion"] != 1
        || envelope["sha256"]
            != evidence_sha256(
                &canonical_evidence_json(&envelope["payload"]).map_err(|e| e.to_string())?,
            )
    {
        return Err("Verification record checksum/schema mismatch".into());
    }
    serde_json::from_value(envelope["payload"].clone()).map_err(|e| e.to_string())
}
fn ps_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn query(identity: &PairIdentity) -> ContractReadinessQuery {
    ContractReadinessQuery {
        project_id: identity.project_policy.project_id.clone(),
        contract: identity.contract.clone(),
        receipt_id: None,
    }
}
impl Controller {
    pub(crate) fn recent_verifications(
        &self,
    ) -> Result<(Vec<VerificationObservation>, bool), String> {
        let root = self
            .config
            .state_directory
            .join("contract-verifications-v1");
        if !root.exists() {
            return Ok((vec![], false));
        }
        crate::activity::private_directory(&root)?;
        // Directory enumeration order is arbitrary. Sampling its first 128
        // entries can hide every new verification forever. Rebuild an index on
        // first use/restart and after admission, retaining only the newest ten.
        let mut history = self
            .verification_history
            .lock()
            .map_err(|_| "Verification index poisoned")?;
        if history.is_none() {
            let mut entries = Vec::new();
            let mut truncated = false;
            for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    continue;
                }
                let path = entry.path().join("plan.json");
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_file() => metadata,
                    Ok(_) => return Err("Nonregular verification plan refused".into()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        truncated = true;
                        continue;
                    }
                    Err(error) => return Err(error.to_string()),
                };
                entries.push((
                    metadata.modified().map_err(|e| e.to_string())?,
                    entry.path(),
                ));
                entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                if entries.len() > PROJECTION_MAX_ITEMS {
                    entries.pop();
                    truncated = true;
                }
            }
            *history = Some(RecentPlans {
                paths: entries.into_iter().map(|(_, path)| path).collect(),
                truncated,
            });
        }
        let snapshot = history.as_ref().unwrap().clone();
        drop(history);
        let mut truncated = snapshot.truncated;
        let mut recent = Vec::new();
        for directory in snapshot.paths {
            if !directory.join("plan.json").exists() {
                // A crash between directory creation and plan publication is not
                // acceptance and must not suppress all other readonly observations.
                truncated = true;
                continue;
            }
            let plan: Plan = durable_read(&directory.join("plan.json"))?;
            if self.verification_directory(&plan.request.verification_key, false)? != directory {
                return Err("Verification directory/key mismatch".into());
            }
            let view = self.get_verification(&plan.request.verification_key)?;
            recent.push((plan.adapter, plan.adapter_kind, view));
        }
        Ok((recent, truncated))
    }
    fn verification_directory(&self, key: &str, create: bool) -> Result<PathBuf, String> {
        if !winbridge_core::config::valid_id(key) {
            return Err("verificationKey must be a stable safe ID, at most 80 characters".into());
        }
        winbridge_platform::require_local_disk(&self.config.state_directory)
            .map_err(|e| e.to_string())?;
        for ancestor in self.config.state_directory.ancestors() {
            let metadata = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err("Linked/non-directory verification ancestor refused".into());
            }
        }
        crate::activity::private_directory(&self.config.state_directory)?;
        let root = self
            .config
            .state_directory
            .join("contract-verifications-v1");
        if create {
            crate::activity::private_directory(&root)?;
        }
        let directory = root.join(evidence_sha256(key.as_bytes()));
        if create {
            crate::activity::private_directory(&directory)?;
        }
        if root.is_symlink() || directory.is_symlink() {
            return Err("Linked verification directory refused".into());
        }
        Ok(directory)
    }
    pub fn get_verification(&self, key: &str) -> Result<VerificationView, String> {
        let directory = self.verification_directory(key, false)?;
        if directory.join("terminal.json").exists() {
            return durable_read(&directory.join("terminal.json"));
        }
        let plan: Plan = durable_read(&directory.join("plan.json"))?;
        let task_ids = ["provider-task.json", "pair-task.json"]
            .into_iter()
            .filter_map(|name| {
                let path = directory.join(name);
                if path.exists() {
                    Some(durable_read::<String>(&path))
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let running = self
            .verification_runs
            .lock()
            .map_err(|_| "verification supervisor poisoned")?
            .contains(key);
        if !running && directory.join("interruption.json").exists() {
            return durable_read(&directory.join("interruption.json"));
        }
        Ok(VerificationView {
            verification_key: key.into(),
            identity: plan.request.identity,
            status: if running {
                VerificationStatus::Running
            } else {
                VerificationStatus::Indeterminate
            },
            stage: if task_ids.is_empty() {
                "accepted"
            } else if task_ids.len() == 1 {
                "provider"
            } else {
                "pair"
            }
            .into(),
            task_ids,
            receipt_id: None,
            detail: if running {
                "Verification runs against pinned scope; UI closure does not cancel accepted tasks"
            } else {
                "Original plan retained; explicit same-key retry reconciles it, never rebinds it"
            }
            .into(),
        })
    }
    pub async fn start_verification(
        self: &Arc<Self>,
        adapter: BuildIdentity,
        kind: ControllerAdapter,
        request: VerifyPairRequest,
    ) -> Result<VerificationView, String> {
        winbridge_core::readiness::validate_identity(&request.identity)?;
        self.consumer_verification(&adapter)?;
        if request.identity.project_policy.project_id != "fixture"
            || request.identity.contract != crate::readiness::definition().contract
        {
            return Err("First registered test profile covers fixture/RPC-2 only; other contracts remain untested".into());
        }
        let directory = self.verification_directory(&request.verification_key, false)?;
        if directory.join("plan.json").exists() {
            let old: Plan = durable_read(&directory.join("plan.json"))?;
            if old.request != request || old.adapter != adapter {
                return Err("Verification key belongs to a different frozen identity; no rebind or new effect".into());
            }
            if directory.join("terminal.json").exists() {
                return self.get_verification(&request.verification_key);
            }
        } else {
            let observation = self
                .observe_contract(&adapter, &kind, query(&request.identity))
                .await?;
            if observation.identity.as_ref() != Some(&request.identity)
                || observation
                    .readiness
                    .reasons
                    .contains(&ReadinessReason::PolicyDenied)
                || observation
                    .readiness
                    .reasons
                    .contains(&ReadinessReason::RouteNotFresh)
            {
                return Err("Caller-pinned verification identity no longer matches the live route/policy; no effect".into());
            }
            let sender = {
                let state = self.state.lock().map_err(|_| "controller state poisoned")?;
                state
                    .active
                    .as_ref()
                    .filter(|s| s.ready && Some(&s.id) == observation.session_id.as_ref())
                    .ok_or("Verification route changed")?
                    .sender
                    .clone()
            };
            let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
                return Err("Missing provider info".into());
            };
            let fixture = info
                .projects
                .iter()
                .find(|p| p.id == "fixture")
                .and_then(|p| p.recipes.iter().find(|r| r.id == "fixture-success"))
                .ok_or("Known fixture-success recipe required")?;
            let executable = fixture.command.executable.replace('/', "\\");
            if !executable
                .to_lowercase()
                .ends_with("\\winbridge-fixture.exe")
                || fixture.command.arguments != ["success"]
            {
                return Err(
                    "Known fixture executable/arguments required; arbitrary recipe is not tested"
                        .into(),
                );
            }
            let runtime = executable
                .rsplit_once('\\')
                .ok_or("Invalid fixture executable path")?
                .0;
            let project=info.projects.iter().find(|p|p.id=="snapshot-fixture" && p.allow_direct_commands).ok_or("Registered snapshot-fixture direct-command scope required for isolated provider test")?;
            let delivery = self
                .config
                .delivery
                .as_ref()
                .ok_or("Shared report mapping required")?;
            let suffix = evidence_sha256(request.verification_key.as_bytes());
            let run = format!("pair-{}", &suffix[..32]);
            let report_directory = delivery
                .root
                .join(&delivery.namespace)
                .join("workflow-validation")
                .join(&run);
            let peer = delivery
                .peer_root
                .as_ref()
                .ok_or("Shared peerRoot required")?
                .trim_end_matches(['\\', '/']);
            let settings = json!({"workerExecutable":format!("{runtime}\\winbridge-worker.exe"),"fixtureExecutable":executable,"workDirectory":format!("{}\\provider-verification\\{run}",project.workspace_root.trim_end_matches(['\\','/'])),"reportDirectory":format!("{peer}\\{}\\workflow-validation\\{run}",delivery.namespace),"expectedBuild":request.identity.provider,"expectedSchemaSha256":request.identity.contract_schema_sha256});
            let script = format!(
                "$wbVerification = {} | ConvertFrom-Json -Depth 30\n{}",
                ps_string(&settings.to_string()),
                include_str!("../../../scripts/verify-installed-provider.ps1")
            );
            let task = TaskSubmission {
                protocol_version: 1,
                request_id: format!("verify-provider-{}", &suffix[..32]),
                project_id: project.id.clone(),
                idempotency_key: format!("verify-provider-{}", &suffix[..32]),
                timeout_ms: 180000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "pwsh.exe".into(),
                    arguments: vec![
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-Command".into(),
                        script,
                    ],
                    working_directory: ".".into(),
                },
            };
            let pair = TaskSubmission {
                protocol_version: 1,
                request_id: format!("verify-pair-{}", &suffix[..32]),
                project_id: "fixture".into(),
                idempotency_key: format!("verify-pair-{}", &suffix[..32]),
                timeout_ms: 10000,
                snapshot_id: None,
                command: CommandSource::Recipe {
                    recipe_id: "fixture-success".into(),
                    arguments: vec![],
                },
            };
            let plan = Plan {
                request: request.clone(),
                adapter,
                adapter_kind: kind,
                provider_request: FencedRequest {
                    request_contract_version: 2,
                    context: request.identity.context.clone(),
                    request: LocalRequest::Submit { request: task },
                },
                pair_request: FencedRequest {
                    request_contract_version: 2,
                    context: request.identity.context.clone(),
                    request: LocalRequest::Submit { request: pair },
                },
                report_directory,
            };
            let directory = self.verification_directory(&request.verification_key, true)?;
            durable_new(&directory.join("plan.json"), &plan)?;
            *self
                .verification_history
                .lock()
                .map_err(|_| "Verification index poisoned")? = None;
        }
        let key = request.verification_key.clone();
        let spawn = self
            .verification_runs
            .lock()
            .map_err(|_| "verification supervisor poisoned")?
            .insert(key.clone());
        if spawn {
            let owner = Arc::clone(self);
            tokio::spawn(async move {
                let _run = VerificationRun {
                    owner: Arc::clone(&owner),
                    key: key.clone(),
                };
                let executor = Arc::clone(&owner);
                let execution_key = key.clone();
                // JoinError becomes an explicit unknown outcome; it cannot strand the key.
                let result =
                    tokio::spawn(async move { executor.run_verification(&execution_key).await })
                        .await
                        .unwrap_or_else(|error| {
                            Err(VerificationFailure::Indeterminate(format!(
                                "Verification executor interrupted: {error}"
                            )))
                        });
                if let Err(error) = result {
                    if let Ok(mut view) = owner.get_verification(&key) {
                        let (name, status, detail) = match error {
                            VerificationFailure::Failed(detail) => {
                                ("terminal.json", VerificationStatus::Failed, detail)
                            }
                            VerificationFailure::Indeterminate(detail) => (
                                "interruption.json",
                                VerificationStatus::Indeterminate,
                                detail,
                            ),
                        };
                        view.status = status;
                        view.detail = detail;
                        // The first interruption stays historical. Explicit same-key retry
                        // may reconcile the original task keys, but never replace the plan.
                        if let Err(error) = durable_new(&directory.join(name), &view) {
                            eprintln!("Verification {key}: cannot persist outcome: {error}");
                        }
                    }
                }
            });
        }
        self.get_verification(&request.verification_key)
    }
    async fn pinned_sender(&self, plan: &Plan) -> Result<RpcSender, String> {
        let view = self
            .observe_contract(
                &plan.adapter,
                &plan.adapter_kind,
                query(&plan.request.identity),
            )
            .await?;
        if view.identity.as_ref() != Some(&plan.request.identity)
            || view
                .readiness
                .reasons
                .contains(&ReadinessReason::PolicyDenied)
            || view
                .readiness
                .reasons
                .contains(&ReadinessReason::RouteNotFresh)
        {
            return Err(
                "Frozen verification identity/policy/route changed; no next effect or rebind"
                    .into(),
            );
        }
        let state = self.state.lock().map_err(|_| "controller state poisoned")?;
        Ok(state
            .active
            .as_ref()
            .filter(|s| s.ready && Some(&s.id) == view.session_id.as_ref())
            .ok_or("Verification session no longer fresh")?
            .sender
            .clone())
    }
    fn consumer_verification(&self, expected: &BuildIdentity) -> Result<BuildVerification, String> {
        if expected != &build_identity() {
            return Err("Current consumer verifier requires adapter and controller from the same tested executable".into());
        }
        let id = evidence_sha256(&serde_json::to_vec(expected).map_err(|e| e.to_string())?);
        let path = self
            .config
            .state_directory
            .join("consumer-build-verifications-v1")
            .join(format!("consumer-{id}.json"));
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Consumer proof must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 256 * 1024 {
            return Err("Consumer proof exceeds 256 KiB".into());
        }
        let proof: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let checks = json!([
            "actual-cli-identity",
            "actual-mcp-stdio",
            "actual-ws-ipc-route",
            "schema-policy-scope-preserved",
            "unknown-contract-no-dispatch",
            "disconnect-no-enable"
        ]);
        if proof["schemaVersion"] != 1
            || proof["profile"] != "controller-consumer-rpc2-v1"
            || proof["passed"] != true
            || proof["platform"] != "linux"
            || proof["build"] != serde_json::to_value(expected).map_err(|e| e.to_string())?
            || proof["checks"] != checks
            || proof["binarySha256"]
                != evidence_sha256(&fs::read("/proc/self/exe").map_err(|e| e.to_string())?)
        {
            return Err("Actual consumer build evidence missing, changed or incomplete; no verification effect".into());
        }
        Ok(BuildVerification {
            build: expected.clone(),
            platform: "linux".into(),
            test_suite: "controller-consumer-rpc2-v1".into(),
            report_sha256: evidence_sha256(&bytes),
        })
    }
    async fn wait_task(
        sender: &RpcSender,
        context: &RequestContext,
        id: &str,
    ) -> Result<TaskRecord, VerificationFailure> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(200);
        loop {
            let response = sender
                .call_fenced(FencedRequest {
                    request_contract_version: 2,
                    context: context.clone(),
                    request: LocalRequest::GetTask { task_id: id.into() },
                })
                .await?;
            let LocalResponse::Task { task } = response.response else {
                return Err("Task observation failed; original scope/key retained".into());
            };
            if task.status.is_terminal() {
                if task.status == TaskStatus::Succeeded && task.exit_code == Some(0) {
                    return Ok(*task);
                }
                return Err(VerificationFailure::Failed(format!(
                    "Verification task {id} ended {:?}; no receipt",
                    task.status
                )));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("Verification observation deadline; no automatic command retry".into());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    async fn accept_task(
        &self,
        plan: &Plan,
        request: &FencedRequest,
        path: &Path,
    ) -> Result<(RpcSender, String), String> {
        let sender = self.pinned_sender(plan).await?;
        let response = sender.call_fenced(request.clone()).await?;
        let LocalResponse::Task { task } = response.response else {
            return Err(
                "Verification acceptance unconfirmed/rejected; pinned request retained".into(),
            );
        };
        durable_new(path, &task.task_id)?;
        Ok((sender, task.task_id))
    }
    async fn run_verification(&self, key: &str) -> Result<(), VerificationFailure> {
        let directory = self.verification_directory(key, false)?;
        let plan: Plan = durable_read(&directory.join("plan.json"))?;
        let consumer = self.consumer_verification(&plan.adapter)?;
        let (sender, provider_id) = self
            .accept_task(
                &plan,
                &plan.provider_request,
                &directory.join("provider-task.json"),
            )
            .await?;
        Self::wait_task(&sender, &plan.request.identity.context, &provider_id).await?;
        let report_bytes = winbridge_workspace_sync::read_bounded(
            &plan.report_directory.join("provider-report.json"),
            256 * 1024,
        )?;
        let ready: Value = serde_json::from_slice(&winbridge_workspace_sync::read_bounded(
            &plan.report_directory.join("provider-report.ready.json"),
            16384,
        )?)
        .map_err(|e| e.to_string())?;
        let proof: Value = serde_json::from_slice(&report_bytes).map_err(|e| e.to_string())?;
        let checks = json!([
            "binary-description",
            "ipc-info-schema-policy",
            "task-success-dedup",
            "output-exit",
            "stale-scope-rejected",
            "managed-tree-cancel",
            "isolated-provider-stopped"
        ]);
        if ready["schemaVersion"] != 1
            || proof["schemaVersion"] != 1
            || ready["passed"] != true
            || ready["sha256"] != evidence_sha256(&report_bytes)
            || ready["bytes"] != report_bytes.len()
            || proof["passed"] != true
            || proof["platform"] != "windows"
            || proof["profile"] != "installed-provider-rpc2-v1"
            || proof["checks"] != checks
            || proof["build"]
                != serde_json::to_value(&plan.request.identity.provider)
                    .map_err(|e| e.to_string())?
            || !proof["binarySha256"].as_str().is_some_and(is_sha256)
        {
            return Err(VerificationFailure::Failed(
                "Actual installed-provider report mismatch/incomplete; no pair receipt".into(),
            ));
        }
        let provider = BuildVerification {
            build: plan.request.identity.provider.clone(),
            platform: "windows".into(),
            test_suite: "installed-provider-rpc2-v1".into(),
            report_sha256: evidence_sha256(&report_bytes),
        };
        let (sender, pair_id) = self
            .accept_task(&plan, &plan.pair_request, &directory.join("pair-task.json"))
            .await?;
        let task = Self::wait_task(&sender, &plan.request.identity.context, &pair_id).await?;
        let duplicate = sender.call_fenced(plan.pair_request.clone()).await?;
        if !matches!(duplicate.response,LocalResponse::Task{task} if task.task_id==pair_id) {
            return Err("Pair duplicate did not return original task".into());
        }
        let mut wrong = plan.pair_request.clone();
        wrong.context.journal_incarnation = "wrong-verification-incarnation".into();
        let refused = sender.call_fenced(wrong).await?;
        if refused.outcome != FencedReplyOutcome::RejectedBeforeDispatch
            || !matches!(refused.response, LocalResponse::Error { .. })
        {
            return Err("Pair stale scope not rejected before dispatch".into());
        }
        let response = sender
            .call_fenced(FencedRequest {
                request_contract_version: 2,
                context: plan.request.identity.context.clone(),
                request: LocalRequest::ReadOutput {
                    task_id: pair_id.clone(),
                    after_sequence: 0,
                },
            })
            .await?;
        let LocalResponse::Output { chunks } = response.response else {
            return Err("Pair output unavailable".into());
        };
        let stdout: Vec<u8> = chunks
            .iter()
            .filter(|c| matches!(c.stream, OutputStream::Stdout))
            .flat_map(|c| c.bytes.clone())
            .collect();
        if !String::from_utf8(stdout)
            .map_err(|e| e.to_string())?
            .contains("Привет ✓")
            || task.exit_code != Some(0)
        {
            return Err("Pair output/exit mismatch".into());
        }
        let _fresh = self.pinned_sender(&plan).await?;
        let report = json!({"schemaVersion":1,"identity":plan.request.identity,"providerTaskId":provider_id,"pairTaskId":pair_id,"checks":crate::readiness::definition().required_checks});
        durable_new(&directory.join("pair-report.json"), &report)?;
        let receipt = PairVerification {
            schema_version: 1,
            identity: plan.request.identity.clone(),
            consumer_verification: consumer,
            provider_verification: provider,
            test_profile: crate::readiness::PAIR_PROFILE.into(),
            report_sha256: evidence_sha256(
                &canonical_evidence_json(&report).map_err(|e| e.to_string())?,
            ),
            checks: crate::readiness::definition().required_checks,
        };
        let receipt_id = ReceiptStore::open(&self.config.state_directory)?.put(&receipt)?;
        let mut view = self.get_verification(key)?;
        view.status = VerificationStatus::Succeeded;
        view.stage = "complete".into();
        view.receipt_id = Some(receipt_id);
        view.detail="Actual consumer/provider and paired route verified; receipt is evidence, not an execution permission".into();
        durable_new(&directory.join("terminal.json"), &view)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interruption_is_reconcilable_and_a_dropped_supervisor_cannot_strand_the_key() {
        let temp = tempfile::tempdir().unwrap();
        let config = serde_json::from_value(json!({"schemaVersion":1,"instanceId":"verification-test","nodeId":"controller","stateDirectory":temp.path().join("state"),"controller":{"listenAddress":"127.0.0.1:1143","workerNodeId":"worker"},"projects":[]})).unwrap();
        let owner = Controller::new(config).unwrap();
        let receipt: PairVerification =
            serde_json::from_str(include_str!("../../../tests/fixtures/readiness/pair.json"))
                .unwrap();
        let key = "original-verification";
        owner
            .verification_directory("unpublished-plan", true)
            .unwrap();
        let directory = owner.verification_directory(key, true).unwrap();
        let request = FencedRequest {
            request_contract_version: 2,
            context: receipt.identity.context.clone(),
            request: LocalRequest::Info,
        };
        let plan = Plan {
            request: VerifyPairRequest {
                verification_key: key.into(),
                identity: receipt.identity,
            },
            adapter: build_identity(),
            adapter_kind: ControllerAdapter::Mcp,
            provider_request: request.clone(),
            pair_request: request,
            report_directory: temp.path().join("report"),
        };
        durable_new(&directory.join("plan.json"), &plan).unwrap();
        let (recent, truncated) = owner.recent_verifications().unwrap();
        assert_eq!(recent.len(), 1);
        assert!(truncated);
        let mut interrupted = owner.get_verification(key).unwrap();
        assert!(matches!(
            interrupted.status,
            VerificationStatus::Indeterminate
        ));
        interrupted.detail = "Reply lost; retain original task keys".into();
        durable_new(&directory.join("interruption.json"), &interrupted).unwrap();
        assert_eq!(
            owner.get_verification(key).unwrap().detail,
            interrupted.detail
        );
        owner.verification_runs.lock().unwrap().insert(key.into());
        let run = VerificationRun {
            owner: Arc::clone(&owner),
            key: key.into(),
        };
        assert!(matches!(
            owner.get_verification(key).unwrap().status,
            VerificationStatus::Running
        ));
        drop(run);
        assert!(matches!(
            owner.get_verification(key).unwrap().status,
            VerificationStatus::Indeterminate
        ));
        let mut terminal = interrupted;
        terminal.status = VerificationStatus::Succeeded;
        terminal.receipt_id = Some("a".repeat(64));
        durable_new(&directory.join("terminal.json"), &terminal).unwrap();
        assert!(matches!(
            owner.get_verification(key).unwrap().status,
            VerificationStatus::Succeeded
        ));
        assert_eq!(
            durable_read::<Plan>(&directory.join("plan.json"))
                .unwrap()
                .request,
            plan.request
        );
        assert!(directory.join("interruption.json").exists());
    }
    #[test]
    fn recent_index_keeps_newest_admissions_beyond_128_and_rebuilds_after_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let config = serde_json::from_value(json!({"schemaVersion":1,"instanceId":"verification-history","nodeId":"controller","stateDirectory":temp.path().join("state"),"controller":{"listenAddress":"127.0.0.1:1143","workerNodeId":"worker"},"projects":[]})).unwrap();
        let owner = Controller::new(config).unwrap();
        let receipt: PairVerification =
            serde_json::from_str(include_str!("../../../tests/fixtures/readiness/pair.json"))
                .unwrap();
        for index in 0..140 {
            let key = format!("verification-{index:04}");
            let directory = owner.verification_directory(&key, true).unwrap();
            let request = FencedRequest {
                request_contract_version: 2,
                context: receipt.identity.context.clone(),
                request: LocalRequest::Info,
            };
            let plan = Plan {
                request: VerifyPairRequest {
                    verification_key: key,
                    identity: receipt.identity.clone(),
                },
                adapter: build_identity(),
                adapter_kind: ControllerAdapter::Mcp,
                provider_request: request.clone(),
                pair_request: request,
                report_directory: temp.path().join("report"),
            };
            let path = directory.join("plan.json");
            durable_new(&path, &plan).unwrap();
            fs::File::open(path)
                .unwrap()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(index))
                .unwrap();
        }
        let assert_latest = |owner: &Controller| {
            let (recent, truncated) = owner.recent_verifications().unwrap();
            assert!(truncated);
            assert_eq!(recent.len(), PROJECTION_MAX_ITEMS);
            assert_eq!(recent[0].2.verification_key, "verification-0139");
            assert_eq!(recent[9].2.verification_key, "verification-0130");
        };
        assert_latest(&owner);
        assert_latest(&owner);
        // The index is disposable. Durable plans remain the source after restart.
        *owner.verification_history.lock().unwrap() = None;
        assert_latest(&owner);
    }
    #[test]
    fn immutable_records_dedup_and_detect_partial_replaced_and_future_data() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = temp.path().join("plan.json");
        let payload = json!({"context":"original-scope","request":{"key":"one"}});
        durable_new(&path, &payload).unwrap();
        durable_new(&path, &payload).unwrap();
        assert_eq!(durable_read::<Value>(&path).unwrap(), payload);
        assert!(durable_new(&path, &json!({"context":"changed-scope"})).is_err());
        assert_eq!(durable_read::<Value>(&path).unwrap(), payload);
        let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope["payload"]["context"] = "forged".into();
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(durable_read::<Value>(&path).is_err());
        fs::write(&path, b"{\"schemaVersion\":1").unwrap();
        assert!(durable_read::<Value>(&path).is_err());
    }
    #[test]
    fn linked_and_nonregular_records_never_write_the_target_or_wait_for_writer() {
        use std::{
            ffi::CString,
            os::unix::{
                ffi::OsStrExt,
                fs::{symlink, PermissionsExt},
            },
        };
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = temp.path().join("untouched");
        fs::write(&target, b"untouched").unwrap();
        let path = temp.path().join("plan.json");
        symlink(&target, &path).unwrap();
        assert!(durable_read::<Value>(&path).is_err());
        assert!(durable_new(&path, &json!({"a":1})).is_err());
        assert_eq!(fs::read(target).unwrap(), b"untouched");
        let fifo = temp.path().join("fifo.json");
        let c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(durable_read::<Value>(&fifo).is_err());
    }
}
