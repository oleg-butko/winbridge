//! Build evidence, tested pairs and live permission are distinct facts.
use crate::{BuildIdentity, ContractRef, RequestContext, WorkerInfo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn evidence_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Sorted object keys at every level, independent of serde_json's optional
/// preserve_order feature. Array order and typed numeric values stay intact.
pub fn canonical_evidence_json(value: &serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    fn sorted(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => {
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                let mut map = serde_json::Map::new();
                for key in keys {
                    map.insert(key.clone(), sorted(&fields[key]));
                }
                serde_json::Value::Object(map)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(sorted).collect())
            }
            scalar => scalar.clone(),
        }
    }
    serde_json::to_vec(&sorted(value))
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

macro_rules! dto {
    ($($item:item)*) => {$ (
        #[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
        #[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
        #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        $item
    )*};
}

dto! {
    pub struct ContractSchemaFingerprint {
        pub contract: ContractRef,
        pub sha256: String,
    }
    pub struct ContractSchemaCatalog {
        #[serde(deserialize_with = "crate::discovery::deserialize_version")]
        pub schema_version: u16,
        pub provider: BuildIdentity,
        pub schemas: Vec<ContractSchemaFingerprint>,
    }
    pub struct ProjectPolicyFingerprint {
        pub project_id: String,
        pub sha256: String,
    }
    pub struct ProjectPolicyCatalog {
        #[serde(deserialize_with = "crate::discovery::deserialize_version")]
        pub schema_version: u16,
        pub provider: BuildIdentity,
        pub projects: Vec<ProjectPolicyFingerprint>,
    }
    pub struct RouteHop {
        /// The ordered route is adapter -> controller -> worker.
        pub role: RouteRole,
        pub node_id: String,
        pub endpoint: String,
        pub build: BuildIdentity,
    }
    pub struct PairIdentity {
        pub schema_version: u16,
        pub contract: ContractRef,
        pub contract_schema_sha256: String,
        pub consumer: BuildIdentity,
        pub provider: BuildIdentity,
        pub context: RequestContext,
        pub route_transport: RouteTransport,
        /// Session UUID is freshness, and deliberately absent from durable identity.
        pub route: Vec<RouteHop>,
        pub project_policy: ProjectPolicyFingerprint,
    }
    pub struct BuildVerification {
        pub build: BuildIdentity,
        pub platform: String,
        pub test_suite: String,
        pub report_sha256: String,
    }
    pub struct PairVerification {
        pub schema_version: u16,
        pub identity: PairIdentity,
        pub consumer_verification: BuildVerification,
        pub provider_verification: BuildVerification,
        pub test_profile: String,
        pub report_sha256: String,
        pub checks: Vec<String>,
    }
    pub struct ContractReadiness {
        pub supported: bool,
        pub build_verified: bool,
        pub pair_tested: bool,
        pub enabled: bool,
        pub reasons: Vec<ReadinessReason>,
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteRole {
    Adapter,
    Controller,
    Worker,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteTransport {
    McpIpcWebsocket,
    CliIpcWebsocket,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessReason {
    UnsupportedContract,
    MissingIdentity,
    InvalidIdentity,
    MissingEvidence,
    InvalidEvidence,
    IdentityChanged,
    SchemaChanged,
    BuildUnverified,
    PairUntested,
    RouteNotFresh,
    PolicyDenied,
}

/// Known readers never infer policy from recipe names or an unknown metadata version.
pub fn interpret_project_policies(info: &WorkerInfo) -> Option<ProjectPolicyCatalog> {
    let catalog: ProjectPolicyCatalog =
        serde_json::from_value(info.project_policies.clone()?).ok()?;
    if catalog.schema_version != 1
        || Some(&catalog.provider) != info.build.as_ref()
        || catalog.provider.build_id.is_empty()
        || catalog.projects.len() != info.projects.len()
    {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    if catalog.projects.iter().any(|p| {
        !is_sha256(&p.sha256)
            || !ids.insert(&p.project_id)
            || !info.projects.iter().any(|i| i.id == p.project_id)
    }) {
        return None;
    }
    Some(catalog)
}

pub fn interpret_contract_schemas(info: &WorkerInfo) -> Option<ContractSchemaCatalog> {
    let schemas: ContractSchemaCatalog =
        serde_json::from_value(info.contract_schemas.clone()?).ok()?;
    let crate::CatalogObservation::Known(catalog) = crate::interpret_catalog(info) else {
        return None;
    };
    let mut seen = std::collections::HashSet::new();
    if schemas.schema_version != 1
        || Some(&schemas.provider) != info.build.as_ref()
        || schemas.schemas.is_empty()
        || schemas.schemas.len() > 64
        || schemas.schemas.iter().any(|item| {
            !is_sha256(&item.sha256)
                || !seen.insert(&item.contract)
                || !catalog.supports(&item.contract.id, item.contract.version)
        })
    {
        return None;
    }
    Some(schemas)
}

/// The digest includes both directions of the actual DTO schema, not a version string.
#[cfg(feature = "jsonschema")]
pub fn fenced_rpc_schema_sha256() -> String {
    static DIGEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIGEST
        .get_or_init(|| {
            let bytes = canonical_evidence_json(&serde_json::json!({
                "requestContractVersion": crate::FENCED_REQUEST_CONTRACT,
                "request": schemars::schema_for!(crate::FencedRequest),
                "response": schemars::schema_for!(crate::FencedResponse)
            }))
            .expect("DTO schemas are JSON serializable");
            evidence_sha256(&bytes)
        })
        .clone()
}

#[cfg(feature = "jsonschema")]
pub fn implemented_contract_schemas(provider: BuildIdentity) -> ContractSchemaCatalog {
    ContractSchemaCatalog {
        schema_version: 1,
        provider,
        schemas: vec![
            ContractSchemaFingerprint {
                contract: ContractRef {
                    id: crate::WORKER_RPC_CONTRACT.into(),
                    version: crate::FENCED_REQUEST_CONTRACT,
                },
                sha256: fenced_rpc_schema_sha256(),
            },
            ContractSchemaFingerprint {
                contract: crate::operation_contract(),
                sha256: crate::operation_schema_sha256(),
            },
        ],
    }
}
