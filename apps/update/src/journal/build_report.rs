//! Immutable report intent and completion evidence, atomic with compiler receipts.
use super::*;
use crate::build_report::{ReportEvidence, ReportIntent};
use winbridge_protocol::update::UpdateBuildRecord;
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE update_build_report_intents(id TEXT PRIMARY KEY REFERENCES update_builds(id),record TEXT NOT NULL CHECK(json_valid(record)));
        CREATE TABLE update_build_reports(id TEXT PRIMARY KEY REFERENCES update_build_report_intents(id),record TEXT NOT NULL CHECK(json_valid(record)));
        CREATE TRIGGER build_report_intent_immutable BEFORE UPDATE ON update_build_report_intents BEGIN SELECT RAISE(ABORT,'Report intent immutable'); END;
        CREATE TRIGGER build_report_intent_retained BEFORE DELETE ON update_build_report_intents BEGIN SELECT RAISE(ABORT,'Report intent retained'); END;
        CREATE TRIGGER build_report_immutable BEFORE UPDATE ON update_build_reports BEGIN SELECT RAISE(ABORT,'Report receipt immutable'); END;
        CREATE TRIGGER build_report_retained BEFORE DELETE ON update_build_reports BEGIN SELECT RAISE(ABORT,'Report receipt retained'); END;
        PRAGMA user_version=6;").map_err(storage)
}
pub(super) fn read(
    c: &Connection,
    build: &UpdateBuildRecord,
) -> Result<(Option<ReportIntent>, Option<ReportEvidence>)> {
    if schema(c)? < 6 {
        return Ok((None, None));
    }
    let intent: Option<String> = c
        .query_row(
            "SELECT record FROM update_build_report_intents WHERE id=?1",
            [&build.spec.attempt_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let report: Option<String> = c
        .query_row(
            "SELECT record FROM update_build_reports WHERE id=?1",
            [&build.spec.attempt_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let intent = intent.map(|s| decode::<ReportIntent>(&s)).transpose()?;
    let report = report.map(|s| decode::<ReportEvidence>(&s)).transpose()?;
    let digest = crate::build_report::spec_sha256(&build.spec)
        .map_err(|e| fail(JournalErrorCode::Corrupt, e))?;
    let bad = || {
        fail(
            JournalErrorCode::Corrupt,
            "Compiler report intent/receipt binding inconsistent",
        )
    };
    if let Some(i) = &intent {
        if i.schema_version != 1
            || i.update_id != build.spec.update_id
            || i.attempt_id != build.spec.attempt_id
            || i.build_spec_sha256 != digest
            || [&i.runtime_identity, &i.update_directory_identity]
                .into_iter()
                .any(|s| s.is_empty() || s.len() > 128)
        {
            return Err(bad());
        }
    }
    if let Some(r) = &report {
        if intent.is_none()
            || !crate::build_report::successful(build)
            || !build.cleanup_verified
            || r.schema_version != 1
            || r.update_id != build.spec.update_id
            || r.attempt_id != build.spec.attempt_id
            || r.build_spec_sha256 != digest
            || r.bytes == 0
            || r.bytes > crate::build_report::MAX_REPORT_BYTES
            || !valid_hash(&r.sha256, &[64])
            || [&r.file_identity, &r.candidate_directory_identity]
                .into_iter()
                .any(|s| s.is_empty() || s.len() > 128)
        {
            return Err(bad());
        }
    }
    if intent.is_some() && crate::build_report::successful(build) && report.is_none() {
        return Err(bad());
    }
    Ok((intent, report))
}
impl UpdateJournal {
    pub(crate) fn build_report_intent(
        &self,
        build: &UpdateBuildRecord,
    ) -> Result<Option<ReportIntent>> {
        Ok(read(&self.connection, build)?.0)
    }
    pub fn inspect_build_report(
        directory: &Path,
        id: &str,
    ) -> Result<crate::build_report::ReportSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        let info = info(&tx)?;
        let build = build::read(&tx, id)?;
        let (intent, report) = read(&tx, &build)?;
        Ok(crate::build_report::ReportSnapshot {
            schema_version: 1,
            kind: "compiler_report_snapshot",
            read_only: true,
            journal: info,
            build,
            intent,
            evidence: report,
        })
    }
}
