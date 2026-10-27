//! Readonly projections isolated by configured controller and connection attempt.
use crate::Worker;
use std::time::{Duration, Instant};
use winbridge_core::controller_sessions::Ticket;
use winbridge_protocol::*;
#[derive(Clone)]
pub(crate) struct CachedProjection {
    projection: ControllerReadinessProjection,
    received: Instant,
}
const MAX_AGE: Duration = Duration::from_secs(10);
impl Worker {
    pub(crate) fn accept_projection(
        &self,
        ticket: &Ticket,
        projection: ControllerReadinessProjection,
    ) -> Result<bool, String> {
        let context = self.request_context()?;
        if !projection.valid()
            || projection.context != context
            || projection.provider_build != build_identity()
            || projection.provider_generation != self.generation
        {
            return Ok(false);
        }
        let Some(links) = &self.controllers else {
            return Ok(false);
        };
        let mut links = links.lock().map_err(|_| "Controller state lock poisoned")?;
        let Some(link) = links.links.get(ticket.controller_id()) else {
            return Ok(false);
        };
        if link.connection.controller_generation.as_ref() != Some(&projection.controller_generation)
            || link.connection.session_id.as_ref() != Some(&projection.session_id)
        {
            return Ok(false);
        }
        let now = links
            .clock
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        if !links
            .registry
            .project(ticket, &projection.session_id, projection.sequence, now)
        {
            return Ok(false);
        }
        links
            .links
            .get_mut(ticket.controller_id())
            .unwrap()
            .projection = Some(CachedProjection {
            projection,
            received: Instant::now(),
        });
        Ok(true)
    }
    fn add_projection(
        &self,
        catalog: &mut ContractCatalog,
        context: &RequestContext,
        snapshot: Option<(ControllerConnection, Option<CachedProjection>)>,
    ) -> Result<(), String> {
        catalog.controller_projection = None;
        let Some((connection, Some(cache))) = snapshot else {
            return Ok(());
        };
        if !matches!(connection.state, ConnectionState::Connected)
            || connection.session_id.as_ref() != Some(&cache.projection.session_id)
            || connection.controller_generation.as_ref()
                != Some(&cache.projection.controller_generation)
            || cache.projection.context != *context
            || cache.received.elapsed() > MAX_AGE
        {
            return Ok(());
        }
        let policies =
            winbridge_core::readiness::project_policy_catalog(&self.config, build_identity())?;
        let mut projection = cache.projection;
        for view in &mut projection.views {
            if view.readiness.enabled
                && !view.identity.as_ref().is_some_and(|identity| {
                    policies
                        .projects
                        .iter()
                        .any(|policy| policy == &identity.project_policy)
                })
            {
                view.readiness.enabled = false;
                view.readiness.reasons.push(ReadinessReason::PolicyDenied);
            }
        }
        catalog.controller_projection =
            Some(serde_json::to_value(projection).map_err(|e| e.to_string())?);
        Ok(())
    }
    pub(crate) fn projection_catalog(
        &self,
        context: &RequestContext,
    ) -> Result<ContractCatalog, String> {
        let snapshot = if let Some(links) = &self.controllers {
            let links = links.lock().map_err(|_| "Controller state lock poisoned")?;
            links
                .links
                .get(&links.primary)
                .map(|l| (l.connection.clone(), l.projection.clone()))
        } else {
            None
        };
        let mut catalog = ContractCatalog::implemented(build_identity());
        self.add_projection(&mut catalog, context, snapshot)?;
        Ok(catalog)
    }
    fn scope_controller_info(&self, ticket: &Ticket, info: &mut WorkerInfo) -> Result<(), String> {
        let links = self
            .controllers
            .as_ref()
            .ok_or("No controller configured")?;
        let snapshot = {
            let links = links.lock().map_err(|_| "Controller state lock poisoned")?;
            if !links.registry.is_current(ticket) {
                return Err("Controller attempt superseded; retry readonly observation".into());
            }
            links
                .links
                .get(ticket.controller_id())
                .map(|l| (l.connection.clone(), l.projection.clone()))
        };
        info.controller_connection = snapshot.as_ref().map(|(c, _)| c.clone());
        let context = &info
            .fenced_requests
            .as_ref()
            .ok_or("Missing provider scope")?
            .context;
        let mut catalog: ContractCatalog =
            serde_json::from_value(info.contract_catalog.clone().ok_or("Missing catalog")?)
                .map_err(|e| e.to_string())?;
        self.add_projection(&mut catalog, context, snapshot)?;
        info.contract_catalog = Some(serde_json::to_value(catalog).map_err(|e| e.to_string())?);
        Ok(())
    }
    pub(crate) fn handle_controller_wire(
        &self,
        ticket: &Ticket,
        request: WireRequest,
        creator: &OperationCreator,
    ) -> WireResponse {
        let mut response = self.handle_wire_from(request, creator);
        let local = match &mut response {
            WireResponse::Legacy(r) => r.as_mut(),
            WireResponse::Fenced(r) => &mut r.response,
        };
        if let LocalResponse::Info { info } = local {
            if let Err(error) = self.scope_controller_info(ticket, info) {
                return response.with_error(error);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_controller_observes_its_own_projection_and_primary_stays_local() {
        use winbridge_core::controller_sessions::Peer;
        let temp = tempfile::tempdir().unwrap();
        let worker = Worker::open(
            serde_json::from_value(serde_json::json!({
                "schemaVersion":1,"instanceId":"projection-two","nodeId":"worker",
                "stateDirectory":temp.path().join("state"),
                "worker":{"keepRunningOnUiExit":true,"controllerTopology":{
                    "schemaVersion":1,"primary":"a","controllers":[
                        {"id":"a","url":"ws://127.0.0.1:1143"},
                        {"id":"b","url":"ws://127.0.0.1:1144"}]}},
                "projects":[{"id":"fixture","workspaceRoot":temp.path(),
                    "allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let links = worker.controllers.as_ref().unwrap();
        let [a, b] = ["a", "b"].map(|id| {
            let mut links = links.lock().unwrap();
            let t = links.begin(id).unwrap();
            links
                .connected(
                    &t,
                    Peer {
                        node: "same-node".into(),
                        generation: format!("generation-{id}"),
                        session: "same-session".into(),
                    },
                )
                .unwrap();
            t
        });
        let p = |generation: &str| ControllerReadinessProjection {
            schema_version: 1,
            controller_build: build_identity(),
            controller_generation: generation.into(),
            session_id: "same-session".into(),
            provider_build: build_identity(),
            provider_generation: worker.generation.clone(),
            context: worker.request_context().unwrap(),
            sequence: 1,
            views: vec![],
            verifications: vec![],
            truncated: false,
        };
        let pa = p("generation-a");
        let pb = p("generation-b");
        assert!(worker.accept_projection(&a, pa.clone()).unwrap());
        assert!(!worker.accept_projection(&b, pa).unwrap());
        assert!(worker.accept_projection(&b, pb).unwrap());
        let observed = |ticket: Option<&Ticket>| {
            let request = WireRequest::Legacy(LocalRequest::Info);
            let response = match ticket {
                Some(t) => worker.handle_controller_wire(t, request, &worker.local_creator()),
                None => worker.handle_wire(request),
            };
            let LocalResponse::Info { info } = response.response() else {
                panic!("{response:?}")
            };
            let catalog: ContractCatalog =
                serde_json::from_value(info.contract_catalog.clone().unwrap()).unwrap();
            let projection: Option<ControllerReadinessProjection> = catalog
                .controller_projection
                .map(|v| serde_json::from_value(v).unwrap());
            (info.controller_connection.clone().unwrap(), projection)
        };
        assert_eq!(
            observed(None).1.unwrap().controller_generation,
            "generation-a"
        );
        assert_eq!(
            observed(Some(&a)).1.unwrap().controller_generation,
            "generation-a"
        );
        assert_eq!(
            observed(Some(&b)).1.unwrap().controller_generation,
            "generation-b"
        );
        links.lock().unwrap().disconnected(&a, "lost a".into());
        assert!(observed(None).1.is_none());
        assert_eq!(
            observed(Some(&b)).1.unwrap().controller_generation,
            "generation-b"
        );
        let fresh = links.lock().unwrap().begin("a").unwrap();
        links
            .lock()
            .unwrap()
            .connected(
                &fresh,
                Peer {
                    node: "same-node".into(),
                    generation: "new-generation".into(),
                    session: "same-session".into(),
                },
            )
            .unwrap();
        let fresh_projection = p("new-generation");
        assert!(!worker
            .accept_projection(&a, fresh_projection.clone())
            .unwrap());
        assert!(worker.accept_projection(&fresh, fresh_projection).unwrap());
        links
            .lock()
            .unwrap()
            .disconnected(&a, "late old callback".into());
        assert_eq!(
            observed(None).1.unwrap().controller_generation,
            "new-generation"
        );
        assert_eq!(
            observed(Some(&b)).1.unwrap().controller_generation,
            "generation-b"
        );
    }
    #[test]
    fn cache_rejects_foreign_reordered_future_and_expired_observations() {
        let temp = tempfile::tempdir().unwrap();
        let worker = Worker::open(serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"projection","nodeId":"worker","stateDirectory":temp.path().join("state"),"worker":{"controllerUrl":"ws://127.0.0.1:1143","keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":temp.path(),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
        let links = worker.controllers.as_ref().unwrap();
        let ticket = links.lock().unwrap().begin("legacy").unwrap();
        links
            .lock()
            .unwrap()
            .connected(
                &ticket,
                winbridge_core::controller_sessions::Peer {
                    node: "controller".into(),
                    session: "session".into(),
                    generation: "generation".into(),
                },
            )
            .unwrap();
        let projection = ControllerReadinessProjection {
            schema_version: 1,
            controller_build: build_identity(),
            controller_generation: "generation".into(),
            session_id: "session".into(),
            provider_build: build_identity(),
            provider_generation: worker.generation.clone(),
            context: worker.request_context().unwrap(),
            sequence: 2,
            views: vec![],
            verifications: vec![],
            truncated: false,
        };
        assert!(worker
            .accept_projection(&ticket, projection.clone())
            .unwrap());
        for mutation in 0..5 {
            let mut changed = projection.clone();
            changed.sequence = 3;
            match mutation {
                0 => changed.context.journal_incarnation = "foreign".into(),
                1 => changed.session_id = "previous-session".into(),
                2 => changed.provider_build.build_id = "other-build".into(),
                3 => changed.schema_version = 2,
                _ => changed.sequence = 1,
            }
            assert!(!worker.accept_projection(&ticket, changed).unwrap());
        }
        assert!(worker
            .projection_catalog(&projection.context)
            .unwrap()
            .controller_projection
            .is_some());
        links
            .lock()
            .unwrap()
            .links
            .get_mut("legacy")
            .unwrap()
            .projection
            .as_mut()
            .unwrap()
            .received = Instant::now() - MAX_AGE - Duration::from_secs(1);
        assert!(worker
            .projection_catalog(&projection.context)
            .unwrap()
            .controller_projection
            .is_none());
        links
            .lock()
            .unwrap()
            .disconnected(&ticket, "test disconnect".into());
        assert!(links.lock().unwrap().links["legacy"].projection.is_none());
    }
}
