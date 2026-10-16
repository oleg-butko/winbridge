//! Root admission policy and a separate local-only queue control contract.
use crate::*;
use serde::{Deserialize, Serialize};
pub const QUEUE_CONTROL_CONTRACT: &str = "winbridge.queue-control";
macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all="camelCase", deny_unknown_fields)]
    $item
)*}; }
dto! {
    pub struct QueuePolicy { pub max_pending_roots: u32, pub max_pending_roots_per_project: Option<u32> }
    pub struct QueueProjectPending { pub project_id: String, #[serde(deserialize_with="crate::work_scheduling::read_count")] pub pending_roots: u32 }
    pub struct QueueControlObservation {
        #[serde(deserialize_with="crate::discovery::deserialize_version")] pub schema_version: u16,
        pub provider: BuildIdentity, pub context: RequestContext, pub revision: String,
        pub mode: QueueMode,
        #[serde(default, deserialize_with="read_policy")] pub policy: Option<QueuePolicy>,
        #[serde(deserialize_with="crate::work_scheduling::read_count")] pub pending_roots: u32,
        pub projects: Vec<QueueProjectPending>, pub blockers: Vec<QueueBlocker>,
        pub last_command_id: Option<String>,
        #[serde(deserialize_with="crate::work_scheduling::read_time")] pub changed_at_ms: u64,
        #[serde(deserialize_with="crate::work_scheduling::read_time")] pub observed_at_ms: u64,
        pub read_only: bool,
    }
    pub struct QueueControlRequest { pub queue_control_version: u16, pub context: RequestContext, pub command: QueueControlCommand }
    pub struct QueueControlReceipt { pub context: RequestContext, pub command_id: String, pub from_revision: String, pub to_revision: String, pub mode: QueueMode, pub changed: bool, pub changed_at_ms: u64 }
    pub struct QueueControlResponse { pub queue_control_version: u16, pub context: Option<RequestContext>, pub result: QueueControlResult }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueMode {
    #[default]
    Running,
    AdmissionPaused,
    DispatchPaused,
    Paused,
}
impl QueueMode {
    pub fn admission_paused(self) -> bool {
        matches!(self, Self::AdmissionPaused | Self::Paused)
    }
    pub fn dispatch_paused(self) -> bool {
        matches!(self, Self::DispatchPaused | Self::Paused)
    }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum QueueControlCommand {
    Observe {
        command_id: Option<String>,
    },
    Set {
        command_id: String,
        expected_revision: String,
        mode: QueueMode,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum QueueBlocker {
    AdmissionPaused,
    DispatchPaused,
    GlobalLimit {
        #[serde(deserialize_with = "crate::work_scheduling::read_count")]
        limit: u32,
    },
    ProjectLimit {
        project_id: String,
        #[serde(deserialize_with = "crate::work_scheduling::read_count")]
        limit: u32,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueControlError {
    UnsupportedVersion,
    InvalidRequest,
    ScopeMismatch,
    ReadOnly,
    RevisionConflict,
    KeyConflict,
    Unavailable,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum QueueControlResult {
    Observed {
        observation: QueueControlObservation,
        receipt: Option<QueueControlReceipt>,
    },
    Applied {
        observation: QueueControlObservation,
        receipt: QueueControlReceipt,
    },
    Rejected {
        code: QueueControlError,
        message: String,
    },
    Unconfirmed {
        message: String,
    },
}
pub fn valid_queue_revision(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<i64>().is_ok()
}
pub fn valid_queue_policy(p: &QueuePolicy) -> bool {
    (1..=10000).contains(&p.max_pending_roots)
        && p.max_pending_roots_per_project
            .is_none_or(|v| v > 0 && v <= p.max_pending_roots)
}
fn read_policy<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<QueuePolicy>, D::Error> {
    use serde::de::Error;
    let Some(mut value) = Option::<serde_json::Value>::deserialize(d)? else {
        return Ok(None);
    };
    for name in ["maxPendingRoots", "maxPendingRootsPerProject"] {
        if let Some(number) = value.get_mut(name).filter(|n| !n.is_null()) {
            let n = number
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0.0 && *n >= 0.0 && *n <= 10000.0)
                .ok_or_else(|| D::Error::custom("Invalid queue policy metadata integer"))?;
            *number = serde_json::json!(n as u32);
        }
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(D::Error::custom)
}
pub fn queue_blockers(
    mode: QueueMode,
    policy: Option<&QueuePolicy>,
    total: u32,
    projects: &[QueueProjectPending],
) -> Vec<QueueBlocker> {
    let mut result = Vec::new();
    if mode.admission_paused() {
        result.push(QueueBlocker::AdmissionPaused)
    }
    if mode.dispatch_paused() {
        result.push(QueueBlocker::DispatchPaused)
    }
    if let Some(p) = policy {
        if total >= p.max_pending_roots {
            result.push(QueueBlocker::GlobalLimit {
                limit: p.max_pending_roots,
            })
        }
        if let Some(limit) = p.max_pending_roots_per_project {
            for project in projects {
                if project.pending_roots >= limit {
                    result.push(QueueBlocker::ProjectLimit {
                        project_id: project.project_id.clone(),
                        limit,
                    })
                }
            }
        }
    }
    result
}
pub fn valid_queue_observation(view: &QueueControlObservation) -> bool {
    if view.schema_version != 1
        || !valid_queue_revision(&view.revision)
        || view.policy.as_ref().is_some_and(|p| !valid_queue_policy(p))
    {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    for p in &view.projects {
        if p.project_id.is_empty() || !seen.insert(&p.project_id) {
            return false;
        }
        total += u64::from(p.pending_roots)
    }
    total == u64::from(view.pending_roots)
        && view.blockers
            == queue_blockers(
                view.mode,
                view.policy.as_ref(),
                view.pending_roots,
                &view.projects,
            )
}
pub fn interpret_queue_control(info: &WorkerInfo) -> Option<QueueControlObservation> {
    let CatalogObservation::Known(catalog) = interpret_catalog(info) else {
        return None;
    };
    if !catalog.supports(QUEUE_CONTROL_CONTRACT, 1) {
        return None;
    }
    let view: QueueControlObservation = serde_json::from_value(catalog.queue_control?).ok()?;
    if !valid_queue_observation(&view)
        || info.build.as_ref() != Some(&view.provider)
        || !info
            .fenced_requests
            .as_ref()
            .is_some_and(|f| f.context == view.context)
    {
        return None;
    }
    Some(view)
}
impl QueueControlResponse {
    pub fn validate_for(&self, request: &QueueControlRequest) -> Result<(), String> {
        let invalid = || {
            "Unconfirmed queue response; retain original command ID/context/revision".to_string()
        };
        if request.queue_control_version != 1
            || self.queue_control_version != 1
            || self.context.as_ref().is_some_and(|c| c != &request.context)
        {
            return Err(invalid());
        }
        let (observation, receipt) = match &self.result {
            QueueControlResult::Observed {
                observation,
                receipt,
            } => {
                if !matches!(request.command, QueueControlCommand::Observe { .. }) {
                    return Err(invalid());
                }
                (observation, receipt.as_ref())
            }
            QueueControlResult::Applied {
                observation,
                receipt,
            } => {
                if !matches!(request.command, QueueControlCommand::Set { .. }) {
                    return Err(invalid());
                }
                (observation, Some(receipt))
            }
            QueueControlResult::Rejected { .. } | QueueControlResult::Unconfirmed { .. } => {
                return Ok(())
            }
        };
        if self.context.as_ref() != Some(&request.context)
            || observation.context != request.context
            || !valid_queue_observation(observation)
        {
            return Err(invalid());
        }
        if let Some(r) = receipt {
            let from = r
                .from_revision
                .parse::<i64>()
                .ok()
                .filter(|_| valid_queue_revision(&r.from_revision))
                .ok_or_else(invalid)?;
            let to = r
                .to_revision
                .parse::<i64>()
                .ok()
                .filter(|_| valid_queue_revision(&r.to_revision))
                .ok_or_else(invalid)?;
            let observed = observation.revision.parse::<i64>().map_err(|_| invalid())?;
            if r.context != request.context
                || r.changed_at_ms > 9_007_199_254_740_991
                || to > observed
                || (r.changed && from.checked_add(1) != Some(to))
                || (!r.changed && from != to)
            {
                return Err(invalid());
            }
            match &request.command {
                QueueControlCommand::Set {
                    command_id,
                    expected_revision,
                    mode,
                } if &r.command_id == command_id
                    && &r.from_revision == expected_revision
                    && &r.mode == mode => {}
                QueueControlCommand::Observe {
                    command_id: Some(id),
                } if &r.command_id == id => {}
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
}
#[cfg(feature = "jsonschema")]
pub fn queue_control_schema_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":ContractRef{id:QUEUE_CONTROL_CONTRACT.into(),version:1},"request":schemars::schema_for!(QueueControlRequest),"response":schemars::schema_for!(QueueControlResponse)})).expect("queue schema"))).clone()
}
