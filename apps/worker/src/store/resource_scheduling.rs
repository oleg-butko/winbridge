//! Atomic named resource sets share the existing physical execution lease.
use super::{now_ms, sql, ExecutionToken, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::BTreeMap;
use winbridge_core::resource_scheduling::{freeze, FrozenResourcePlan, FrozenResourceRequirement};
use winbridge_protocol::*;

fn current_policy(c: &Connection) -> Result<(i64, Option<ResourceSchedulingPolicy>)> {
    let (epoch, bytes): (i64, Option<String>) = c
        .query_row(
            "SELECT epoch,policy FROM resource_policy WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    Ok((
        epoch,
        bytes
            .map(|b| serde_json::from_str(&b).map_err(|e| e.to_string()))
            .transpose()?,
    ))
}

fn claim_policy(c: &Connection) -> Result<Option<ResourceSchedulingPolicy>> {
    let (epoch, policy) = current_policy(c)?;
    if policy.is_some() && epoch != Store::read_authority(c)?.epoch as i64 {
        return Err(
            "resource busy: Политика ресурсов ещё не активирована текущим owner; запуск запрещён"
                .into(),
        );
    }
    Ok(policy)
}
fn plan(c: &Connection, kind: &str, id: &str) -> Result<FrozenResourcePlan> {
    let text: String = c
        .query_row(
            "SELECT plan FROM resource_requests WHERE kind=?1 AND operation=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let p: FrozenResourcePlan = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if p.schema_version != 1 {
        return Err("Unknown frozen resource plan; no dispatch".into());
    }
    Ok(p)
}
fn used(c: &Connection, id: &str) -> Result<(u32, u32)> {
    let counts:(i64,i64)=c.query_row("SELECT coalesce(sum(c.units),0),coalesce(sum(CASE WHEN l.state='uncertain' THEN c.units ELSE 0 END),0) FROM resource_claims c JOIN resource_leases l ON l.id=c.lease WHERE c.resource=?1 AND l.state!='released' AND (c.phase='process' OR l.scope='project_mutation')",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(sql)?;
    Ok((
        u32::try_from(counts.0).map_err(|_| "Resource units overflow")?,
        u32::try_from(counts.1).map_err(|_| "Uncertain resource units overflow")?,
    ))
}
fn kind_text(kind: ResourceKind) -> Result<String> {
    serde_json::to_value(kind)
        .map_err(|e| e.to_string())?
        .as_str()
        .map(String::from)
        .ok_or("Invalid resource kind".into())
}
fn kind_parse(kind: String) -> Result<ResourceKind> {
    serde_json::from_value(serde_json::Value::String(kind)).map_err(|e| e.to_string())
}
fn requirements<'a>(
    p: &'a FrozenResourcePlan,
    kind: &str,
) -> Result<&'a [FrozenResourceRequirement]> {
    match kind {
        "task" => Ok(&p.task),
        "import" => Ok(&p.import),
        _ => Err("Invalid physical operation resource kind".into()),
    }
}
fn validate_requirements(
    c: &Connection,
    kind: &str,
    id: &str,
) -> Result<Vec<FrozenResourceRequirement>> {
    let p = plan(c, kind, id)?;
    let needs = requirements(&p, kind)?;
    let policy = claim_policy(c)?;
    let mut seen = std::collections::HashSet::new();
    for r in needs {
        if r.units == 0 || r.units > 64 || !seen.insert(&r.resource_id) {
            return Err("Invalid frozen resource requirements; no dispatch".into());
        }
        let definition = policy
            .as_ref()
            .and_then(|p| p.resources.iter().find(|d| d.id == r.resource_id));
        let Some(d) = definition else {
            return Err(format!("resource busy: Ресурс {} из принятого запроса отсутствует в текущей политике; требуется явное исправление конфигурации",r.resource_id));
        };
        if d.kind != r.kind {
            return Err(format!(
                "resource busy: Вид ресурса {} отличается от принятого запроса; запуск запрещён",
                r.resource_id
            ));
        }
        let (count, uncertain) = used(c, &r.resource_id)?;
        if count
            .checked_add(u32::from(r.units))
            .is_none_or(|n| n > u32::from(d.capacity))
        {
            return Err(format!("resource busy: Ресурс {}: занято {count}/{}, требуется {}, неподтверждённо {uncertain}; ожидается освобождение владельца",r.resource_id,d.capacity,r.units));
        }
    }
    Ok(needs.to_vec())
}
impl Store {
    pub(super) fn migrate_resource_scheduling(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE resource_policy(singleton INTEGER PRIMARY KEY CHECK(singleton=1),epoch INTEGER NOT NULL CHECK(epoch>=0),policy TEXT CHECK(policy IS NULL OR json_valid(policy)));
          INSERT INTO resource_policy VALUES(1,0,NULL);
          CREATE TABLE resource_requests(kind TEXT NOT NULL CHECK(kind IN ('task','import','workflow')),operation TEXT NOT NULL,plan TEXT NOT NULL CHECK(json_valid(plan)),PRIMARY KEY(kind,operation));
          CREATE TABLE resource_claims(lease TEXT NOT NULL REFERENCES resource_leases(id),resource TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('build','cache','binary','ipc','port','interactive')),units INTEGER NOT NULL CHECK(typeof(units)='integer' AND units BETWEEN 1 AND 64),phase TEXT NOT NULL CHECK(phase IN ('work','process')),PRIMARY KEY(lease,resource));
          CREATE INDEX resource_claims_resource ON resource_claims(resource,lease);
          CREATE TRIGGER resource_requests_update_guard BEFORE UPDATE ON resource_requests BEGIN SELECT RAISE(ABORT,'Frozen resource requests are immutable'); END;
          CREATE TRIGGER resource_requests_delete_guard BEFORE DELETE ON resource_requests BEGIN SELECT RAISE(ABORT,'Frozen resource requests are retained'); END;
          CREATE TRIGGER resource_claims_update_guard BEFORE UPDATE ON resource_claims BEGIN SELECT RAISE(ABORT,'Resource claims are immutable'); END;
          CREATE TRIGGER resource_claims_delete_guard BEFORE DELETE ON resource_claims BEGIN SELECT RAISE(ABORT,'Resource claims are retained'); END;
          PRAGMA user_version=14;").map_err(sql)?;
        let empty =
            serde_json::to_string(&FrozenResourcePlan::default()).map_err(|e| e.to_string())?;
        for (kind, table) in [
            ("task", "tasks"),
            ("import", "imports"),
            ("workflow", "workflows"),
        ] {
            c.execute(
                &format!("INSERT INTO resource_requests SELECT ?1,id,?2 FROM {table}"),
                params![kind, empty],
            )
            .map_err(sql)?;
        }
        Ok(())
    }
    pub(crate) fn configure_resource_scheduling(
        &mut self,
        policy: Option<ResourceSchedulingPolicy>,
    ) -> Result<()> {
        if self.read_only {
            return Ok(());
        }
        self.guard_callback(None, None)?;
        let epoch = self.authority.epoch as i64;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        let active_named:i64=tx.query_row("SELECT count(*) FROM resource_claims c JOIN resource_leases l ON l.id=c.lease WHERE l.state!='released' AND (c.phase='process' OR l.scope='project_mutation')",[],|r|r.get(0)).map_err(sql)?;
        if policy.is_none() && active_named != 0 {
            return Err("Cannot disable resource policy while named physical owners remain; stop/reconcile them first".into());
        }
        if let Some(p) = &policy {
            let legacy_pending:i64=tx.query_row("SELECT count(*) FROM resource_requests r WHERE json_extract(r.plan,'$.policySha256') IS NULL AND ((r.kind='task' AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=r.operation AND json_extract(t.record,'$.status') IN ('queued','running'))) OR (r.kind='import' AND EXISTS(SELECT 1 FROM imports i WHERE i.id=r.operation AND json_extract(i.record,'$.status') IN ('queued','running'))) OR (r.kind='workflow' AND EXISTS(SELECT 1 FROM workflows w WHERE w.id=r.operation AND w.status IN ('queued','running'))))",[],|r|r.get(0)).map_err(sql)?;
            let legacy_owners:i64=tx.query_row("SELECT count(*) FROM resource_leases l WHERE l.state!='released' AND NOT EXISTS(SELECT 1 FROM resource_requests r WHERE r.kind=l.kind AND r.operation=l.operation AND json_extract(r.plan,'$.policySha256') IS NOT NULL)",[],|r|r.get(0)).map_err(sql)?;
            if legacy_pending != 0 || legacy_owners != 0 {
                return Err("Finish/cancel legacy admissions and reconcile physical owners before enabling named resources; accepted requests cannot be rebound".into());
            }
            // Reusing a name for a different kind cannot weaken retained owners.
            let mut statement=tx.prepare("SELECT c.resource,c.kind FROM resource_claims c JOIN resource_leases l ON l.id=c.lease WHERE l.state!='released' AND (c.phase='process' OR l.scope='project_mutation')").map_err(sql)?;
            for row in statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(sql)?
            {
                let (id, kind) = row.map_err(sql)?;
                if !p.resources.iter().any(|d| d.id == id) {
                    return Err(format!(
                        "Cannot remove resource {id} while a physical owner remains"
                    ));
                }
                if p.resources
                    .iter()
                    .any(|d| d.id == id && kind_text(d.kind).ok().as_ref() != Some(&kind))
                {
                    return Err(format!(
                        "Resource {id} changes kind while a physical owner is retained"
                    ));
                }
            }
            let mut statement=tx.prepare("SELECT r.plan FROM resource_requests r WHERE (r.kind='task' AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=r.operation AND json_extract(t.record,'$.status') IN ('queued','running'))) OR (r.kind='import' AND EXISTS(SELECT 1 FROM imports i WHERE i.id=r.operation AND json_extract(i.record,'$.status') IN ('queued','running'))) OR (r.kind='workflow' AND EXISTS(SELECT 1 FROM workflows w WHERE w.id=r.operation AND w.status IN ('queued','running')))").map_err(sql)?;
            for row in statement
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(sql)?
            {
                let frozen: FrozenResourcePlan =
                    serde_json::from_str(&row.map_err(sql)?).map_err(|e| e.to_string())?;
                for r in frozen
                    .task
                    .iter()
                    .chain(&frozen.import)
                    .chain(&frozen.build)
                    .chain(&frozen.start)
                {
                    if p.resources
                        .iter()
                        .any(|d| d.id == r.resource_id && d.kind != r.kind)
                    {
                        return Err(format!(
                            "Resource {} changes kind in an accepted request",
                            r.resource_id
                        ));
                    }
                }
            }
        }
        let bytes = policy
            .map(|p| serde_json::to_string(&p).map_err(|e| e.to_string()))
            .transpose()?;
        if tx
            .execute(
                "UPDATE resource_policy SET epoch=?1,policy=?2 WHERE singleton=1",
                params![epoch, bytes],
            )
            .map_err(sql)?
            != 1
        {
            return Err("Resource policy activation not committed".into());
        }
        tx.commit().map_err(sql)
    }
    fn insert_resource_plan(
        c: &Connection,
        kind: &str,
        id: &str,
        p: &FrozenResourcePlan,
    ) -> Result<()> {
        let bytes = serde_json::to_string(p).map_err(|e| e.to_string())?;
        let existing: Option<String> = c
            .query_row(
                "SELECT plan FROM resource_requests WHERE kind=?1 AND operation=?2",
                params![kind, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(old) = existing {
            return if old == bytes {
                Ok(())
            } else {
                Err("Existing operation has another frozen resource plan; no rebind".into())
            };
        }
        if c.execute(
            "INSERT INTO resource_requests(kind,operation,plan) VALUES(?1,?2,?3)",
            params![kind, id, bytes],
        )
        .map_err(sql)?
            != 1
        {
            return Err("Frozen resource plan not stored; no acceptance ACK".into());
        }
        Ok(())
    }
    pub(crate) fn freeze_resource_root(
        c: &Connection,
        kind: &str,
        id: &str,
        project: &str,
    ) -> Result<()> {
        let policy = claim_policy(c)?;
        let mut frozen = freeze(policy.as_ref(), project)?;
        match kind {
            "task" => {
                frozen.import.clear();
                frozen.build.clear();
                frozen.start.clear();
            }
            "import" => {
                frozen.task.clear();
                frozen.build.clear();
                frozen.start.clear();
            }
            "workflow" => {}
            _ => return Err("Invalid root resource kind".into()),
        }
        Self::insert_resource_plan(c, kind, id, &frozen)
    }
    pub(crate) fn freeze_resource_child(
        c: &Connection,
        parent: &str,
        kind: &str,
        id: &str,
        step: usize,
    ) -> Result<()> {
        let p = plan(c, "workflow", parent)?;
        let frozen = FrozenResourcePlan {
            policy_sha256: p.policy_sha256.clone(),
            task: match step {
                1 => p.build.clone(),
                2 => p.start.clone(),
                0 => vec![],
                _ => return Err("Invalid resource child step".into()),
            },
            import: if step == 0 { p.import.clone() } else { vec![] },
            ..Default::default()
        };
        Self::insert_resource_plan(c, kind, id, &frozen)
    }
    pub(super) fn acquire_named_resources(
        c: &Connection,
        token: &ExecutionToken,
        lease: &str,
    ) -> Result<()> {
        let needs = validate_requirements(
            c,
            token.operation.kind.as_str(),
            &token.operation.operation_id,
        )?;
        // No writes before every capacity check succeeds. The enclosing claim also
        // rolls back intent/attempt/project lease if any claim INSERT is ignored.
        for r in needs {
            let n=c.execute("INSERT INTO resource_claims(lease,resource,kind,units,phase) VALUES(?1,?2,?3,?4,?5)",params![lease,r.resource_id,kind_text(r.kind)?,r.units,if r.kind.process_lifetime(){"process"}else{"work"}]).map_err(sql)?;
            if n != 1 {
                return Err("Resource set not fully stored; no launch authorized".into());
            }
        }
        Ok(())
    }
    pub(crate) fn named_resource_wait(
        &self,
        kind: super::OperationKind,
        id: &str,
    ) -> Result<Option<String>> {
        match validate_requirements(&self.connection, kind.as_str(), id) {
            Ok(_) => Ok(None),
            Err(e) if e.starts_with("resource busy:") => Ok(Some(e)),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn resource_scheduling_observation(
        &self,
        context: RequestContext,
    ) -> Result<ResourceSchedulingObservation> {
        if context.journal_incarnation != self.authority.incarnation {
            return Err("Foreign resource observation scope".into());
        }
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        let (epoch, policy) = current_policy(&tx)?;
        let mut resources = BTreeMap::new();
        if let Some(p) = &policy {
            for r in &p.resources {
                resources.insert(
                    r.id.clone(),
                    ResourceCapacityObservation {
                        id: r.id.clone(),
                        kind: r.kind,
                        capacity: Some(r.capacity),
                        used_units: 0,
                        uncertain_units: 0,
                    },
                );
            }
        }
        let mut statement=tx.prepare("SELECT c.resource,c.kind FROM resource_claims c JOIN resource_leases l ON l.id=c.lease WHERE l.state!='released' AND (c.phase='process' OR l.scope='project_mutation') GROUP BY c.resource,c.kind ORDER BY c.resource").map_err(sql)?;
        for row in statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?
        {
            let (id, kind) = row.map_err(sql)?;
            let kind = kind_parse(kind)?;
            let entry = resources
                .entry(id.clone())
                .or_insert(ResourceCapacityObservation {
                    id: id.clone(),
                    kind,
                    capacity: None,
                    used_units: 0,
                    uncertain_units: 0,
                });
            if entry.kind != kind {
                return Err("Resource kind conflicts with retained ownership".into());
            }
            (entry.used_units, entry.uncertain_units) = used(&tx, &id)?;
        }
        drop(statement);
        let mut owners = vec![];
        let mut statement=tx.prepare("SELECT c.resource,l.id,l.project,l.kind,l.operation,c.units,l.state,l.scope FROM resource_claims c JOIN resource_leases l ON l.id=c.lease WHERE l.state!='released' AND (c.phase='process' OR l.scope='project_mutation') ORDER BY c.rowid LIMIT 257").map_err(sql)?;
        for row in statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, u16>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(sql)?
        {
            let (resource_id, lease_id, project_id, kind, operation_id, units, state, scope) =
                row.map_err(sql)?;
            owners.push(ResourceOwnerObservation {
                resource_id,
                lease_id,
                project_id,
                operation_kind: if kind == "task" {
                    HistoryKind::Task
                } else {
                    HistoryKind::Import
                },
                operation_id,
                units,
                state: if state == "held" {
                    ObservedLeaseState::Held
                } else {
                    ObservedLeaseState::Uncertain
                },
                application_lifetime: scope == "application_lifetime",
            });
        }
        drop(statement);
        tx.commit().map_err(sql)?;
        let truncated = resources.len() > 128 || owners.len() > 256;
        owners.truncate(256);
        Ok(ResourceSchedulingObservation {
            schema_version: 1,
            provider: build_identity(),
            context,
            policy_generation: epoch.to_string(),
            enabled: policy.is_some(),
            resources: resources.into_values().take(128).collect(),
            owners,
            truncated,
            observed_at_ms: now_ms(),
            read_only: self.read_only,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CleanupProof, OperationKind};
    fn policy() -> ResourceSchedulingPolicy {
        serde_json::from_value(serde_json::json!({"resources":[{"id":"build","kind":"build","capacity":2},{"id":"cache","kind":"cache","capacity":1}],"projects":[{"projectId":"a","task":[{"resourceId":"build","units":1},{"resourceId":"cache","units":1}]},{"projectId":"b","task":[{"resourceId":"build","units":1},{"resourceId":"cache","units":1}]},{"projectId":"c","task":[{"resourceId":"build","units":1}]}]})).unwrap()
    }
    fn admit(s: &mut Store, project: &str, key: &str) -> TaskRecord {
        s.accept(
            TaskSubmission {
                protocol_version: 1,
                request_id: key.into(),
                idempotency_key: key.into(),
                project_id: project.into(),
                timeout_ms: 5000,
                snapshot_id: None,
                command: CommandSource::Direct {
                    executable: "never-executed".into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                },
            },
            ResolvedCommand {
                executable: "never-executed".into(),
                arguments: vec![],
                working_directory: ".".into(),
                timeout_ms: 5000,
            },
            None,
        )
        .unwrap()
    }
    fn view(s: &Store) -> ResourceSchedulingObservation {
        s.resource_scheduling_observation(RequestContext {
            instance_id: "resources".into(),
            journal_incarnation: s.authority.incarnation.clone(),
        })
        .unwrap()
    }
    fn count(s: &Store, table: &str) -> i64 {
        s.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn complete_sets_share_persisted_policy_and_roll_back_when_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let a = admit(&mut s, "a", "a");
        let b = admit(&mut s, "b", "b");
        let c = admit(&mut s, "c", "c");
        let (_, owner) = s.claim_task(&a.task_id).unwrap();
        let mut peer = Store::open(&path).unwrap();
        assert!(peer.claim_task(&b.task_id).unwrap_err().contains("cache"));
        assert_eq!(peer.get(&b.task_id).unwrap().status, TaskStatus::Queued);
        assert_eq!(count(&s, "execution_attempts"), 1);
        assert_eq!(count(&s, "resource_claims"), 2);
        peer.claim_task(&c.task_id).unwrap();
        assert_eq!(used(&s.connection, "build").unwrap().0, 2);
        s.finish_task_execution(
            &owner,
            TaskStatus::Succeeded,
            Some(0),
            "synthetic no-process cleanup",
            CleanupProof::NoProcessCreated,
        )
        .unwrap();
        peer.claim_task(&b.task_id).unwrap();
        assert_eq!(used(&s.connection, "build").unwrap().0, 2);
        assert_eq!(used(&s.connection, "cache").unwrap().0, 1);
        assert_eq!(view(&peer).owners.len(), 3);
    }
    #[test]
    fn concurrent_connections_cannot_split_or_overcommit_sets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let tasks = [admit(&mut s, "a", "a"), admit(&mut s, "b", "b")];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = tasks
            .into_iter()
            .map(|t| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut p = Store::open(&path).unwrap();
                    barrier.wait();
                    p.claim_task(&t.task_id)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(count(&s, "execution_attempts"), 1);
        assert_eq!(count(&s, "resource_claims"), 2);
        assert_eq!(used(&s.connection, "build").unwrap().0, 1);
    }
    #[test]
    fn ignored_claim_or_frozen_request_rolls_back_every_related_row() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("journal.sqlite")).unwrap();
        s.begin_authority().unwrap();
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let a = admit(&mut s, "a", "a");
        s.connection.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON resource_claims WHEN NEW.resource='cache' BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(s.claim_task(&a.task_id).is_err());
        assert_eq!(count(&s, "execution_attempts"), 0);
        assert_eq!(count(&s, "resource_leases"), 0);
        assert_eq!(count(&s, "resource_claims"), 0);
        assert_eq!(s.get(&a.task_id).unwrap().status, TaskStatus::Queued);
        s.connection.execute_batch("DROP TRIGGER fault; CREATE TRIGGER fault BEFORE INSERT ON resource_requests BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        let mut request = a.submission.clone();
        request.idempotency_key = "denied".into();
        assert!(s.accept(request, a.resolved_command, None).is_err());
        assert_eq!(count(&s, "tasks"), 1);
        assert_eq!(count(&s, "queue_roots"), 1);
    }
    #[test]
    fn retained_requirements_survive_policy_removal_and_uncertain_owners() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let a = admit(&mut s, "a", "a");
        let b = admit(&mut s, "b", "b");
        let original = plan(&s.connection, "task", &b.task_id).unwrap();
        s.configure_resource_scheduling(None).unwrap();
        assert!(s
            .claim_task(&b.task_id)
            .unwrap_err()
            .contains("отсутствует"));
        assert_eq!(plan(&s.connection, "task", &b.task_id).unwrap(), original);
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let (_, owner) = s.claim_task(&a.task_id).unwrap();
        s.with_execution(&owner, |s| {
            s.finish(
                &a.task_id,
                TaskStatus::Indeterminate,
                None,
                "uncertain synthetic owner",
            )
        })
        .unwrap();
        assert_eq!(used(&s.connection, "cache").unwrap(), (1, 1));
        assert!(s.configure_resource_scheduling(None).is_err());
        assert_eq!(plan(&s.connection, "task", &b.task_id).unwrap(), original);
        let mut p = policy();
        p.resources[1].kind = ResourceKind::Binary;
        assert!(s.configure_resource_scheduling(Some(p)).is_err());
        drop(s);
        let mut next = Store::open(&path).unwrap();
        next.begin_authority().unwrap();
        next.recover_resource_leases().unwrap();
        next.configure_resource_scheduling(Some(policy())).unwrap();
        assert!(next.claim_task(&b.task_id).unwrap_err().contains("cache"));
        assert_eq!(used(&next.connection, "cache").unwrap(), (1, 1));
        let generation = view(&next).policy_generation;
        assert_eq!(generation, next.authority.epoch.to_string());
        assert!(next
            .execution_attempt(OperationKind::Task, &b.task_id)
            .unwrap()
            .is_none());
    }
    #[test]
    fn enabling_policy_requires_legacy_queue_and_physical_owners_to_be_drained() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("journal.sqlite")).unwrap();
        s.begin_authority().unwrap();
        let old = admit(&mut s, "a", "old");
        assert!(s
            .configure_resource_scheduling(Some(policy()))
            .unwrap_err()
            .contains("legacy"));
        let (_, token) = s.claim_task(&old.task_id).unwrap();
        assert!(s.configure_resource_scheduling(Some(policy())).is_err());
        s.finish_task_execution(
            &token,
            TaskStatus::Succeeded,
            Some(0),
            "legacy owner cleanup",
            CleanupProof::NoProcessCreated,
        )
        .unwrap();
        s.configure_resource_scheduling(Some(policy())).unwrap();
        let next = admit(&mut s, "a", "new");
        assert!(plan(&s.connection, "task", &next.task_id)
            .unwrap()
            .policy_sha256
            .is_some());
    }
    #[test]
    fn schema_thirteen_migration_backfills_legacy_plans_and_preserves_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        let task = admit(&mut s, "a", "legacy");
        drop(s);
        let db = Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE operation_cancellations; DROP TABLE operation_creators; DROP TABLE worker_journal_activations; DROP TABLE worker_shutdown_receipts; DROP TABLE worker_update_commands; DROP TABLE worker_update_leases; DROP TABLE resource_claims; DROP TABLE resource_requests; DROP TABLE resource_policy; PRAGMA user_version=13;").unwrap();
        drop(db);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.get(&task.task_id).unwrap().task_id, task.task_id);
        assert_eq!(
            plan(&s.connection, "task", &task.task_id).unwrap(),
            FrozenResourcePlan::default()
        );
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("journal-v13-"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let db = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            db.query_row::<u32, _, _>("PRAGMA user_version", [], |r| r.get(0))
                .unwrap(),
            13
        );
        assert_eq!(
            db.query_row::<u32, _, _>(
                "SELECT count(*) FROM sqlite_master WHERE name='resource_requests'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
    }
}
