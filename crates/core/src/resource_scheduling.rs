//! Static resource declarations and immutable admission plans; no OS effects.
use crate::config::valid_id;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use winbridge_protocol::*;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenResourceRequirement {
    pub resource_id: String,
    pub kind: ResourceKind,
    pub units: u16,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenResourcePlan {
    pub schema_version: u16,
    pub policy_sha256: Option<String>,
    pub task: Vec<FrozenResourceRequirement>,
    pub import: Vec<FrozenResourceRequirement>,
    pub build: Vec<FrozenResourceRequirement>,
    pub start: Vec<FrozenResourceRequirement>,
}
impl Default for FrozenResourcePlan {
    fn default() -> Self {
        Self {
            schema_version: 1,
            policy_sha256: None,
            task: vec![],
            import: vec![],
            build: vec![],
            start: vec![],
        }
    }
}
pub fn validate(policy: &ResourceSchedulingPolicy, projects: &[String]) -> Result<(), String> {
    if policy.resources.is_empty()
        || policy.resources.len() > 64
        || policy.projects.is_empty()
        || policy.projects.len() > 32
    {
        return Err(
            "resourceScheduling requires 1..64 resources and 1..32 project bindings".into(),
        );
    }
    let mut definitions = HashMap::new();
    for r in &policy.resources {
        if !valid_id(&r.id)
            || r.capacity == 0
            || r.capacity > 64
            || (r.kind != ResourceKind::Build && r.capacity != 1)
            || definitions.insert(&r.id, r).is_some()
        {
            return Err(
                "resourceScheduling: invalid/duplicate resource ID, kind or capacity".into(),
            );
        }
    }
    let mut bound = HashSet::new();
    for p in &policy.projects {
        if !projects.contains(&p.project_id) || !bound.insert(&p.project_id) {
            return Err("resourceScheduling: unknown/duplicate project binding".into());
        }
        for set in [&p.task, &p.import, &p.build, &p.start] {
            if set.len() > 16 {
                return Err("resourceScheduling: at most 16 requirements per set".into());
            }
            let mut ids = HashSet::new();
            for r in set {
                if !definitions
                    .get(&r.resource_id)
                    .is_some_and(|d| r.units > 0 && r.units <= d.capacity)
                    || !ids.insert(&r.resource_id)
                {
                    return Err(
                        "resourceScheduling: unknown/duplicate resource or impossible units".into(),
                    );
                }
            }
        }
        for step in [&p.build, &p.start] {
            if p.task.len() + step.len() > 16
                || step
                    .iter()
                    .any(|r| p.task.iter().any(|b| b.resource_id == r.resource_id))
            {
                return Err(
                    "resourceScheduling: duplicate/oversized combined task and workflow-step set"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
pub fn freeze(
    policy: Option<&ResourceSchedulingPolicy>,
    project: &str,
) -> Result<FrozenResourcePlan, String> {
    let Some(p) = policy else {
        return Ok(FrozenResourcePlan::default());
    };
    let sha = evidence_sha256(
        &canonical_evidence_json(&serde_json::to_value(p).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    );
    let Some(b) = p.projects.iter().find(|b| b.project_id == project) else {
        return Ok(FrozenResourcePlan {
            policy_sha256: Some(sha),
            ..Default::default()
        });
    };
    let set =
        |requirements: &[ResourceRequirement]| -> Result<Vec<FrozenResourceRequirement>, String> {
            requirements
                .iter()
                .map(|r| {
                    Ok(FrozenResourceRequirement {
                        resource_id: r.resource_id.clone(),
                        units: r.units,
                        kind: p
                            .resources
                            .iter()
                            .find(|d| d.id == r.resource_id)
                            .ok_or("Unknown resource in loaded policy")?
                            .kind,
                    })
                })
                .collect()
        };
    let task = set(&b.task)?;
    let mut build = task.clone();
    build.extend(set(&b.build)?);
    let mut start = task.clone();
    start.extend(set(&b.start)?);
    Ok(FrozenResourcePlan {
        schema_version: 1,
        policy_sha256: Some(sha),
        task,
        import: set(&b.import)?,
        build,
        start,
    })
}

/// Availability may depend on other projects; permission/evidence for this
/// project depends only on its binding and referenced definitions.
pub fn project_policy(policy: &ResourceSchedulingPolicy, project: &str) -> serde_json::Value {
    let binding = policy.projects.iter().find(|p| p.project_id == project);
    let referenced = binding
        .into_iter()
        .flat_map(|p| {
            p.task
                .iter()
                .chain(&p.import)
                .chain(&p.build)
                .chain(&p.start)
        })
        .map(|r| &r.resource_id)
        .collect::<HashSet<_>>();
    let mut definitions = policy
        .resources
        .iter()
        .filter(|r| referenced.contains(&r.id))
        .collect::<Vec<_>>();
    definitions.sort_by(|a, b| a.id.cmp(&b.id));
    serde_json::json!({"enabled":true,"project":binding,"resources":definitions})
}
