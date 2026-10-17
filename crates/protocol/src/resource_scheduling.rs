//! Declared shared resources. This observation does not dispatch commands.
use crate::*;
use serde::{Deserialize, Serialize};
pub const RESOURCE_SCHEDULING_CONTRACT: &str = "winbridge.resource-scheduling";
macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all="camelCase", deny_unknown_fields)]
    $item
)*}; }
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Build,
    Cache,
    Binary,
    Ipc,
    Port,
    Interactive,
}
impl ResourceKind {
    pub fn process_lifetime(self) -> bool {
        !matches!(self, Self::Build | Self::Cache)
    }
}
dto! {
    pub struct ResourceDefinition { pub id: String, pub kind: ResourceKind, pub capacity: u16 }
    pub struct ResourceRequirement { pub resource_id: String, pub units: u16 }
    pub struct ProjectResourceBindings {
        pub project_id: String,
        #[serde(default)] pub task: Vec<ResourceRequirement>,
        #[serde(default)] pub import: Vec<ResourceRequirement>,
        #[serde(default)] pub build: Vec<ResourceRequirement>,
        #[serde(default)] pub start: Vec<ResourceRequirement>,
    }
    pub struct ResourceSchedulingPolicy { pub resources: Vec<ResourceDefinition>, pub projects: Vec<ProjectResourceBindings> }
    pub struct ResourceCapacityObservation {
        pub id: String, pub kind: ResourceKind, #[serde(default,deserialize_with="read_capacity")] pub capacity: Option<u16>,
        #[serde(deserialize_with="crate::work_scheduling::read_count")] pub used_units: u32,
        #[serde(deserialize_with="crate::work_scheduling::read_count")] pub uncertain_units: u32,
    }
    pub struct ResourceOwnerObservation {
        pub resource_id: String, pub lease_id: String, pub project_id: String,
        pub operation_kind: HistoryKind, pub operation_id: String, #[serde(deserialize_with="read_units")] pub units: u16,
        pub state: ObservedLeaseState, pub application_lifetime: bool,
    }
    pub struct ResourceSchedulingObservation {
        #[serde(deserialize_with="crate::discovery::deserialize_version")] pub schema_version: u16,
        pub provider: BuildIdentity, pub context: RequestContext, pub policy_generation: String,
        pub enabled: bool, pub resources: Vec<ResourceCapacityObservation>, pub owners: Vec<ResourceOwnerObservation>, pub truncated: bool,
        #[serde(deserialize_with="crate::work_scheduling::read_time")] pub observed_at_ms: u64,
        pub read_only: bool,
    }
}
fn read_units<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    u16::try_from(crate::work_scheduling::read_count(d)?).map_err(serde::de::Error::custom)
}
fn read_capacity<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    value
        .map(|v| {
            serde_json::from_value::<ResourceNumber>(serde_json::json!({"value":v}))
                .map(|v| v.value)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
}
#[derive(Deserialize)]
struct ResourceNumber {
    #[serde(deserialize_with = "read_units")]
    value: u16,
}
pub fn interpret_resource_scheduling(info: &WorkerInfo) -> Option<ResourceSchedulingObservation> {
    let CatalogObservation::Known(catalog) = interpret_catalog(info) else {
        return None;
    };
    if !catalog.supports(RESOURCE_SCHEDULING_CONTRACT, 1) {
        return None;
    }
    let v: ResourceSchedulingObservation =
        serde_json::from_value(catalog.resource_scheduling?).ok()?;
    if v.schema_version != 1
        || !valid_queue_revision(&v.policy_generation)
        || info.build.as_ref() != Some(&v.provider)
        || !info
            .fenced_requests
            .as_ref()
            .is_some_and(|f| f.context == v.context)
        || v.resources.len() > 128
        || v.owners.len() > 256
    {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    for r in &v.resources {
        if r.id.is_empty()
            || !ids.insert(&r.id)
            || r.uncertain_units > r.used_units
            || r.capacity
                .is_some_and(|n| n == 0 || n > 64 || (r.kind != ResourceKind::Build && n != 1))
        {
            return None;
        }
    }
    let mut owners = std::collections::HashSet::new();
    let mut totals = std::collections::HashMap::<&str, (u64, u64)>::new();
    for o in &v.owners {
        if o.resource_id.is_empty()
            || o.lease_id.is_empty()
            || o.project_id.is_empty()
            || o.operation_id.is_empty()
            || o.units == 0
            || o.units > 64
            || !matches!(o.operation_kind, HistoryKind::Task | HistoryKind::Import)
            || !matches!(
                o.state,
                ObservedLeaseState::Held | ObservedLeaseState::Uncertain
            )
            || !owners.insert((&o.lease_id, &o.resource_id))
            || (!v.truncated && !ids.contains(&o.resource_id))
        {
            return None;
        }
        if o.application_lifetime
            && v.resources
                .iter()
                .any(|r| r.id == o.resource_id && !r.kind.process_lifetime())
        {
            return None;
        }
        let counts = totals.entry(&o.resource_id).or_default();
        counts.0 += u64::from(o.units);
        if o.state == ObservedLeaseState::Uncertain {
            counts.1 += u64::from(o.units)
        }
    }
    for r in &v.resources {
        let (used, uncertain) = totals.get(r.id.as_str()).copied().unwrap_or_default();
        if (!v.enabled && r.capacity.is_some())
            || used > u64::from(r.used_units)
            || uncertain > u64::from(r.uncertain_units)
            || (!v.truncated
                && (used != u64::from(r.used_units) || uncertain != u64::from(r.uncertain_units)))
        {
            return None;
        }
    }
    Some(v)
}
#[cfg(feature = "jsonschema")]
pub fn resource_scheduling_schema_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":ContractRef {id:RESOURCE_SCHEDULING_CONTRACT.into(),version:1},"observation":schemars::schema_for!(ResourceSchedulingObservation)})).expect("resource schema"))).clone()
}
