//! Health intent/probe/result CAS. No persisted row can mint a launch permit.
use super::*;
use crate::health::*;
use winbridge_protocol::update::{SupervisorRecord, UpdateEvent};
const KEY: &str = "candidate-health-admit-v1";
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE candidate_health(id TEXT PRIMARY KEY REFERENCES candidate_artifact_sets(id),revision INTEGER NOT NULL CHECK(revision>=0),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER health_intent_immutable BEFORE UPDATE ON candidate_health WHEN json_extract(NEW.record,'$.intent') IS NOT json_extract(OLD.record,'$.intent') BEGIN SELECT RAISE(ABORT,'Health intent immutable'); END;
CREATE TRIGGER health_identity BEFORE UPDATE ON candidate_health WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1 OR json_extract(OLD.record,'$.result') IS NOT NULL BEGIN SELECT RAISE(ABORT,'Health identity/revision/terminal immutable'); END;
CREATE TRIGGER health_retained BEFORE DELETE ON candidate_health BEGIN SELECT RAISE(ABORT,'Health retained'); END;
PRAGMA user_version=10;").map_err(storage)
}
fn bad(message: &str) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<HealthRecord>> {
    if schema(c)? < 10 {
        return Ok(None);
    }
    let saved: Option<(i64, String)> = c
        .query_row(
            "SELECT revision,record FROM candidate_health WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    let Some((revision, text)) = saved else {
        return Ok(None);
    };
    let h: HealthRecord = decode(&text)?;
    let intent = &h.intent;
    let current = record(c, id)?;
    let artifacts = candidate::read(c, id)?;
    let preimages = runtime_preimages::read(c, id)?
        .receipt
        .ok_or_else(|| bad("Health without verified preimages"))?;
    let sup = supervisor::read(c, &intent.launch_id)?;
    if h.schema_version != 1
        || h.revision != counter(revision)?
        || h.revision > MAX_UPDATE_COUNTER
        || intent.schema_version != 1
        || intent.update_id != id
        || Uuid::parse_str(&intent.attempt_id).is_err()
        || intent.owner_id != artifacts.owner_id
        || intent.owner_epoch != artifacts.owner_epoch
        || intent.artifacts_sha256 != digest(&artifacts).map_err(storage)?
        || intent.preimages_sha256 != digest(&preimages).map_err(storage)?
        || intent.parent_revision > MAX_UPDATE_COUNTER
        || !std::path::Path::new(&intent.worker_config_path).is_absolute()
        || intent.parent_revision != artifacts.parent_revision + 1
        || intent.parent_revision > current.revision
        || intent.admitted_at_ms > MAX_UPDATE_COUNTER
        || sup.spec.update_id != id
        || sup.owner_id.as_deref() != Some(intent.owner_id.as_str())
        || sup.owner_epoch != Some(intent.owner_epoch)
        || intent.directory
            != directory(&sup.spec.runtime_directory, id)
                .to_str()
                .ok_or_else(|| bad("Health path UTF-8"))?
        || h.probes.len() > 2
    {
        return Err(bad(
            "Health intent/artifact/preimages/owner binding inconsistent",
        ));
    }
    let transition_at = runtime_baseline::transition(
        c,
        id,
        intent.parent_revision,
        winbridge_protocol::update::UpdateEvent::CandidateHealthAdmitted,
        KEY,
        &intent.owner_id,
        intent.owner_epoch,
    )?;
    if intent.admitted_at_ms > transition_at {
        return Err(bad("Health intent time exceeds transition"));
    }
    let binding = sup
        .spec
        .worker
        .as_ref()
        .ok_or_else(|| bad("Health worker binding missing"))?;
    if let Some(context) = &h.context {
        context
            .compatibility
            .validate_binding(binding)
            .map_err(|e| bad(&e))?;
        let baseline = runtime_baseline::read(c, id)?
            .receipt
            .ok_or_else(|| bad("Health baseline missing"))?;
        if context.compatibility.identity != baseline.loaded_worker
            || context.compatibility.observed_at_ms < intent.admitted_at_ms
            || context.compatibility.updater_journal_schema > schema(c)?
        {
            return Err(bad(
                "Health context differs from original loaded worker baseline",
            ));
        }
        if context.artifacts.inspection.artifacts.len() != 4
            || context.artifacts != artifacts
            || context.worker_config.path != intent.worker_config_path
            || context.worker_config.sha256 != binding.config_sha256
            || context.worker_config.bytes == 0
            || context.worker_config.bytes > 65536
            || context.worker_config.file_identity.is_empty()
        {
            return Err(bad("Health context differs from artifact/config scope"));
        }
    } else if !h.probes.is_empty() {
        return Err(bad("Probe without health context"));
    }
    for (index, p) in h.probes.iter().enumerate() {
        let expected = if index == 0 {
            HealthProbeKind::Config
        } else {
            HealthProbeKind::Rehearsal
        };
        if index > 0 && !crate::health::completed(&h.probes[index - 1]) {
            return Err(bad("Second probe without first complete"));
        }
        if p.kind != expected
            || p.job_name != job(intent, expected)
            || Some(&p.command)
                != h.context
                    .as_ref()
                    .map(|ctx| crate::health::command(intent, ctx, expected))
                    .as_ref()
            || (p.resume_granted && p.process.is_none())
            || (p.resume_acknowledged && !p.resume_granted)
        {
            return Err(bad("Health probe command/PID/grant inconsistent"));
        }
        if p.process.as_ref().is_some_and(|p| {
            p.process_id == 0
                || p.birth.is_empty()
                || !p.birth.bytes().all(|b| b.is_ascii_digit())
                || !valid_hash(&p.host_id, &[64])
        }) {
            return Err(bad("Health process identity invalid"));
        }
        if let Some(cleanup) = &p.cleanup {
            if cleanup.failure.as_ref().is_some_and(|s| s.len() > 4096)
                || cleanup.job_name != p.job_name
                || cleanup.process != p.process
                || (cleanup.creation_confirmed && cleanup.process.is_none())
                || (cleanup.resume_requested && !p.resume_granted)
                || (cleanup.no_process_created
                    && (cleanup.creation_confirmed
                        || cleanup.process.is_some()
                        || cleanup.resume_requested))
                || (cleanup.failure.is_none()
                    && (!cleanup.creation_confirmed
                        || !cleanup.resume_requested
                        || !cleanup.tree_empty
                        || !cleanup.capture_complete
                        || cleanup.output_exceeded
                        || cleanup.exit_code != Some(0)))
            {
                return Err(bad("Health cleanup evidence inconsistent"));
            }
        }
    }
    if let Some(result) = &h.result {
        if result.completed_at_ms < intent.admitted_at_ms
            || result.completed_at_ms > MAX_UPDATE_COUNTER
            || result.reason.is_empty()
            || result.reason.len() > 4096
        {
            return Err(bad("Health result time/reason invalid"));
        }
        if result.outcome == HealthOutcome::Verified {
            crate::health::validate_success(&h).map_err(|e| bad(&e))?;
            let revision = result
                .parent_revision
                .ok_or_else(|| bad("Health result transition missing"))?;
            let at = runtime_baseline::transition(
                c,
                id,
                revision,
                winbridge_protocol::update::UpdateEvent::CandidateAccepted,
                "candidate-health-verified-v1",
                &intent.owner_id,
                intent.owner_epoch,
            )?;
            if revision != intent.parent_revision + 1
                || revision > current.revision
                || result.completed_at_ms > at
            {
                return Err(bad("Health acceptance transition inconsistent"));
            }
        } else if result.parent_revision.is_some() {
            return Err(bad("Unverified health cannot accept candidate"));
        }
    }
    Ok(Some(h))
}
fn require(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    h: &HealthRecord,
    cleanup: bool,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let p = record(c, &h.intent.update_id)?;
    if h.intent.owner_id != a.owner_id()
        || h.intent.owner_epoch != a.epoch()
        || h.intent.launch_id != s.spec.launch_id
        || h.result.is_some()
        || !matches!(
            p.phase,
            UpdatePhase::ValidatingCandidate | UpdatePhase::Aborting
        )
        || (!cleanup
            && (p.phase != UpdatePhase::ValidatingCandidate
                || p.revision != h.intent.parent_revision))
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Fresh pending health authority required; no replay",
        ));
    }
    Ok(())
}
/// advance_with_driver has already written the new root/transition in its open
/// transaction. The pending-phase guard belongs BEFORE that transition; inside
/// the hook require the exact acceptance edge instead of the old parent phase.
pub(super) fn require_acceptance(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
    h: &HealthRecord,
    parent: &UpdateRecord,
) -> Result<()> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    if h.intent.owner_id != a.owner_id()
        || h.intent.owner_epoch != a.epoch()
        || h.intent.launch_id != s.spec.launch_id
        || h.result.is_some()
        || parent.update_id != h.intent.update_id
        || parent.phase != UpdatePhase::CandidateVerified
        || h.intent.parent_revision.checked_add(1) != Some(parent.revision)
        || record(c, &h.intent.update_id)? != *parent
    {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Health acceptance requires its exact committed parent edge in this transaction",
        ));
    }
    runtime_baseline::transition(
        c,
        &h.intent.update_id,
        parent.revision,
        UpdateEvent::CandidateAccepted,
        "candidate-health-verified-v1",
        a.owner_id(),
        a.epoch(),
    )?;
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_health(directory: &Path, id: &str) -> Result<Option<HealthRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn health(&self, id: &str) -> Result<Option<HealthRecord>> {
        read(&self.connection, id)
    }
    pub(crate) fn admit_health(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        intent: HealthIntent,
    ) -> Result<HealthRecord> {
        self.check_directory(a)?;
        let previous = self.get(&intent.update_id)?;
        let command = UpdateCommand {
            idempotency_key: KEY.into(),
            expected_revision: previous.revision,
            event: UpdateEvent::CandidateHealthAdmitted,
            reason:
                "Проверка совместимости и запуска кандидата принята; установка ещё не разрешена"
                    .into(),
        };
        let h = HealthRecord {
            schema_version: 1,
            revision: 0,
            intent,
            context: None,
            probes: vec![],
            result: None,
        };
        self.advance_with_driver(a, &h.intent.update_id, &command, Driver::Health, |c, _| {
            runtime_baseline::current_supervisor(c, a, s)?;
            let artifacts = candidate::read(c, &h.intent.update_id)?;
            if artifacts.owner_id != a.owner_id() || artifacts.owner_epoch != a.epoch() {
                return Err(fail(JournalErrorCode::StaleOwner, "Artifact owner changed"));
            }
            one(c
                .execute(
                    "INSERT INTO candidate_health VALUES(?1,0,?2)",
                    params![h.intent.update_id, encode(&h)?],
                )
                .map_err(storage)?)?;
            if read(c, &h.intent.update_id)?.as_ref() != Some(&h) {
                return Err(bad("Health intent write ignored"));
            }
            Ok(())
        })?;
        self.health(&h.intent.update_id)?
            .ok_or_else(|| bad("Health intent missing after admission"))
    }
    pub(crate) fn update_health(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        old: &HealthRecord,
        mut next: HealthRecord,
        cleanup: bool,
    ) -> Result<HealthRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require(&tx, a, s, old, cleanup)?;
        if read(&tx, &old.intent.update_id)?.as_ref() != Some(old) || next.intent != old.intent {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Health revision changed",
            ));
        }
        crate::health::validate_step(old, &next).map_err(|e| bad(&e))?;
        if next
            .result
            .as_ref()
            .is_some_and(|r| r.outcome == HealthOutcome::Verified)
        {
            return Err(bad("Acceptance requires atomic parent transition"));
        }
        next.revision = old
            .revision
            .checked_add(1)
            .filter(|r| *r <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| bad("Health revision exhausted"))?;
        one(tx
            .execute(
                "UPDATE candidate_health SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",
                params![
                    next.revision as i64,
                    encode(&next)?,
                    old.intent.update_id,
                    old.revision as i64
                ],
            )
            .map_err(storage)?)?;
        if read(&tx, &old.intent.update_id)?.as_ref() != Some(&next) {
            return Err(bad("Health record write ignored"));
        }
        tx.commit().map_err(storage)?;
        Ok(next)
    }
    pub(crate) fn accept_health(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        old: &HealthRecord,
        mut next: HealthRecord,
    ) -> Result<HealthRecord> {
        self.check_directory(a)?;
        require(&self.connection, a, s, old, false)?;
        let p = self.get(&old.intent.update_id)?;
        crate::health::validate_step(old, &next).map_err(|e| bad(&e))?;
        crate::health::validate_fresh_success(&next).map_err(|e| bad(&e))?;
        let result = next
            .result
            .as_mut()
            .ok_or_else(|| bad("Health result missing"))?;
        result.parent_revision = Some(p.revision + 1);
        next.revision = old.revision + 1;
        let command=UpdateCommand{idempotency_key:"candidate-health-verified-v1".into(),expected_revision:p.revision,event:UpdateEvent::CandidateAccepted,reason:"Конфигурация и изолированный запуск кандидата проверены; процессы завершены, установка ожидает drain".into()};
        self.advance_with_driver(a,&old.intent.update_id,&command,Driver::Health,|c,parent| {
            require_acceptance(c,a,s,old,parent)?;
            if read(c,&old.intent.update_id)?.as_ref()!=Some(old) || next.intent!=old.intent {return Err(fail(JournalErrorCode::RevisionConflict,"Health acceptance changed"));}
            one(c.execute("UPDATE candidate_health SET revision=?1,record=?2 WHERE id=?3 AND revision=?4",params![next.revision as i64,encode(&next)?,old.intent.update_id,old.revision as i64]).map_err(storage)?)?;
            if read(c,&old.intent.update_id)?.as_ref()!=Some(&next){return Err(bad("Health acceptance write ignored"));}Ok(())
        })?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn health_schema_nine_migration_backs_up_history_fences_owner_and_invents_no_result() {
        let temp = tempfile::tempdir().unwrap();
        let scope = "a".repeat(64);
        let mut j = UpdateJournal::open(temp.path(), &scope).unwrap();
        let owner = j.acquire_owner().unwrap();
        let epoch = owner.epoch();
        drop(owner);
        j.connection.execute_batch("DROP TABLE drain_history; DROP TABLE drain_commands; DROP TABLE drain_intents; DROP TABLE candidate_health; PRAGMA user_version=9; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(j);
        let mut j = UpdateJournal::open(temp.path(), &scope).unwrap();
        assert_eq!(j.info().unwrap().schema_version, UPDATE_SCHEMA_VERSION);
        assert!(j.info().unwrap().owner_epoch > epoch);
        let count: i64 = j
            .connection
            .query_row("SELECT count(*) FROM candidate_health", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let backup = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("updates-v9-")
            })
            .unwrap();
        let c = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(schema(&c).unwrap(), 9);
        assert_eq!(
            c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}

#[cfg(test)]
mod bypass_tests {
    use super::*;
    fn request(j: &mut UpdateJournal) -> winbridge_protocol::update::UpdateRequest {
        let i = j.info().unwrap();
        winbridge_protocol::update::UpdateRequest {
            contract_version: 1,
            idempotency_key: "health-raw-fixture".into(),
            source: winbridge_protocol::update::UpdateSourcePin {
                archive_name: "fixture.zip".into(),
                archive_sha256: "a".repeat(64),
                manifest_sha256: "b".repeat(64),
                source_commit: "c".repeat(40),
            },
            expected: winbridge_protocol::update::UpdateInstallationPin {
                installation_id: i.installation_id,
                scope_sha256: i.scope_sha256,
                worker_build_id: "fixture".into(),
                worker_config_sha256: "d".repeat(64),
                baseline_manifest_sha256: "e".repeat(64),
            },
        }
    }
    #[test]
    fn health_raw_events_cannot_approve_or_admit_helpers_without_trusted_receipts() {
        let temp = tempfile::tempdir().unwrap();
        let mut j = UpdateJournal::open(temp.path(), &"a".repeat(64)).unwrap();
        let a = j.acquire_owner().unwrap();
        let req = request(&mut j);
        let mut r = j.accept(&a, &req, &req.expected).unwrap();
        // Metadata-only old lifecycle fixture; no compiler or executable health is claimed.
        for (n, event) in [
            UpdateEvent::BeginPreparation,
            UpdateEvent::SourceVerified,
            UpdateEvent::BuildSucceeded,
        ]
        .into_iter()
        .enumerate()
        {
            r = j
                .advance(
                    &a,
                    &r.update_id,
                    &UpdateCommand {
                        idempotency_key: format!("fixture-{n}"),
                        expected_revision: r.revision,
                        event,
                        reason: "Metadata fixture".into(),
                    },
                )
                .unwrap();
        }
        assert_eq!(r.phase, UpdatePhase::ValidatingCandidate);
        let before = j.info().unwrap().last_sequence;
        for event in [
            UpdateEvent::CandidateHealthAdmitted,
            UpdateEvent::CandidateAccepted,
        ] {
            assert!(j
                .advance(
                    &a,
                    &r.update_id,
                    &UpdateCommand {
                        idempotency_key: format!("raw-{event:?}"),
                        expected_revision: r.revision,
                        event,
                        reason: "No typed receipt".into()
                    }
                )
                .is_err());
            assert_eq!(j.get(&r.update_id).unwrap(), r);
            assert!(j.health(&r.update_id).unwrap().is_none());
        }
        assert_eq!(j.info().unwrap().last_sequence, before);
        for schema in [1, 2, 3, 4] {
            let v = match schema {
                1 => UpdateJournal::preparation_snapshot(temp.path(), &r.update_id, false, None),
                2 => UpdateJournal::runtime_preparation_snapshot(
                    temp.path(),
                    &r.update_id,
                    false,
                    None,
                ),
                3 => UpdateJournal::preimages_preparation_snapshot(
                    temp.path(),
                    &r.update_id,
                    false,
                    None,
                ),
                _ => UpdateJournal::health_preparation_snapshot(
                    temp.path(),
                    &r.update_id,
                    false,
                    None,
                ),
            }
            .unwrap();
            let json = serde_json::to_value(&v).unwrap();
            assert_eq!(json.get("candidateHealth").is_some(), schema == 4);
            if schema == 4 {
                assert_eq!(
                    v.candidate_health.unwrap().phase,
                    winbridge_protocol::update::CandidateHealthPhase::NotAdmitted
                );
            }
        }
        drop(a);
        drop(j);
        let mut j = UpdateJournal::open(temp.path(), &"a".repeat(64)).unwrap();
        assert_eq!(j.get(&r.update_id).unwrap(), r);
        assert!(j.health(&r.update_id).unwrap().is_none());
    }
}
