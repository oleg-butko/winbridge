use super::*;
use winbridge_protocol::ImportSubmission;

#[test]
fn pending_reads_keep_order_and_all_live_states_and_use_partial_indexes() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(&temp.path().join("journal.sqlite")).unwrap();
    let states = [
        TaskStatus::Succeeded,
        TaskStatus::Running,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
        TaskStatus::Queued,
        TaskStatus::Interrupted,
        TaskStatus::Indeterminate,
    ];
    let mut template: TaskRecord = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/legacy-v1/task-record.json"
    ))
    .unwrap();
    let mut task_ids = Vec::new();
    let mut import_ids = Vec::new();
    for (n, status) in states.into_iter().enumerate() {
        template.task_id = format!("task-{n}");
        template.status = status;
        template.submission.idempotency_key = format!("task-{n}");
        store
            .connection
            .execute(
                "INSERT INTO tasks(id,project,key,payload,record) VALUES(?1,?2,?3,?4,?5)",
                params![
                    template.task_id,
                    template.submission.project_id,
                    template.submission.idempotency_key,
                    Store::fingerprint(&template.submission).unwrap(),
                    serde_json::to_string(&template).unwrap()
                ],
            )
            .unwrap();
        let import = store
            .accept_import(
                ImportSubmission {
                    protocol_version: 1,
                    request_id: format!("import-{n}"),
                    project_id: "fixture".into(),
                    snapshot_id: format!("snapshot-{n}"),
                    manifest_sha256: "a".repeat(64),
                    idempotency_key: format!("import-{n}"),
                    timeout_ms: 5000,
                },
                "shared".into(),
                "workspace".into(),
            )
            .unwrap();
        // Synthetic states only, without launching any external process/import.
        store.connection.execute("UPDATE imports SET record=json_set(record,'$.status',?2),revision=revision+1 WHERE id=?1",
            params![import.import_id,serde_json::to_value(status).unwrap().as_str().unwrap()]).unwrap();
        if matches!(status, TaskStatus::Queued | TaskStatus::Running) {
            task_ids.push(template.task_id.clone());
            import_ids.push(import.import_id);
        }
    }
    assert_eq!(
        store
            .pending_tasks()
            .unwrap()
            .into_iter()
            .map(|t| t.task_id)
            .collect::<Vec<_>>(),
        task_ids
    );
    assert_eq!(
        store
            .pending_imports()
            .unwrap()
            .into_iter()
            .map(|t| t.import_id)
            .collect::<Vec<_>>(),
        import_ids
    );
    assert_eq!(store.all().unwrap().len(), states.len());
    assert_eq!(store.imports(false).unwrap().len(), states.len());
    for (query, index) in [
        (PENDING_TASKS_SQL, "queue_pending_tasks"),
        (PENDING_IMPORTS_SQL, "queue_pending_imports"),
    ] {
        let plan = store
            .connection
            .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .map(|row| row.unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            plan.contains(index),
            "pending work must use its partial index: {plan}"
        );
    }
}
