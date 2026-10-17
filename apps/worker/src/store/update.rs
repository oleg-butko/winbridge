//! Worker-side source/dispatch coordination. The updater journal is a different DB.
use super::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use winbridge_core::update::{advance_worker_update, valid_hash, MAX_UPDATE_COUNTER};
use winbridge_protocol::update::*;

fn encode(value: &(impl serde::Serialize + ?Sized)) -> Result<String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn one(n: usize) -> Result<()> {
    if n != 1 {
        return Err("Worker update write was ignored; no ACK/effect permitted".into());
    }
    Ok(())
}
fn validate(spec: &WorkerUpdateSpec) -> Result<()> {
    for id in [
        &spec.update_id,
        &spec.journal_incarnation,
        &spec.worker_generation,
        &spec.updater_owner_id,
    ] {
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid worker update identity")?;
    }
    if !winbridge_core::config::valid_id(&spec.instance_id)
        || spec.updater_epoch == 0
        || spec.updater_epoch > MAX_UPDATE_COUNTER
        || spec.source_directory.is_empty()
        || spec.source_directory.len() > 4096
        || !valid_hash(&spec.baseline_manifest_sha256, &[64])
        || !valid_hash(&spec.target_manifest_sha256, &[64])
        || spec.projects.len() > 128
        || spec
            .projects
            .iter()
            .any(|p| !winbridge_core::config::valid_id(p))
        || spec.projects.windows(2).any(|p| p[0] >= p[1])
    {
        return Err("Invalid worker update source/scope".into());
    }
    Ok(())
}
fn read(c: &Connection, id: &str) -> Result<WorkerUpdateLease> {
    let row: Option<(String, String, i64, String, bool)> = c
        .query_row(
            "SELECT record,spec,revision,phase,active FROM worker_update_leases WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(sql)?;
    let (json, spec, revision, phase, active) = row.ok_or("Worker update lease not found")?;
    if json.len() > 32 * 1024 {
        return Err("Oversized worker update lease".into());
    }
    let record: WorkerUpdateLease =
        serde_json::from_str(&json).map_err(|_| "Invalid worker update lease")?;
    validate(&record.spec)?;
    if record.spec.update_id != id
        || encode(&record.spec)? != spec
        || i64::try_from(record.revision).ok() != Some(revision)
        || record.revision > MAX_UPDATE_COUNTER
        || encode(&record.phase)? != phase
        || active != (record.phase != WorkerUpdatePhase::Released)
        || record.accepted_at_ms > record.updated_at_ms
        || record.updated_at_ms > MAX_UPDATE_COUNTER
        || record.reason.len() > 4096
    {
        return Err("Worker update lease/index identity mismatch".into());
    }
    Ok(record)
}
fn active(c: &Connection) -> Result<Option<WorkerUpdateLease>> {
    let id: Option<String> = c
        .query_row(
            "SELECT id FROM worker_update_leases WHERE active=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    id.map(|id| read(c, &id)).transpose()
}
fn frozen(phase: WorkerUpdatePhase) -> bool {
    matches!(
        phase,
        WorkerUpdatePhase::Quiescing | WorkerUpdatePhase::Quiesced
    )
}

impl Store {
    pub(crate) fn validate_worker_update_spec(spec: &WorkerUpdateSpec) -> Result<()> {
        validate(spec)
    }
    pub(super) fn migrate_update_coordination(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE worker_update_leases(id TEXT PRIMARY KEY,spec TEXT NOT NULL CHECK(json_valid(spec)),revision INTEGER NOT NULL CHECK(revision>=0),phase TEXT NOT NULL,active INTEGER NOT NULL CHECK(active IN(0,1)),record TEXT NOT NULL CHECK(json_valid(record)));
            CREATE UNIQUE INDEX one_worker_update ON worker_update_leases(active) WHERE active=1;
            CREATE TABLE worker_update_commands(update_id TEXT NOT NULL REFERENCES worker_update_leases(id),key TEXT NOT NULL,payload TEXT NOT NULL,receipt TEXT NOT NULL CHECK(json_valid(receipt)),PRIMARY KEY(update_id,key));
            CREATE TRIGGER worker_update_identity_guard BEFORE UPDATE ON worker_update_leases WHEN NEW.id!=OLD.id OR NEW.spec!=OLD.spec OR NEW.revision!=OLD.revision+1 OR OLD.active=0 BEGIN SELECT RAISE(ABORT,'Worker update identity/revision is immutable'); END;
            CREATE TRIGGER worker_update_delete_guard BEFORE DELETE ON worker_update_leases BEGIN SELECT RAISE(ABORT,'Worker update history is retained'); END;
            CREATE TRIGGER worker_update_command_update_guard BEFORE UPDATE ON worker_update_commands BEGIN SELECT RAISE(ABORT,'Worker update receipts are immutable'); END;
            CREATE TRIGGER worker_update_command_delete_guard BEFORE DELETE ON worker_update_commands BEGIN SELECT RAISE(ABORT,'Worker update receipts are retained'); END;
            PRAGMA user_version=15;").map_err(sql)
    }

    /// Minimal readonly pre-migration guard. A missing FS marker never releases a DB lease.
    pub fn reject_pending_update_start(path: &std::path::Path) -> Result<()> {
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            winbridge_platform::require_safe_path(std::path::Path::new(&sidecar))
                .map_err(|e| e.to_string())?;
        }
        let c = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(sql)?;
        let version: u32 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql)?;
        if version > super::JOURNAL_SCHEMA_VERSION {
            return Err(format!(
                "journal schema {version} is unsupported; normal startup/migration blocked"
            ));
        }
        if version >= 15 {
            if let Some(lease) = active(&c)? {
                return Err(format!("WinBridge update {} is unfinished ({:?}); normal startup/migration is blocked. Inspect updater journal; do not delete update guards or restart old binaries",lease.spec.update_id,lease.phase));
            }
        }
        Ok(())
    }
    pub fn worker_update_lease(&self, id: &str) -> Result<WorkerUpdateLease> {
        read(&self.connection, id)
    }
    pub fn active_worker_update(&self) -> Result<Option<WorkerUpdateLease>> {
        active(&self.connection)
    }
    pub(crate) fn update_frozen(&self) -> Result<bool> {
        Ok(active(&self.connection)?.is_some_and(|l| frozen(l.phase)))
    }
    pub(crate) fn require_update_writes(c: &Connection) -> Result<()> {
        if active(c)?.is_some_and(|l| frozen(l.phase)) {
            return Err(
                "WinBridge update: journal mutations are frozen after dispatch closed".into(),
            );
        }
        Ok(())
    }
    pub(crate) fn require_update_activation(c: &Connection) -> Result<()> {
        if active(c)?.is_some() {
            return Err("Unfinished worker update blocks journal authority activation".into());
        }
        Ok(())
    }
    pub(crate) fn require_update_administration(&self) -> Result<()> {
        Self::require_update_administration_on(&self.connection)
    }
    pub(crate) fn require_update_administration_on(c: &Connection) -> Result<()> {
        if active(c)?.is_some() {
            return Err("WinBridge update: configuration/output cleanup waits until update coordination is released".into());
        }
        Ok(())
    }
    pub(crate) fn require_update_admission(c: &Connection, project: &str) -> Result<()> {
        if let Some(lease) = active(c)? {
            if lease.phase != WorkerUpdatePhase::SourcePending
                && lease.phase != WorkerUpdatePhase::SourceHeld
                || lease.spec.projects.iter().any(|p| p == project)
            {
                return Err(format!(
                    "Новый запрос не принят: обновление WinBridge {} удерживает проект {project}",
                    lease.spec.update_id
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn require_update_project_dispatch(c: &Connection, project: &str) -> Result<()> {
        if let Some(lease) = active(c)? {
            if frozen(lease.phase)
                || lease.phase != WorkerUpdatePhase::SourcePending
                    && lease.spec.projects.iter().any(|p| p == project)
            {
                return Err(format!(
                    "resource busy: Обновление WinBridge {} удерживает запуск проекта {project}",
                    lease.spec.update_id
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn require_update_resume(&self, project: &str) -> Result<()> {
        Self::require_update_project_dispatch(&self.connection, project)
    }

    fn update_transaction(&mut self) -> Result<Transaction<'_>> {
        self.guard_callback(None, None)?;
        let authority = self.authority.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        Self::assert_authority(&tx, &authority)?;
        Self::read_admission_boundary(&tx, &authority.incarnation)?;
        Ok(tx)
    }
    pub(crate) fn existing_worker_update(
        &self,
        spec: &WorkerUpdateSpec,
    ) -> Result<Option<WorkerUpdateLease>> {
        self.guard_callback(None, None)?;
        let exists: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM worker_update_leases WHERE id=?1)",
                [&spec.update_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if !exists {
            return Ok(None);
        }
        let lease = read(&self.connection, &spec.update_id)?;
        if lease.spec != *spec {
            return Err("Worker update ID already belongs to another scope/payload".into());
        }
        Ok(Some(lease))
    }
    pub(crate) fn begin_worker_update(
        &mut self,
        spec: WorkerUpdateSpec,
    ) -> Result<WorkerUpdateLease> {
        if let Some(lease) = self.existing_worker_update(&spec)? {
            return Ok(lease);
        }
        validate(&spec)?;
        if spec.journal_incarnation != self.authority.incarnation {
            return Err("Worker update belongs to another journal".into());
        }
        let tx = self.update_transaction()?;
        let existing: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM worker_update_leases WHERE id=?1)",
                [&spec.update_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if existing {
            let lease = read(&tx, &spec.update_id)?;
            if lease.spec != spec {
                return Err("Worker update ID already belongs to another payload".into());
            }
            return Ok(lease);
        }
        if active(&tx)?.is_some() {
            return Err("Another worker update holds this installation".into());
        }
        let now = now_ms();
        let lease = WorkerUpdateLease {
            spec,
            revision: 0,
            phase: WorkerUpdatePhase::SourcePending,
            accepted_at_ms: now,
            updated_at_ms: now,
            reason: "Ожидается завершение прежней работы с исходниками".into(),
        };
        one(tx
            .execute(
                "INSERT INTO worker_update_leases VALUES(?1,?2,0,?3,1,?4)",
                params![
                    lease.spec.update_id,
                    encode(&lease.spec)?,
                    encode(&lease.phase)?,
                    encode(&lease)?
                ],
            )
            .map_err(sql)?)?;
        if read(&tx, &lease.spec.update_id)? != lease {
            return Err("Worker update acceptance readback failed".into());
        }
        tx.commit().map_err(sql)?;
        Ok(lease)
    }
    pub(crate) fn worker_update_receipt(
        &self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
    ) -> Result<Option<WorkerUpdateLease>> {
        self.guard_callback(None, None)?;
        if read(&self.connection, &spec.update_id)?.spec != *spec {
            return Err("Worker update command scope mismatch".into());
        }
        Self::update_receipt_on(&self.connection, spec, command)
    }
    fn update_receipt_on(
        c: &Connection,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
    ) -> Result<Option<WorkerUpdateLease>> {
        let row: Option<(String, String)> = c
            .query_row(
                "SELECT payload,receipt FROM worker_update_commands WHERE update_id=?1 AND key=?2",
                params![spec.update_id, command.command_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        row.map(|(payload, json)| {
            if payload != encode(command)? {
                return Err("Worker update command key has another payload".into());
            }
            if json.len() > 32 * 1024 {
                return Err("Oversized worker update receipt".into());
            }
            let snapshot: WorkerUpdateLease =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if snapshot.spec != *spec
                || snapshot.revision != command.expected_revision.saturating_add(1)
            {
                return Err("Worker update receipt identity mismatch".into());
            }
            Ok(snapshot)
        })
        .transpose()
    }
    /// The physical observations come from Worker after dispatch closure, never agent input.
    /// A readonly file scan may run after this short transaction. SourcePending
    /// already blocks new roots in the affected projects; an empty accepted/owner
    /// set means there is no accepted child which could begin using those files.
    /// This check is not a source grant or a capacity reservation. The final CAS
    /// repeats the checks and reserves capacity before acknowledging SourceHeld.
    pub(crate) fn require_source_verification(
        &mut self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
        mut blockers: WorkerUpdateBlockers,
    ) -> Result<()> {
        let tx = self.update_transaction()?;
        let current = read(&tx, &spec.update_id)?;
        if current.spec != *spec || current.revision != command.expected_revision {
            return Err("Worker source verification scope/revision changed".into());
        }
        let projects =
            (current.phase == WorkerUpdatePhase::SourcePending).then_some(spec.projects.as_slice());
        let logical = Self::update_blockers_on(&tx, projects)?;
        blockers.accepted_operations = logical.accepted_operations;
        blockers.logical_owners = logical.logical_owners;
        advance_worker_update(current.phase, command, &blockers)?;
        Ok(())
    }

    /// Commit the grant only after fresh physical/source checks.
    pub(crate) fn advance_worker_update(
        &mut self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
        mut blockers: WorkerUpdateBlockers,
    ) -> Result<WorkerUpdateLease> {
        let capacity = self.work_scheduling.clone();
        let tx = self.update_transaction()?;
        let current = read(&tx, &spec.update_id)?;
        if current.spec != *spec {
            return Err("Worker update command belongs to another owner/scope".into());
        }
        if let Some(snapshot) = Self::update_receipt_on(&tx, spec, command)? {
            return Ok(snapshot);
        }
        if current.revision != command.expected_revision {
            return Err("Worker update revision changed; reload the lease".into());
        }
        if command.action == WorkerUpdateAction::AcquireSource
            && current.phase == WorkerUpdatePhase::SourcePending
        {
            // The phase transition itself is the durable update work reservation.
            // All ordinary claims count it in the same IMMEDIATE transaction.
            Self::require_work_capacity(&tx, capacity.as_ref())?;
            let named: Option<String> = tx
                .query_row(
                    "SELECT policy FROM resource_policy WHERE singleton=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            if named.is_some() {
                return Err(
                    "Named-resource update plan is not implemented; no source/build grant".into(),
                );
            }
        }
        let projects = if current.phase == WorkerUpdatePhase::SourcePending {
            Some(spec.projects.as_slice())
        } else {
            None
        };
        let logical = Self::update_blockers_on(&tx, projects)?;
        blockers.accepted_operations = logical.accepted_operations;
        blockers.logical_owners = logical.logical_owners;
        let phase = advance_worker_update(current.phase, command, &blockers)?;
        if command.action == WorkerUpdateAction::Release
            && current.phase != WorkerUpdatePhase::SourcePending
            && command.source_manifest_sha256.as_ref().is_none_or(|h| {
                h != &spec.baseline_manifest_sha256 && h != &spec.target_manifest_sha256
            })
        {
            return Err("Source release must verify the pinned baseline or target manifest".into());
        }
        let lease = WorkerUpdateLease {
            phase,
            revision: current
                .revision
                .checked_add(1)
                .filter(|r| *r <= MAX_UPDATE_COUNTER)
                .ok_or("Worker update revision exhausted")?,
            updated_at_ms: now_ms().max(current.updated_at_ms),
            reason: command.reason.clone(),
            ..current
        };
        one(tx.execute("UPDATE worker_update_leases SET record=?1,revision=?2,phase=?3,active=?4 WHERE id=?5 AND revision=?6",params![encode(&lease)?,lease.revision as i64,encode(&phase)?,phase!=WorkerUpdatePhase::Released,spec.update_id,command.expected_revision as i64]).map_err(sql)?)?;
        one(tx
            .execute(
                "INSERT INTO worker_update_commands VALUES(?1,?2,?3,?4)",
                params![
                    spec.update_id,
                    command.command_id,
                    encode(command)?,
                    encode(&lease)?
                ],
            )
            .map_err(sql)?)?;
        if read(&tx, &spec.update_id)? != lease
            || Self::update_receipt_on(&tx, spec, command)? != Some(lease.clone())
        {
            return Err("Worker update transition/receipt readback failed".into());
        }
        tx.commit().map_err(sql)?;
        Ok(lease)
    }
    pub(crate) fn update_blockers_on(
        c: &Connection,
        projects: Option<&[String]>,
    ) -> Result<WorkerUpdateBlockers> {
        let filter = projects.map(encode).transpose()?;
        let accepted:i64=c.query_row("SELECT
            (SELECT count(*) FROM tasks WHERE json_extract(record,'$.status') IN ('queued','running') AND (?1 IS NULL OR project IN (SELECT value FROM json_each(?1))))+
            (SELECT count(*) FROM imports WHERE json_extract(record,'$.status') IN ('queued','running') AND (?1 IS NULL OR project IN (SELECT value FROM json_each(?1))))+
            (SELECT count(*) FROM workflows WHERE status IN ('queued','running') AND (?1 IS NULL OR project IN (SELECT value FROM json_each(?1))))+
            (SELECT count(*) FROM applications a JOIN tasks t ON t.id=a.task WHERE a.status IN ('starting','ready','stopping','indeterminate') AND (?1 IS NULL OR t.project IN (SELECT value FROM json_each(?1))))",[&filter],|r|r.get(0)).map_err(sql)?;
        let owners:i64=c.query_row("SELECT
            (SELECT count(*) FROM resource_leases WHERE state!='released' AND (?1 IS NULL OR project IN (SELECT value FROM json_each(?1))))+
            (SELECT count(*) FROM execution_attempts WHERE completed IS NULL AND (?1 IS NULL OR project IN (SELECT value FROM json_each(?1))))",[&filter],|r|r.get(0)).map_err(sql)?;
        Ok(WorkerUpdateBlockers {
            accepted_operations: u64::try_from(accepted)
                .map_err(|_| "Invalid accepted operations count")?,
            logical_owners: u64::try_from(owners).map_err(|_| "Invalid logical owner count")?,
            in_flight_effects: 0,
            physical_owners: 0,
            uncertain_owners: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    fn spec(store: &Store) -> WorkerUpdateSpec {
        WorkerUpdateSpec {
            update_id: uuid::Uuid::new_v4().to_string(),
            instance_id: "update-test".into(),
            journal_incarnation: store.journal_authority().incarnation.clone(),
            worker_generation: uuid::Uuid::new_v4().to_string(),
            updater_owner_id: uuid::Uuid::new_v4().to_string(),
            updater_epoch: 1,
            source_directory: "local-source".into(),
            baseline_manifest_sha256: "a".repeat(64),
            target_manifest_sha256: "b".repeat(64),
            projects: vec!["source".into()],
        }
    }
    fn command(id: &str, rev: u64, action: WorkerUpdateAction) -> WorkerUpdateCommand {
        WorkerUpdateCommand {
            command_id: id.into(),
            expected_revision: rev,
            action,
            source_manifest_sha256: None,
            reason: format!("Verified {id}"),
        }
    }
    fn empty() -> WorkerUpdateBlockers {
        WorkerUpdateBlockers {
            accepted_operations: 0,
            logical_owners: 0,
            in_flight_effects: 0,
            physical_owners: 0,
            uncertain_owners: 0,
        }
    }
    fn other_task(store: &mut Store) -> winbridge_protocol::TaskRecord {
        use winbridge_protocol::*;
        store
            .accept(
                TaskSubmission {
                    protocol_version: 1,
                    request_id: "capacity".into(),
                    idempotency_key: "capacity".into(),
                    project_id: "other".into(),
                    timeout_ms: 5000,
                    snapshot_id: None,
                    command: CommandSource::Direct {
                        executable: "synthetic-not-executed".into(),
                        arguments: vec![],
                        working_directory: ".".into(),
                    },
                },
                ResolvedCommand {
                    executable: "synthetic-not-executed".into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                    timeout_ms: 5000,
                },
                None,
            )
            .unwrap()
    }
    #[test]
    fn update_grant_and_task_claim_share_one_atomic_work_slot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        store.begin_authority().unwrap();
        store
            .configure_work_scheduling(Some(winbridge_protocol::WorkSchedulingPolicy {
                max_concurrent_work: 1,
            }))
            .unwrap();
        let task = other_task(&mut store);
        let spec = spec(&store);
        store.begin_worker_update(spec.clone()).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for update in [true, false] {
            let (path, barrier, spec, id) = (
                path.clone(),
                barrier.clone(),
                spec.clone(),
                task.task_id.clone(),
            );
            handles.push(std::thread::spawn(move || {
                let mut s = Store::open(&path).unwrap();
                s.configure_work_scheduling(Some(winbridge_protocol::WorkSchedulingPolicy {
                    max_concurrent_work: 1,
                }))
                .unwrap();
                barrier.wait();
                if update {
                    s.advance_worker_update(
                        &spec,
                        &command("capacity", 0, WorkerUpdateAction::AcquireSource),
                        empty(),
                    )
                    .map(|_| ())
                } else {
                    s.claim_task(&id).map(|_| ())
                }
            }));
        }
        let result: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(result.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(result
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| e.starts_with("resource busy:")));
        let view = store
            .work_scheduling_observation(winbridge_protocol::RequestContext {
                instance_id: "update-test".into(),
                journal_incarnation: store.journal_authority().incarnation.clone(),
            })
            .unwrap();
        assert_eq!(view.occupied_slots, 1);
    }
    #[test]
    fn ignored_update_grant_does_not_reserve_capacity_and_named_policy_defers() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("journal.sqlite")).unwrap();
        store.begin_authority().unwrap();
        store
            .configure_work_scheduling(Some(winbridge_protocol::WorkSchedulingPolicy {
                max_concurrent_work: 1,
            }))
            .unwrap();
        let spec = spec(&store);
        store.begin_worker_update(spec.clone()).unwrap();
        store.connection.execute_batch("CREATE TRIGGER ignored_budget BEFORE UPDATE ON worker_update_leases BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(store
            .advance_worker_update(
                &spec,
                &command("budget", 0, WorkerUpdateAction::AcquireSource),
                empty()
            )
            .is_err());
        assert_eq!(
            store.worker_update_lease(&spec.update_id).unwrap().phase,
            WorkerUpdatePhase::SourcePending
        );
        store
            .connection
            .execute_batch("DROP TRIGGER ignored_budget")
            .unwrap();
        store
            .configure_resource_scheduling(Some(winbridge_protocol::ResourceSchedulingPolicy {
                resources: vec![],
                projects: vec![],
            }))
            .unwrap();
        assert!(store
            .advance_worker_update(
                &spec,
                &command("named", 0, WorkerUpdateAction::AcquireSource),
                empty()
            )
            .unwrap_err()
            .contains("Named-resource"));
        store.configure_resource_scheduling(None).unwrap();
        let task = other_task(&mut store);
        assert!(store.claim_task(&task.task_id).is_ok());
    }
    #[test]
    fn accepted_workflow_children_continue_during_global_drain() {
        use winbridge_protocol::*;
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
        store.begin_authority().unwrap();
        let command_source = CommandSource::Direct {
            executable: "synthetic-never-run".into(),
            arguments: vec![],
            working_directory: ".".into(),
        };
        let request = WorkflowSubmission {
            protocol_version: 1,
            request_id: "flow".into(),
            idempotency_key: "flow".into(),
            project_id: "other".into(),
            name: "Accepted before update".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
            build: command_source.clone(),
            start: command_source,
            readiness: ReadinessProbe {
                stdout_contains: "READY".into(),
                timeout_ms: 1000,
                netwatch: None,
            },
            log_path: "app.log".into(),
            artifacts: vec![],
        };
        let flow = store.accept_workflow(request.clone()).unwrap();
        let spec = spec(&store);
        store.begin_worker_update(spec.clone()).unwrap();
        store
            .advance_worker_update(
                &spec,
                &command("source", 0, WorkerUpdateAction::AcquireSource),
                empty(),
            )
            .unwrap();
        store
            .advance_worker_update(
                &spec,
                &command("drain", 1, WorkerUpdateAction::BeginDrain),
                empty(),
            )
            .unwrap();
        assert_eq!(
            store.accept_workflow(request.clone()).unwrap().workflow_id,
            flow.workflow_id
        );
        let mut fresh = request;
        fresh.idempotency_key = "new-root".into();
        assert!(store.accept_workflow(fresh).is_err());
        let import = ImportSubmission {
            protocol_version: 1,
            request_id: "child".into(),
            idempotency_key: "child".into(),
            project_id: "other".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
        };
        let mut snapshot = store.workflow_snapshot(&flow.workflow_id).unwrap();
        store
            .accept_workflow_import(
                &mut snapshot,
                import,
                "unused-shared".into(),
                "unused-local".into(),
            )
            .unwrap();
        assert!(snapshot.record.steps[0].operation_id.is_some());
        assert!(store
            .advance_worker_update(
                &spec,
                &command("close", 2, WorkerUpdateAction::CloseDispatch),
                empty()
            )
            .unwrap_err()
            .contains("accepted work"));
        assert_eq!(
            store.worker_update_lease(&spec.update_id).unwrap().phase,
            WorkerUpdatePhase::Draining
        );
    }
    #[test]
    fn acceptance_receipts_cas_restart_and_fencing_do_not_release_the_lease() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        store.begin_authority().unwrap();
        let spec = spec(&store);
        let accepted = store.begin_worker_update(spec.clone()).unwrap();
        let acquire = command("source", 0, WorkerUpdateAction::AcquireSource);
        let held = store
            .advance_worker_update(&spec, &acquire, empty())
            .unwrap();
        let drain = store
            .advance_worker_update(
                &spec,
                &command("drain", 1, WorkerUpdateAction::BeginDrain),
                empty(),
            )
            .unwrap();
        assert_eq!(
            store
                .advance_worker_update(&spec, &acquire, empty())
                .unwrap(),
            held
        );
        let mut conflict = acquire.clone();
        conflict.expected_revision = 99;
        assert!(store
            .advance_worker_update(&spec, &conflict, empty())
            .unwrap_err()
            .contains("another payload"));
        assert!(store
            .advance_worker_update(
                &spec,
                &command("stale", 0, WorkerUpdateAction::CloseDispatch),
                empty()
            )
            .unwrap_err()
            .contains("revision changed"));
        assert_eq!(accepted.revision, 0);
        assert_eq!(drain.revision, 2);
        drop(store);
        assert!(Store::reject_pending_update_start(&path).is_err());
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.active_worker_update().unwrap(), Some(drain));
        assert!(store.begin_authority().unwrap_err().contains("Unfinished"));
        assert_eq!(
            store.begin_worker_update(spec.clone()).unwrap().phase,
            WorkerUpdatePhase::Draining
        );
        store
            .connection
            .execute(
                "UPDATE journal_authority SET epoch=epoch+1,authority=?1",
                [uuid::Uuid::new_v4().to_string()],
            )
            .unwrap();
        assert!(store
            .advance_worker_update(&spec, &acquire, empty())
            .unwrap_err()
            .contains("stale"));
    }
    #[test]
    fn competing_acceptance_and_transition_connections_are_atomic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut first = Store::open(&path).unwrap();
        first.begin_authority().unwrap();
        let second = Store::open(&path).unwrap();
        let spec = spec(&first);
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [first, second]
            .into_iter()
            .map(|mut store| {
                let spec = spec.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let accepted = store.begin_worker_update(spec).unwrap();
                    (store, accepted)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results[0].1, results[1].1);
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = results
            .into_iter()
            .enumerate()
            .map(|(i, (mut store, _))| {
                let spec = spec.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.advance_worker_update(
                        &spec,
                        &command(&format!("source-{i}"), 0, WorkerUpdateAction::AcquireSource),
                        empty(),
                    )
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let mut store = Store::open(&path).unwrap();
        let mut next = spec.clone();
        next.update_id = uuid::Uuid::new_v4().to_string();
        assert!(store
            .begin_worker_update(next)
            .unwrap_err()
            .contains("Another"));
        assert_eq!(store.active_worker_update().unwrap().unwrap().revision, 1);
    }
    #[test]
    fn ignored_state_or_receipt_writes_never_ack_or_close_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
        store.begin_authority().unwrap();
        let spec = spec(&store);
        store.connection.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON worker_update_leases BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(store.begin_worker_update(spec.clone()).is_err());
        assert!(store.active_worker_update().unwrap().is_none());
        store
            .connection
            .execute_batch("DROP TRIGGER fault;")
            .unwrap();
        let accepted = store.begin_worker_update(spec.clone()).unwrap();
        for sql in ["CREATE TRIGGER fault BEFORE UPDATE ON worker_update_leases BEGIN SELECT RAISE(IGNORE); END;","CREATE TRIGGER fault BEFORE INSERT ON worker_update_commands BEGIN SELECT RAISE(IGNORE); END;"] {
            store.connection.execute_batch(sql).unwrap();
            assert!(store.advance_worker_update(&spec,&command("source",0,WorkerUpdateAction::AcquireSource),empty()).is_err());
            assert_eq!(store.worker_update_lease(&spec.update_id).unwrap(),accepted);
            store.connection.execute_batch("DROP TRIGGER fault;").unwrap();
        }
        assert_eq!(
            store
                .advance_worker_update(
                    &spec,
                    &command("source", 0, WorkerUpdateAction::AcquireSource),
                    empty()
                )
                .unwrap()
                .revision,
            1
        );
    }
    #[test]
    fn physical_unknowns_block_confirmation_after_journal_writes_are_frozen() {
        use winbridge_protocol::{ApplyOutputCleanup, OutputCleanupQuery};
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(&root.path().join("journal.sqlite")).unwrap();
        store.begin_authority().unwrap();
        let cleanup = ApplyOutputCleanup {
            idempotency_key: "cleanup-before-update".into(),
            plan: store
                .preview_output_cleanup(&OutputCleanupQuery {
                    older_than_days: 30,
                    project_id: None,
                })
                .unwrap()
                .plan,
        };
        let cleanup_receipt = store.apply_output_cleanup(&cleanup).unwrap();
        let spec = spec(&store);
        store.begin_worker_update(spec.clone()).unwrap();
        for (i, action) in [
            WorkerUpdateAction::AcquireSource,
            WorkerUpdateAction::BeginDrain,
            WorkerUpdateAction::CloseDispatch,
        ]
        .into_iter()
        .enumerate()
        {
            store
                .advance_worker_update(
                    &spec,
                    &command(&format!("step-{i}"), i as u64, action),
                    empty(),
                )
                .unwrap();
        }
        assert!(store.update_frozen().unwrap());
        assert_eq!(
            serde_json::to_value(store.apply_output_cleanup(&cleanup).unwrap()).unwrap(),
            serde_json::to_value(cleanup_receipt).unwrap()
        );
        let mut new_cleanup = cleanup.clone();
        new_cleanup.idempotency_key = "new-cleanup".into();
        assert!(store.apply_output_cleanup(&new_cleanup).is_err());
        assert!(store
            .write_transaction(TransactionBehavior::Immediate)
            .is_err());
        let confirm = command("physical", 3, WorkerUpdateAction::ConfirmQuiesced);
        for uncertain in [false, true] {
            let mut blockers = empty();
            if uncertain {
                blockers.uncertain_owners = 1;
            } else {
                blockers.physical_owners = 1;
            }
            assert!(store
                .advance_worker_update(&spec, &confirm, blockers)
                .is_err());
            assert_eq!(
                store.worker_update_lease(&spec.update_id).unwrap().phase,
                WorkerUpdatePhase::Quiescing
            );
        }
        let quiesced = store
            .advance_worker_update(&spec, &confirm, empty())
            .unwrap();
        assert_eq!(quiesced.phase, WorkerUpdatePhase::Quiesced);
        let mut release = command("release", 4, WorkerUpdateAction::Release);
        release.source_manifest_sha256 = Some("0".repeat(64));
        assert!(store
            .advance_worker_update(&spec, &release, empty())
            .is_err());
        release.source_manifest_sha256 = Some(spec.baseline_manifest_sha256.clone());
        assert_eq!(
            store
                .advance_worker_update(&spec, &release, empty())
                .unwrap()
                .phase,
            WorkerUpdatePhase::Released
        );
        assert!(!store.update_frozen().unwrap());
        assert_eq!(
            store
                .advance_worker_update(&spec, &confirm, empty())
                .unwrap(),
            quiesced
        );
        assert!(store
            .connection
            .execute("DELETE FROM worker_update_commands", [])
            .is_err());
        assert!(store
            .connection
            .execute("DELETE FROM worker_update_leases", [])
            .is_err());
    }
}
