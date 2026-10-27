//! Readonly network observations, separate from task execution and frozen Info.
use crate::*;
use serde::{Deserialize, Serialize};

pub const CONTROLLER_CONNECTIONS_CONTRACT: &str = "winbridge.controller-connections";
/// Maximum configured controller endpoints; shared by config and observation validation.
pub const MAX_CONTROLLERS: usize = 4;
/// Aggregate network handler capacity, independent of the number of controllers.
pub const NETWORK_RPC_LIMIT: u16 = 32;
pub const CONTROLLER_RPC_LIMIT: u16 = 16;
pub fn controller_connections_contract() -> ContractRef {
    ContractRef {
        id: CONTROLLER_CONNECTIONS_CONTRACT.into(),
        version: 1,
    }
}
macro_rules! dto { ($($item:item)*) => {$ (
    #[cfg_attr(feature="jsonschema", derive(schemars::JsonSchema))]
    #[cfg_attr(feature="typescript", derive(ts_rs::TS))]
    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(rename_all="camelCase", deny_unknown_fields)]
    $item
)*}; }
dto! {
    pub struct ControllerConnectionsQuery {}
    pub struct ControllerConnectionsRequest { pub context: RequestContext }
    /// Decimal strings retain exact lifetime counters across JSON/JavaScript.
    pub struct NetworkRpcObservation {
        pub in_flight: u16, pub limit: u16,
        pub admitted: String, pub completed: String, pub rejected: String,
    }
    pub struct ControllerLinkObservation {
        pub id: String, pub connection: ControllerConnection, pub attempts: String,
        pub last_connected_at_ms: Option<u64>, pub last_disconnected_at_ms: Option<u64>,
        pub last_error: Option<String>, pub rpc: NetworkRpcObservation,
    }
    pub struct ControllerConnections {
        pub schema_version: u16, pub context: RequestContext, pub provider: BuildIdentity,
        pub worker_generation: String, pub observed_at_ms: u64, pub primary: Option<String>,
        pub connections: Vec<ControllerLinkObservation>, pub rpc: NetworkRpcObservation,
    }
}

impl NetworkRpcObservation {
    fn counters(&self) -> Option<(u64, u64, u64)> {
        fn decimal(s: &str) -> Option<u64> {
            if s.is_empty()
                || (s.len() > 1 && s.starts_with('0'))
                || !s.bytes().all(|c| c.is_ascii_digit())
            {
                return None;
            }
            s.parse().ok()
        }
        Some((
            decimal(&self.admitted)?,
            decimal(&self.completed)?,
            decimal(&self.rejected)?,
        ))
    }
    fn valid(&self, limit: u16) -> bool {
        self.limit == limit
            && self.in_flight <= limit
            && self.counters().is_some_and(|(a, c, _)| {
                c <= a && (a == u64::MAX || a - c == u64::from(self.in_flight))
            })
    }
}

#[cfg(feature = "jsonschema")]
pub fn controller_connections_schema_sha256() -> String {
    use std::sync::OnceLock;
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST.get_or_init(||evidence_sha256(&canonical_evidence_json(&serde_json::json!({
        "contract":controller_connections_contract(),"query":schemars::schema_for!(ControllerConnectionsQuery),
        "observation":schemars::schema_for!(ControllerConnections)
    })).expect("Controller connections DTO schema"))).clone()
}

