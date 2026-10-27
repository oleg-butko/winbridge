//! One immutable intent, committed with the first effective cancellation change.
use super::{sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use winbridge_protocol::*;
pub(crate) struct StoredCancellation {
    pub at: u64,
    pub actor: Option<OperationCreator>,
    pub parent: Option<(HistoryKind, String)>,
}

impl Store {
    pub(super) fn migrate_cancellation(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE operation_cancellations(kind TEXT NOT NULL CHECK(kind IN('task','import','workflow','application')),operation TEXT NOT NULL,requested INTEGER NOT NULL CHECK(typeof(requested)='integer' AND requested>=0),actor TEXT NOT NULL CHECK(json_valid(actor)),parent_kind TEXT CHECK(parent_kind IN('workflow','application')),parent TEXT,PRIMARY KEY(kind,operation),CHECK((parent_kind IS NULL)=(parent IS NULL)),CHECK(parent_kind IS NULL OR parent_kind!=kind OR parent!=operation));
CREATE TRIGGER operation_cancellation_identity BEFORE INSERT ON operation_cancellations WHEN NOT CASE NEW.kind WHEN 'task' THEN EXISTS(SELECT 1 FROM tasks WHERE id=NEW.operation) WHEN 'import' THEN EXISTS(SELECT 1 FROM imports WHERE id=NEW.operation) WHEN 'workflow' THEN EXISTS(SELECT 1 FROM workflows WHERE id=NEW.operation) WHEN 'application' THEN EXISTS(SELECT 1 FROM applications WHERE id=NEW.operation) END OR (NEW.parent_kind='workflow' AND NOT EXISTS(SELECT 1 FROM workflows WHERE id=NEW.parent)) OR (NEW.parent_kind='application' AND NOT EXISTS(SELECT 1 FROM applications WHERE id=NEW.parent)) BEGIN SELECT RAISE(ABORT,'Cancellation requires existing operation/parent');END;
CREATE TRIGGER operation_cancellation_immutable BEFORE UPDATE ON operation_cancellations BEGIN SELECT RAISE(ABORT,'Cancellation intent is immutable');END;
CREATE TRIGGER operation_cancellation_retained BEFORE DELETE ON operation_cancellations BEGIN SELECT RAISE(ABORT,'Cancellation intent is retained');END;
PRAGMA user_version=19;").map_err(sql)
    }
    pub(crate) fn record_cancellation(
        c: &Connection,
        kind: &str,
        id: &str,
        actor: Option<&OperationCreator>,
        parent: Option<(HistoryKind, &str)>,
    ) -> Result<()> {
        if actor.is_some_and(|a| !a.valid()) {
            return Err("Invalid trusted cancellation actor".into());
        }
        let (pk, p) = match parent {
            Some((k, id)) => (Some(super::attribution::kind(k)), Some(id)),
            None => (None, None),
        };
        let at = i64::try_from(super::now_ms())
            .map_err(|_| "Cancellation time outside journal range")?;
        let n=c.execute("INSERT INTO operation_cancellations(kind,operation,requested,actor,parent_kind,parent) VALUES(?1,?2,?3,?4,?5,?6)",params![kind,id,at,serde_json::to_string(&actor).map_err(|e|e.to_string())?,pk,p]).map_err(sql)?;
        if n != 1 {
            return Err("Cancellation intent not stored; cancellation rolled back".into());
        }
        Ok(())
    }
    pub(crate) fn cancellation_on(
        c: &Connection,
        kind: &str,
        id: &str,
    ) -> Result<Option<StoredCancellation>> {
        let row:Option<(i64,String,Option<String>,Option<String>)>=c.query_row("SELECT requested,actor,parent_kind,parent FROM operation_cancellations WHERE kind=?1 AND operation=?2",params![kind,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql)?;
        row.map(|(at, raw, pk, p)| {
            let at = u64::try_from(at).map_err(|_| "Negative cancellation time")?;
            let actor: Option<OperationCreator> =
                serde_json::from_str(&raw).map_err(|_| "Corrupt cancellation actor")?;
            if actor.as_ref().is_some_and(|a| !a.valid()) || at > 9_007_199_254_740_991 {
                return Err("Invalid stored cancellation intent".into());
            }
            let parent = match (pk.as_deref(), p) {
                (None, None) => None,
                (Some("workflow"), Some(p)) => Some((HistoryKind::Workflow, p)),
                (Some("application"), Some(p)) => Some((HistoryKind::Application, p)),
                _ => return Err("Invalid cancellation parent".into()),
            };
            Ok(StoredCancellation { at, actor, parent })
        })
        .transpose()
    }
    pub fn operation_cancellation(
        &self,
        context: &RequestContext,
        q: &OperationAttributionQuery,
        generation: &str,
    ) -> Result<OperationCancellation> {
        // Same scope/operation existence checks as creator observation; no mutation.
        let reference = self
            .operation_attribution(context, q, generation)?
            .reference;
        let intent = Self::cancellation_on(
            &self.connection,
            super::attribution::kind(q.kind),
            &q.operation_id,
        )?
        .map(|i| CancellationIntent {
            requested_at_ms: i.at,
            actor: i.actor,
            inherited_from: i.parent.map(|(kind, operation_id)| OperationRef {
                context: context.clone(),
                kind,
                operation_id,
            }),
        });
        Ok(OperationCancellation {
            schema_version: 1,
            reference,
            intent,
            provider: build_identity(),
            worker_generation: generation.into(),
            observed_at_ms: super::now_ms(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_core::task::TaskEvent;
    fn actor(id: &str) -> OperationCreator {
        OperationCreator::Controller {
            worker_generation: "worker".into(),
            controller_id: id.into(),
            controller_node_id: "same-node".into(),
            controller_generation: format!("generation-{id}"),
            session_id: format!("session-{id}"),
        }
    }
    fn admit(s: &mut Store, key: &str) -> TaskRecord {
        s.accept_attributed(
            TaskSubmission {
                protocol_version: 1,
                request_id: key.into(),
                project_id: "p".into(),
                idempotency_key: key.into(),
                timeout_ms: 1000,
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
                timeout_ms: 1000,
            },
            None,
            None,
            Some(&actor("creator")),
        )
        .unwrap()
    }
    #[test]
    fn task_first_cancel_is_immutable_and_late_terminal_cancel_stays_unattributed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        let task = admit(&mut s, "first");
        let cancelled = s
            .cancel_attributed(&task.task_id, Some(&actor("b")), None)
            .unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        let first = Store::cancellation_on(&s.connection, "task", &task.task_id)
            .unwrap()
            .unwrap();
        let revision = s.task_snapshot(&task.task_id).unwrap().revision();
        s.cancel_attributed(&task.task_id, Some(&actor("a")), None)
            .unwrap();
        assert_eq!(s.task_snapshot(&task.task_id).unwrap().revision(), revision);
        assert_eq!(
            Store::cancellation_on(&s.connection, "task", &task.task_id)
                .unwrap()
                .unwrap()
                .actor,
            Some(actor("b"))
        );
        assert_eq!(
            Store::creator_on(&s.connection, "task", &task.task_id)
                .unwrap()
                .0,
            Some(actor("creator"))
        );
        assert!(s
            .connection
            .execute("UPDATE operation_cancellations SET actor='null'", [])
            .is_err());
        assert!(s
            .connection
            .execute("DELETE FROM operation_cancellations", [])
            .is_err());
        let legacy = admit(&mut s, "legacy");
        s.transition(&legacy.task_id, TaskEvent::CancellationRequested)
            .unwrap();
        s.cancel_attributed(&legacy.task_id, Some(&actor("a")), None)
            .unwrap();
        assert!(
            Store::cancellation_on(&s.connection, "task", &legacy.task_id)
                .unwrap()
                .is_none()
        );
        drop(s);
        let s = Store::open(&path).unwrap();
        let reopened = Store::cancellation_on(&s.connection, "task", &task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(reopened.at, first.at);
        assert_eq!(reopened.actor, first.actor);
    }
    #[test]
    fn concurrent_cancels_keep_one_actor_and_ignored_insert_rolls_back_the_effect() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        let t = admit(&mut s, "race");
        drop(s);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writers = ["a", "b"].map(|id| {
            let path = path.clone();
            let barrier = barrier.clone();
            let task_id = t.task_id.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(&path).unwrap();
                barrier.wait();
                s.cancel_attributed(&task_id, Some(&actor(id)), None)
                    .unwrap();
            })
        });
        for w in writers {
            w.join().unwrap();
        }
        let mut s = Store::open(&path).unwrap();
        let first = Store::cancellation_on(&s.connection, "task", &t.task_id)
            .unwrap()
            .unwrap();
        assert!(first.actor == Some(actor("a")) || first.actor == Some(actor("b")));
        let pending = admit(&mut s, "rollback");
        let before = serde_json::to_value(&pending).unwrap();
        s.connection.execute_batch("CREATE TRIGGER ignore_cancel BEFORE INSERT ON operation_cancellations BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        assert!(s
            .cancel_attributed(&pending.task_id, Some(&actor("a")), None)
            .is_err());
        assert_eq!(
            serde_json::to_value(s.get(&pending.task_id).unwrap()).unwrap(),
            before
        );
        assert!(
            Store::cancellation_on(&s.connection, "task", &pending.task_id)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn queued_import_cancel_does_not_claim_to_have_run_and_metadata_failure_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Store::open(&root.path().join("journal.sqlite")).unwrap();
        let import = s
            .accept_import(
                ImportSubmission {
                    protocol_version: 1,
                    request_id: "r".into(),
                    project_id: "p".into(),
                    snapshot_id: "snapshot".into(),
                    manifest_sha256: "a".repeat(64),
                    idempotency_key: "import".into(),
                    timeout_ms: 1000,
                },
                "unread-shared".into(),
                "unused-workspace".into(),
            )
            .unwrap();
        s.connection.execute_batch("CREATE TRIGGER ignore_cancel BEFORE INSERT ON operation_cancellations BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        assert!(s
            .cancel_import_attributed(&import.import_id, Some(&actor("b")), None)
            .is_err());
        assert_eq!(
            s.get_import(&import.import_id).unwrap().status,
            TaskStatus::Queued
        );
        s.connection
            .execute_batch("DROP TRIGGER ignore_cancel;")
            .unwrap();
        assert_eq!(
            s.cancel_import_attributed(&import.import_id, Some(&actor("b")), None)
                .unwrap()
                .status,
            TaskStatus::Cancelled
        );
        s.cancel_import_attributed(&import.import_id, Some(&actor("a")), None)
            .unwrap();
        assert_eq!(
            Store::cancellation_on(&s.connection, "import", &import.import_id)
                .unwrap()
                .unwrap()
                .actor,
            Some(actor("b"))
        );
    }
    #[test]
    fn migration_eighteen_preserves_creator_and_does_not_backfill_old_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        let task = admit(&mut s, "old");
        s.transition(&task.task_id, TaskEvent::CancellationRequested)
            .unwrap();
        s.connection.execute_batch("DROP TABLE operation_cancellations;PRAGMA user_version=18;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(s);
        let mut s = Store::open(&path).unwrap();
        s.cancel_attributed(&task.task_id, Some(&actor("b")), None)
            .unwrap();
        assert!(Store::cancellation_on(&s.connection, "task", &task.task_id)
            .unwrap()
            .is_none());
        assert_eq!(
            Store::creator_on(&s.connection, "task", &task.task_id)
                .unwrap()
                .0,
            Some(actor("creator"))
        );
        let backups: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.extension().is_some_and(|e| e == "sqlite")
                    && p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("journal-v18-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let old =
            Connection::open_with_flags(&backups[0], rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            18
        );
        assert_eq!(
            Store::creator_on(&old, "task", &task.task_id).unwrap().0,
            Some(actor("creator"))
        );
    }
    #[test]
    fn active_update_eighteen_is_refused_before_backup_or_schema_writes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let (s, _, _) = super::super::shutdown_metadata_fixture(&path);
        s.connection.execute_batch("DROP TABLE operation_cancellations;PRAGMA user_version=18;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(s);
        let before = std::fs::read(&path).unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("journal-v18-")));
    }
}
