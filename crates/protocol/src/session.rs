use crate::{Capability, Component, LocalRequest, LocalResponse};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeerIdentity {
    pub protocol_version: u16,
    pub application_version: String,
    pub component: Component,
    pub instance_id: String,
    pub node_id: String,
    pub generation: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionMessage {
    Hello {
        peer: PeerIdentity,
    },
    Welcome {
        peer: PeerIdentity,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Rejected {
        message: String,
    },
    Request {
        id: String,
        request: LocalRequest,
    },
    Response {
        id: String,
        response: LocalResponse,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerConnection {
    pub url: String,
    pub state: ConnectionState,
    pub session_id: Option<String>,
    pub controller_node_id: Option<String>,
    pub controller_generation: Option<String>,
    pub last_confirmed_at_ms: Option<u64>,
    pub error: Option<String>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerStatus {
    pub instance_id: String,
    pub node_id: String,
    pub generation: String,
    pub listen_address: String,
    pub connected: bool,
    pub session_id: Option<String>,
    pub worker_node_id: Option<String>,
    pub worker_generation: Option<String>,
    pub worker_address: Option<String>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub activity_log: Option<ActivityLogStatus>,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogStatus {
    pub path: String,
    pub max_file_bytes: u64,
    pub retained_archives: u32,
    pub dropped_records: u64,
    pub last_error: Option<String>,
}
