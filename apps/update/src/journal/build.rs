//! One compiler attempt per update. Intent and receipts never authorize replay.
use super::*;
use winbridge_protocol::{update::*, ResolvedCommand};

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE update_builds(id TEXT PRIMARY KEY,update_id TEXT NOT NULL UNIQUE REFERENCES updates(id),spec TEXT NOT NULL,revision INTEGER NOT NULL,phase TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
        CREATE TABLE update_build_receipts(id TEXT NOT NULL REFERENCES update_builds(id),revision INTEGER NOT NULL,record TEXT NOT NULL,PRIMARY KEY(id,revision));
        CREATE TRIGGER attempt_identity BEFORE UPDATE ON update_builds WHEN NEW.id!=OLD.id OR NEW.update_id!=OLD.update_id OR NEW.spec!=OLD.spec OR NEW.revision!=OLD.revision+1 OR OLD.phase IN ('\"completed\"','\"indeterminate\"') BEGIN SELECT RAISE(ABORT,'Build identity/revision is immutable'); END;
        CREATE TRIGGER build_retained BEFORE DELETE ON update_builds BEGIN SELECT RAISE(ABORT,'Build history retained'); END;
        CREATE TRIGGER build_receipt_immutable BEFORE UPDATE ON update_build_receipts BEGIN SELECT RAISE(ABORT,'Build receipt immutable'); END;
        CREATE TRIGGER build_receipt_retained BEFORE DELETE ON update_build_receipts BEGIN SELECT RAISE(ABORT,'Build receipt retained'); END;
        PRAGMA user_version=4;").map_err(storage)
}
fn validate(r: &UpdateBuildRecord) -> Result<()> {
    let s = &r.spec;
    for id in [&s.attempt_id, &s.update_id, &s.owner_id] {
        Uuid::parse_str(id)
            .map_err(|_| fail(JournalErrorCode::Corrupt, "Invalid build identity"))?;
    }
    let invalid = || {
        fail(
            JournalErrorCode::Corrupt,
            "Build record/evidence is inconsistent",
        )
    };
    if r.schema_version != 1
        || r.revision > 4 // v1: Intent → Spawned → ResumeGranted → Running → terminal
        || r.updated_at_ms < s.accepted_at_ms
        || r.updated_at_ms > MAX_UPDATE_COUNTER
        || s.owner_epoch == 0
        || s.owner_epoch > MAX_UPDATE_COUNTER
        || s.worker_lease_revision > MAX_UPDATE_COUNTER
        || s.worker_source_spec.update_id != s.update_id
        || s.worker_source_spec.updater_owner_id != s.owner_id
        || s.worker_source_spec.updater_epoch != s.owner_epoch
        || s.worker_source_spec.source_directory != s.source_directory
        || s.worker_source_spec.target_manifest_sha256 != s.source_manifest_sha256
        || !valid_hash(&s.source_manifest_sha256, &[64])
        || validate_update_key(&s.request_key).is_err()
        || !matches!(s.platform.as_str(), "windows" | "linux")
        || (s.platform == "windows") != s.job_name.is_some()
        || s.source_available_bytes_before < 6 * 1024 * 1024 * 1024
        || s.runtime_available_bytes_before < 6 * 1024 * 1024 * 1024
        || s.command.working_directory != s.source_directory
        || s.command.executable.is_empty()
        || s.command.executable.len() > 4096
        || s.command.executable.contains('\0')
        || s.command.arguments.len() > 256
        || s.command
            .arguments
            .iter()
            .any(|v| v.len() > 8192 || v.contains('\0'))
        || s.command.timeout_ms == 0
        || s.command.timeout_ms > 3_600_000
        || s.source_directory.is_empty()
        || s.runtime_directory.is_empty()
        || s.source_directory.len() > 4096
        || s.runtime_directory.len() > 4096
        || s.job_name
            .as_ref()
            .is_some_and(|n| n != &crate::build::job_name(&s.attempt_id))
        || r.reason.is_empty()
        || r.reason.len() > 4096
        || r.reason
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(invalid());
    }
    if let Some(p) = &r.process {
        if p.process_id == 0
            || p.birth.is_empty()
            || !p.birth.bytes().all(|v| v.is_ascii_digit())
            || !valid_hash(&p.host_id, &[64])
        {
            return Err(invalid());
        }
    }
    for o in [&r.stdout, &r.stderr].into_iter().flatten() {
        if o.retained_bytes > o.observed_bytes
            || o.observed_bytes > MAX_UPDATE_COUNTER
            || o.retained_bytes > crate::build::MAX_OUTPUT_BYTES
            || !valid_hash(&o.sha256, &[64])
            || o.error.as_ref().is_some_and(|e| e.len() > 4096)
        {
            return Err(invalid());
        }
    }
    match r.phase {
        UpdateBuildPhase::Intent if r.revision != 0 || r.process.is_some() => return Err(invalid()),
        UpdateBuildPhase::Spawned | UpdateBuildPhase::ResumeGranted | UpdateBuildPhase::Running
            if r.process.is_none() =>
        {
            return Err(invalid())
        }
        UpdateBuildPhase::Completed if !r.cleanup_verified || r.outcome.is_none() => {
            return Err(invalid())
        }
        _ => {}
    }
    if r.phase != UpdateBuildPhase::Completed
        && (r.outcome.is_some() || r.exit_code.is_some() || r.cleanup_verified)
        || r.phase != UpdateBuildPhase::Completed && (r.stdout.is_some() || r.stderr.is_some())
        || r.outcome == Some(UpdateBuildOutcome::Succeeded)
            && (r.exit_code != Some(0)
                || r.process.is_none()
                || r.stdout.is_none()
                || r.stderr.is_none()
                || r.stdout.as_ref().is_some_and(|o| o.error.is_some())
                || r.stderr.as_ref().is_some_and(|o| o.error.is_some()))
        || r.outcome == Some(UpdateBuildOutcome::SpawnFailed) && r.process.is_some()
        || r.phase == UpdateBuildPhase::Completed
            && r.process.is_some()
            && (r.stdout.is_none() || r.stderr.is_none())
        || matches!(
            r.outcome,
            Some(UpdateBuildOutcome::Failed | UpdateBuildOutcome::TimedOut)
        ) && r.process.is_none()
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn read(c: &Connection, id: &str) -> Result<UpdateBuildRecord> {
    struct Row {
        json: String,
        spec: String,
        attempt_id: String,
        update_id: String,
        revision: i64,
        phase: String,
    }
    let row: Option<Row> = c
        .query_row(
            "SELECT record,spec,id,update_id,revision,phase FROM update_builds WHERE update_id=?1 OR id=?1",
            [id],
            |r| Ok(Row { json:r.get(0)?, spec:r.get(1)?, attempt_id:r.get(2)?, update_id:r.get(3)?, revision:r.get(4)?, phase:r.get(5)? }),
        )
        .optional()
        .map_err(storage)?;
    let Row {
        json,
        spec,
        attempt_id,
        update_id,
        revision,
        phase,
    } = row.ok_or_else(|| fail(JournalErrorCode::NotFound, "Build attempt not found"))?;
    let r: UpdateBuildRecord = decode(&json)?;
    validate(&r)?;
    if encode(&r.spec)? != spec
        || counter(revision)? != r.revision
        || encode(&r.phase)? != phase
        || r.spec.attempt_id != attempt_id
        || r.spec.update_id != update_id
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Build record/index mismatch",
        ));
    }
    let (count,last): (i64,Option<String>) = c.query_row(
        "SELECT count(*),(SELECT record FROM update_build_receipts WHERE id=?1 ORDER BY revision DESC LIMIT 1) FROM update_build_receipts WHERE id=?1",
        [&r.spec.attempt_id], |v| Ok((v.get(0)?,v.get(1)?))).map_err(storage)?;
    if counter(count)? != r.revision + 1 || last.as_deref() != Some(json.as_str()) {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Build history/receipt mismatch",
        ));
    }
    // Check every bounded v1 receipt, not only the current row/last receipt.
    // A damaged old binding or skipped PID/resume phase must never be treated as
    // evidence for a new grant or candidate approval.
    let mut statement=c.prepare("SELECT revision,record FROM update_build_receipts WHERE id=?1 ORDER BY revision LIMIT 6").map_err(storage)?;
    let rows = statement
        .query_map([&r.spec.attempt_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage)?;
    let mut previous: Option<UpdateBuildRecord> = None;
    for row in rows {
        let (revision, json) = row.map_err(storage)?;
        let receipt: UpdateBuildRecord = decode(&json)?;
        validate(&receipt)?;
        if receipt.spec != r.spec
            || counter(revision)? != receipt.revision
            || previous.as_ref().map_or(
                receipt.revision != 0 || receipt.phase != UpdateBuildPhase::Intent,
                |p| {
                    receipt.revision != p.revision + 1
                        || receipt.updated_at_ms < p.updated_at_ms
                        || winbridge_core::update::advance_build_phase(p.phase, receipt.phase)
                            .is_err()
                        || receipt.outcome == Some(UpdateBuildOutcome::Succeeded)
                            && p.phase != UpdateBuildPhase::Running
                        || p.process
                            .as_ref()
                            .is_some_and(|process| receipt.process.as_ref() != Some(process))
                },
            )
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Compiler receipt history/binding/FSM mismatch",
            ));
        }
        previous = Some(receipt);
    }
    if previous.as_ref() != Some(&r) {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Compiler receipt history incomplete",
        ));
    }
    let u = record(c, &r.spec.update_id)?;
    if u.request.source.manifest_sha256 != r.spec.source_manifest_sha256 {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Build source pin differs from update",
        ));
    }
    Ok(r)
}
pub(super) fn require_driver(c: &Connection, id: &str, event: UpdateEvent) -> Result<()> {
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM update_builds WHERE update_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if exists
        && matches!(
            event,
            UpdateEvent::BuildSucceeded | UpdateEvent::Fail | UpdateEvent::Defer
        )
    {
        return Err(fail(JournalErrorCode::PreconditionsChanged,"Compiler cleanup/artifacts require the coordinated driver; raw lifecycle event cannot bypass receipts"));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_build(directory: &Path, id: &str) -> Result<UpdateBuildRecord> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? < 4 {
            return Err(fail(
                JournalErrorCode::UnsupportedSchema,
                "Build inspection requires updater schema 4",
            ));
        }
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn existing_build(
        &self,
        id: &str,
        key: &str,
        command: &ResolvedCommand,
    ) -> Result<Option<UpdateBuildRecord>> {
        match read(&self.connection, id) {
            Ok(r) if r.spec.request_key == key && r.spec.command == *command => Ok(Some(r)),
            Ok(_) => Err(fail(
                JournalErrorCode::KeyConflict,
                "This update already owns another frozen build request",
            )),
            Err(e) if e.code == JournalErrorCode::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn accept_build(
        &mut self,
        a: &UpdateAuthority,
        spec: UpdateBuildSpec,
    ) -> Result<UpdateBuildRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        match read(&tx, &spec.update_id) {
            Ok(_) => {
                return Err(fail(
                    JournalErrorCode::InstallationBusy,
                    "Build already accepted; observe without replay",
                ))
            }
            Err(e) if e.code == JournalErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
        let u = record(&tx, &spec.update_id)?;
        supervisor::require_driver(&tx, a, &spec.update_id, UpdateEvent::BuildSucceeded)?;
        let source = source::header(&tx, &spec.update_id)?;
        if u.phase != UpdatePhase::Building
            || source.phase != SourceTransactionPhase::Verified
            || source.worker_source_spec != spec.worker_source_spec
            || source.worker_lease_revision != spec.worker_lease_revision
            || source.source_directory != spec.source_directory
            || source.runtime_directory != spec.runtime_directory
            || spec.owner_epoch != a.epoch
            || spec.owner_id != a.owner_id
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Verified source/current compiler owner required",
            ));
        }
        let r = UpdateBuildRecord {
            schema_version: 1,
            revision: 0,
            phase: UpdateBuildPhase::Intent,
            process: None,
            outcome: None,
            exit_code: None,
            cleanup_verified: false,
            stdout: None,
            stderr: None,
            updated_at_ms: spec.accepted_at_ms,
            reason: "Намерение сборки сохранено до создания процесса".into(),
            spec,
        };
        validate(&r)?;
        one(tx
            .execute(
                "INSERT INTO update_builds VALUES(?1,?2,?3,0,?4,?5)",
                params![
                    r.spec.attempt_id,
                    r.spec.update_id,
                    encode(&r.spec)?,
                    encode(&r.phase)?,
                    encode(&r)?
                ],
            )
            .map_err(storage)?)?;
        one(tx
            .execute(
                "INSERT INTO update_build_receipts VALUES(?1,0,?2)",
                params![r.spec.attempt_id, encode(&r)?],
            )
            .map_err(storage)?)?;
        if read(&tx, &r.spec.update_id)? != r {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Build intent readback failed",
            ));
        }
        tx.commit().map_err(storage)?;
        Ok(r)
    }
    pub(crate) fn advance_build(
        &mut self,
        a: &UpdateAuthority,
        previous: &UpdateBuildRecord,
        mut next: UpdateBuildRecord,
    ) -> Result<UpdateBuildRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        let current = read(&tx, &previous.spec.update_id)?;
        if current != *previous
            || next.spec != current.spec
            || current.spec.owner_epoch != a.epoch
            || current.spec.owner_id != a.owner_id
        {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Build scope/revision/owner changed; no effect",
            ));
        }
        let allowed =
            winbridge_core::update::advance_build_phase(current.phase, next.phase).is_ok();
        if !allowed || record(&tx, &current.spec.update_id)?.phase != UpdatePhase::Building {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Build transition/parent phase rejected",
            ));
        }
        if next.outcome == Some(UpdateBuildOutcome::Succeeded)
            && current.phase != UpdateBuildPhase::Running
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Compiler success requires a saved actual-start receipt",
            ));
        }
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Build revision exhausted"))?;
        next.updated_at_ms = now_ms()?.max(current.updated_at_ms);
        validate(&next)?;
        one(tx.execute("UPDATE update_builds SET record=?1,revision=?2,phase=?3 WHERE id=?4 AND revision=?5",params![encode(&next)?,next.revision as i64,encode(&next.phase)?,next.spec.attempt_id,current.revision as i64]).map_err(storage)?)?;
        one(tx
            .execute(
                "INSERT INTO update_build_receipts VALUES(?1,?2,?3)",
                params![next.spec.attempt_id, next.revision as i64, encode(&next)?],
            )
            .map_err(storage)?)?;
        if read(&tx, &next.spec.update_id)? != next {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Build transition readback failed",
            ));
        }
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}
