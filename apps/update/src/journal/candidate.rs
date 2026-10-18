//! One immutable artifact-set receipt, atomic with the start of candidate validation.
use super::*;
use crate::candidate::ArtifactSetRecord;
use winbridge_protocol::update::UpdateEvent;
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE candidate_artifact_sets(id TEXT PRIMARY KEY REFERENCES updates(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER candidate_set_immutable BEFORE UPDATE ON candidate_artifact_sets BEGIN SELECT RAISE(ABORT,'Candidate artifact set immutable'); END;
CREATE TRIGGER candidate_set_retained BEFORE DELETE ON candidate_artifact_sets BEGIN SELECT RAISE(ABORT,'Candidate artifact set retained'); END;
PRAGMA user_version=7;").map_err(storage)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<ArtifactSetRecord> {
    if schema(c)? < 7 {
        return Err(fail(
            JournalErrorCode::NotFound,
            "No artifact-set receipts in this schema",
        ));
    }
    let json: Option<String> = c
        .query_row(
            "SELECT record FROM candidate_artifact_sets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let r: ArtifactSetRecord = decode(&json.ok_or_else(|| {
        fail(
            JournalErrorCode::NotFound,
            "Candidate artifact set not found",
        )
    })?)?;
    let build = build::read(c, id)?;
    let source = source::header(c, id)?;
    let (_, report) = build_report::read(c, &build)?;
    let update = record(c, id)?;
    let evidence = &r.inspection;
    if r.schema_version != 1
        || r.owner_id != build.spec.owner_id
        || r.owner_epoch != build.spec.owner_epoch
        || r.build_digest != crate::candidate::digest(&build).map_err(storage)?
        || r.source_digest != crate::candidate::digest(&source).map_err(storage)?
        || r.observed_at_ms > MAX_UPDATE_COUNTER
        || r.parent_revision == 0
        || r.parent_revision > MAX_UPDATE_COUNTER
        || r.parent_revision > update.revision
        || evidence.schema_version != 1
        || evidence.kind != "candidate_artifact_inspection"
        || !evidence.read_only
        || evidence.runtime_applied
        || evidence.candidate_verified
        || Some(&evidence.report) != report.as_ref()
        || evidence.report.update_id != id
        || evidence.payload_directory_identity.is_empty()
        || evidence.payload_directory_identity.len() > 128
        || evidence.artifacts.len() != 4
        || !valid_hash(&evidence.set_sha256, &[64])
        || crate::sha256(
            &serde_json::to_vec(&(
                &evidence.report,
                &evidence.payload_directory_identity,
                &evidence.artifacts,
            ))
            .map_err(storage)?,
        ) != evidence.set_sha256
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Candidate artifact-set evidence/owner/compiler binding inconsistent",
        ));
    }
    crate::candidate_artifacts::validate_saved(evidence, &build.spec, &update.request.source)
        .map_err(|e| fail(JournalErrorCode::Corrupt, e))?;
    let (sequence, transition, event, snapshot): (i64, String, String, String) = c
        .query_row(
            "SELECT sequence,transition,payload,record FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, r.parent_revision as i64],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(storage)?;
    let event: UpdateCommand = decode(&event)?;
    let snapshot: UpdateRecord = decode(&snapshot)?;
    let transition: UpdateTransition = decode(&transition)?;
    if event.event != UpdateEvent::BuildSucceeded
        || event.idempotency_key != "candidate-artifacts-v1"
        || event.expected_revision + 1 != r.parent_revision
        || snapshot.update_id != id
        || snapshot.phase != UpdatePhase::ValidatingCandidate
        || snapshot.revision != r.parent_revision
        || snapshot.request != update.request
        || r.observed_at_ms > snapshot.updated_at_ms
        || transition.sequence != counter(sequence)?
        || transition.update_id != id
        || transition.revision != r.parent_revision
        || transition.owner_id != r.owner_id
        || transition.owner_epoch != r.owner_epoch
        || transition.previous_phase != Some(UpdatePhase::Building)
        || transition.phase != UpdatePhase::ValidatingCandidate
        || transition.command.as_ref() != Some(&event)
        || transition.occurred_at_ms != snapshot.updated_at_ms
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Candidate artifact receipt requires its atomic parent transition",
        ));
    }
    Ok(r)
}
impl UpdateJournal {
    pub fn inspect_candidate_artifacts(directory: &Path, id: &str) -> Result<ArtifactSetRecord> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn candidate_artifacts(&self, id: &str) -> Result<ArtifactSetRecord> {
        read(&self.connection, id)
    }
    pub(crate) fn record_candidate_artifacts(
        &mut self,
        a: &UpdateAuthority,
        candidate: &ArtifactSetRecord,
    ) -> Result<ArtifactSetRecord> {
        let id = &candidate.inspection.report.update_id;
        let command=UpdateCommand {idempotency_key:"candidate-artifacts-v1".into(),expected_revision:candidate.parent_revision.checked_sub(1).ok_or_else(||fail(JournalErrorCode::InvalidRequest,"Candidate parent revision missing"))?,event:UpdateEvent::BuildSucceeded,reason:"Четыре artifact identity/hash проверены; ожидается проверка совместимости и здоровья worker".into()};
        self.advance_with_driver(a, id, &command, Driver::Candidate, |c, _| {
            let build = build::read(c, id)?;
            let source = source::header(c, id)?;
            let (_, report) = build_report::read(c, &build)?;
            if candidate.owner_id != a.owner_id
                || candidate.owner_epoch != a.epoch
                || build.spec.owner_id != a.owner_id
                || build.spec.owner_epoch != a.epoch
                || candidate.build_digest != crate::candidate::digest(&build).map_err(storage)?
                || candidate.source_digest != crate::candidate::digest(&source).map_err(storage)?
                || report.as_ref() != Some(&candidate.inspection.report)
            {
                return Err(fail(
                    JournalErrorCode::StaleOwner,
                    "Candidate source/build/report proof changed before commit",
                ));
            }
            one(c
                .execute(
                    "INSERT INTO candidate_artifact_sets VALUES(?1,?2)",
                    params![id, encode(candidate)?],
                )
                .map_err(storage)?)?;
            if read(c, id)? != *candidate {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Candidate artifact receipt readback failed",
                ));
            }
            Ok(())
        })?;
        read(&self.connection, id)
    }
}
