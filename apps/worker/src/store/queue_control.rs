//! Durable roots, queue FSM and command receipts. All writes share the Store executor.
use super::{now_ms, sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use winbridge_protocol::*;

fn mode(value: &str) -> Result<QueueMode> {
    serde_json::from_value(serde_json::Value::String(value.into())).map_err(|e| e.to_string())
}

fn mode_text(value: QueueMode) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|e| e.to_string())?
        .as_str()
        .map(String::from)
        .ok_or("Invalid queue mode".into())
}
fn state(c: &Connection) -> Result<(i64, QueueMode, Option<String>, u64)> {
    let (revision, text, last, time): (i64, String, Option<String>, i64) = c
        .query_row(
            "SELECT revision,mode,last_command,changed FROM queue_control WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(sql)?;
    Ok((
        revision,
        mode(&text)?,
        last,
        u64::try_from(time).map_err(|_| "Invalid queue control timestamp")?,
    ))
}
fn pending(c: &Connection) -> Result<Vec<QueueProjectPending>> {
    let mut stmt=c.prepare("SELECT project,count(*) FROM (
        SELECT r.project FROM queue_roots r JOIN tasks t ON r.kind='task' AND r.id=t.id WHERE json_extract(t.record,'$.status') IN ('queued','running')
        UNION ALL SELECT r.project FROM queue_roots r JOIN imports i ON r.kind='import' AND r.id=i.id WHERE json_extract(i.record,'$.status') IN ('queued','running')
        UNION ALL SELECT r.project FROM queue_roots r JOIN workflows w ON r.kind='workflow' AND r.id=w.id WHERE w.status IN ('queued','running')
        ) GROUP BY project ORDER BY project").map_err(sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(sql)?;
    rows.map(|row| {
        let (p, n) = row.map_err(sql)?;
        Ok(QueueProjectPending {
            project_id: p,
            pending_roots: u32::try_from(n).map_err(|_| "Invalid pending roots count")?,
        })
    })
    .collect()
}
fn observation(
    c: &Connection,
    policy: Option<&QueuePolicy>,
    context: RequestContext,
    read_only: bool,
) -> Result<QueueControlObservation> {
    let (revision, mode, last_command_id, changed_at_ms) = state(c)?;
    let projects = pending(c)?;
    let pending_roots = projects.iter().try_fold(0u32, |s, p| {
        s.checked_add(p.pending_roots)
            .ok_or("Pending root count overflow")
    })?;
    let blockers = queue_blockers(mode, policy, pending_roots, &projects);
    Ok(QueueControlObservation {
        schema_version: 1,
        provider: build_identity(),
        context,
        revision: revision.to_string(),
        mode,
        policy: policy.cloned(),
        pending_roots,
        projects,
        blockers,
        last_command_id,
        changed_at_ms,
        observed_at_ms: now_ms(),
        read_only,
    })
}
fn control_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn reject(code: QueueControlError, message: &str) -> QueueControlResult {
    QueueControlResult::Rejected {
        code,
        message: message.into(),
    }
}
impl Store {
    pub(super) fn migrate_queue_control(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE queue_control(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL CHECK(typeof(revision)='integer' AND revision>=0),mode TEXT NOT NULL CHECK(mode IN ('running','admission_paused','dispatch_paused','paused')),last_command TEXT,changed INTEGER NOT NULL CHECK(typeof(changed)='integer' AND changed>=0));
            INSERT INTO queue_control VALUES(1,0,'running',NULL,0);
            CREATE TABLE queue_commands(id TEXT PRIMARY KEY,payload TEXT NOT NULL,receipt TEXT NOT NULL CHECK(json_valid(receipt)));
            CREATE TABLE queue_roots(kind TEXT NOT NULL CHECK(kind IN ('task','import','workflow')),id TEXT NOT NULL,project TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE INDEX queue_roots_project ON queue_roots(project,kind,id);
            CREATE INDEX queue_pending_tasks ON tasks(project,id) WHERE json_extract(record,'$.status') IN ('queued','running');
            CREATE INDEX queue_pending_imports ON imports(project,id) WHERE json_extract(record,'$.status') IN ('queued','running');
            CREATE INDEX queue_pending_workflows ON workflows(project,id) WHERE status IN ('queued','running');
            INSERT INTO queue_roots SELECT 'workflow',id,project FROM workflows;
            INSERT INTO queue_roots SELECT 'task',t.id,t.project FROM tasks t WHERE NOT EXISTS(SELECT 1 FROM workflows w,json_each(w.record,'$.steps') s WHERE json_extract(s.value,'$.name') IN ('build','start') AND json_extract(s.value,'$.operationId')=t.id);
            INSERT INTO queue_roots SELECT 'import',i.id,i.project FROM imports i WHERE NOT EXISTS(SELECT 1 FROM workflows w,json_each(w.record,'$.steps') s WHERE json_extract(s.value,'$.name')='import' AND json_extract(s.value,'$.operationId')=i.id);
            CREATE TRIGGER queue_state_update_guard BEFORE UPDATE ON queue_control WHEN NEW.singleton!=OLD.singleton OR NEW.revision!=OLD.revision+1 OR NEW.mode=OLD.mode OR NEW.last_command IS NULL BEGIN SELECT RAISE(ABORT,'Queue state requires next revision and a changed mode'); END;
            CREATE TRIGGER queue_state_insert_guard BEFORE INSERT ON queue_control WHEN EXISTS(SELECT 1 FROM queue_control) BEGIN SELECT RAISE(ABORT,'Queue state already exists'); END;
            CREATE TRIGGER queue_state_delete_guard BEFORE DELETE ON queue_control BEGIN SELECT RAISE(ABORT,'Queue state cannot be deleted'); END;
            CREATE TRIGGER queue_commands_update_guard BEFORE UPDATE ON queue_commands BEGIN SELECT RAISE(ABORT,'Queue receipts are immutable'); END;
            CREATE TRIGGER queue_commands_delete_guard BEFORE DELETE ON queue_commands BEGIN SELECT RAISE(ABORT,'Queue receipts are retained'); END;
            CREATE TRIGGER queue_roots_update_guard BEFORE UPDATE ON queue_roots BEGIN SELECT RAISE(ABORT,'Root identity is immutable'); END;
            CREATE TRIGGER queue_roots_delete_guard BEFORE DELETE ON queue_roots BEGIN SELECT RAISE(ABORT,'Root identity is retained'); END;
            PRAGMA user_version=13;").map_err(sql)
    }
    pub(crate) fn configure_queue_policy(&mut self, policy: Option<QueuePolicy>) -> Result<()> {
        if policy.as_ref().is_some_and(|p| !valid_queue_policy(p)) {
            return Err("Invalid queue policy".into());
        }
        self.queue_policy = policy;
        Ok(())
    }
    pub(crate) fn loaded_queue_policy(&self) -> Option<QueuePolicy> {
        self.queue_policy.clone()
    }
    pub(crate) fn require_root_admission(
        c: &Connection,
        policy: Option<&QueuePolicy>,
        project_id: &str,
    ) -> Result<()> {
        let (_, mode, _, _) = state(c)?;
        if !mode.admission_paused() && policy.is_none() {
            return Ok(());
        }
        let projects = pending(c)?;
        let total = projects.iter().try_fold(0u32, |s, p| {
            s.checked_add(p.pending_roots)
                .ok_or("Pending root count overflow")
        })?;
        let project = projects
            .iter()
            .find(|p| p.project_id == project_id)
            .map_or(0, |p| p.pending_roots);
        if let Some(reason) = winbridge_core::queue_control::admission_blocker(
            mode, policy, total, project, project_id,
        ) {
            return Err(match reason {
                QueueBlocker::AdmissionPaused => {
                    "Новые запросы не приняты: приём приостановлен пользователем".into()
                }
                QueueBlocker::GlobalLimit { limit } => format!(
                    "Новый запрос не принят: общий лимит незавершённых запросов {total}/{limit}"
                ),
                QueueBlocker::ProjectLimit { project_id, limit } => {
                    format!("Новый запрос не принят: лимит проекта {project_id} {project}/{limit}")
                }
                QueueBlocker::DispatchPaused => unreachable!(),
            });
        }
        Ok(())
    }
    pub(crate) fn insert_queue_root(
        c: &Connection,
        kind: &str,
        id: &str,
        project: &str,
    ) -> Result<()> {
        let changed = c
            .execute(
                "INSERT INTO queue_roots(kind,id,project) VALUES(?1,?2,?3)",
                params![kind, id, project],
            )
            .map_err(sql)?;
        if changed != 1 {
            return Err("Root index not committed; admission rejected".into());
        }
        Ok(())
    }
    pub(super) fn require_dispatch(c: &Connection) -> Result<()> {
        if state(c)?.1.dispatch_paused() {
            return Err("resource busy: Запуск приостановлен пользователем; уже запущенные исполнители продолжают работу".into());
        }
        Ok(())
    }
    pub(crate) fn dispatch_wait(&self) -> Result<Option<String>> {
        match Self::require_dispatch(&self.connection) {
            Ok(()) => Ok(None),
            Err(reason) if reason.starts_with("resource busy:") => Ok(Some(reason)),
            Err(error) => Err(error),
        }
    }
    pub(crate) fn queue_observation(
        &self,
        context: RequestContext,
    ) -> Result<QueueControlObservation> {
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        let view = observation(&tx, self.queue_policy.as_ref(), context, self.read_only)?;
        tx.commit().map_err(sql)?;
        Ok(view)
    }
    pub(crate) fn queue_control(
        &mut self,
        context: RequestContext,
        command: QueueControlCommand,
    ) -> Result<QueueControlResult> {
        if context.journal_incarnation != self.authority.incarnation {
            return Ok(reject(
                QueueControlError::ScopeMismatch,
                "Queue control belongs to another journal",
            ));
        }
        let policy = self.queue_policy.clone();
        let read_only = self.read_only;
        if let QueueControlCommand::Observe { command_id } = &command {
            if command_id.as_ref().is_some_and(|id| !control_id(id)) {
                return Ok(reject(
                    QueueControlError::InvalidRequest,
                    "Invalid queue command ID",
                ));
            }
            let tx = self.connection.unchecked_transaction().map_err(sql)?;
            let receipt = command_id
                .as_ref()
                .map(|id| {
                    tx.query_row(
                        "SELECT receipt FROM queue_commands WHERE id=?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql)
                })
                .transpose()?
                .flatten()
                .map(|r| serde_json::from_str::<QueueControlReceipt>(&r).map_err(|e| e.to_string()))
                .transpose()?;
            let observation = observation(&tx, policy.as_ref(), context, read_only)?;
            tx.commit().map_err(sql)?;
            return Ok(QueueControlResult::Observed {
                observation,
                receipt,
            });
        }
        if read_only {
            return Ok(reject(
                QueueControlError::ReadOnly,
                "Read-only journal cannot change queue control",
            ));
        }
        self.guard_callback(None, None)?;
        let QueueControlCommand::Set {
            ref command_id,
            ref expected_revision,
            mode: requested,
        } = &command
        else {
            unreachable!()
        };
        if !control_id(command_id) || !valid_queue_revision(expected_revision) {
            return Ok(reject(
                QueueControlError::InvalidRequest,
                "Invalid queue command/revision",
            ));
        }
        let payload = serde_json::to_string(&command).map_err(|e| e.to_string())?;
        let tx = self.write_transaction(TransactionBehavior::Immediate)?;
        let old: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,receipt FROM queue_commands WHERE id=?1",
                [command_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((stored, receipt)) = old {
            if stored != payload {
                return Ok(reject(
                    QueueControlError::KeyConflict,
                    "Command ID belongs to another queue change",
                ));
            }
            let receipt = serde_json::from_str(&receipt).map_err(|e| e.to_string())?;
            let observation = observation(&tx, policy.as_ref(), context, read_only)?;
            tx.commit().map_err(sql)?;
            return Ok(QueueControlResult::Applied {
                observation,
                receipt,
            });
        }
        let (revision, current, _, changed_at_ms) = state(&tx)?;
        if expected_revision != &revision.to_string() {
            return Ok(reject(
                QueueControlError::RevisionConflict,
                "Queue revision changed; refresh and make a new explicit decision",
            ));
        }
        let changed = current != *requested;
        let next = if changed {
            revision.checked_add(1).ok_or("Queue revision exhausted")?
        } else {
            revision
        };
        let at = if changed { now_ms() } else { changed_at_ms };
        let receipt = QueueControlReceipt {
            context: context.clone(),
            command_id: command_id.clone(),
            from_revision: revision.to_string(),
            to_revision: next.to_string(),
            mode: *requested,
            changed,
            changed_at_ms: at,
        };
        if changed {
            let at_sql = i64::try_from(at).map_err(|_| "Queue timestamp exceeds SQLite range")?;
            let n=tx.execute("UPDATE queue_control SET revision=?1,mode=?2,last_command=?3,changed=?4 WHERE singleton=1 AND revision=?5",params![next,mode_text(*requested)?,command_id,at_sql,revision]).map_err(sql)?;
            if n != 1 {
                return Err("Queue state CAS failed; no acknowledgement".into());
            }
        }
        let n = tx
            .execute(
                "INSERT INTO queue_commands(id,payload,receipt) VALUES(?1,?2,?3)",
                params![
                    command_id,
                    payload,
                    serde_json::to_string(&receipt).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        if n != 1 {
            return Err("Queue receipt not stored; no acknowledgement".into());
        }
        let observation = observation(&tx, policy.as_ref(), context, read_only)?;
        tx.commit().map_err(sql)?;
        Ok(QueueControlResult::Applied {
            observation,
            receipt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    fn context(s: &Store) -> RequestContext {
        RequestContext {
            instance_id: "queue-test".into(),
            journal_incarnation: s.journal_authority().incarnation.clone(),
        }
    }
    fn task(key: &str, project: &str) -> TaskSubmission {
        TaskSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            project_id: project.into(),
            timeout_ms: 5000,
            snapshot_id: None,
            command: command(),
        }
    }
    fn command() -> CommandSource {
        CommandSource::Direct {
            executable: "synthetic-never-run".into(),
            arguments: vec![],
            working_directory: ".".into(),
        }
    }
    fn resolved() -> ResolvedCommand {
        ResolvedCommand {
            executable: "synthetic-never-run".into(),
            arguments: vec![],
            working_directory: ".".into(),
            timeout_ms: 5000,
        }
    }
    fn import(key: &str, project: &str) -> ImportSubmission {
        ImportSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            project_id: project.into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
        }
    }
    fn workflow(key: &str, project: &str) -> WorkflowSubmission {
        WorkflowSubmission {
            protocol_version: 1,
            request_id: key.into(),
            idempotency_key: key.into(),
            project_id: project.into(),
            name: "queue root".into(),
            snapshot_id: "snapshot".into(),
            manifest_sha256: "a".repeat(64),
            timeout_ms: 5000,
            build: command(),
            start: command(),
            readiness: ReadinessProbe {
                stdout_contains: "READY".into(),
                timeout_ms: 1000,
                netwatch: None,
            },
            log_path: "logs/app.log".into(),
            artifacts: vec![],
        }
    }
    fn set(id: &str, rev: &str, mode: QueueMode) -> QueueControlCommand {
        QueueControlCommand::Set {
            command_id: id.into(),
            expected_revision: rev.into(),
            mode,
        }
    }
    fn apply(s: &mut Store, id: &str, rev: &str, mode: QueueMode) -> QueueControlReceipt {
        let c = context(s);
        let QueueControlResult::Applied { receipt, .. } =
            s.queue_control(c, set(id, rev, mode)).unwrap()
        else {
            panic!("control not applied")
        };
        receipt
    }
    fn view(s: &Store) -> QueueControlObservation {
        s.queue_observation(context(s)).unwrap()
    }
    fn child(s: &mut Store, flow: &WorkflowRecord) {
        let mut snapshot = s.workflow_snapshot(&flow.workflow_id).unwrap();
        s.accept_workflow_import(
            &mut snapshot,
            import("child-import", &flow.submission.project_id),
            "unused-shared".into(),
            "unused-workspace".into(),
        )
        .unwrap();
        let mut request = task("child-build", &flow.submission.project_id);
        request.snapshot_id = Some("snapshot".into());
        s.accept_workflow_task(
            &mut snapshot,
            1,
            request,
            resolved(),
            Some(VerifiedSnapshotRef {
                snapshot_id: "snapshot".into(),
                manifest_sha256: "a".repeat(64),
                source: SnapshotSource::GitCommit {
                    commit: "b".repeat(40),
                },
            }),
            None,
        )
        .unwrap();
    }
    #[test]
    fn mixed_roots_duplicates_pause_child_exemption_and_terminal_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("journal.sqlite")).unwrap();
        s.begin_authority().unwrap();
        s.configure_queue_policy(Some(QueuePolicy {
            max_pending_roots: 3,
            max_pending_roots_per_project: Some(2),
        }))
        .unwrap();
        let first = s.accept(task("first", "a"), resolved(), None).unwrap();
        let imp = s
            .accept_import(import("imp", "a"), "unused".into(), "unused".into())
            .unwrap();
        assert!(s
            .accept(task("project-full", "a"), resolved(), None)
            .unwrap_err()
            .contains("лимит проекта"));
        let flow = s.accept_workflow(workflow("flow", "b")).unwrap();
        assert_eq!(view(&s).pending_roots, 3);
        assert!(s
            .accept(task("global-full", "c"), resolved(), None)
            .unwrap_err()
            .contains("общий лимит"));
        apply(&mut s, "pause", "0", QueueMode::Paused);
        assert_eq!(
            s.accept(task("first", "a"), resolved(), None)
                .unwrap()
                .task_id,
            first.task_id
        );
        assert_eq!(
            s.accept_import(import("imp", "a"), "unused".into(), "unused".into())
                .unwrap()
                .import_id,
            imp.import_id
        );
        assert_eq!(
            s.accept_workflow(workflow("flow", "b"))
                .unwrap()
                .workflow_id,
            flow.workflow_id
        );
        let mut changed = task("first", "a");
        changed.timeout_ms += 1;
        assert!(s
            .accept(changed, resolved(), None)
            .unwrap_err()
            .contains("different"));
        assert!(s
            .accept(task("paused", "c"), resolved(), None)
            .unwrap_err()
            .contains("приостановлен"));
        child(&mut s, &flow);
        assert_eq!(
            view(&s).pending_roots,
            3,
            "workflow children must not consume root quota"
        );
        assert!(s
            .claim_task(&first.task_id)
            .unwrap_err()
            .contains("Запуск приостановлен"));
        s.cancel(&first.task_id).unwrap();
        assert_eq!(view(&s).pending_roots, 2);
        apply(&mut s, "resume", "1", QueueMode::Running);
        s.accept(task("next", "a"), resolved(), None).unwrap();
        assert_eq!(view(&s).pending_roots, 3);
        s.configure_queue_policy(Some(QueuePolicy {
            max_pending_roots: 1,
            max_pending_roots_per_project: None,
        }))
        .unwrap();
        assert_eq!(
            view(&s).pending_roots,
            3,
            "lowering policy must not evict accepted roots"
        );
        assert!(s.accept(task("lowered", "c"), resolved(), None).is_err());
    }
    #[test]
    fn concurrent_root_admissions_and_failed_index_insert_cannot_overbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|n| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut s = Store::open(&path).unwrap();
                    s.configure_queue_policy(Some(QueuePolicy {
                        max_pending_roots: 1,
                        max_pending_roots_per_project: None,
                    }))
                    .unwrap();
                    barrier.wait();
                    s.accept(task(&format!("race-{n}"), "a"), resolved(), None)
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(Result::is_ok)
                .count(),
            1
        );
        assert_eq!(view(&s).pending_roots, 1);
        s.connection.execute_batch("CREATE TRIGGER root_fault BEFORE INSERT ON queue_roots BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(s
            .accept(task("must-rollback", "b"), resolved(), None)
            .is_err());
        assert!(s.existing(&task("must-rollback", "b")).unwrap().is_none());
        assert_eq!(view(&s).pending_roots, 1);
    }
    #[test]
    fn receipt_failure_cas_replay_restart_readonly_and_stale_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        let ctx = context(&s);
        s.connection.execute_batch("CREATE TRIGGER receipt_fault BEFORE INSERT ON queue_commands BEGIN SELECT RAISE(IGNORE); END;").unwrap();
        assert!(s
            .queue_control(ctx.clone(), set("lost", "0", QueueMode::Paused))
            .is_err());
        assert_eq!(view(&s).revision, "0");
        assert_eq!(view(&s).mode, QueueMode::Running);
        s.connection
            .execute_batch("DROP TRIGGER receipt_fault;")
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles = [QueueMode::AdmissionPaused, QueueMode::DispatchPaused]
            .into_iter()
            .enumerate()
            .map(|(n, mode)| {
                let path = path.clone();
                let ctx = ctx.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut s = Store::open(&path).unwrap();
                    barrier.wait();
                    let cmd = set(&format!("race-{n}"), "0", mode);
                    let result = s.queue_control(ctx, cmd.clone()).unwrap();
                    (cmd, result)
                })
            })
            .collect::<Vec<_>>();
        let outcomes = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|(_, r)| matches!(r, QueueControlResult::Applied { .. }))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|(_, r)| matches!(
                    r,
                    QueueControlResult::Rejected {
                        code: QueueControlError::RevisionConflict,
                        ..
                    }
                ))
                .count(),
            1
        );
        let original = outcomes
            .into_iter()
            .find(|(_, r)| matches!(r, QueueControlResult::Applied { .. }))
            .unwrap()
            .0;
        let mut next = Store::open(&path).unwrap();
        next.begin_authority().unwrap();
        assert!(s
            .queue_control(ctx.clone(), set("stale-owner", "1", QueueMode::Paused))
            .is_err());
        apply(&mut next, "newer", "1", QueueMode::Paused);
        let QueueControlResult::Applied {
            receipt,
            observation,
        } = next.queue_control(ctx.clone(), original.clone()).unwrap()
        else {
            panic!("retained duplicate")
        };
        assert_eq!(receipt.to_revision, "1");
        assert_eq!(observation.revision, "2");
        assert_eq!(observation.mode, QueueMode::Paused);
        let QueueControlCommand::Set { command_id, .. } = original else {
            unreachable!()
        };
        assert!(matches!(
            next.queue_control(ctx.clone(), set(&command_id, "0", QueueMode::Paused))
                .unwrap(),
            QueueControlResult::Rejected {
                code: QueueControlError::KeyConflict,
                ..
            }
        ));
        assert!(matches!(
            next.queue_control(
                ctx.clone(),
                QueueControlCommand::Observe {
                    command_id: Some(command_id)
                }
            )
            .unwrap(),
            QueueControlResult::Observed {
                receipt: Some(_),
                ..
            }
        ));
        let mut readonly = Store::open_read_only(&path).unwrap();
        assert!(matches!(
            readonly
                .queue_control(ctx, set("readonly", "2", QueueMode::Running))
                .unwrap(),
            QueueControlResult::Rejected {
                code: QueueControlError::ReadOnly,
                ..
            }
        ));
        assert_eq!(view(&readonly).revision, "2");
    }
    #[test]
    fn schema_twelve_backfill_excludes_children_and_creates_consistent_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        s.begin_authority().unwrap();
        let flow = s.accept_workflow(workflow("flow", "b")).unwrap();
        child(&mut s, &flow);
        let root = s.accept(task("root", "a"), resolved(), None).unwrap();
        assert_eq!(view(&s).pending_roots, 2);
        drop(s);
        // A reproducible schema-12 fixture, not a user journal or backup.
        let c = Connection::open(&path).unwrap();
        c.execute_batch("DROP TABLE queue_roots;DROP TABLE queue_commands;DROP TABLE queue_control;DROP INDEX queue_pending_tasks;DROP INDEX queue_pending_imports;DROP INDEX queue_pending_workflows;PRAGMA user_version=12;").unwrap();
        drop(c);
        let s = Store::open(&path).unwrap();
        assert_eq!(view(&s).pending_roots, 2);
        assert_eq!(s.get(&root.task_id).unwrap().task_id, root.task_id);
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("journal-v12-"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let c = Connection::open(backups[0].path()).unwrap();
        let version: u32 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 12);
        let n: u32 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='queue_control'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
}
