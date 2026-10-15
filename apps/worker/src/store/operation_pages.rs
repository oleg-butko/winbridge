//! One read transaction per page. Cursors fence admissions, not mutable statuses.
use super::{now_ms, sql, Result, Store};
use rusqlite::params;
use winbridge_core::operation_query::{decimal_counter, query_sha256, validate};
use winbridge_protocol::{
    AdmissionWatermarks, HistoryCursor, HistoryKind, ObservedLeaseScope, ObservedLeaseState,
    OperationCursor, OperationObservation, OperationPage, OperationQuery, OperationRef,
    OperationResource, OperationState, RequestContext, WorkflowStep,
};

fn kind(value: &str) -> Result<HistoryKind> {
    match value {
        "task" => Ok(HistoryKind::Task),
        "import" => Ok(HistoryKind::Import),
        "workflow" => Ok(HistoryKind::Workflow),
        "application" => Ok(HistoryKind::Application),
        _ => Err("Unknown operation kind".into()),
    }
}

fn name(value: HistoryKind) -> &'static str {
    match value {
        HistoryKind::Task => "task",
        HistoryKind::Import => "import",
        HistoryKind::Workflow => "workflow",
        HistoryKind::Application => "application",
    }
}
fn bounded(value: String, limit: usize) -> (String, bool) {
    let truncated = value.chars().count() > limit;
    (value.chars().take(limit).collect(), truncated)
}
fn counter(value: i64) -> Result<String> {
    if value < 0 {
        Err("Negative journal counter".into())
    } else {
        Ok(value.to_string())
    }
}
struct Row {
    id: String,
    kind: String,
    project: String,
    label: String,
    status: String,
    created: i64,
    finished: Option<i64>,
    reason: Option<String>,
    revision: i64,
    event: Option<i64>,
    steps: Option<String>,
    workflow: Option<String>,
    task: Option<String>,
}
impl Store {
    pub fn operation_page(
        &self,
        context: &RequestContext,
        query: &OperationQuery,
    ) -> Result<OperationPage> {
        self.operation_page_inner(context, query, || {})
    }
    fn operation_page_inner(
        &self,
        context: &RequestContext,
        query: &OperationQuery,
        after_rows: impl FnOnce(),
    ) -> Result<OperationPage> {
        validate(context, query)?;
        if context.journal_incarnation != self.journal_authority().incarnation {
            return Err("Operation query scope differs from this journal".into());
        }
        let transaction = self.connection.unchecked_transaction().map_err(sql)?;
        let current:[i64;4]=transaction.query_row("SELECT (SELECT coalesce(max(rowid),0) FROM tasks),(SELECT coalesce(max(rowid),0) FROM imports),(SELECT coalesce(max(rowid),0) FROM workflows),(SELECT coalesce(max(rowid),0) FROM applications)",[],|r|Ok([r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?])).map_err(sql)?;
        let watermarks = query
            .cursor
            .as_ref()
            .map(|c| c.watermarks.clone())
            .unwrap_or(AdmissionWatermarks {
                tasks: counter(current[0])?,
                imports: counter(current[1])?,
                workflows: counter(current[2])?,
                applications: counter(current[3])?,
            });
        let parsed = [
            decimal_counter(&watermarks.tasks)?,
            decimal_counter(&watermarks.imports)?,
            decimal_counter(&watermarks.workflows)?,
            decimal_counter(&watermarks.applications)?,
        ];
        if parsed.iter().zip(current).any(|(old, now)| *old > now) {
            return Err(
                "Journal admissions shrank; retained cursor cannot describe this snapshot".into(),
            );
        }
        let before = query
            .cursor
            .as_ref()
            .map(|c| c.before.clone())
            .unwrap_or(HistoryCursor {
                created_at_ms: i64::MAX as u64,
                kind: HistoryKind::Application,
                operation_id: "~".into(),
            });
        let before_kind = if query.cursor.is_some() {
            name(before.kind)
        } else {
            "~"
        };
        let mut statement=transaction.prepare("WITH h AS (
          SELECT id,'task' AS kind,project,record,revision FROM tasks WHERE rowid<=?1
          UNION ALL SELECT id,'import',project,record,revision FROM imports WHERE rowid<=?2
          UNION ALL SELECT id,'workflow',project,record,revision FROM workflows WHERE rowid<=?3
          UNION ALL SELECT id,'application',json_extract(record,'$.projectId'),record,revision FROM applications WHERE rowid<=?4
        ), v AS (SELECT id,kind,project,revision,
          CASE kind WHEN 'task' THEN json_extract(record,'$.resolvedCommand.executable') WHEN 'import' THEN json_extract(record,'$.submission.snapshotId') WHEN 'workflow' THEN json_extract(record,'$.submission.name') ELSE json_extract(record,'$.taskId') END AS label,
          json_extract(record,'$.status') AS status,
          coalesce(json_extract(record,'$.acceptedAtMs'),json_extract(record,'$.createdAtMs')) AS created,
          json_extract(record,'$.finishedAtMs') AS finished,
          coalesce(json_extract(record,'$.failureReason'),json_extract(record,'$.reason')) AS reason,
          json_extract(record,'$.events[#-1].sequence') AS event,
          CASE kind WHEN 'workflow' THEN json_extract(record,'$.steps') END AS steps,
          CASE kind WHEN 'application' THEN json_extract(record,'$.workflowId') END AS workflow,
          CASE kind WHEN 'application' THEN json_extract(record,'$.taskId') END AS task
          FROM h)
          SELECT id,kind,project,label,status,created,finished,reason,revision,event,steps,workflow,task FROM v
          WHERE (?5 IS NULL OR project=?5) AND (?6 IS NULL OR kind=?6)
          AND (created,kind,id)<(?7,?8,?9)
          AND (instr(lower(id),lower(?10))>0 OR instr(lower(project),lower(?10))>0 OR instr(lower(coalesce(label,'')),lower(?10))>0)
          ORDER BY created DESC,kind DESC,id DESC LIMIT ?11").map_err(sql)?;
        let mut rows = statement
            .query_map(
                params![
                    parsed[0],
                    parsed[1],
                    parsed[2],
                    parsed[3],
                    query.project_id,
                    query.kind.map(name),
                    before.created_at_ms as i64,
                    before_kind,
                    before.operation_id,
                    query.search,
                    i64::from(query.limit) + 1
                ],
                |r| {
                    Ok(Row {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        project: r.get(2)?,
                        label: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        status: r.get(4)?,
                        created: r.get(5)?,
                        finished: r.get(6)?,
                        reason: r.get(7)?,
                        revision: r.get(8)?,
                        event: r.get(9)?,
                        steps: r.get(10)?,
                        workflow: r.get(11)?,
                        task: r.get(12)?,
                    })
                },
            )
            .map_err(sql)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql)?;
        drop(statement);
        let has_more = rows.len() > usize::from(query.limit);
        rows.truncate(usize::from(query.limit));
        after_rows();
        let mut records = Vec::new();
        for row in rows {
            let operation_kind = kind(&row.kind)?;
            let (label, mut truncated) = bounded(row.label, 256);
            let reason = row.reason.map(|value| {
                let (text, cut) = bounded(value, 512);
                truncated |= cut;
                text
            });
            let state = if matches!(operation_kind, HistoryKind::Application) {
                OperationState::Application {
                    status: serde_json::from_value(serde_json::json!(row.status))
                        .map_err(|e| e.to_string())?,
                }
            } else {
                OperationState::Execution {
                    status: serde_json::from_value(serde_json::json!(row.status))
                        .map_err(|e| e.to_string())?,
                }
            };
            let mut related = Vec::new();
            if let Some(workflow) = row.workflow {
                related.push(OperationRef {
                    context: context.clone(),
                    kind: HistoryKind::Workflow,
                    operation_id: workflow,
                });
            }
            if let Some(task) = row.task {
                related.push(OperationRef {
                    context: context.clone(),
                    kind: HistoryKind::Task,
                    operation_id: task,
                });
            }
            if let Some(steps) = row.steps {
                let steps: Vec<WorkflowStep> =
                    serde_json::from_str(&steps).map_err(|e| e.to_string())?;
                for step in steps {
                    if let (Some(id), Some(kind)) = (
                        step.operation_id,
                        match step.name.as_str() {
                            "import" => Some(HistoryKind::Import),
                            "build" | "start" => Some(HistoryKind::Task),
                            _ => None,
                        },
                    ) {
                        related.push(OperationRef {
                            context: context.clone(),
                            kind,
                            operation_id: id,
                        });
                    }
                }
            }
            let mut resources = Vec::new();
            let mut leases=transaction.prepare("SELECT id,kind,operation,state,scope,revision,reason,incarnation FROM resource_leases WHERE project=?1 AND state!='released' ORDER BY rowid DESC LIMIT 6").map_err(sql)?;
            let values = leases
                .query_map([&row.project], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, String>(7)?,
                    ))
                })
                .map_err(sql)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql)?;
            let resources_truncated = values.len() > 5;
            for (id, owner_kind, operation, state, scope, revision, reason, incarnation) in
                values.into_iter().take(5)
            {
                if incarnation != context.journal_incarnation {
                    return Err(
                        "Resource belongs to a foreign journal incarnation; no scope rebind".into(),
                    );
                }
                resources.push(OperationResource {
                    lease_id: id,
                    owner: OperationRef {
                        context: context.clone(),
                        kind: kind(&owner_kind)?,
                        operation_id: operation,
                    },
                    state: serde_json::from_value::<ObservedLeaseState>(serde_json::json!(state))
                        .map_err(|e| e.to_string())?,
                    scope: serde_json::from_value::<ObservedLeaseScope>(serde_json::json!(scope))
                        .map_err(|e| e.to_string())?,
                    revision: counter(revision)?,
                    reason: reason.map(|s| {
                        let (text, cut) = bounded(s, 256);
                        truncated |= cut;
                        text
                    }),
                });
            }
            records.push(OperationObservation {
                reference: OperationRef {
                    context: context.clone(),
                    kind: operation_kind,
                    operation_id: row.id,
                },
                project_id: row.project,
                state,
                revision: counter(row.revision)?,
                label,
                reason,
                text_truncated: truncated,
                accepted_at_ms: u64::try_from(row.created)
                    .map_err(|_| "Negative admission time")?,
                finished_at_ms: row
                    .finished
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| "Negative finish time")?,
                last_event_sequence: row.event.map(counter).transpose()?,
                related,
                resources,
                resources_truncated,
            });
        }
        let next_cursor = if has_more {
            records.last().map(|r| OperationCursor {
                context: context.clone(),
                query_sha256: query_sha256(query).expect("serializable query"),
                watermarks,
                before: HistoryCursor {
                    created_at_ms: r.accepted_at_ms,
                    kind: r.reference.kind,
                    operation_id: r.reference.operation_id.clone(),
                },
            })
        } else {
            None
        };
        let page = OperationPage {
            schema_version: 1,
            context: context.clone(),
            observation_id: uuid::Uuid::new_v4().to_string(),
            observed_at_ms: now_ms(),
            records,
            next_cursor,
            has_more,
            read_only: self.read_only,
        };
        transaction.commit().map_err(sql)?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_protocol::*;
    fn admit(store: &mut Store, key: &str, project: &str) -> TaskRecord {
        // Synthetic journal commands are never dispatched by these tests.
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
                        executable: key.into(),
                        arguments: vec![],
                        working_directory: ".".into(),
                    },
                },
                ResolvedCommand {
                    executable: key.into(),
                    arguments: vec![],
                    working_directory: ".".into(),
                    timeout_ms: 5000,
                },
                None,
            )
            .unwrap()
    }
    fn context(store: &Store) -> RequestContext {
        RequestContext {
            instance_id: "pages".into(),
            journal_incarnation: store.journal_authority().incarnation.clone(),
        }
    }
    fn query() -> OperationQuery {
        OperationQuery {
            project_id: None,
            kind: None,
            search: String::new(),
            limit: 25,
            cursor: None,
        }
    }
    #[test]
    fn ties_reopen_filter_and_clock_rollback_preserve_admission_selection() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        let ctx = context(&store);
        let mut ids = Vec::new();
        for n in 0..65 {
            let task = admit(&mut store, &format!("task-{n}"), "fixture");
            let mut snapshot = store.task_snapshot(&task.task_id).unwrap();
            snapshot.record.accepted_at_ms = 100;
            store.save(&snapshot).unwrap();
            ids.push(task.task_id);
        }
        let mut q = query();
        let first = store.operation_page(&ctx, &q).unwrap();
        assert_eq!(first.records.len(), 25);
        let late = admit(&mut store, "literal_%_Привет", "other");
        let mut snapshot = store.task_snapshot(&late.task_id).unwrap();
        snapshot.record.accepted_at_ms = 1;
        store.save(&snapshot).unwrap();
        let mut found = first
            .records
            .iter()
            .map(|r| r.reference.operation_id.clone())
            .collect::<Vec<_>>();
        q.cursor = first.next_cursor;
        drop(store);
        let store = Store::open(&path).unwrap();
        loop {
            let page = store.operation_page(&ctx, &q).unwrap();
            found.extend(
                page.records
                    .iter()
                    .map(|r| r.reference.operation_id.clone()),
            );
            if !page.has_more {
                break;
            }
            q.cursor = page.next_cursor;
        }
        found.sort();
        ids.sort();
        assert_eq!(found, ids);
        assert!(!found.contains(&late.task_id));
        let mut changed = q.clone();
        changed.search = "different".into();
        assert!(store.operation_page(&ctx, &changed).is_err());
        let mut foreign = ctx.clone();
        foreign.journal_incarnation = "foreign".into();
        assert!(store.operation_page(&foreign, &q).is_err());
        let mut search = query();
        search.search = "_%_Привет".into();
        let page = store.operation_page(&ctx, &search).unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].reference.operation_id, late.task_id);
        drop(store);
        let archived = Store::open_read_only(&path).unwrap();
        assert!(archived.operation_page(&ctx, &query()).unwrap().read_only);
    }
    #[test]
    fn row_revisions_and_resource_states_share_one_snapshot_during_a_writer() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = Store::open(&path).unwrap();
        let task = admit(&mut store, "waiting", "fixture");
        let owner = admit(&mut store, "owner", "fixture");
        let ctx = context(&store);
        let authority = store.journal_authority().clone();
        store.connection.execute("INSERT INTO resource_leases(id,project,kind,operation,attempt,incarnation,epoch,authority,acquired,state,scope,members,reason) VALUES('lease','fixture','task',?1,NULL,?2,?3,?4,1,'held','project_mutation','[]','before writer')",params![owner.task_id,authority.incarnation,i64::try_from(authority.epoch).unwrap(),authority.authority_id]).unwrap();
        let mut writer = Store::open(&path).unwrap();
        let page=store.operation_page_inner(&ctx,&query(),|| {
            writer.cancel(&task.task_id).unwrap();
            writer.connection.execute("UPDATE resource_leases SET state='uncertain',reason='after writer',revision=revision+1 WHERE id='lease'",[]).unwrap();
        }).unwrap();
        let old = page
            .records
            .iter()
            .find(|r| r.reference.operation_id == task.task_id)
            .unwrap();
        assert!(matches!(
            old.state,
            OperationState::Execution {
                status: TaskStatus::Queued
            }
        ));
        assert_eq!(old.revision, "0");
        assert!(matches!(old.resources[0].state, ObservedLeaseState::Held));
        assert_eq!(old.resources[0].revision, "0");
        let fresh = store.operation_page(&ctx, &query()).unwrap();
        let current = fresh
            .records
            .iter()
            .find(|r| r.reference.operation_id == task.task_id)
            .unwrap();
        assert!(matches!(
            current.state,
            OperationState::Execution {
                status: TaskStatus::Cancelled
            }
        ));
        assert!(matches!(
            current.resources[0].state,
            ObservedLeaseState::Uncertain
        ));
        assert_eq!(current.resources[0].revision, "1");
    }
    #[test]
    fn all_four_domains_and_application_links_are_scoped_without_commands_or_output() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temp.path().join("journal.sqlite")).unwrap();
        let task = admit(&mut store, "task", "fixture");
        let import = ImportSubmission {
            protocol_version: 1,
            request_id: "import".into(),
            idempotency_key: "import".into(),
            project_id: "fixture".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
        };
        store
            .accept_import(import, "shared".into(), "workspace".into())
            .unwrap();
        let command = CommandSource::Direct {
            executable: "not-dispatched".into(),
            arguments: vec![],
            working_directory: ".".into(),
        };
        let flow = store
            .accept_workflow(WorkflowSubmission {
                protocol_version: 1,
                request_id: "workflow".into(),
                idempotency_key: "workflow".into(),
                name: "Synthetic workflow".into(),
                project_id: "fixture".into(),
                snapshot_id: "snapshot".into(),
                manifest_sha256: "a".repeat(64),
                timeout_ms: 5000,
                build: command.clone(),
                start: command,
                readiness: ReadinessProbe {
                    stdout_contains: "READY".into(),
                    timeout_ms: 1000,
                    netwatch: None,
                },
                log_path: "log.txt".into(),
                artifacts: vec![],
            })
            .unwrap();
        let app = ApplicationRecord {
            application_id: "synthetic-app".into(),
            workflow_id: flow.workflow_id.clone(),
            project_id: "fixture".into(),
            task_id: task.task_id.clone(),
            status: ApplicationStatus::Stopped,
            process_id: None,
            created_at_ms: 1,
            ready_at_ms: None,
            finished_at_ms: Some(2),
            log_path: "log.txt".into(),
            reason: Some("synthetic".into()),
            failure_reason: None,
        };
        store.connection.execute("INSERT INTO applications(id,workflow,task,status,record) VALUES(?1,?2,?3,'stopped',?4)",params![app.application_id,app.workflow_id,app.task_id,serde_json::to_string(&app).unwrap()]).unwrap();
        let page = store.operation_page(&context(&store), &query()).unwrap();
        assert_eq!(page.records.len(), 4);
        for expected in [
            HistoryKind::Task,
            HistoryKind::Import,
            HistoryKind::Workflow,
            HistoryKind::Application,
        ] {
            assert!(page.records.iter().any(|r| r.reference.kind == expected));
        }
        let app = page
            .records
            .iter()
            .find(|r| r.reference.kind == HistoryKind::Application)
            .unwrap();
        assert!(app
            .related
            .iter()
            .any(|r| r.kind == HistoryKind::Workflow && r.operation_id == flow.workflow_id));
        assert!(app.related.iter().all(|r| r.context == page.context));
        let wire = serde_json::to_string(&page).unwrap();
        assert!(
            !wire.contains("resolvedCommand")
                && !wire.contains("stdout")
                && !wire.contains("arguments")
        );
    }
}
