//! Optional readonly session projection; it never carries an execution request.
use crate::*;
use serde::{Deserialize, Serialize};

pub const CONTROLLER_PROJECTION_CONTRACT: &str = "winbridge.controller-readiness";
pub const PROJECTION_MAX_ITEMS: usize = 10;

#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerReadinessProjection {
    pub schema_version: u16,
    pub controller_build: BuildIdentity,
    pub controller_generation: String,
    pub session_id: String,
    pub provider_build: BuildIdentity,
    pub provider_generation: String,
    pub context: RequestContext,
    pub sequence: u64,
    pub views: Vec<ContractReadinessView>,
    pub verifications: Vec<VerificationView>,
    pub truncated: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionSessionMessage {
    ControllerProjection {
        projection: Box<ControllerReadinessProjection>,
    },
}

impl ControllerReadinessProjection {
    /// Bind live observations to this frame. Historical verification identities
    /// deliberately retain their original scope/build and cannot enable a view.
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && self.sequence > 0
            && self.sequence <= 9_007_199_254_740_991
            && self.views.len() <= PROJECTION_MAX_ITEMS
            && self.verifications.len() <= PROJECTION_MAX_ITEMS
            && self.views.iter().all(|view| {
                view.controller_generation == self.controller_generation
                    && view.session_id.as_ref() == Some(&self.session_id)
                    && view.identity.as_ref().is_none_or(|id| {
                        id.schema_version == 1
                            && id.context == self.context
                            && id.provider == self.provider_build
                            && id.project_policy.project_id == view.project_id
                            && id.contract == view.contract
                            && id.route.len() == 3
                            && id.route[1].role == RouteRole::Controller
                            && id.route[1].build == self.controller_build
                    })
                    && (!view.readiness.enabled
                        || (view.identity.is_some()
                            && view.readiness.supported
                            && view.readiness.build_verified
                            && view.readiness.pair_tested
                            && view.readiness.reasons.is_empty()))
            })
            && self
                .verifications
                .iter()
                .all(|view| view.identity.schema_version == 1 && view.identity.route.len() == 3)
    }
}

pub enum ProjectionObservation {
    Missing,
    Known(Box<ControllerReadinessProjection>),
    Unsupported,
}
pub fn interpret_controller_projection(info: &WorkerInfo) -> ProjectionObservation {
    let CatalogObservation::Known(catalog) = interpret_catalog(info) else {
        return ProjectionObservation::Unsupported;
    };
    if !catalog.supports(CONTROLLER_PROJECTION_CONTRACT, 1) {
        return ProjectionObservation::Unsupported;
    }
    let Some(raw) = catalog.controller_projection else {
        return ProjectionObservation::Missing;
    };
    let Ok(projection) = serde_json::from_value::<ControllerReadinessProjection>(raw) else {
        return ProjectionObservation::Unsupported;
    };
    let live = info
        .controller_connection
        .as_ref()
        .is_some_and(|connection| {
            matches!(connection.state, ConnectionState::Connected)
                && connection.session_id.as_ref() == Some(&projection.session_id)
                && connection.controller_generation.as_ref()
                    == Some(&projection.controller_generation)
        });
    if !projection.valid()
        || !live
        || info.build.as_ref() != Some(&projection.provider_build)
        || info.generation != projection.provider_generation
        || !info
            .fenced_requests
            .as_ref()
            .is_some_and(|f| f.request_contract_version == 2 && f.context == projection.context)
    {
        return ProjectionObservation::Unsupported;
    }
    ProjectionObservation::Known(Box::new(projection))
}
