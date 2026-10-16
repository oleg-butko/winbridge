//! Admission policy is pure; only the Store transaction may commit a root.
use winbridge_protocol::{QueueBlocker, QueueMode, QueuePolicy};
pub fn admission_blocker(
    mode: QueueMode,
    policy: Option<&QueuePolicy>,
    total: u32,
    project: u32,
    project_id: &str,
) -> Option<QueueBlocker> {
    if mode.admission_paused() {
        return Some(QueueBlocker::AdmissionPaused);
    }
    let policy = policy?;
    if total >= policy.max_pending_roots {
        return Some(QueueBlocker::GlobalLimit {
            limit: policy.max_pending_roots,
        });
    }
    if let Some(limit) = policy
        .max_pending_roots_per_project
        .filter(|limit| project >= *limit)
    {
        return Some(QueueBlocker::ProjectLimit {
            project_id: project_id.into(),
            limit,
        });
    }
    None
}
