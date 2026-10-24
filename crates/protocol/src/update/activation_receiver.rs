//! Private startup v1 only. No worker RPC registration or state-writer grant.
use serde::{Deserialize, Serialize};
pub const ACTIVATION_RECEIVER_MARKER: &[u8] = b"WINBRIDGE_PRIVATE_ACTIVATION_RECEIVER_V1";
pub const ACTIVATION_RECEIVER_CONTRACT: u16 = 1;

// No Debug: the transient nonce must never appear in diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverChallenge {
    pub contract_version: u16,
    pub update_id: String,
    pub attempt_id: String,
    pub nonce: String,
}
/// Same immutable receipt shape as the original activation ledger18 model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReady {
    pub attempt_id: String,
    pub identity: crate::LoadedWorkerIdentity,
    pub worker_config_sha256: String,
    pub observed_at_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationReceiverReady {
    pub contract_version: u16,
    pub ready: ActivationReady,
}
