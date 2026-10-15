//! Controller-local contracts are separate from frozen worker RPC 1/2.
use crate::{BuildIdentity, ContractReadiness, ContractRef, PairIdentity};
use serde::{Deserialize, Serialize};

pub const CONTROLLER_API_CONTRACT: u16 = 1;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerApiRequest {
    pub controller_contract_version: u16,
    pub adapter: BuildIdentity,
    pub adapter_kind: ControllerAdapter,
    pub request: ControllerOperation,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerAdapter {
    Mcp,
    Cli,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerOperation {
    OperationPage {
        request: Box<crate::OperationPageRequest>,
    },
    ContractReadiness {
        query: ContractReadinessQuery,
    },
    VerifyPair {
        request: Box<VerifyPairRequest>,
    },
    GetVerification {
        #[serde(rename = "verificationKey")]
        verification_key: String,
    },
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyPairRequest {
    pub verification_key: String,
    /// Copy identity from the readiness observation when creating the operation;
    /// never replace it on retry or after reconnect/recovery.
    pub identity: PairIdentity,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Indeterminate,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationView {
    pub verification_key: String,
    pub identity: PairIdentity,
    pub status: VerificationStatus,
    pub stage: String,
    pub task_ids: Vec<String>,
    pub receipt_id: Option<String>,
    pub detail: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractReadinessQuery {
    pub project_id: String,
    pub contract: ContractRef,
    #[serde(default)]
    pub receipt_id: Option<String>,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractReadinessView {
    pub project_id: String,
    pub contract: ContractRef,
    pub identity: Option<PairIdentity>,
    pub readiness: ContractReadiness,
    pub receipt_id: Option<String>,
    pub test_profile: String,
    pub controller_generation: String,
    pub session_id: Option<String>,
    pub detail: String,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerApiResponse {
    pub controller_contract_version: u16,
    pub result: ControllerApiResult,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerApiResult {
    OperationPage { page: Box<crate::OperationPage> },
    ContractReadiness { view: Box<ContractReadinessView> },
    Verification { view: Box<VerificationView> },
    Error { message: String },
}
