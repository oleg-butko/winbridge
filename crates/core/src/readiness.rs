//! Pure readiness policy. No filesystem, transport, wall clock or task execution.
use crate::config::{valid_id, WorkerConfig};
use winbridge_protocol::*;

pub fn project_policy_catalog(
    config: &WorkerConfig,
    provider: BuildIdentity,
) -> Result<ProjectPolicyCatalog, String> {
    let projects = config
        .projects
        .iter()
        .map(|project| {
            // Effective recipes include expanded profiles. These are the loaded settings,
            // not a pending edit on disk. Unrelated project edits do not alter this scope.
            let mut policy = serde_json::json!({
                "schemaVersion": 1, "instanceId": config.instance_id, "nodeId": config.node_id,
                "stateDirectory": config.state_directory, "project": project,
                "workerSettings": config.worker,
                "effectiveRecipes": config.project_recipes(project)?, "delivery": config.delivery,
                "maxOutputBytesPerTask": config.max_output_bytes_per_task,
                "outputRetention": config.output_retention,
            });
            if let Some(scheduling) = &config.work_scheduling {
                policy["workScheduling"] =
                    serde_json::to_value(scheduling).map_err(|e| e.to_string())?;
            }
            if let Some(queue) = &config.queue_policy {
                policy["queuePolicy"] = serde_json::to_value(queue).map_err(|e| e.to_string())?;
            }
            if let Some(resources) = &config.resource_scheduling {
                policy["resourceScheduling"] =
                    serde_json::to_value(resources).map_err(|e| e.to_string())?;
            }
            let bytes = canonical_evidence_json(&policy).map_err(|e| e.to_string())?;
            Ok(ProjectPolicyFingerprint {
                project_id: project.id.clone(),
                sha256: evidence_sha256(&bytes),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProjectPolicyCatalog {
        schema_version: 1,
        provider,
        projects,
    })
}

fn build_valid(build: &BuildIdentity) -> bool {
    !build.source_dirty
        && !build.build_id.is_empty()
        && build.build_id.len() <= 128
        && !build.version.is_empty()
        && build.version.len() <= 80
        && build.source_commit.as_ref().is_some_and(|c| {
            c.len() == 40
                && c.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        && build
            .source_manifest_sha256
            .as_ref()
            .is_none_or(|s| is_sha256(s))
}

pub fn validate_identity(identity: &PairIdentity) -> Result<(), String> {
    if identity.schema_version != 1
        || identity.contract.id.is_empty()
        || identity.contract.id.len() > 128
        || !identity
            .contract
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        || identity.contract.version == 0
        || !is_sha256(&identity.contract_schema_sha256)
        || !is_sha256(&identity.project_policy.sha256)
        || !valid_id(&identity.project_policy.project_id)
        || !valid_id(&identity.context.instance_id)
        || !valid_id(&identity.context.journal_incarnation)
        || !build_valid(&identity.consumer)
        || !build_valid(&identity.provider)
    {
        return Err("Invalid or unversioned pair identity".into());
    }
    let route = &identity.route;
    if route.len() != 3
        || route[0].role != RouteRole::Adapter
        || route[1].role != RouteRole::Controller
        || route[2].role != RouteRole::Worker
        || route[0].build != identity.consumer
        || route[2].build != identity.provider
        || route.iter().any(|hop| {
            !valid_id(&hop.node_id)
                || !build_valid(&hop.build)
                || hop.endpoint.is_empty()
                || hop.endpoint.len() > 1024
                || hop.endpoint.chars().any(char::is_control)
        })
    {
        return Err("Unknown, incomplete or mismatched route; no readiness inferred".into());
    }
    Ok(())
}

pub fn validate_verification(receipt: &PairVerification) -> Result<(), String> {
    validate_identity(&receipt.identity)?;
    let build_proof = |proof: &BuildVerification, expected: &BuildIdentity| {
        &proof.build == expected
            && is_sha256(&proof.report_sha256)
            && valid_id(&proof.test_suite)
            && matches!(proof.platform.as_str(), "linux" | "windows")
    };
    if receipt.schema_version != 1
        || !is_sha256(&receipt.report_sha256)
        || !valid_id(&receipt.test_profile)
        || receipt.checks.is_empty()
        || receipt.checks.len() > 64
        || receipt.checks.iter().any(|c| !valid_id(c))
        || receipt
            .checks
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != receipt.checks.len()
        || !build_proof(&receipt.consumer_verification, &receipt.identity.consumer)
        || !build_proof(&receipt.provider_verification, &receipt.identity.provider)
        || receipt.consumer_verification.platform != "linux"
        || receipt.provider_verification.platform != "windows"
    {
        return Err("Invalid, incomplete or build-mismatched verification".into());
    }
    Ok(())
}

/// Required evidence belongs to a compiled known contract definition, never an
/// agent-supplied enable flag. An advertised capability is only the first fact.
pub struct ContractDefinition {
    pub contract: ContractRef,
    pub schema_sha256: String,
    pub test_profile: String,
    pub required_checks: Vec<String>,
}

/// A projection received over IPC is not an execution decision. Only this
/// evaluator can construct a decision used by a future dispatch guard.
#[derive(Debug)]
pub struct ReadinessDecision {
    projection: ContractReadiness,
}
impl ReadinessDecision {
    pub fn projection(&self) -> &ContractReadiness {
        &self.projection
    }
}
impl std::ops::Deref for ReadinessDecision {
    type Target = ContractReadiness;
    fn deref(&self) -> &Self::Target {
        &self.projection
    }
}
fn decision(projection: ContractReadiness) -> ReadinessDecision {
    ReadinessDecision { projection }
}

pub fn evaluate_readiness(
    definition: &ContractDefinition,
    supported: bool,
    observed: Option<&PairIdentity>,
    evidence: Option<&PairVerification>,
    route_fresh: bool,
    policy_allowed: bool,
) -> ReadinessDecision {
    let mut result = ContractReadiness {
        supported,
        build_verified: false,
        pair_tested: false,
        enabled: false,
        reasons: Vec::new(),
    };
    if !supported {
        result.reasons.push(ReadinessReason::UnsupportedContract);
    }
    if !route_fresh {
        result.reasons.push(ReadinessReason::RouteNotFresh);
    }
    if !policy_allowed {
        result.reasons.push(ReadinessReason::PolicyDenied);
    }
    let Some(observed) = observed else {
        result.reasons.push(ReadinessReason::MissingIdentity);
        return decision(result);
    };
    if validate_identity(observed).is_err() {
        result.reasons.push(ReadinessReason::InvalidIdentity);
        return decision(result);
    }
    if observed.contract != definition.contract
        || observed.contract_schema_sha256 != definition.schema_sha256
        || !is_sha256(&definition.schema_sha256)
    {
        result.reasons.push(ReadinessReason::SchemaChanged);
        return decision(result);
    }
    let Some(receipt) = evidence else {
        result.reasons.push(ReadinessReason::MissingEvidence);
        return decision(result);
    };
    if validate_verification(receipt).is_err() {
        result.reasons.push(ReadinessReason::InvalidEvidence);
        return decision(result);
    }
    result.build_verified = receipt.consumer_verification.build == observed.consumer
        && receipt.provider_verification.build == observed.provider;
    if !result.build_verified {
        result.reasons.push(ReadinessReason::BuildUnverified);
    }
    if receipt.identity != *observed {
        result.reasons.push(ReadinessReason::IdentityChanged);
        return decision(result);
    }
    result.pair_tested = receipt.test_profile == definition.test_profile
        && !definition.required_checks.is_empty()
        && definition
            .required_checks
            .iter()
            .all(|c| receipt.checks.contains(c));
    if !result.pair_tested {
        result.reasons.push(ReadinessReason::PairUntested);
    }
    result.enabled = result.supported
        && result.build_verified
        && result.pair_tested
        && route_fresh
        && policy_allowed;
    decision(result)
}

/// Only future contracts opt into this guard. Existing A2 admission is unchanged.
/// A received/edited read model cannot be used as an authoritative decision:
/// ```compile_fail
/// let view = winbridge_protocol::ContractReadiness {
///     supported: true, build_verified: true, pair_tested: true,
///     enabled: true, reasons: vec![],
/// };
/// winbridge_core::readiness::require_enabled(&view).unwrap();
/// ```
pub fn require_enabled(readiness: &ReadinessDecision) -> Result<(), String> {
    if readiness.enabled {
        Ok(())
    } else {
        Err(format!(
            "Contract disabled before dispatch: {:?}",
            readiness.reasons
        ))
    }
}
