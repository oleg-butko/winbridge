//! Advertisement is separate from build evidence, pair evidence and permission.
use crate::{BuildIdentity, WorkerInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CATALOG_SCHEMA: u16 = 1;
pub const WORKER_RPC_CONTRACT: &str = "winbridge.worker-rpc";

// Preserve explicit JSON null too. Option<Value>'s ordinary decoder turns it
// into missing metadata, silently changing a forwarded observation.
pub(crate) fn deserialize_catalog<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error> {
    serde_json::Value::deserialize(deserializer).map(Some)
}

// JSON/TypeScript treat 1 and 1.0 as the same integer. Keep the catalog's known
// reader consistent across languages; this does not relax command DTO parsing.
pub(crate) fn deserialize_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u16, D::Error> {
    let number = serde_json::Number::deserialize(deserializer)?;
    let value = number
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n >= 0.0 && *n <= u16::MAX as f64)
        .ok_or_else(|| serde::de::Error::custom("catalog version must be a bounded integer"))?;
    Ok(value as u16)
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractRef {
    pub id: String,
    #[serde(deserialize_with = "deserialize_version")]
    pub version: u16,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractCatalog {
    #[serde(deserialize_with = "deserialize_version")]
    pub schema_version: u16,
    pub provider: BuildIdentity,
    pub supported: Vec<ContractRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_catalog"
    )]
    #[cfg_attr(feature = "typescript", ts(optional, type = "unknown"))]
    pub controller_projection: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_catalog"
    )]
    #[cfg_attr(feature = "typescript", ts(optional, type = "unknown"))]
    pub work_scheduling: Option<serde_json::Value>,
}
impl ContractCatalog {
    pub fn implemented(provider: BuildIdentity) -> Self {
        Self {
            schema_version: CATALOG_SCHEMA,
            provider,
            controller_projection: None,
            work_scheduling: None,
            supported: vec![
                ContractRef {
                    id: crate::EXTENSION_TRANSPORT.into(),
                    version: 1,
                },
                crate::operation_contract(),
                ContractRef {
                    id: crate::WORK_SCHEDULING_CONTRACT.into(),
                    version: 1,
                },
                ContractRef {
                    id: WORKER_RPC_CONTRACT.into(),
                    version: 1,
                },
                ContractRef {
                    id: WORKER_RPC_CONTRACT.into(),
                    version: crate::FENCED_REQUEST_CONTRACT,
                },
                ContractRef {
                    id: crate::CONTROLLER_PROJECTION_CONTRACT.into(),
                    version: 1,
                },
            ],
        }
    }
    pub fn supports(&self, id: &str, version: u16) -> bool {
        self.supported
            .iter()
            .any(|item| item.id == id && item.version == version)
    }
}

#[derive(Debug)]
pub enum CatalogObservation {
    Missing,
    Known(ContractCatalog),
    Unsupported(String),
}

/// Never use a catalog to grant execution or to rebind a pending request.
pub fn interpret_catalog(info: &WorkerInfo) -> CatalogObservation {
    let Some(raw) = &info.contract_catalog else {
        return CatalogObservation::Missing;
    };
    let Ok(catalog) = serde_json::from_value::<ContractCatalog>(raw.clone()) else {
        return CatalogObservation::Unsupported(
            "Unknown or malformed contract catalog; keep legacy discovery".into(),
        );
    };
    if catalog.schema_version != CATALOG_SCHEMA {
        return CatalogObservation::Unsupported(
            "Unsupported catalog schema; no new action enabled".into(),
        );
    }
    if info.build.as_ref() != Some(&catalog.provider) || catalog.provider.build_id.is_empty() {
        return CatalogObservation::Unsupported(
            "Catalog provider differs from worker build; no support inferred".into(),
        );
    }
    let mut seen = HashSet::new();
    if catalog.supported.len() > 64
        || catalog.supported.iter().any(|entry| {
            entry.version == 0
                || entry.id.is_empty()
                || entry.id.len() > 128
                || !entry
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
                || !seen.insert(entry.clone())
        })
    {
        return CatalogObservation::Unsupported(
            "Invalid, duplicate or oversized catalog entries; no support inferred".into(),
        );
    }
    CatalogObservation::Known(catalog)
}
