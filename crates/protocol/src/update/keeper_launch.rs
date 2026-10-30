//! Frozen local delegation envelope. No journal Root, executable path or argv.
use super::{KeeperBinding, SupervisorProcess};
use serde::{Deserialize, Serialize};
pub const KEEPER_LAUNCH_CONTRACT: u16 = 1;
pub const KEEPER_LAUNCH_STARTUP_MS: u64 = 10_000;
pub const KEEPER_LAUNCH_MARKER: &[u8] = b"WINBRIDGE_KEEPER_LAUNCH_V1";
macro_rules! dto {($($i:item)*)=>{$(
 #[cfg_attr(feature="jsonschema",derive(schemars::JsonSchema))]
 #[cfg_attr(feature="typescript",derive(ts_rs::TS))]
 #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
 #[serde(rename_all="camelCase",deny_unknown_fields)] $i
)*};}
dto! {
 pub struct KeeperLaunchFile {
    pub sha256:String,
    pub bytes:u64,
    pub file_identity:String,
 }
 pub struct KeeperLaunchRequest {
    pub keeper_launch_version:u16,
    pub request_id:String,
    pub binding:KeeperBinding,
    pub command:KeeperLaunchCommand,
 }
 pub struct KeeperLaunchResponse {
    pub keeper_launch_version:u16,
    pub request_id:Option<String>,
    pub result:KeeperLaunchResult,
 }
 pub struct KeeperLaunchStatus {
    pub launch_id:String,
    pub ticket_sha256:String,
    pub phase:KeeperLaunchPhase,
    pub process:Option<SupervisorProcess>,
    pub expires_at_ms:u64,
    pub reason:Option<String>,
 }
}
/// Private channel material: Debug deliberately never reveals the token.
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeeperLaunchTicket {
    pub update_id: String,
    pub launch_id: String,
    pub image: KeeperLaunchFile,
    pub config: KeeperLaunchFile,
    pub stdout_identity: String,
    pub stderr_identity: String,
    pub token: String,
    pub excluded_job: String,
    pub predecessor: Option<SupervisorProcess>,
    pub prepared_at_ms: u64,
}
impl std::fmt::Debug for KeeperLaunchTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeeperLaunchTicket")
            .field("update_id", &self.update_id)
            .field("launch_id", &self.launch_id)
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum KeeperLaunchCommand {
    Start {
        ticket: Box<KeeperLaunchTicket>,
    },
    Observe {
        launch_id: String,
        ticket_sha256: String,
    },
    ConfirmReady {
        launch_id: String,
        ticket_sha256: String,
        process: SupervisorProcess,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeeperLaunchPhase {
    Preparing,
    Created,
    ReadyConfirmed,
    Failed,
    Indeterminate,
    Exited,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeeperLaunchResult {
    Status { status: Box<KeeperLaunchStatus> },
    Rejected { message: String },
}
