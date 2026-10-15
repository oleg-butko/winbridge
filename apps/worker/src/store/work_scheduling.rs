//! Loaded owner policy and durable lease counts. No counter is released by timeout.
use super::{sql, Result, Store};
use rusqlite::Connection;
use std::collections::BTreeMap;
use winbridge_protocol::*;

fn counts(connection: &Connection) -> Result<(u32, u32, u32)> {
    let row:(i64,i64,i64)=connection.query_row("SELECT
        (SELECT count(*) FROM resource_leases WHERE state!='released' AND (scope='project_mutation' OR state='uncertain')),
        (SELECT count(*) FROM resource_leases WHERE state='uncertain'),
        (SELECT count(*) FROM tasks WHERE json_extract(record,'$.status')='queued')+(SELECT count(*) FROM imports WHERE json_extract(record,'$.status')='queued')+(SELECT count(*) FROM workflows WHERE status='queued')",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
    Ok((
        u32::try_from(row.0).map_err(|_| "Invalid occupied slot count")?,
        u32::try_from(row.1).map_err(|_| "Invalid uncertain slot count")?,
        u32::try_from(row.2).map_err(|_| "Invalid queue count")?,
    ))
}

impl Store {
    // Only Worker startup configures its single Store owner. This is not an RPC parameter.
    pub(crate) fn configure_work_scheduling(
        &mut self,
        policy: Option<WorkSchedulingPolicy>,
    ) -> Result<()> {
        if let Some(value) = &policy {
            winbridge_core::work_scheduling::validate(value)?;
        }
        self.work_scheduling = policy;
        Ok(())
    }
    pub(super) fn require_work_capacity(
        connection: &Connection,
        policy: Option<&WorkSchedulingPolicy>,
    ) -> Result<()> {
        let Some(policy) = policy else {
            return Ok(());
        };
        let (used, uncertain, _) = counts(connection)?;
        if used >= u32::from(policy.max_concurrent_work) {
            return Err(format!("resource busy: Общий лимит работ: занято {used}/{}, с неподтверждённым освобождением {uncertain}; ожидается физическое завершение владельца",policy.max_concurrent_work));
        }
        Ok(())
    }
    pub(crate) fn fair_projects(&self, projects: &[String]) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT project,max(rowid) FROM execution_attempts GROUP BY project")
            .map_err(sql)?;
        let mut grants = BTreeMap::new();
        for row in statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql)?
        {
            let (project, ordinal) = row.map_err(sql)?;
            if projects.contains(&project) {
                grants.insert(
                    project,
                    u64::try_from(ordinal).map_err(|_| "Invalid dispatch ordinal")?,
                );
            }
        }
        Ok(winbridge_core::work_scheduling::fair_project_order(
            projects, &grants,
        ))
    }
    pub(crate) fn work_scheduling_observation(
        &self,
        context: RequestContext,
    ) -> Result<WorkSchedulingObservation> {
        if context.journal_incarnation != self.authority.incarnation {
            return Err("Foreign scheduling scope".into());
        }
        let transaction = self.connection.unchecked_transaction().map_err(sql)?;
        let (used, uncertain, queued) = counts(&transaction)?;
        transaction.commit().map_err(sql)?;
        Ok(WorkSchedulingObservation {
            schema_version: 1,
            provider: build_identity(),
            context,
            max_concurrent_work: self.work_scheduling.as_ref().map(|p| p.max_concurrent_work),
            occupied_slots: used,
            uncertain_slots: uncertain,
            queued_operations: queued,
            observed_at_ms: super::now_ms(),
            read_only: self.read_only,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CleanupProof, OperationKind};
    use std::{
        sync::{Arc, Barrier},
        thread,
    };
    fn admit(store: &mut Store, project: &str, key: &str) -> TaskRecord {
        store
            .accept(
                TaskSubmission {
                    protocol_version: 1,
                    request_id: key.into(),
                    idempotency_key: key.into(),
                    project_id: project.into(),
                    timeout_ms: 5000,
                    snapshot_id: None,
                    command: CommandSource::Direct {
                        executable: "synthetic-not-dispatched".into(),
                        arguments: vec![],
                        working_directory: ".".into(),
                    },
                },
                ResolvedCommand {
                    executable: "synthetic-not-dispatched".into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                    timeout_ms: 5000,
                },
                None,
            )
            .unwrap()
    }
    fn policy(store: &mut Store, limit: u16) {
        store
            .configure_work_scheduling(Some(WorkSchedulingPolicy {
                max_concurrent_work: limit,
            }))
            .unwrap();
    }
    fn snapshot(store: &Store) -> WorkSchedulingObservation {
        store
            .work_scheduling_observation(RequestContext {
                instance_id: "slots".into(),
                journal_incarnation: store.authority.incarnation.clone(),
            })
            .unwrap()
    }
    #[test]
    fn two_connections_cannot_claim_one_global_slot_twice() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        store.begin_authority().unwrap();
        policy(&mut store, 1);
        let tasks = [
            admit(&mut store, "a", "first"),
            admit(&mut store, "b", "second"),
        ];
        let barrier = Arc::new(Barrier::new(2));
        let handles = tasks
            .iter()
            .map(|task| {
                let path = path.clone();
                let id = task.task_id.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    let mut s = Store::open(&path).unwrap();
                    policy(&mut s, 1);
                    barrier.wait();
                    s.claim_task(&id)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| e.starts_with("resource busy: Общий лимит")));
        let view = snapshot(&store);
        assert_eq!(view.occupied_slots, 1);
        assert_eq!(view.queued_operations, 1);
        assert_eq!(
            store
                .connection
                .query_row::<i64, _, _>("SELECT count(*) FROM execution_attempts", [], |r| r.get(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn failed_lease_insert_rolls_back_slot_attempt_and_launch_intent() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temp.path().join("journal.sqlite")).unwrap();
        store.begin_authority().unwrap();
        policy(&mut store, 1);
        let task = admit(&mut store, "a", "task");
        store.connection.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON resource_leases BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(store.claim_task(&task.task_id).is_err());
        assert_eq!(store.get(&task.task_id).unwrap().status, TaskStatus::Queued);
        assert!(store
            .execution_attempt(OperationKind::Task, &task.task_id)
            .unwrap()
            .is_none());
        assert_eq!(snapshot(&store).occupied_slots, 0);
        store
            .connection
            .execute_batch("DROP TRIGGER fault;")
            .unwrap();
        let (_, token) = store.claim_task(&task.task_id).unwrap();
        store
            .with_execution(&token, |s| {
                s.finish_task_execution(
                    &token,
                    TaskStatus::Succeeded,
                    Some(0),
                    "synthetic",
                    CleanupProof::NoProcessCreated,
                )
            })
            .unwrap();
        assert_eq!(snapshot(&store).occupied_slots, 0);
    }
    #[test]
    fn terminal_without_cleanup_retains_capacity_across_restart_and_blocks_imports_too() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        store.begin_authority().unwrap();
        policy(&mut store, 1);
        let task = admit(&mut store, "a", "unknown");
        let (_, token) = store.claim_task(&task.task_id).unwrap();
        store
            .with_execution(&token, |s| {
                s.finish(
                    &task.task_id,
                    TaskStatus::Indeterminate,
                    None,
                    "cleanup unknown",
                )
            })
            .unwrap();
        let import = store
            .accept_import(
                ImportSubmission {
                    protocol_version: 1,
                    request_id: "import".into(),
                    idempotency_key: "import".into(),
                    project_id: "b".into(),
                    snapshot_id: "snapshot".into(),
                    manifest_sha256: "a".repeat(64),
                    timeout_ms: 5000,
                },
                "shared".into(),
                "workspace".into(),
            )
            .unwrap();
        assert_eq!(snapshot(&store).uncertain_slots, 1);
        assert!(store
            .claim_import(&import.import_id)
            .unwrap_err()
            .contains("Общий лимит"));
        drop(store);
        let mut store = Store::open(&path).unwrap();
        policy(&mut store, 1);
        store.begin_authority().unwrap();
        store.recover_resource_leases().unwrap();
        let view = snapshot(&store);
        assert_eq!((view.occupied_slots, view.uncertain_slots), (1, 1));
        assert!(store.claim_import(&import.import_id).is_err());
        assert!(store
            .execution_attempt(OperationKind::Import, &import.import_id)
            .unwrap()
            .is_none());
    }
}
