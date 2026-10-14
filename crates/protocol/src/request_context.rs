//! Versioned client scope; connection generation never upgrades a logical retry.
use crate::{LocalRequest, LocalResponse, SessionMessage};
use serde::{Deserialize, Serialize};

pub const FENCED_REQUEST_CONTRACT: u16 = 2;

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestContext {
    pub instance_id: String,
    pub journal_incarnation: String,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FencedRequest {
    pub request_contract_version: u16,
    pub context: RequestContext,
    pub request: LocalRequest,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FencedResponse {
    pub request_contract_version: u16,
    pub outcome: FencedReplyOutcome,
    pub context: Option<RequestContext>,
    pub response: LocalResponse,
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FencedReplyOutcome {
    Responded,
    RejectedBeforeDispatch,
    Unconfirmed,
}
impl FencedResponse {
    pub fn rejected(message: String) -> Self {
        Self {
            request_contract_version: FENCED_REQUEST_CONTRACT,
            outcome: FencedReplyOutcome::RejectedBeforeDispatch,
            context: None,
            response: LocalResponse::Error { message },
        }
    }
    pub fn unconfirmed(message: String) -> Self {
        Self {
            outcome: FencedReplyOutcome::Unconfirmed,
            ..Self::rejected(message)
        }
    }
    pub fn validate_for(&self, expected: &RequestContext) -> Result<(), String> {
        if self.request_contract_version != FENCED_REQUEST_CONTRACT {
            return Err(
                "Unsupported fenced response contract; no automatic fallback or retry".into(),
            );
        }
        if !matches!(self.response, LocalResponse::Error { .. })
            && (self.context.as_ref() != Some(expected)
                || self.outcome != FencedReplyOutcome::Responded)
        {
            return Err("Worker response belongs to another journal incarnation; reconcile the original request scope without retrying in the new scope".into());
        }
        Ok(())
    }
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FencedRequestInfo {
    pub request_contract_version: u16,
    pub context: RequestContext,
    pub legacy_mutations_allowed: bool,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryCoverageInfo {
    pub complete: bool,
    pub reason: String,
}

// Keep the closed v1 DTOs unchanged. A v2 frame cannot parse as a v1 mutation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum WireRequest {
    Fenced(Box<FencedRequest>),
    Legacy(LocalRequest),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum WireResponse {
    Fenced(Box<FencedResponse>),
    Legacy(Box<LocalResponse>),
}
impl WireResponse {
    pub fn response(&self) -> &LocalResponse {
        match self {
            Self::Legacy(response) => response,
            Self::Fenced(response) => &response.response,
        }
    }
    pub fn with_error(self, message: String) -> Self {
        match self {
            Self::Legacy(_) => Self::Legacy(Box::new(LocalResponse::Error { message })),
            Self::Fenced(_) => Self::Fenced(Box::new(FencedResponse::unconfirmed(message))),
        }
    }
    pub fn rejected_for(request: &WireRequest, message: String) -> Self {
        match request {
            WireRequest::Fenced(_) => Self::Fenced(Box::new(FencedResponse::rejected(message))),
            WireRequest::Legacy(_) => Self::Legacy(Box::new(LocalResponse::Error { message })),
        }
    }
}
impl WireRequest {
    pub fn request(&self) -> &LocalRequest {
        match self {
            Self::Fenced(request) => &request.request,
            Self::Legacy(request) => request,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FencedSessionMessage {
    Request {
        id: String,
        request: FencedRequest,
    },
    Response {
        id: String,
        response: FencedResponse,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SessionFrame {
    Fenced(FencedSessionMessage),
    Legacy(SessionMessage),
}
impl SessionFrame {
    pub fn request(id: String, request: WireRequest) -> Self {
        match request {
            WireRequest::Legacy(request) => Self::Legacy(SessionMessage::Request { id, request }),
            WireRequest::Fenced(request) => Self::Fenced(FencedSessionMessage::Request {
                id,
                request: *request,
            }),
        }
    }
    pub fn response(id: String, response: WireResponse) -> Self {
        match response {
            WireResponse::Legacy(response) => Self::Legacy(SessionMessage::Response {
                id,
                response: *response,
            }),
            WireResponse::Fenced(response) => Self::Fenced(FencedSessionMessage::Response {
                id,
                response: *response,
            }),
        }
    }
}

impl LocalRequest {
    /// Conservative classification: future methods default to mutation.
    pub fn is_observation(&self) -> bool {
        matches!(
            self,
            Self::Info
                | Self::ControllerStatus
                | Self::GetRuntimeSettings
                | Self::ListTasks
                | Self::GetTask { .. }
                | Self::ReadOutput { .. }
                | Self::History { .. }
                | Self::StorageUsage
                | Self::PreviewOutputCleanup { .. }
                | Self::ListImports
                | Self::GetImport { .. }
                | Self::ListWorkflows
                | Self::GetWorkflow { .. }
                | Self::ListApplications
                | Self::GetApplication { .. }
                | Self::ReadApplicationLog { .. }
                | Self::ListAttention { .. }
                | Self::ListAttentionEvents { .. }
                | Self::ListSnapshots { .. }
                | Self::PreviewSnapshot { .. }
        )
    }
}
