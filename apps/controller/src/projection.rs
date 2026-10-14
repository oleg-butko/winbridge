//! Negotiated latest-value metadata; independent of execution and task output.
use crate::Controller;
use std::{sync::Arc, time::Duration};
use winbridge_protocol::*;
use winbridge_transport::websocket::{Result, RpcSender};

pub(crate) async fn publish(
    owner: Arc<Controller>,
    sender: RpcSender,
    session: String,
) -> Result<()> {
    let mut sequence = 0_u64;
    loop {
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Projection requires provider Info".into());
        };
        if !matches!(interpret_catalog(&info), CatalogObservation::Known(catalog) if catalog.supports(CONTROLLER_PROJECTION_CONTRACT, 1))
        {
            // An old provider must never receive an unknown session opcode.
            return Ok(());
        }
        let provider = info
            .build
            .as_ref()
            .ok_or("Missing projection provider identity")?
            .clone();
        let context = info
            .fenced_requests
            .as_ref()
            .ok_or("Missing projection scope")?
            .context
            .clone();
        let (recent, mut truncated) = owner.recent_verifications()?;
        let mut views = Vec::new();
        let mut verifications = Vec::new();
        for (adapter, kind, verification) in recent {
            let query = ContractReadinessQuery {
                project_id: verification.identity.project_policy.project_id.clone(),
                contract: verification.identity.contract.clone(),
                receipt_id: verification.receipt_id.clone(),
            };
            let view = owner.observe_contract(&adapter, &kind, query).await?;
            if view.session_id.as_ref() != Some(&session) {
                return Ok(());
            }
            views.push(view);
            verifications.push(verification);
        }
        for project in &info.projects {
            if views.iter().any(|view| view.project_id == project.id) {
                continue;
            }
            if views.len() == PROJECTION_MAX_ITEMS {
                truncated = true;
                break;
            }
            let query = ContractReadinessQuery {
                project_id: project.id.clone(),
                contract: crate::readiness::definition().contract,
                receipt_id: None,
            };
            let view = owner
                .observe_contract(&build_identity(), &ControllerAdapter::Mcp, query)
                .await?;
            if view.session_id.as_ref() != Some(&session) {
                return Ok(());
            }
            views.push(view);
        }
        sequence = sequence
            .checked_add(1)
            .ok_or("Projection sequence exhausted")?;
        let projection = ControllerReadinessProjection {
            schema_version: 1,
            controller_build: build_identity(),
            controller_generation: owner.generation.clone(),
            session_id: session.clone(),
            provider_build: provider,
            provider_generation: info.generation,
            context,
            sequence,
            views,
            verifications,
            truncated,
        };
        if !projection.valid() {
            return Err("Foreign/inconsistent projection refused".into());
        }
        if !owner.status()?.connected || owner.status()?.session_id.as_ref() != Some(&session) {
            return Ok(());
        }
        sender.publish_projection(projection)?;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