/// Negotiate before transmission. Never silently fall back to a mutation route.
#[cfg(feature = "jsonschema")]
pub fn controller_connections_envelope(
    info: &WorkerInfo,
    request: &ControllerConnectionsRequest,
) -> Result<ExtensionRequest, String> {
    if !matches!(interpret_catalog(info),CatalogObservation::Known(c) if c.supports(EXTENSION_TRANSPORT,1)&&c.supports(CONTROLLER_CONNECTIONS_CONTRACT,1))
        || !interpret_contract_schemas(info).is_some_and(|s| {
            s.schemas.iter().any(|s| {
                s.contract == controller_connections_contract()
                    && s.sha256 == controller_connections_schema_sha256()
            })
        })
    {
        return Err(
            "Worker does not support controller connections contract/schema; no fallback".into(),
        );
    }
    if !info
        .fenced_requests
        .as_ref()
        .is_some_and(|f| f.context == request.context)
    {
        return Err("Controller connections scope changed; refresh provider explicitly".into());
    }
    Ok(ExtensionRequest {
        extension_version: 1,
        contract: controller_connections_contract(),
        context: request.context.clone(),
        payload: serde_json::json!({}),
    })
}

pub fn decode_controller_connections(
    response: ExtensionResponse,
    info: &WorkerInfo,
    request: &ControllerConnectionsRequest,
) -> Result<ControllerConnections, String> {
    response.validate_for(&ExtensionRequest {
        extension_version: 1,
        contract: controller_connections_contract(),
        context: request.context.clone(),
        payload: serde_json::json!({}),
    })?;
    let page: ControllerConnections = match response.result {
        ExtensionResult::Error { code, message } => {
            return Err(format!("Connection observation {code:?}: {message}"))
        }
        ExtensionResult::Data { payload } => serde_json::from_value(payload)
            .map_err(|_| "Unsupported connection observation format")?,
    };
    let valid_id = |s: &str| {
        !s.is_empty()
            && s.len() <= 80
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    };
    let bounded = |s: &Option<String>| s.as_ref().is_none_or(|s| s.len() <= 4096);
    let valid_url = |url: &str| {
        let Some(address) = url
            .strip_prefix("ws://")
            .and_then(|s| s.strip_suffix('/').or(Some(s)))
            .and_then(|s| s.parse::<std::net::SocketAddr>().ok())
        else {
            return false;
        };
        matches!(address.ip(),std::net::IpAddr::V4(ip) if ip.is_private()||ip.is_loopback())
            && address.port() != 0
    };
    let time = |time: u64| time <= 9_007_199_254_740_991;
    let mut ids = std::collections::BTreeSet::new();
    if page.schema_version != 1
        || page.context != request.context
        || page.worker_generation != info.generation
        || info.build.as_ref() != Some(&page.provider)
        || page.connections.len() > MAX_CONTROLLERS
        || !page.rpc.valid(NETWORK_RPC_LIMIT)
        || !time(page.observed_at_ms)
        || page.connections.iter().any(|l| {
            let c = &l.connection;
            !valid_id(&l.id)
                || !ids.insert(l.id.clone())
                || !l.rpc.valid(CONTROLLER_RPC_LIMIT)
                || l.attempts.parse::<u64>().is_err()
                || l.attempts.is_empty()
                || (l.attempts.len() > 1 && l.attempts.starts_with('0'))
                || !l.attempts.bytes().all(|b| b.is_ascii_digit())
                || c.url.len() > 128
                || !valid_url(&c.url)
                || [
                    l.last_connected_at_ms,
                    l.last_disconnected_at_ms,
                    c.last_confirmed_at_ms,
                ]
                .into_iter()
                .flatten()
                .any(|t| !time(t))
                || !bounded(&c.error)
                || !bounded(&l.last_error)
                || [
                    c.session_id.as_deref(),
                    c.controller_generation.as_deref(),
                    c.controller_node_id.as_deref(),
                ]
                .iter()
                .flatten()
                .any(|s| !valid_id(s))
                || (matches!(c.state, ConnectionState::Connected)
                    && (c.session_id.is_none()
                        || c.controller_generation.is_none()
                        || c.controller_node_id.is_none()
                        || c.last_confirmed_at_ms.is_none()
                        || c.error.is_some()
                        || l.last_connected_at_ms.is_none()
                        || l.attempts == "0"))
                || (!matches!(c.state, ConnectionState::Connected)
                    && (c.session_id.is_some()
                        || c.controller_generation.is_some()
                        || c.controller_node_id.is_some()))
        })
        || match &page.primary {
            Some(p) => !ids.contains(p),
            None => !page.connections.is_empty(),
        }
        || page
            .connections
            .iter()
            .map(|l| u32::from(l.rpc.in_flight))
            .sum::<u32>()
            != u32::from(page.rpc.in_flight)
        || [0, 1, 2].iter().any(|index| {
            let value = |c: (u64, u64, u64)| match index {
                0 => c.0,
                1 => c.1,
                _ => c.2,
            };
            let total = page.rpc.counters().map(value);
            let sum = page
                .connections
                .iter()
                .try_fold(0u64, |sum, l| sum.checked_add(value(l.rpc.counters()?)));
            total != sum && total != Some(u64::MAX)
        })
    {
        return Err("Invalid/stale controller connection observation; no scope rebind".into());
    }
    Ok(page)
}

