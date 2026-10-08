use crate::store::{now_ms, sql, Result, Store};
use rusqlite::{params, OptionalExtension};
use winbridge_protocol::*;

fn kind_name(kind: HistoryKind) -> &'static str {
    match kind {
        HistoryKind::Task => "task",
        HistoryKind::Import => "import",
        HistoryKind::Workflow => "workflow",
        HistoryKind::Application => "application",
    }
}
fn bounded(text: &str) -> Result<()> {
    if text.len() > 200 || text.chars().any(char::is_control) {
        return Err(
            "History text/cursor must be up to 200 UTF-8 bytes without control characters".into(),
        );
    }
    Ok(())
}
fn timestamp(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(column)?
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
        })
        .transpose()
}
impl Store {
    pub fn history(&self, query: &HistoryQuery) -> Result<HistoryPage> {
        bounded(&query.search)?;
        if let Some(project) = &query.project_id {
            if !winbridge_core::config::valid_id(project) {
                return Err("Invalid history project ID".into());
            }
        }
        if let Some(status) = &query.status {
            if ![
                "queued",
                "running",
                "succeeded",
                "failed",
                "cancelled",
                "interrupted",
                "indeterminate",
                "starting",
                "ready",
                "stopping",
                "stopped",
                "exited",
            ]
            .contains(&status.as_str())
            {
                return Err("Unknown history status".into());
            }
        }
        let (before, before_kind, before_id) = if let Some(cursor) = &query.before {
            bounded(&cursor.operation_id)?;
            (
                i64::try_from(cursor.created_at_ms)
                    .map_err(|_| "History cursor timestamp out of range")?,
                kind_name(cursor.kind),
                cursor.operation_id.as_str(),
            )
        } else {
            (i64::MAX, "~", "~")
        };
        let mut stmt=self.connection.prepare("WITH h AS (
            SELECT id,'task' AS kind,project,json_extract(record,'$.resolvedCommand.executable') AS label,json_extract(record,'$.status') AS status,json_extract(record,'$.acceptedAtMs') AS created,json_extract(record,'$.finishedAtMs') AS finished,NULL AS workflow FROM tasks
            UNION ALL SELECT id,'import',project,json_extract(record,'$.submission.snapshotId'),json_extract(record,'$.status'),json_extract(record,'$.acceptedAtMs'),json_extract(record,'$.finishedAtMs'),NULL FROM imports
            UNION ALL SELECT id,'workflow',project,json_extract(record,'$.submission.name'),status,json_extract(record,'$.acceptedAtMs'),json_extract(record,'$.finishedAtMs'),id FROM workflows
            UNION ALL SELECT id,'application',json_extract(record,'$.projectId'),json_extract(record,'$.taskId'),status,json_extract(record,'$.createdAtMs'),json_extract(record,'$.finishedAtMs'),workflow FROM applications
        ) SELECT id,kind,project,label,status,created,finished,workflow FROM h
        WHERE (?1 IS NULL OR project=?1) AND (?2 IS NULL OR kind=?2) AND (?3 IS NULL OR status=?3)
          AND (created,kind,id)<(?4,?5,?6)
          AND (instr(lower(id),lower(?7))>0 OR instr(lower(coalesce(label,'')),lower(?7))>0 OR instr(lower(project),lower(?7))>0)
        ORDER BY created DESC,kind DESC,id DESC LIMIT 51").map_err(sql)?;
        let mut records = stmt
            .query_map(
                params![
                    query.project_id,
                    query.kind.map(kind_name),
                    query.status,
                    before,
                    before_kind,
                    before_id,
                    query.search
                ],
                |r| {
                    let kind: String = r.get(1)?;
                    let kind = match kind.as_str() {
                        "task" => HistoryKind::Task,
                        "import" => HistoryKind::Import,
                        "workflow" => HistoryKind::Workflow,
                        _ => HistoryKind::Application,
                    };
                    Ok(HistoryEntry {
                        operation_id: r.get(0)?,
                        kind,
                        project_id: r.get(2)?,
                        label: r.get(3)?,
                        status: r.get(4)?,
                        created_at_ms: timestamp(r, 5)?.ok_or(
                            rusqlite::Error::InvalidColumnType(
                                5,
                                "created".into(),
                                rusqlite::types::Type::Null,
                            ),
                        )?,
                        finished_at_ms: timestamp(r, 6)?,
                        workflow_id: r.get(7)?,
                    })
                },
            )
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        let has_more = records.len() > 50;
        records.truncate(50);
        let next_cursor = records.last().map(|r| HistoryCursor {
            created_at_ms: r.created_at_ms,
            kind: r.kind,
            operation_id: r.operation_id.clone(),
        });
        Ok(HistoryPage {
            records,
            next_cursor,
            has_more,
            observed_at_ms: now_ms(),
        })
    }
    pub fn storage_usage(&self) -> Result<StorageUsage> {
        let count = |table: &str| -> Result<u64> {
            let n: i64 = self
                .connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .map_err(sql)?;
            Ok(n as u64)
        };
        let pending: i64 = self
            .connection
            .query_row(
                "SELECT count(*) FROM attention WHERE acknowledged IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let bytes: i64 = self
            .connection
            .query_row(
                "SELECT coalesce(sum(length(bytes)),0) FROM output",
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        Ok(StorageUsage {
            task_count: count("tasks")?,
            import_count: count("imports")?,
            workflow_count: count("workflows")?,
            application_count: count("applications")?,
            attention_count: count("attention")?,
            pending_attention_count: pending as u64,
            stored_output_bytes: bytes as u64,
            journal_bytes: 0,
            wal_bytes: 0,
            output_limit_bytes_per_task: 0,
        })
    }
    pub fn diagnostic(&mut self, request: &DiagnosticRequest) -> Result<DiagnosticPreview> {
        bounded(&request.task_id)?;
        winbridge_core::diagnostics::validate_redactions(&request.redact_values)?;
        let transaction = self.connection.transaction().map_err(sql)?;
        let raw: String = transaction
            .query_row(
                "SELECT record FROM tasks WHERE id=?1",
                [&request.task_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?
            .ok_or_else(|| format!("Unknown diagnostic Task ID: {}", request.task_id))?;
        let task: TaskRecord = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let mut record = serde_json::to_value(&task).map_err(|e| e.to_string())?;
        record["submission"]
            .as_object_mut()
            .unwrap()
            .remove("idempotencyKey");
        record["submission"]
            .as_object_mut()
            .unwrap()
            .remove("requestId");
        if !request.include_command {
            record["submission"]["command"] = serde_json::json!({"omitted":true});
            record["resolvedCommand"] = serde_json::json!({"omitted":true});
        }
        let mut chunks = Vec::new();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut captured = 0usize;
        if request.include_output {
            let mut statement=transaction.prepare("SELECT sequence,stream,offset,substr(bytes,1,65536) FROM output WHERE task=?1 ORDER BY sequence LIMIT 128").map_err(sql)?;
            let mut rows = statement.query([&request.task_id]).map_err(sql)?;
            while let Some(row) = rows.next().map_err(sql)? {
                let bytes: Vec<u8> = row.get(3).map_err(sql)?;
                let available = 65536usize.saturating_sub(captured);
                if available == 0 {
                    break;
                }
                let accepted = bytes.len().min(available);
                captured += accepted;
                let stream: String = row.get(1).map_err(sql)?;
                if stream == "stdout" {
                    stdout.extend_from_slice(&bytes[..accepted]);
                } else {
                    stderr.extend_from_slice(&bytes[..accepted]);
                }
                chunks.push(serde_json::json!({"sequence":row.get::<_,i64>(0).map_err(sql)?,"stream":stream,"offset":row.get::<_,i64>(2).map_err(sql)?,"capturedBytes":accepted}));
            }
        }
        let truncated = request.include_output
            && (task.output_truncated || task.output_bytes > captured as u64);
        let observed = now_ms();
        let output = serde_json::json!({"stdout":String::from_utf8_lossy(&stdout),"stderr":String::from_utf8_lossy(&stderr),"chunks":chunks});
        let mut report = serde_json::json!({"schemaVersion":1,"kind":"winbridge_task_diagnostic","observedAtMs":observed,"workerBuild":build_identity(),"task":record,"output":output,
            "commandIncluded":request.include_command,"outputIncluded":request.include_output,"capturedOutputBytes":captured,"outputTruncated":truncated,"taskTerminal":task.status.is_terminal(),"encoding":"UTF-8; invalid bytes are replaced","redaction":"Common credential fields/arguments and encoded command payloads masked; literal replacements applied; review editable preview before saving"});
        winbridge_core::diagnostics::redact(&mut report, &request.redact_values);
        let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        if json.len() > 512 * 1024 {
            return Err("Diagnostic preview exceeds 512 KiB; exclude command/output".into());
        }
        transaction.commit().map_err(sql)?;
        Ok(DiagnosticPreview {
            task_id: request.task_id.clone(),
            observed_at_ms: observed,
            json,
            command_included: request.include_command,
            output_included: request.include_output,
            output_truncated: truncated,
        })
    }
}
