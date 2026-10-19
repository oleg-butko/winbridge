//! Durable abort intent/proof/release; never infer cleanup from a missing PID alone.
use super::*;
use crate::abort::{AbortPhase, AbortRecord, AbortSpec};
use winbridge_protocol::update::*;

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE update_aborts(id TEXT PRIMARY KEY REFERENCES updates(id),spec TEXT NOT NULL,revision INTEGER NOT NULL,phase TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE update_abort_receipts(id TEXT NOT NULL REFERENCES update_aborts(id),revision INTEGER NOT NULL,record TEXT NOT NULL,PRIMARY KEY(id,revision));
CREATE TRIGGER abort_identity BEFORE UPDATE ON update_aborts WHEN NEW.id!=OLD.id OR NEW.spec!=OLD.spec OR NEW.revision!=OLD.revision+1 OR OLD.phase IN ('\"completed\"','\"blocked\"') BEGIN SELECT RAISE(ABORT,'Abort identity/revision immutable'); END;
CREATE TRIGGER abort_retained BEFORE DELETE ON update_aborts BEGIN SELECT RAISE(ABORT,'Abort history retained'); END;
CREATE TRIGGER abort_receipt_immutable BEFORE UPDATE ON update_abort_receipts BEGIN SELECT RAISE(ABORT,'Abort receipt immutable'); END;
CREATE TRIGGER abort_receipt_retained BEFORE DELETE ON update_abort_receipts BEGIN SELECT RAISE(ABORT,'Abort receipt retained'); END;
DROP TRIGGER source_identity;
CREATE TRIGGER source_identity BEFORE UPDATE ON source_transactions WHEN NEW.id!=OLD.id OR NEW.binding!=OLD.binding OR NEW.revision!=OLD.revision+1 OR OLD.phase IN ('\"verified\"','\"blocked\"','\"aborted\"') BEGIN SELECT RAISE(ABORT,'Source identity/revision immutable'); END;
PRAGMA user_version=5;").map_err(storage)
}
fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(crate::sha256(encode(value)?.as_bytes()))
}
pub(crate) type AbortEvidence = (Option<String>, Option<String>, Option<String>);
pub(super) fn evidence(c: &Connection, id: &str) -> Result<AbortEvidence> {
    let source = match source::header(c, id) {
        Ok(r) => Some(digest(&r)?),
        Err(e) if e.code == JournalErrorCode::NotFound => None,
        Err(e) => return Err(e),
    };
    let build = match build::read(c, id) {
        Ok(r) => Some(digest(&r)?),
        Err(e) if e.code == JournalErrorCode::NotFound => None,
        Err(e) => return Err(e),
    };
    let candidate = match candidate::read(c, id) {
        Ok(r) => Some(digest(&r)?),
        Err(e) if e.code == JournalErrorCode::NotFound => None,
        Err(e) => return Err(e),
    };
    Ok((source, build, candidate))
}
fn validate(r: &AbortRecord) -> Result<()> {
    let bad = || {
        fail(
            JournalErrorCode::Corrupt,
            "Abort record/evidence inconsistent",
        )
    };
    if r.schema_version != 1
        || r.revision > 3
        || Uuid::parse_str(&r.spec.update_id).is_err()
        || Uuid::parse_str(&r.spec.owner_id).is_err()
        || r.spec.owner_epoch == 0
        || r.spec.owner_epoch > MAX_UPDATE_COUNTER
        || r.spec.accepted_at_ms > r.updated_at_ms
        || r.updated_at_ms > MAX_UPDATE_COUNTER
        || r.spec.runtime_directory.is_empty()
        || r.spec.runtime_identity.is_empty()
        || winbridge_core::update::advance_update(r.spec.original_phase, UpdateEvent::BeginAbort)
            .is_err()
        || r.reason.len() > 4096
        || [
            &r.spec.source_digest,
            &r.spec.build_digest,
            &r.spec.candidate_digest,
        ]
        .into_iter()
        .flatten()
        .any(|h| !valid_hash(h, &[64]))
        || r.source_sha256
            .as_ref()
            .is_some_and(|h| !valid_hash(h, &[64]))
    {
        return Err(bad());
    }
    if r.phase == AbortPhase::Intent
        && (r.revision != 0
            || r.cleanup_verified
            || r.worker_before.is_some()
            || r.release_command.is_some()
            || r.worker_released.is_some()
            || r.source_sha256.is_some())
    {
        return Err(bad());
    }
    if matches!(
        r.phase,
        AbortPhase::Verified | AbortPhase::Released | AbortPhase::Completed
    ) && !r.cleanup_verified
    {
        return Err(bad());
    }
    if let Some(before) = &r.worker_before {
        if before.spec.update_id != r.spec.update_id
            || before.spec.updater_owner_id != r.spec.owner_id
            || before.spec.updater_epoch != r.spec.owner_epoch
            || before.revision >= MAX_UPDATE_COUNTER
        {
            return Err(bad());
        }
        if let Some(command) = &r.release_command {
            if before.phase == WorkerUpdatePhase::Released
                || command.action != WorkerUpdateAction::Release
                || command.expected_revision != before.revision
                || command.source_manifest_sha256 != r.source_sha256
                || command.command_id != format!("abort-release-{}", r.spec.update_id)
            {
                return Err(bad());
            }
        } else if before.phase != WorkerUpdatePhase::Released && r.phase != AbortPhase::Blocked {
            return Err(bad());
        }
    } else if r.release_command.is_some() || r.worker_released.is_some() {
        return Err(bad());
    }
    if let Some(released) = &r.worker_released {
        let Some(before) = &r.worker_before else {
            return Err(bad());
        };
        if released.phase != WorkerUpdatePhase::Released
            || released.spec != before.spec
            || released.revision
                != before.revision + u64::from(before.phase != WorkerUpdatePhase::Released)
        {
            return Err(bad());
        }
    }
    if matches!(r.phase, AbortPhase::Released | AbortPhase::Completed)
        && r.worker_before.is_some()
        && r.worker_released.is_none()
    {
        return Err(bad());
    }
    Ok(())
}
fn proof_preserved(old: &AbortRecord, next: &AbortRecord) -> bool {
    (old.phase == AbortPhase::Intent
        || (old.cleanup_verified == next.cleanup_verified
            && old.source_sha256 == next.source_sha256
            && old.worker_before == next.worker_before
            && old.release_command == next.release_command))
        && (old.phase != AbortPhase::Released || old.worker_released == next.worker_released)
        && next.updated_at_ms >= old.updated_at_ms
}
fn read(c: &Connection, id: &str) -> Result<AbortRecord> {
    read_with_interleave(c, id, || {})
}
fn read_with_interleave(
    c: &Connection,
    id: &str,
    after_header: impl FnOnce(),
) -> Result<AbortRecord> {
    let row: Option<(String, String, i64, String)> = c
        .query_row(
            "SELECT record,spec,revision,phase FROM update_aborts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(storage)?;
    let (json, spec, rev, phase) =
        row.ok_or_else(|| fail(JournalErrorCode::NotFound, "Abort not found"))?;
    let current: AbortRecord = decode(&json)?;
    validate(&current)?;
    if encode(&current.spec)? != spec
        || counter(rev)? != current.revision
        || encode(&current.phase)? != phase
        || current.spec.update_id != id
    {
        return Err(fail(JournalErrorCode::Corrupt, "Abort indexes differ"));
    }
    after_header();
    let mut q=c.prepare("SELECT revision,record FROM update_abort_receipts WHERE id=?1 ORDER BY revision LIMIT 6").map_err(storage)?;
    let mut rows = q.query([id]).map_err(storage)?;
    let mut previous: Option<AbortRecord> = None;
    let mut count = 0;
    while let Some(row) = rows.next().map_err(storage)? {
        let rev: i64 = row.get(0).map_err(storage)?;
        let raw: String = row.get(1).map_err(storage)?;
        let r: AbortRecord = decode(&raw)?;
        validate(&r)?;
        if counter(rev)? != count
            || r.revision != count
            || r.spec != current.spec
            || count == 0 && r.phase != AbortPhase::Intent
            || previous.as_ref().is_some_and(|p| {
                !crate::abort::transition(p.phase, r.phase) || !proof_preserved(p, &r)
            })
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Abort receipt history differs",
            ));
        }
        previous = Some(r);
        count += 1;
    }
    if count != current.revision + 1 || previous.as_ref() != Some(&current) {
        return Err(fail(JournalErrorCode::Corrupt, "Abort receipt missing"));
    }
    Ok(current)
}
fn persist(c: &Connection, old: &AbortRecord, next: &AbortRecord) -> Result<()> {
    validate(next)?;
    if next.spec != old.spec
        || next.revision != old.revision + 1
        || !crate::abort::transition(old.phase, next.phase)
        || !proof_preserved(old, next)
    {
        return Err(fail(
            JournalErrorCode::InvalidRequest,
            "Abort transition rejected",
        ));
    }
    one(c
        .execute(
            "UPDATE update_aborts SET revision=?1,phase=?2,record=?3 WHERE id=?4 AND revision=?5",
            params![
                next.revision as i64,
                encode(&next.phase)?,
                encode(next)?,
                next.spec.update_id,
                old.revision as i64
            ],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO update_abort_receipts VALUES(?1,?2,?3)",
            params![next.spec.update_id, next.revision as i64, encode(next)?],
        )
        .map_err(storage)?)?;
    if read(c, &next.spec.update_id)? != *next {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Abort receipt readback failed",
        ));
    }
    Ok(())
}
pub(super) fn require_driver(
    c: &Connection,
    id: &str,
    event: UpdateEvent,
    driver: Driver,
) -> Result<()> {
    if driver == Driver::Abort {
        return Ok(());
    }
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM update_aborts WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if exists || matches!(event, UpdateEvent::BeginAbort | UpdateEvent::AbortCompleted) {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Abort requires its coordinated driver",
        ));
    }
    Ok(())
}
fn inspect_snapshot(
    directory: &Path,
    id: &str,
    after_header: impl FnOnce(),
) -> Result<AbortRecord> {
    check_files(directory)?;
    let mut c = connection(&directory.join("updates.sqlite"), true)?;
    if schema(&c)? < 5 {
        return Err(fail(
            JournalErrorCode::UnsupportedSchema,
            "Abort inspection requires updater schema 5 or newer",
        ));
    }
    let tx = c.transaction().map_err(storage)?;
    read_with_interleave(&tx, id, after_header)
}
impl UpdateJournal {
    pub fn inspect_abort(directory: &Path, id: &str) -> Result<AbortRecord> {
        inspect_snapshot(directory, id, || {})
    }
    pub(crate) fn abort_evidence(&self, id: &str) -> Result<AbortEvidence> {
        evidence(&self.connection, id)
    }
    pub(crate) fn admit_abort(
        &mut self,
        a: &UpdateAuthority,
        spec: AbortSpec,
    ) -> Result<(AbortRecord, bool)> {
        let mut fresh = false;
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        match read(&self.connection, &spec.update_id) {
            Ok(r) => return Ok((r, false)),
            Err(e) if e.code == JournalErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
        let update = record(&self.connection, &spec.update_id)?;
        let command = UpdateCommand {
            idempotency_key: "abort-intent-v1".into(),
            expected_revision: update.revision,
            event: UpdateEvent::BeginAbort,
            reason: "Отмена до замены runtime принята; cleanup и освобождение ресурсов ещё не подтверждены".into(),
        };
        self.advance_with_driver(a, &spec.update_id, &command, Driver::Abort, |c, _| {
            if spec.owner_id != a.owner_id
                || spec.owner_epoch != a.epoch
                || spec.original_phase != update.phase
                || evidence(c, &spec.update_id)?
                    != (
                        spec.source_digest.clone(),
                        spec.build_digest.clone(),
                        spec.candidate_digest.clone(),
                    )
            {
                return Err(fail(
                    JournalErrorCode::StaleOwner,
                    "Abort admission evidence/owner changed",
                ));
            }
            let r = AbortRecord {
                schema_version: 1,
                updated_at_ms: spec.accepted_at_ms,
                spec: spec.clone(),
                revision: 0,
                phase: AbortPhase::Intent,
                cleanup_verified: false,
                source_sha256: None,
                worker_before: None,
                release_command: None,
                worker_released: None,
                reason: "Abort intent saved before verification/release".into(),
            };
            validate(&r)?;
            one(c
                .execute(
                    "INSERT INTO update_aborts VALUES(?1,?2,0,?3,?4)",
                    params![
                        spec.update_id,
                        encode(&spec)?,
                        encode(&r.phase)?,
                        encode(&r)?
                    ],
                )
                .map_err(storage)?)?;
            one(c
                .execute(
                    "INSERT INTO update_abort_receipts VALUES(?1,0,?2)",
                    params![spec.update_id, encode(&r)?],
                )
                .map_err(storage)?)?;
            if read(c, &spec.update_id)? != r {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Abort intent readback failed",
                ));
            }
            fresh = true;
            Ok(())
        })?;
        Ok((read(&self.connection, &spec.update_id)?, fresh))
    }
    pub(crate) fn advance_abort(
        &mut self,
        a: &UpdateAuthority,
        old: &AbortRecord,
        mut next: AbortRecord,
    ) -> Result<AbortRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if read(&tx, &old.spec.update_id)? != *old
            || a.owner_id != old.spec.owner_id
            || a.epoch != old.spec.owner_epoch
            || record(&tx, &old.spec.update_id)?.phase != UpdatePhase::Aborting
        {
            return Err(fail(
                JournalErrorCode::StaleOwner,
                "Abort owner/state changed",
            ));
        }
        next.revision = old.revision + 1;
        next.updated_at_ms = now_ms()?.max(old.updated_at_ms);
        persist(&tx, old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    pub(crate) fn finish_abort(
        &mut self,
        a: &UpdateAuthority,
        old: &AbortRecord,
        success: bool,
        reason: &str,
    ) -> Result<AbortRecord> {
        let update = record(&self.connection, &old.spec.update_id)?;
        let cmd = UpdateCommand {
            idempotency_key: if success {
                "abort-completed-v1"
            } else {
                "abort-blocked-v1"
            }
            .into(),
            expected_revision: update.revision,
            event: if success {
                UpdateEvent::AbortCompleted
            } else {
                UpdateEvent::Fail
            },
            reason: reason.chars().take(1000).collect(),
        };
        self.advance_with_driver(a, &old.spec.update_id, &cmd, Driver::Abort, |c, _| {
            if read(c, &old.spec.update_id)? != *old
                || old.spec.owner_id != a.owner_id
                || old.spec.owner_epoch != a.epoch
                || success && old.phase != AbortPhase::Released
            {
                return Err(fail(
                    JournalErrorCode::StaleOwner,
                    "Abort terminal evidence changed",
                ));
            }
            let mut next = old.clone();
            next.revision += 1;
            next.updated_at_ms = now_ms()?.max(old.updated_at_ms);
            next.phase = if success {
                AbortPhase::Completed
            } else {
                AbortPhase::Blocked
            };
            next.reason = cmd.reason.clone();
            persist(c, old, &next)?;
            if success {
                source::abort_prepared(c, &old.spec.update_id)?;
                supervisor::stop_after_abort(c, a, &old.spec.update_id)?;
            }
            Ok(())
        })?;
        read(&self.connection, &old.spec.update_id)
    }
    pub fn abort_release_authorized(
        directory: &Path,
        id: &str,
        owner: &str,
        epoch: u64,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
    ) -> Result<bool> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, id)?;
        Ok(
            winbridge_platform::source_files::directory_identity(Path::new(
                &r.spec.runtime_directory,
            ))
            .map_err(storage)?
                == r.spec.runtime_identity
                && record(&tx, id)?.phase == UpdatePhase::Aborting
                && r.spec.owner_id == owner
                && r.spec.owner_epoch == epoch
                && matches!(r.phase, AbortPhase::Verified | AbortPhase::Released)
                && r.cleanup_verified
                && r.worker_before.as_ref().is_some_and(|l| &l.spec == spec)
                && r.release_command.as_ref() == Some(command)
                && evidence(&tx, id)?
                    == (
                        r.spec.source_digest,
                        r.spec.build_digest,
                        r.spec.candidate_digest,
                    ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Connection, AbortRecord) {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE updates(id TEXT PRIMARY KEY);CREATE TABLE source_transactions(id TEXT,phase TEXT);CREATE TRIGGER source_identity BEFORE UPDATE ON source_transactions BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        migrate(&c).unwrap();
        let r = AbortRecord {
            schema_version: 1,
            spec: AbortSpec {
                update_id: Uuid::new_v4().to_string(),
                owner_id: Uuid::new_v4().to_string(),
                owner_epoch: 1,
                original_phase: UpdatePhase::Requested,
                runtime_directory: "local-root".into(),
                runtime_identity: "root-identity".into(),
                source_digest: None,
                build_digest: None,
                candidate_digest: None,
                accepted_at_ms: 1,
            },
            revision: 0,
            phase: AbortPhase::Intent,
            cleanup_verified: false,
            source_sha256: None,
            worker_before: None,
            release_command: None,
            worker_released: None,
            updated_at_ms: 1,
            reason: "intent".into(),
        };
        c.execute("INSERT INTO updates VALUES(?1)", [&r.spec.update_id])
            .unwrap();
        c.execute(
            "INSERT INTO update_aborts VALUES(?1,?2,0,?3,?4)",
            params![
                r.spec.update_id,
                encode(&r.spec).unwrap(),
                encode(&r.phase).unwrap(),
                encode(&r).unwrap()
            ],
        )
        .unwrap();
        c.execute(
            "INSERT INTO update_abort_receipts VALUES(?1,0,?2)",
            params![r.spec.update_id, encode(&r).unwrap()],
        )
        .unwrap();
        assert_eq!(read(&c, &r.spec.update_id).unwrap(), r);
        (c, r)
    }
    fn next(r: &AbortRecord, phase: AbortPhase) -> AbortRecord {
        let mut n = r.clone();
        n.phase = phase;
        n.revision += 1;
        n.updated_at_ms += 1;
        n.cleanup_verified = true;
        n
    }
    #[test]
    fn abort_history_requires_receipts_and_freezes_verified_evidence() {
        let (c, intent) = fixture();
        let verified = next(&intent, AbortPhase::Verified);
        persist(&c, &intent, &verified).unwrap();
        let mut released = next(&verified, AbortPhase::Released);
        released.source_sha256 = Some("a".repeat(64));
        assert!(persist(&c, &verified, &released).is_err());
        assert_eq!(read(&c, &intent.spec.update_id).unwrap(), verified);
        released.source_sha256 = None;
        persist(&c, &verified, &released).unwrap();
        assert!(c
            .execute("DELETE FROM update_abort_receipts WHERE revision=1", [])
            .is_err());
        c.execute_batch("DROP TRIGGER abort_receipt_retained; DELETE FROM update_abort_receipts WHERE revision=1;").unwrap();
        assert_eq!(
            read(&c, &intent.spec.update_id).unwrap_err().code,
            JournalErrorCode::Corrupt
        );
    }
    #[test]
    fn ignored_abort_write_never_returns_a_committed_proof() {
        let (c, intent) = fixture();
        let verified = next(&intent, AbortPhase::Verified);
        c.execute_batch("CREATE TRIGGER injected BEFORE UPDATE ON update_aborts BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(persist(&c, &intent, &verified).is_err());
        assert_eq!(read(&c, &intent.spec.update_id).unwrap(), intent);
        assert_eq!(
            c.query_row("SELECT count(*) FROM update_abort_receipts", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn terminal_abort_is_retained_and_never_becomes_a_new_intent() {
        let (c, intent) = fixture();
        let verified = next(&intent, AbortPhase::Verified);
        persist(&c, &intent, &verified).unwrap();
        let released = next(&verified, AbortPhase::Released);
        persist(&c, &verified, &released).unwrap();
        let completed = next(&released, AbortPhase::Completed);
        persist(&c, &released, &completed).unwrap();
        assert!(persist(&c, &completed, &next(&completed, AbortPhase::Intent)).is_err());
        assert!(c.execute("DELETE FROM update_aborts", []).is_err());
        assert_eq!(read(&c, &intent.spec.update_id).unwrap(), completed);
    }
    #[test]
    fn readonly_abort_snapshot_never_combines_header_and_receipts_from_different_commits() {
        for use_snapshot in [false, true] {
            let (c, intent) = fixture();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("updates.sqlite");
            c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
                .unwrap();
            let mut writer = Connection::open(&path).unwrap();
            writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA application_id=1463965008; PRAGMA user_version=7;").unwrap();
            let verified = next(&intent, AbortPhase::Verified);
            let mut commit_new = || {
                let tx = writer.transaction().unwrap();
                persist(&tx, &intent, &verified).unwrap();
                tx.commit().unwrap();
            };
            let result = if use_snapshot {
                inspect_snapshot(dir.path(), &intent.spec.update_id, commit_new)
            } else {
                let reader = connection(&path, true).unwrap();
                read_with_interleave(&reader, &intent.spec.update_id, &mut commit_new)
            };
            if use_snapshot {
                assert_eq!(result.unwrap(), intent);
            } else {
                assert_eq!(result.unwrap_err().code, JournalErrorCode::Corrupt);
            }
            assert_eq!(
                UpdateJournal::inspect_abort(dir.path(), &intent.spec.update_id).unwrap(),
                verified
            );
        }
    }
}
