//! Durable artifact observation; worker health and installation are separate contracts.
use crate::{
    candidate_artifacts::CandidateArtifactInspection,
    journal::{UpdateAuthority, UpdateJournal},
    source::SourceControl,
    UpdateConfig,
};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactSetRecord {
    pub schema_version: u16,
    pub owner_id: String,
    pub owner_epoch: u64,
    pub build_digest: String,
    pub source_digest: String,
    pub parent_revision: u64,
    pub observed_at_ms: u64,
    pub inspection: CandidateArtifactInspection,
}
pub(crate) fn digest<T: Serialize>(v: &T) -> Result<String> {
    Ok(crate::sha256(&serde_json::to_vec(v).map_err(err)?))
}
/// Duplicate-first readonly observation. Rechecking/recording a set executes no program.
/// Success advances only to ValidatingCandidate; it cannot approve/install the candidate.
pub fn verify_and_record(
    j: &mut UpdateJournal,
    a: &UpdateAuthority,
    cfg: &UpdateConfig,
    id: &str,
    control: &mut impl SourceControl,
) -> Result<ArtifactSetRecord> {
    j.check_source_authority(a).map_err(err)?;
    match j.candidate_artifacts(id) {
        Ok(saved) => return Ok(saved),
        Err(e) if e.code == crate::journal::JournalErrorCode::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    let source = crate::build::saved_source_guard(j, a, cfg, id)?;
    let view = UpdateJournal::inspect_build_report(&cfg.runtime_directory.join("update-state"), id)
        .map_err(err)?;
    if view.build.spec.owner_id != a.owner_id()
        || view.build.spec.owner_epoch != a.epoch()
        || !crate::build_report::successful(&view.build)
        || view.build.spec.worker_source_spec != source.worker_source_spec
        || view.build.spec.worker_lease_revision != source.worker_lease_revision
    {
        return Err("Current completed compiler/source authority required for artifact set".into());
    }
    crate::build::source_guard(j, a, cfg, id, control)?;
    crate::abort::compiler_empty(&view.build)?;
    let (inspection, _pins) = crate::candidate_artifacts::inspect_pinned(cfg, id)?;
    crate::build::source_guard(j, a, cfg, id, control)?;
    crate::abort::compiler_empty(&view.build)?;
    if UpdateJournal::inspect_build_report(&cfg.runtime_directory.join("update-state"), id)
        .map_err(err)?
        != view
    {
        return Err("Compiler/report changed during artifact validation".into());
    }
    let parent = j.get(id).map_err(err)?;
    let candidate = ArtifactSetRecord {
        schema_version: 1,
        owner_id: a.owner_id().into(),
        owner_epoch: a.epoch(),
        build_digest: digest(&view.build)?,
        source_digest: digest(&source)?,
        parent_revision: parent.revision + 1,
        observed_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_millis()
            .try_into()
            .map_err(err)?,
        inspection,
    };
    j.record_candidate_artifacts(a, &candidate).map_err(err)
}
