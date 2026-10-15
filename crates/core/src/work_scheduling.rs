//! Deterministic ordering only; durable claims remain the execution authority.
use std::collections::BTreeMap;
use winbridge_protocol::WorkSchedulingPolicy;

pub fn validate(policy: &WorkSchedulingPolicy) -> Result<(), String> {
    if !(1..=32).contains(&policy.max_concurrent_work) {
        return Err("workScheduling.maxConcurrentWork must be 1..32".into());
    }
    Ok(())
}
pub fn fair_project_order(projects: &[String], last_grants: &BTreeMap<String, u64>) -> Vec<String> {
    let mut ordered = projects.to_vec();
    ordered.sort_by(|a, b| {
        last_grants
            .get(a)
            .copied()
            .unwrap_or(0)
            .cmp(&last_grants.get(b).copied().unwrap_or(0))
            .then(a.cmp(b))
    });
    ordered
}
