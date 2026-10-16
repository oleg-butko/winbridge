//! Readonly worker scheduling observation, carried in opaque catalog metadata.
use crate::*;
use serde::{Deserialize, Serialize};
pub const WORK_SCHEDULING_CONTRACT: &str = "winbridge.work-scheduling";

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkSchedulingPolicy {
    pub max_concurrent_work: u16,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkSchedulingObservation {
    #[serde(deserialize_with = "crate::discovery::deserialize_version")]
    pub schema_version: u16,
    pub provider: BuildIdentity,
    pub context: RequestContext,
    #[serde(default, deserialize_with = "read_limit")]
    pub max_concurrent_work: Option<u16>,
    #[serde(deserialize_with = "read_count")]
    pub occupied_slots: u32,
    #[serde(deserialize_with = "read_count")]
    pub uncertain_slots: u32,
    #[serde(deserialize_with = "read_count")]
    pub queued_operations: u32,
    #[serde(deserialize_with = "read_time")]
    pub observed_at_ms: u64,
    pub read_only: bool,
}
fn integer<E: serde::de::Error>(number: serde_json::Number, max: u64) -> Result<u64, E> {
    number
        .as_f64()
        .filter(|v| v.is_finite() && v.fract() == 0.0 && *v >= 0.0 && *v <= max as f64)
        .map(|v| v as u64)
        .ok_or_else(|| E::custom("Invalid readonly metadata integer"))
}
pub(crate) fn read_count<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    integer(serde_json::Number::deserialize(d)?, u32::MAX as u64).map(|v| v as u32)
}
pub(crate) fn read_time<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    integer(serde_json::Number::deserialize(d)?, 9_007_199_254_740_991)
}
fn read_limit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    Option::<serde_json::Number>::deserialize(d)?
        .map(|n| integer(n, u16::MAX as u64).map(|v| v as u16))
        .transpose()
}
pub fn interpret_work_scheduling(info: &WorkerInfo) -> Option<WorkSchedulingObservation> {
    let CatalogObservation::Known(catalog) = interpret_catalog(info) else {
        return None;
    };
    if !catalog.supports(WORK_SCHEDULING_CONTRACT, 1) {
        return None;
    }
    let view: WorkSchedulingObservation = serde_json::from_value(catalog.work_scheduling?).ok()?;
    if view.schema_version != 1
        || info.build.as_ref() != Some(&view.provider)
        || !info
            .fenced_requests
            .as_ref()
            .is_some_and(|f| f.context == view.context)
        || view
            .max_concurrent_work
            .is_some_and(|v| !(1..=32).contains(&v))
        || view.uncertain_slots > view.occupied_slots
        || view.observed_at_ms > 9_007_199_254_740_991
    {
        return None;
    }
    Some(view)
}
#[cfg(feature = "jsonschema")]
pub fn work_scheduling_schema_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({"contract":ContractRef {id:WORK_SCHEDULING_CONTRACT.into(),version:1},"observation":schemars::schema_for!(WorkSchedulingObservation)})).expect("scheduling schema"))).clone()
}
