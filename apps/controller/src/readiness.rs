//! Fresh observation of a specific route; receipt data never grants permission.
use crate::{readiness_store::ReceiptStore, Controller};
use winbridge_core::readiness::{evaluate_readiness, validate_identity, ContractDefinition};
use winbridge_protocol::*;

pub const PAIR_PROFILE: &str = "fenced-task-pair-v1";
pub fn definition() -> ContractDefinition {
    ContractDefinition {
        contract: ContractRef {
            id: WORKER_RPC_CONTRACT.into(),
            version: 2,
        },
        schema_sha256: fenced_rpc_schema_sha256(),
        test_profile: PAIR_PROFILE.into(),
        required_checks: [
            "info-schema-policy",
            "accepted-task",
            "duplicate-key",
            "output-exit",
            "stale-scope-refused",
            "same-route-after",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }
}

impl Controller {
    pub async fn call_controller_api(
        self: &std::sync::Arc<Self>,
        request: ControllerApiRequest,
    ) -> ControllerApiResponse {
        let result = if request.controller_contract_version != CONTROLLER_API_CONTRACT {
            Err("Unsupported controller-local contract; no worker request transmitted".into())
        } else {
            match request.request {
                ControllerOperation::ContractReadiness { query } => self
                    .observe_contract(&request.adapter, &request.adapter_kind, query)
                    .await
                    .map(|v| ControllerApiResult::ContractReadiness { view: Box::new(v) }),
                ControllerOperation::VerifyPair { request: verify } => self
                    .start_verification(request.adapter, request.adapter_kind, *verify)
                    .await
                    .map(|v| ControllerApiResult::Verification { view: Box::new(v) }),
                ControllerOperation::GetVerification { verification_key } => self
                    .get_verification(&verification_key)
                    .map(|v| ControllerApiResult::Verification { view: Box::new(v) }),
            }
        };
        ControllerApiResponse {
            controller_contract_version: CONTROLLER_API_CONTRACT,
            result: result.unwrap_or_else(|message| ControllerApiResult::Error { message }),
        }
    }

    pub async fn observe_contract(
        &self,
        adapter: &BuildIdentity,
        kind: &ControllerAdapter,
        query: ContractReadinessQuery,
    ) -> Result<ContractReadinessView, String> {
        if !winbridge_core::config::valid_id(&query.project_id)
            || query.contract.id.is_empty()
            || query.contract.id.len() > 128
            || query.contract.version == 0
        {
            return Err("Invalid project/contract query".into());
        }
        let expected = definition();
        let mut view = ContractReadinessView {
            project_id: query.project_id.clone(),
            contract: query.contract.clone(),
            identity: None,
            readiness: evaluate_readiness(&expected, false, None, None, false, false)
                .projection()
                .clone(),
            receipt_id: query.receipt_id.clone(),
            test_profile: PAIR_PROFILE.into(),
            controller_generation: self.generation.clone(),
            session_id: None,
            detail: String::new(),
        };
        if query.contract != expected.contract {
            view.detail =
                "Unknown contract has no definition, tests or permission; no fallback".into();
            return Ok(view);
        }
        let (sender, session, peer) = {
            let state = self
                .state
                .lock()
                .map_err(|_| "controller session state poisoned")?;
            let Some(active) = state.active.as_ref().filter(|s| s.ready) else {
                view.detail =
                    "Windows worker is offline; historical evidence cannot enable execution".into();
                return Ok(view);
            };
            (
                active.sender.clone(),
                active.id.clone(),
                state
                    .last_peer
                    .as_ref()
                    .map(|(p, _)| p.clone())
                    .ok_or("Missing live peer identity")?,
            )
        };
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Cannot observe provider identity".into());
        };
        let fresh = self
            .state
            .lock()
            .map_err(|_| "controller session state poisoned")?
            .active
            .as_ref()
            .is_some_and(|s| s.ready && s.id == session);
        view.session_id = Some(session.clone());
        let supported = matches!(interpret_catalog(&info), CatalogObservation::Known(c) if c.supports(&query.contract.id,query.contract.version));
        let identity = (|| {
            if info.instance_id != self.config.instance_id
                || info.node_id != peer.node_id
                || info.generation != peer.generation
            {
                return None;
            }
            let scope = info
                .fenced_requests
                .as_ref()
                .filter(|c| c.request_contract_version == 2)?
                .context
                .clone();
            if scope.instance_id != self.config.instance_id {
                return None;
            }
            let policies = interpret_project_policies(&info)?;
            let policy = policies
                .projects
                .into_iter()
                .find(|p| p.project_id == query.project_id)?;
            let schemas = interpret_contract_schemas(&info)?;
            let schema = schemas
                .schemas
                .into_iter()
                .find(|s| s.contract == query.contract)?
                .sha256;
            let provider = info.build.clone()?;
            let (name, transport) = match kind {
                ControllerAdapter::Mcp => ("winbridge-mcp", RouteTransport::McpIpcWebsocket),
                ControllerAdapter::Cli => ("winbridge-cli", RouteTransport::CliIpcWebsocket),
            };
            let identity = PairIdentity {
                schema_version: 1,
                contract: query.contract.clone(),
                contract_schema_sha256: schema,
                consumer: adapter.clone(),
                provider: provider.clone(),
                context: scope,
                route_transport: transport,
                route: vec![
                    RouteHop {
                        role: RouteRole::Adapter,
                        node_id: name.into(),
                        endpoint: "stdio".into(),
                        build: adapter.clone(),
                    },
                    RouteHop {
                        role: RouteRole::Controller,
                        node_id: self.config.node_id.clone(),
                        endpoint: crate::endpoint(&self.config),
                        build: build_identity(),
                    },
                    RouteHop {
                        role: RouteRole::Worker,
                        node_id: peer.node_id.clone(),
                        endpoint: format!("ws://{}", self.config.controller.listen_address),
                        build: provider,
                    },
                ],
                project_policy: policy,
            };
            Some(identity)
        })();
        let policy = info.journal_inspection.is_none()
            && info.journal_recovery_required.is_none()
            && info.projects.iter().any(|p| p.id == query.project_id)
            && !info.resource_recovery.as_ref().is_some_and(|r| {
                r.truncated || r.resources.iter().any(|p| p.project_id == query.project_id)
            });
        let evidence = match query.receipt_id.as_deref() {
            Some(id) => ReceiptStore::open(&self.config.state_directory)?.get(id)?,
            None => None,
        };
        let fresh = fresh
            && self
                .state
                .lock()
                .map_err(|_| "controller session state poisoned")?
                .active
                .as_ref()
                .is_some_and(|s| s.ready && s.id == session);
        view.readiness = evaluate_readiness(
            &expected,
            supported,
            identity.as_ref(),
            evidence.as_ref(),
            fresh,
            policy,
        )
        .projection()
        .clone();
        view.detail = if identity
            .as_ref()
            .is_some_and(|id| validate_identity(id).is_ok())
        {
            "Readiness applies to the named test profile and exact observed tuple; existing A2 admission remains unchanged".into()
        } else {
            "Build, scope, schema, policy or a route hop is missing/invalid; no readiness inferred"
                .into()
        };
        view.identity = identity;
        Ok(view)
    }
}

pub async fn call(
    address: &str,
    request: &ControllerApiRequest,
) -> Result<ControllerApiResponse, String> {
    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let response = tokio::time::timeout(winbridge_transport::IPC_TIMEOUT, async {
        let mut stream = tokio::net::UnixStream::connect(address)
            .await
            .map_err(|e| e.to_string())?;
        winbridge_transport::write_frame(&mut stream, &bytes)
            .await
            .map_err(|e| e.to_string())?;
        let bytes = winbridge_transport::read_frame(&mut stream)
            .await
            .map_err(|e| e.to_string())?;
        let response: ControllerApiResponse = serde_json::from_slice(&bytes)
            .map_err(|_| "Unknown controller reply contract; no automatic fallback")?;
        if response.controller_contract_version != CONTROLLER_API_CONTRACT {
            return Err::<ControllerApiResponse, String>(
                "Unsupported controller response contract".into(),
            );
        }
        Ok(response)
    })
    .await
    .map_err(|_| "Controller readiness observation timeout")??;
    Ok(response)
}
