//! A disposable readonly cache, scoped to the active controller session.
use crate::Worker;
use std::time::{Duration, Instant};
use winbridge_protocol::*;
pub(crate) struct CachedProjection {
    projection: ControllerReadinessProjection,
    received: Instant,
}
const MAX_AGE: Duration = Duration::from_secs(10);
impl Worker {
    pub(crate) fn clear_projection(&self) {
        if let Ok(mut cache) = self.controller_projection.lock() {
            *cache = None;
        }
    }
    pub(crate) fn accept_projection(
        &self,
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
        let connection = self
            .controller_connection
            .lock()
            .map_err(|_| "Controller connection lock poisoned")?;
        let Some(connection) = connection.as_ref() else {
            return Ok(false);
        };
        if !matches!(connection.state, ConnectionState::Connected)
            || connection.session_id.as_ref() != Some(&projection.session_id)
            || connection.controller_generation.as_ref() != Some(&projection.controller_generation)
        {
            return Ok(false);
        }
        let mut cache = self
            .controller_projection
            .lock()
            .map_err(|_| "Controller projection lock poisoned")?;
        if cache.as_ref().is_some_and(|old| {
            old.projection.session_id == projection.session_id
                && old.projection.sequence >= projection.sequence
        }) {
            return Ok(false);
        }
        *cache = Some(CachedProjection {
            projection,
            received: Instant::now(),
        });
        Ok(true)
    }
    pub(crate) fn projection_catalog(
        &self,
        context: &RequestContext,
    ) -> Result<ContractCatalog, String> {
        let mut catalog = ContractCatalog::implemented(build_identity());
        let connection = self
            .controller_connection
            .lock()
            .map_err(|_| "Controller connection lock poisoned")?;
        let cache = self
            .controller_projection
            .lock()
            .map_err(|_| "Controller projection lock poisoned")?;
        if let (Some(connection), Some(cache)) = (connection.as_ref(), cache.as_ref()) {
            if matches!(connection.state, ConnectionState::Connected)
                && connection.session_id.as_ref() == Some(&cache.projection.session_id)
                && connection.controller_generation.as_ref()
                    == Some(&cache.projection.controller_generation)
                && cache.projection.context == *context
                && cache.received.elapsed() <= MAX_AGE
            {
                let policies = winbridge_core::readiness::project_policy_catalog(
                    &self.config,
                    build_identity(),
                )?;
                // Local policy is still authoritative; a projection never grants it.
                let mut projection = cache.projection.clone();
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
            }
        }
        Ok(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_foreign_reordered_future_and_expired_observations() {
        let temp = tempfile::tempdir().unwrap();
        let worker = Worker::open(serde_json::from_value(serde_json::json!({"schemaVersion":1,"instanceId":"projection","nodeId":"worker","stateDirectory":temp.path().join("state"),"worker":{"controllerUrl":"ws://127.0.0.1:1143","keepRunningOnUiExit":true},"projects":[{"id":"fixture","workspaceRoot":temp.path(),"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]})).unwrap()).unwrap();
        {
            let mut connection = worker.controller_connection.lock().unwrap();
            let connection = connection.as_mut().unwrap();
            connection.state = ConnectionState::Connected;
            connection.session_id = Some("session".into());
            connection.controller_generation = Some("generation".into());
        }
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
        assert!(worker.accept_projection(projection.clone()).unwrap());
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
            assert!(!worker.accept_projection(changed).unwrap());
        }
        assert!(worker
            .projection_catalog(&projection.context)
            .unwrap()
            .controller_projection
            .is_some());
        worker
            .controller_projection
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .received = Instant::now() - MAX_AGE - Duration::from_secs(1);
        assert!(worker
            .projection_catalog(&projection.context)
            .unwrap()
            .controller_projection
            .is_none());
        worker.clear_projection();
        assert!(worker.controller_projection.lock().unwrap().is_none());
    }
}