#[cfg(all(test, feature = "jsonschema"))]
mod tests {
    use super::*;
    fn fixture() -> (WorkerInfo, ControllerConnectionsRequest, ExtensionResponse) {
        let mut info: WorkerInfo = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../../../tests/fixtures/legacy-v1/worker-info.json"
            ))
            .unwrap()["info"]
                .clone(),
        )
        .unwrap();
        info.build = Some(build_identity());
        info.generation = "worker-generation".into();
        let context = RequestContext {
            instance_id: info.instance_id.clone(),
            journal_incarnation: "journal".into(),
        };
        info.fenced_requests = Some(FencedRequestInfo {
            request_contract_version: 2,
            context: context.clone(),
            legacy_mutations_allowed: false,
        });
        info.contract_catalog =
            Some(serde_json::to_value(ContractCatalog::implemented(build_identity())).unwrap());
        info.contract_schemas =
            Some(serde_json::to_value(implemented_contract_schemas(build_identity())).unwrap());
        let rpc = |limit| NetworkRpcObservation {
            in_flight: 1,
            limit,
            admitted: "9007199254740993".into(),
            completed: "9007199254740992".into(),
            rejected: "0".into(),
        };
        let page = ControllerConnections {
            schema_version: 1,
            context: context.clone(),
            provider: build_identity(),
            worker_generation: info.generation.clone(),
            observed_at_ms: 1,
            primary: Some("a".into()),
            rpc: rpc(NETWORK_RPC_LIMIT),
            connections: vec![ControllerLinkObservation {
                id: "a".into(),
                attempts: "1".into(),
                last_connected_at_ms: Some(1),
                last_disconnected_at_ms: None,
                last_error: None,
                rpc: rpc(CONTROLLER_RPC_LIMIT),
                connection: ControllerConnection {
                    url: "ws://127.0.0.1:1143".into(),
                    state: ConnectionState::Connected,
                    session_id: Some("session".into()),
                    controller_generation: Some("controller".into()),
                    controller_node_id: Some("node".into()),
                    last_confirmed_at_ms: Some(1),
                    error: None,
                },
            }],
        };
        let response = ExtensionResponse {
            extension_version: 1,
            contract: controller_connections_contract(),
            context: Some(context.clone()),
            result: ExtensionResult::Data {
                payload: serde_json::to_value(page).unwrap(),
            },
        };
        (info, ControllerConnectionsRequest { context }, response)
    }
    #[test]
    fn observation_accepts_all_supported_counts_and_last_primary_but_refuses_overflow() {
        let (info, request, response) = fixture();
        let ExtensionResult::Data { payload } = &response.result else {
            panic!("data")
        };
        let template: ControllerConnections = serde_json::from_value(payload.clone()).unwrap();
        for count in 1..=MAX_CONTROLLERS + 1 {
            let mut page = template.clone();
            page.rpc.in_flight = count as u16;
            page.rpc.admitted = count.to_string();
            page.rpc.completed = "0".into();
            page.primary = Some(format!("peer-{}", count - 1));
            page.connections = (0..count)
                .map(|i| {
                    let mut link = template.connections[0].clone();
                    link.id = format!("peer-{i}");
                    link.connection.url = format!("ws://127.0.0.1:{}", 1143 + i);
                    link.rpc.admitted = "1".into();
                    link.rpc.completed = "0".into();
                    link
                })
                .collect();
            let mut response = response.clone();
            response.result = ExtensionResult::Data {
                payload: serde_json::to_value(page).unwrap(),
            };
            assert_eq!(
                decode_controller_connections(response, &info, &request).is_ok(),
                count <= MAX_CONTROLLERS
            );
        }
    }

    #[test]
    fn scoped_snapshot_keeps_exact_large_counters() {
        let (info, request, response) = fixture();
        assert_eq!(
            controller_connections_envelope(&info, &request)
                .unwrap()
                .payload,
            serde_json::json!({})
        );
        let page = decode_controller_connections(response, &info, &request).unwrap();
        assert_eq!(page.rpc.admitted, "9007199254740993");
        assert_eq!(page.rpc.in_flight, 1);
    }
    #[test]
    fn decoder_refuses_stale_malformed_and_inconsistent_observations() {
        let (info, request, response) = fixture();
        for case in 0..21 {
            let mut r = response.clone();
            let ExtensionResult::Data { payload: p } = &mut r.result else {
                unreachable!()
            };
            match case {
                0 => p["schemaVersion"] = 2.into(),
                1 => p["context"]["journalIncarnation"] = "foreign".into(),
                2 => p["workerGeneration"] = "restarted".into(),
                3 => p["provider"]["buildId"] = "foreign".into(),
                4 => {
                    let duplicate = p["connections"][0].clone();
                    p["connections"].as_array_mut().unwrap().push(duplicate)
                }
                5 => p["rpc"]["inFlight"] = 0.into(),
                6 => p["connections"][0]["rpc"]["limit"] = 32.into(),
                7 => p["rpc"]["admitted"] = "01".into(),
                8 => p["rpc"]["rejected"] = "-1".into(),
                9 => p["rpc"]["completed"] = "9007199254740994".into(),
                10 => p["primary"] = "missing".into(),
                11 => p["connections"][0]["connection"]["sessionId"] = serde_json::Value::Null,
                12 => p["connections"][0]["connection"]["state"] = "disconnected".into(),
                13 => p["connections"][0]["connection"]["url"] = "ws://8.8.8.8:1143".into(),
                14 => p["connections"][0]["lastError"] = "a".repeat(4097).into(),
                15 => p["connections"][0]["attempts"] = "01".into(),
                16 => p["observedAtMs"] = 9_007_199_254_740_992u64.into(),
                17 => p["command"] = "submit".into(),
                18 => r.context.as_mut().unwrap().journal_incarnation = "foreign".into(),
                19 => r.contract.version = 2,
                _ => p["connections"][0]["rpc"]["rejected"] = "1".into(),
            }
            assert!(
                decode_controller_connections(r, &info, &request).is_err(),
                "case {case}"
            );
        }
    }
    #[test]
    fn negotiation_rejects_old_worker_unknown_schema_and_changed_scope() {
        let (info, request, _) = fixture();
        let mut old = info.clone();
        old.contract_catalog = None;
        assert!(controller_connections_envelope(&old, &request).is_err());
        let mut changed = info.clone();
        let schemas = changed.contract_schemas.as_mut().unwrap()["schemas"]
            .as_array_mut()
            .unwrap();
        schemas
            .iter_mut()
            .find(|s| s["contract"]["id"] == CONTROLLER_CONNECTIONS_CONTRACT)
            .unwrap()["sha256"] = "0".repeat(64).into();
        assert!(controller_connections_envelope(&changed, &request).is_err());
        let mut foreign = request;
        foreign.context.journal_incarnation = "foreign".into();
        assert!(controller_connections_envelope(&info, &foreign).is_err());
    }
}
