//! Creator identity is committed with fresh acceptance, never with a retry.
use super::{sql, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use winbridge_protocol::{HistoryKind, OperationCreator, OperationRef as WireRef, RequestContext};

pub(super) fn kind(k: HistoryKind) -> &'static str {
    match k {
        HistoryKind::Task => "task",
        HistoryKind::Import => "import",
        HistoryKind::Workflow => "workflow",
        HistoryKind::Application => "application",
    }
}

impl Store {
    pub(super) fn migrate_attribution(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE operation_creators(kind TEXT NOT NULL CHECK(kind IN('task','import','workflow','application')),operation TEXT NOT NULL,creator TEXT NOT NULL CHECK(json_valid(creator)),parent TEXT REFERENCES workflows(id),PRIMARY KEY(kind,operation),CHECK(kind!='workflow' OR parent IS NULL));
CREATE TRIGGER operation_creator_identity BEFORE INSERT ON operation_creators WHEN NOT CASE NEW.kind WHEN 'task' THEN EXISTS(SELECT 1 FROM tasks WHERE id=NEW.operation) WHEN 'import' THEN EXISTS(SELECT 1 FROM imports WHERE id=NEW.operation) WHEN 'workflow' THEN EXISTS(SELECT 1 FROM workflows WHERE id=NEW.operation) WHEN 'application' THEN EXISTS(SELECT 1 FROM applications WHERE id=NEW.operation) END BEGIN SELECT RAISE(ABORT,'Creator requires accepted operation');END;
CREATE TRIGGER operation_creator_immutable BEFORE UPDATE ON operation_creators BEGIN SELECT RAISE(ABORT,'Operation creator is immutable');END;
CREATE TRIGGER operation_creator_retained BEFORE DELETE ON operation_creators BEGIN SELECT RAISE(ABORT,'Operation creator is retained');END;
PRAGMA user_version=18;").map_err(sql)
    }
    pub(crate) fn record_creator(
        c: &Connection,
        k: &str,
        id: &str,
        creator: Option<&OperationCreator>,
        parent: Option<&str>,
    ) -> Result<()> {
        if creator.is_some_and(|c| !c.valid()) {
            return Err("Invalid trusted creator identity".into());
        }
        let json = serde_json::to_string(&creator).map_err(|e| e.to_string())?;
        let n = c
            .execute(
                "INSERT INTO operation_creators(kind,operation,creator,parent) VALUES(?1,?2,?3,?4)",
                params![k, id, json, parent],
            )
            .map_err(sql)?;
        if n != 1 {
            return Err("Operation creator was not stored; acceptance rolled back".into());
        }
        Ok(())
    }
    pub(crate) fn creator_on(
        c: &Connection,
        k: &str,
        id: &str,
    ) -> Result<(Option<OperationCreator>, Option<String>)> {
        let row: Option<(String, Option<String>)> = c
            .query_row(
                "SELECT creator,parent FROM operation_creators WHERE kind=?1 AND operation=?2",
                params![k, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        match row {
            None => Ok((None, None)),
            Some((raw, parent)) => {
                let creator: Option<OperationCreator> =
                    serde_json::from_str(&raw).map_err(|_| "Invalid stored operation creator")?;
                if creator.as_ref().is_some_and(|c| !c.valid()) {
                    return Err("Corrupt stored operation creator".into());
                }
                Ok((creator, parent))
            }
        }
    }
    pub fn operation_attribution(
        &self,
        context: &RequestContext,
        query: &winbridge_protocol::OperationAttributionQuery,
        generation: &str,
    ) -> Result<winbridge_protocol::OperationAttribution> {
        if context.journal_incarnation != self.journal_authority().incarnation
            || query.operation_id.is_empty()
            || query.operation_id.len() > 160
        {
            return Err("Invalid attribution query scope/ID".into());
        }
        let table = match query.kind {
            HistoryKind::Task => "tasks",
            HistoryKind::Import => "imports",
            HistoryKind::Workflow => "workflows",
            HistoryKind::Application => "applications",
        };
        let exists: bool = self
            .connection
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
                [&query.operation_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if !exists {
            return Err("Attribution operation not found in this journal".into());
        }
        let (creator, parent) =
            Self::creator_on(&self.connection, kind(query.kind), &query.operation_id)?;
        Ok(winbridge_protocol::OperationAttribution {
            schema_version: 1,
            reference: WireRef {
                context: context.clone(),
                kind: query.kind,
                operation_id: query.operation_id.clone(),
            },
            creator,
            inherited_from: parent.map(|operation_id| WireRef {
                context: context.clone(),
                kind: HistoryKind::Workflow,
                operation_id,
            }),
            provider: winbridge_protocol::build_identity(),
            worker_generation: generation.into(),
            observed_at_ms: super::now_ms(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winbridge_protocol::*;
    fn request() -> TaskSubmission {
        TaskSubmission {
            protocol_version: 1,
            request_id: "r".into(),
            project_id: "p".into(),
            idempotency_key: "key".into(),
            timeout_ms: 1000,
            snapshot_id: None,
            command: CommandSource::Direct {
                executable: "never-executed".into(),
                arguments: vec![],
                working_directory: ".".into(),
            },
        }
    }
    fn command() -> ResolvedCommand {
        ResolvedCommand {
            executable: "never-executed".into(),
            arguments: vec![],
            working_directory: ".".into(),
            timeout_ms: 1000,
        }
    }
    fn creator(id: &str) -> OperationCreator {
        OperationCreator::Controller {
            worker_generation: "worker".into(),
            controller_id: id.into(),
            controller_node_id: "same-node".into(),
            controller_generation: format!("generation-{id}"),
            session_id: format!("session-{id}"),
        }
    }
    #[test]
    fn concurrent_duplicate_keeps_one_creator_and_reopen_keeps_history() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        drop(Store::open(&path).unwrap());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = ["a", "b"].map(|id| {
            let barrier = barrier.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(&path).unwrap();
                barrier.wait();
                s.accept_attributed(request(), command(), None, None, Some(&creator(id)))
                    .unwrap()
                    .task_id
            })
        });
        let [a, b] = handles.map(|h| h.join().unwrap());
        assert_eq!(a, b);
        let mut s = Store::open(&path).unwrap();
        let original = Store::creator_on(&s.connection, "task", &a)
            .unwrap()
            .0
            .unwrap();
        assert!(original == creator("a") || original == creator("b"));
        let mut retry = request();
        retry.request_id = "retry".into();
        assert_eq!(
            s.accept_attributed(retry, command(), None, None, Some(&creator("retry")))
                .unwrap()
                .task_id,
            a
        );
        assert_eq!(
            Store::creator_on(&s.connection, "task", &a).unwrap().0,
            Some(original)
        );
        assert!(s
            .connection
            .execute("UPDATE operation_creators SET creator='null'", [])
            .is_err());
        assert!(s
            .connection
            .execute("DELETE FROM operation_creators", [])
            .is_err());
    }
    #[test]
    fn missing_creator_insert_rolls_back_task_and_admission() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Store::open(&root.path().join("journal.sqlite")).unwrap();
        s.connection.execute_batch("CREATE TRIGGER ignore_creator BEFORE INSERT ON operation_creators BEGIN SELECT RAISE(IGNORE);END;").unwrap();
        assert!(s
            .accept_attributed(request(), command(), None, None, Some(&creator("a")))
            .is_err());
        assert!(s.all().unwrap().is_empty());
        let count:i64=s.connection.query_row("SELECT (SELECT count(*) FROM queue_roots)+(SELECT count(*) FROM resource_requests)+(SELECT count(*) FROM operation_creators)",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn seventeen_migration_archives_consistent_history_and_does_not_invent_author() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.sqlite");
        let mut s = Store::open(&path).unwrap();
        let task = s.accept(request(), command(), None).unwrap();
        s.connection.execute_batch("DROP TABLE operation_cancellations; DROP TABLE operation_creators;PRAGMA user_version=17;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        let authority = s.journal_authority().clone();
        drop(s);
        let mut next = Store::open(&path).unwrap();
        assert_eq!(next.journal_authority(), &authority);
        assert!(Store::creator_on(&next.connection, "task", &task.task_id)
            .unwrap()
            .0
            .is_none());
        assert_eq!(
            next.accept_attributed(request(), command(), None, None, Some(&creator("a")))
                .unwrap()
                .task_id,
            task.task_id
        );
        assert!(Store::creator_on(&next.connection, "task", &task.task_id)
            .unwrap()
            .0
            .is_none());
        let backups: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.extension().is_some_and(|e| e == "sqlite")
                    && p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("journal-v17-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let old =
            Connection::open_with_flags(&backups[0], rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            17
        );
        let saved: String = old
            .query_row(
                "SELECT record FROM tasks WHERE id=?1",
                [&task.task_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
            serde_json::to_value(task).unwrap()
        );
    }
}
