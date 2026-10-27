//! Controller-local readonly route; never extends frozen worker/controller RPC.
use crate::*;
use serde::{Deserialize, Serialize};
pub const CONTROLLER_DIAGNOSTICS_VERSION: u16 = 1;
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerDiagnosticsRequest {
    pub controller_diagnostics_version: u16,
    pub request: ControllerDiagnosticQuery,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerDiagnosticsResponse {
    pub controller_diagnostics_version: u16,
    pub result: ControllerDiagnosticResult,
}

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerDiagnosticQuery {
    ControllerConnections {
        request: ControllerConnectionsRequest,
    },
    OperationAttribution {
        request: OperationAttributionRequest,
    },
    OperationCancellation {
        request: OperationAttributionRequest,
    },
}
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerDiagnosticResult {
    ControllerConnections {
        observation: Box<ControllerConnections>,
    },
    OperationAttribution {
        observation: Box<OperationAttribution>,
    },
    OperationCancellation {
        observation: Box<OperationCancellation>,
    },
    Error {
        message: String,
    },
}
impl ControllerDiagnosticQuery {
    pub fn method(&self) -> &'static str {
        match self {
            Self::ControllerConnections { .. } => "controller_connections",
            Self::OperationAttribution { .. } => "operation_attribution",
            Self::OperationCancellation { .. } => "operation_cancellation",
        }
    }
}
impl ControllerDiagnosticsResponse {
    pub fn error(message: String) -> Self {
        Self {
            controller_diagnostics_version: CONTROLLER_DIAGNOSTICS_VERSION,
            result: ControllerDiagnosticResult::Error { message },
        }
    }
    pub fn validate_for(&self, request: &ControllerDiagnosticsRequest) -> Result<(), String> {
        if request.controller_diagnostics_version != CONTROLLER_DIAGNOSTICS_VERSION
            || self.controller_diagnostics_version != CONTROLLER_DIAGNOSTICS_VERSION
        {
            return Err("Unsupported controller diagnostics version; no fallback".into());
        }
        let reference = |r: &OperationRef, q: &OperationAttributionRequest| {
            r.context == q.context
                && r.kind == q.query.kind
                && r.operation_id == q.query.operation_id
        };
        let valid = match (&request.request, &self.result) {
            (_, ControllerDiagnosticResult::Error { .. }) => true,
            (
                ControllerDiagnosticQuery::ControllerConnections { request },
                ControllerDiagnosticResult::ControllerConnections { observation },
            ) => {
                observation.schema_version == 1
                    && !observation.worker_generation.is_empty()
                    && observation.context == request.context
            }
            (
                ControllerDiagnosticQuery::OperationAttribution { request },
                ControllerDiagnosticResult::OperationAttribution { observation },
            ) => {
                observation.schema_version == 1
                    && !observation.worker_generation.is_empty()
                    && reference(&observation.reference, request)
            }
            (
                ControllerDiagnosticQuery::OperationCancellation { request },
                ControllerDiagnosticResult::OperationCancellation { observation },
            ) => {
                observation.schema_version == 1
                    && !observation.worker_generation.is_empty()
                    && reference(&observation.reference, request)
            }
            _ => false,
        };
        if !valid {
            return Err(
                "Controller diagnostics reply kind/scope/operation mismatch; no rebind".into(),
            );
        }
        Ok(())
    }
}
