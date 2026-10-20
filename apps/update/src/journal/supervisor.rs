use super::*;
use winbridge_protocol::update::*;
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE supervisor_launches(update_id TEXT PRIMARY KEY REFERENCES updates(id),launch_id TEXT NOT NULL UNIQUE,spec TEXT NOT NULL,revision INTEGER NOT NULL,active INTEGER NOT NULL CHECK(active IN(0,1)),record TEXT NOT NULL CHECK(json_valid(record)));
 CREATE UNIQUE INDEX single_supervisor ON supervisor_launches(active) WHERE active=1;
 CREATE TABLE supervisor_receipts(launch_id TEXT NOT NULL REFERENCES supervisor_launches(launch_id),phase TEXT NOT NULL,record TEXT NOT NULL,PRIMARY KEY(launch_id,phase));
 CREATE TRIGGER supervisor_identity BEFORE UPDATE ON supervisor_launches WHEN NEW.update_id!=OLD.update_id OR NEW.launch_id!=OLD.launch_id OR NEW.spec!=OLD.spec OR NEW.revision!=OLD.revision+1 OR OLD.active=0 BEGIN SELECT RAISE(ABORT,'Supervisor identity/revision is immutable'); END;
 CREATE TRIGGER supervisor_retained BEFORE DELETE ON supervisor_launches BEGIN SELECT RAISE(ABORT,'Supervisor history is retained'); END;
 CREATE TRIGGER supervisor_receipt_immutable BEFORE UPDATE ON supervisor_receipts BEGIN SELECT RAISE(ABORT,'Supervisor receipt immutable'); END;
 CREATE TRIGGER supervisor_receipt_retained BEFORE DELETE ON supervisor_receipts BEGIN SELECT RAISE(ABORT,'Supervisor receipt retained'); END;
 PRAGMA user_version=3;").map_err(storage)
}
pub(super) fn active(c: &Connection) -> Result<bool> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM supervisor_launches WHERE active=1)",
        [],
        |r| r.get(0),
    )
    .map_err(storage)
}
fn is_active(phase: SupervisorPhase) -> bool {
    phase != SupervisorPhase::Stopped
}
pub(super) fn read(c: &Connection, id: &str) -> Result<SupervisorRecord> {
    let row:Option<(String,String,i64,i64)>=c.query_row("SELECT record,spec,revision,active FROM supervisor_launches WHERE update_id=?1 OR launch_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage)?;
    let (json, spec, rev, active) =
        row.ok_or_else(|| fail(JournalErrorCode::NotFound, "Supervisor launch not found"))?;
    let r: SupervisorRecord = decode(&json)?;
    if r.schema_version != 1
        || encode(&r.spec)? != spec
        || counter(rev)? != r.revision
        || r.revision > MAX_UPDATE_COUNTER
        || r.updated_at_ms < r.spec.accepted_at_ms
        || r.updated_at_ms > MAX_UPDATE_COUNTER
        || is_active(r.phase) != (active == 1)
        || Uuid::parse_str(&r.spec.launch_id).is_err()
        || Uuid::parse_str(&r.spec.update_id).is_err()
        || r.spec.parent_epoch >= MAX_UPDATE_COUNTER
        || Uuid::parse_str(&r.spec.parent_owner_id).is_err()
        || !valid_hash(&r.spec.image_sha256, &[64])
        || !valid_hash(&r.spec.config_sha256, &[64])
        || r.spec.parent.process_id == 0
        || r.spec.parent.birth.is_empty()
        || !r.spec.parent.birth.bytes().all(|b| b.is_ascii_digit())
        || !valid_hash(&r.spec.parent.host_id, &[64])
        || r.owner_epoch.is_some_and(|e| e > MAX_UPDATE_COUNTER)
        || r.owner_id
            .as_ref()
            .is_some_and(|s| Uuid::parse_str(s).is_err())
        || (matches!(
            r.phase,
            SupervisorPhase::Ready | SupervisorPhase::Active | SupervisorPhase::Stopped
        ) && (r.owner_id.is_none() || r.owner_epoch != Some(r.spec.parent_epoch + 1)))
        || !valid_hash(&r.spec.token_sha256, &[64])
        || r.process.is_none()
            && matches!(
                r.phase,
                SupervisorPhase::Spawned | SupervisorPhase::Ready | SupervisorPhase::Active
            )
        || r.reason.len() > 4096
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Supervisor record/binding mismatch",
        ));
    }
    record(c, &r.spec.update_id)?;
    Ok(r)
}
fn persist(c: &Connection, old: &SupervisorRecord, next: &SupervisorRecord) -> Result<()> {
    one(c.execute("UPDATE supervisor_launches SET revision=?1,active=?2,record=?3 WHERE launch_id=?4 AND revision=?5",params![next.revision as i64,is_active(next.phase) as i64,encode(next)?,next.spec.launch_id,old.revision as i64]).map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO supervisor_receipts VALUES(?1,?2,?3)",
            params![next.spec.launch_id, encode(&next.phase)?, encode(next)?],
        )
        .map_err(storage)?)?;
    if read(c, &next.spec.launch_id)? != *next {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Supervisor receipt readback mismatch",
        ));
    }
    Ok(())
}
pub(super) fn stop_after_abort(c: &Connection, a: &UpdateAuthority, id: &str) -> Result<()> {
    let old = read(c, id)?;
    if old.phase != SupervisorPhase::Active
        || old.owner_id.as_deref() != Some(a.owner_id.as_str())
        || old.owner_epoch != Some(a.epoch)
    {
        return Err(fail(
            JournalErrorCode::StaleOwner,
            "Abort completion requires its active supervisor",
        ));
    }
    let mut next = old.clone();
    next.phase = SupervisorPhase::Stopped;
    next.revision = next
        .revision
        .checked_add(1)
        .filter(|v| *v <= MAX_UPDATE_COUNTER)
        .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Supervisor revision exhausted"))?;
    next.updated_at_ms = now_ms()?.max(old.updated_at_ms);
    next.reason = "Отмена до установки подтверждена; worker lease освобождена".into();
    persist(c, &old, &next)
}
impl UpdateJournal {
    pub fn preparation_snapshot(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        Self::preparation_snapshot_inner(directory, id, driver_active, driver_error, 1)
    }
    /// Internal v7 projection: baseline and parent use one SQLite read snapshot.
    pub fn runtime_preparation_snapshot(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        Self::preparation_snapshot_inner(directory, id, driver_active, driver_error, 2)
    }
    /// V8 projection: baseline, preimages and parent share one SQLite snapshot.
    pub fn preimages_preparation_snapshot(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        Self::preparation_snapshot_inner(directory, id, driver_active, driver_error, 3)
    }
    /// Health evidence and all preceding receipts share the same readonly snapshot.
    pub fn health_preparation_snapshot(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        Self::preparation_snapshot_inner(directory, id, driver_active, driver_error, 4)
    }
    /// V13/view5: drain and all preceding evidence share one SQLite snapshot.
    pub fn drain_preparation_snapshot(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
    ) -> Result<UpdatePreparationView> {
        Self::preparation_snapshot_inner(directory, id, driver_active, driver_error, 5)
    }
    fn preparation_snapshot_inner(
        directory: &Path,
        id: &str,
        driver_active: bool,
        driver_error: Option<String>,
        view_schema: u16,
    ) -> Result<UpdatePreparationView> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? != UPDATE_SCHEMA_VERSION {
            return Err(fail(
                JournalErrorCode::UnsupportedSchema,
                "Preparation view requires the current updater schema",
            ));
        }
        let tx = c.transaction().map_err(storage)?;
        let update = record(&tx, id)?;
        let source = match source::header(&tx, id) {
            Ok(value) => Some(value),
            Err(e) if e.code == JournalErrorCode::NotFound => None,
            Err(e) => return Err(e),
        };
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM source_file_receipts WHERE update_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let count = counter(count)?;
        if count > source.as_ref().map_or(0, |s| u64::from(s.total_files)) {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Invalid source receipt count",
            ));
        }
        let build = match build::read(&tx, id) {
            Ok(value) => Some(value),
            Err(e) if e.code == JournalErrorCode::NotFound => None,
            Err(e) => return Err(e),
        };
        let candidate_drain = if view_schema >= 5 {
            let released = if matches!(
                update.phase,
                UpdatePhase::Aborting | UpdatePhase::Deferred | UpdatePhase::FailedBeforeCutover
            ) {
                super::abort::released_after_abort(&tx, id)?
            } else {
                false
            };
            Some(crate::drain::view(
                &super::drain::read(&tx, id)?,
                update.phase,
                driver_active,
                released,
            ))
        } else {
            None
        };
        let view = UpdatePreparationView {
            schema_version: view_schema,
            observed_at_ms: now_ms()?,
            update,
            source,
            source_applied_files: count as u32,
            build,
            driver_active,
            driver_error,
            candidate_drain,
            runtime_baseline: if view_schema >= 2 {
                Some(super::runtime_baseline::read(&tx, id)?.view())
            } else {
                None
            },
            runtime_preimages: if view_schema >= 3 {
                Some(super::runtime_preimages::read(&tx, id)?.view())
            } else {
                None
            },
            candidate_health: if view_schema >= 4 {
                Some(crate::health::view(super::health::read(&tx, id)?.as_ref()))
            } else {
                None
            },
        };
        tx.commit().map_err(storage)?;
        Ok(view)
    }
    pub fn supervisor(&mut self, id: &str) -> Result<SupervisorRecord> {
        read(&self.connection, id)
    }
    pub fn inspect_supervisor(directory: &Path, id: &str) -> Result<SupervisorRecord> {
        Self::supervisor_snapshot(directory, id).map(|(_, r, _)| r)
    }
    pub fn supervisor_snapshot(
        directory: &Path,
        id: &str,
    ) -> Result<(UpdateJournalInfo, SupervisorRecord, UpdateRecord)> {
        Self::supervisor_authority_snapshot(directory, id).map(|(i, s, u, _)| (i, s, u))
    }
    pub fn supervisor_authority_snapshot(
        directory: &Path,
        id: &str,
    ) -> Result<(UpdateJournalInfo, SupervisorRecord, UpdateRecord, String)> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? < 3 {
            return Err(fail(
                JournalErrorCode::UnsupportedSchema,
                "Supervisor inspection needs schema 3",
            ));
        }
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, id)?;
        let state = info(&tx)?;
        let update = record(&tx, &r.spec.update_id)?;
        let owner: String = tx
            .query_row(
                "SELECT owner FROM installation WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok((state, r, update, owner))
    }
    pub(crate) fn prepare_supervisor(
        &mut self,
        a: &UpdateAuthority,
        spec: SupervisorSpec,
    ) -> Result<SupervisorRecord> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if spec.parent_epoch != a.epoch
            || spec.parent_owner_id != a.owner_id
            || record(&tx, &spec.update_id)?.phase != UpdatePhase::Requested
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Supervisor needs current owner and Requested",
            ));
        }
        let r = SupervisorRecord {
            schema_version: 1,
            updated_at_ms: spec.accepted_at_ms,
            spec,
            phase: SupervisorPhase::Intent,
            revision: 0,
            process: None,
            owner_id: None,
            owner_epoch: None,
            reason: "Запуск отдельного updater подготовлен; файловые действия не разрешены".into(),
        };
        one(tx
            .execute(
                "INSERT INTO supervisor_launches VALUES(?1,?2,?3,0,1,?4)",
                params![
                    r.spec.update_id,
                    r.spec.launch_id,
                    encode(&r.spec)?,
                    encode(&r)?
                ],
            )
            .map_err(storage)?)?;
        one(tx
            .execute(
                "INSERT INTO supervisor_receipts VALUES(?1,?2,?3)",
                params![r.spec.launch_id, encode(&r.phase)?, encode(&r)?],
            )
            .map_err(storage)?)?;
        if read(&tx, &r.spec.launch_id)? != r {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Supervisor intent readback mismatch",
            ));
        }
        tx.commit().map_err(storage)?;
        Ok(r)
    }
    pub(crate) fn advance_supervisor(
        &mut self,
        a: &UpdateAuthority,
        id: &str,
        expected: u64,
        phase: SupervisorPhase,
        process: Option<SupervisorProcess>,
        reason: &str,
    ) -> Result<SupervisorRecord> {
        if expected > MAX_UPDATE_COUNTER {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Supervisor revision exceeds supported counter",
            ));
        }
        let terminal_stop = phase == SupervisorPhase::Stopped
            && matches!(
                record(
                    &self.connection,
                    &read(&self.connection, id)?.spec.update_id
                )?
                .phase,
                UpdatePhase::ManualIntervention | UpdatePhase::FailedBeforeCutover
            );
        if phase == SupervisorPhase::Stopped && !terminal_stop {
            let old = read(&self.connection, id)?;
            let update = record(&self.connection, &old.spec.update_id)?;
            let command = UpdateCommand {
                idempotency_key: format!("supervisor-stop-{}", old.spec.launch_id),
                expected_revision: update.revision,
                event: UpdateEvent::Fail,
                reason: reason.into(),
            };
            self.advance_with_source(a, &old.spec.update_id, &command, true, |c, _| {
                let current = read(c, id)?;
                if current.revision != expected
                    || !matches!(
                        current.phase,
                        SupervisorPhase::Ready | SupervisorPhase::Active
                    )
                    || current.owner_epoch != Some(a.epoch)
                    || current.owner_id.as_deref() != Some(a.owner_id.as_str())
                    || process.is_some()
                {
                    return Err(fail(
                        JournalErrorCode::PreconditionsChanged,
                        "Supervisor stop needs exact current owner and no new process",
                    ));
                }
                let mut next = current.clone();
                next.phase = SupervisorPhase::Stopped;
                next.revision = next
                    .revision
                    .checked_add(1)
                    .filter(|n| *n <= MAX_UPDATE_COUNTER)
                    .ok_or_else(|| {
                        fail(JournalErrorCode::Corrupt, "Supervisor revision exhausted")
                    })?;
                next.updated_at_ms = now_ms()?.max(next.updated_at_ms);
                next.reason = reason.into();
                persist(c, &current, &next)
            })?;
            return read(&self.connection, id);
        }
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        let old = read(&tx, id)?;
        let receipt: Option<String> = tx
            .query_row(
                "SELECT record FROM supervisor_receipts WHERE launch_id=?1 AND phase=?2",
                params![old.spec.launch_id, encode(&phase)?],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(json) = receipt {
            let r: SupervisorRecord = decode(&json)?;
            if r.revision != expected + 1 || process.is_some() && r.process != process {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Supervisor phase receipt differs",
                ));
            }
            return Ok(r);
        }
        if old.revision != expected || reason.len() > 4096 {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Supervisor revision/reason changed",
            ));
        }
        if process
            .as_ref()
            .is_some_and(|p| old.process.as_ref().is_some_and(|old| old != p))
        {
            return Err(fail(
                JournalErrorCode::KeyConflict,
                "Supervisor process identity is immutable",
            ));
        }
        let parent = a.epoch == old.spec.parent_epoch && a.owner_id == old.spec.parent_owner_id;
        let child = a.epoch == old.spec.parent_epoch + 1
            && old.owner_id.as_ref().is_none_or(|s| s == &a.owner_id)
            && old.owner_epoch.is_none_or(|e| e == a.epoch);
        let allowed = match (old.phase, phase) {
            (SupervisorPhase::Intent, SupervisorPhase::Spawned) => parent && process.is_some(),
            (
                SupervisorPhase::Intent | SupervisorPhase::Spawned,
                SupervisorPhase::Indeterminate,
            ) => parent,
            (SupervisorPhase::Spawned, SupervisorPhase::Ready) => child,
            (SupervisorPhase::Ready, SupervisorPhase::Active) => child,
            (SupervisorPhase::Ready | SupervisorPhase::Active, SupervisorPhase::Stopped) => child,
            _ => false,
        };
        let update_phase = record(&tx, &old.spec.update_id)?.phase;
        if !allowed
            || !(update_phase == UpdatePhase::Requested
                || phase == SupervisorPhase::Stopped
                    && matches!(
                        update_phase,
                        UpdatePhase::ManualIntervention | UpdatePhase::FailedBeforeCutover
                    ))
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Supervisor transition/owner/effect scope rejected",
            ));
        }
        let mut next = old.clone();
        next.phase = phase;
        next.revision = old
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Supervisor revision exhausted"))?;
        next.updated_at_ms = now_ms()?.max(old.updated_at_ms);
        next.reason = reason.into();
        if process.is_some() {
            next.process = process;
        }
        if phase == SupervisorPhase::Ready {
            next.owner_epoch = Some(a.epoch);
            next.owner_id = Some(a.owner_id.clone());
        }
        persist(&tx, &old, &next)?;
        tx.commit().map_err(storage)?;
        Ok(next)
    }
}

pub(super) fn require_driver(
    c: &Connection,
    a: &UpdateAuthority,
    id: &str,
    event: UpdateEvent,
) -> Result<()> {
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM supervisor_launches WHERE update_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Ok(());
    }
    let r = read(c, id)?;
    let owner =
        r.owner_epoch == Some(a.epoch) && r.owner_id.as_deref() == Some(a.owner_id.as_str());
    if r.phase == SupervisorPhase::Active && owner
        || event == UpdateEvent::Fail
            && ((r.phase == SupervisorPhase::Ready && owner)
                || (matches!(r.phase, SupervisorPhase::Intent | SupervisorPhase::Spawned)
                    && r.spec.parent_epoch == a.epoch
                    && r.spec.parent_owner_id == a.owner_id))
    {
        Ok(())
    } else {
        Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Only the handed-off supervisor may advance update effects; no takeover/replay",
        ))
    }
}
