use crate::{
    store::{now_ms, sql, JournalSnapshot, Store},
    Result, Worker,
};
use rusqlite::{params, OptionalExtension};
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use winbridge_protocol::*;
use winbridge_workspace_sync as sync;
impl Store {
    pub fn existing_import(&self, request: &ImportSubmission) -> Result<Option<ImportRecord>> {
        Self::existing_import_on(&self.connection, request)
    }
    fn existing_import_on(
        connection: &rusqlite::Connection,
        request: &ImportSubmission,
    ) -> Result<Option<ImportRecord>> {
        let row: Option<(String, String)> = connection
            .query_row(
                "SELECT payload,record FROM imports WHERE project=?1 AND key=?2",
                params![request.project_id, request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        let mut payload = request.clone();
        payload.request_id.clear();
        match row {
            None => Ok(None),
            Some((stored, record)) => {
                if stored != serde_json::to_string(&payload).map_err(|e| e.to_string())? {
                    return Err("Import idempotencyKey belongs to a different payload".into());
                }
                Ok(Some(
                    serde_json::from_str(&record).map_err(|e| e.to_string())?,
                ))
            }
        }
    }
    pub fn accept_import(
        &mut self,
        request: ImportSubmission,
        shared_path: String,
        workspace_path: String,
    ) -> Result<ImportRecord> {
        self.guard_callback(None, None)?;
        let queue_policy = self.loaded_queue_policy();
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(existing) = Self::existing_import_on(&transaction, &request)? {
            return Ok(existing);
        }
        Self::require_root_admission(&transaction, queue_policy.as_ref(), &request.project_id)?;
        let import = Self::accept_import_on(&transaction, request, shared_path, workspace_path)?;
        Self::freeze_resource_root(
            &transaction,
            "import",
            &import.import_id,
            &import.submission.project_id,
        )?;
        Self::insert_queue_root(
            &transaction,
            "import",
            &import.import_id,
            &import.submission.project_id,
        )?;
        transaction.commit().map_err(sql)?;
        Ok(import)
    }
    pub(crate) fn accept_import_on(
        connection: &rusqlite::Connection,
        request: ImportSubmission,
        shared_path: String,
        workspace_path: String,
    ) -> Result<ImportRecord> {
        if let Some(import) = Self::existing_import_on(connection, &request)? {
            return Ok(import);
        }
        let mut import = ImportRecord {
            import_id: uuid::Uuid::new_v4().to_string(),
            submission: request.clone(),
            status: TaskStatus::Queued,
            phase: "queued".into(),
            delta: None,
            transfer: None,
            accepted_at_ms: now_ms(),
            finished_at_ms: None,
            cancel_requested: false,
            completed_files: 0,
            completed_bytes: 0,
            total_files: None,
            total_bytes: None,
            commit: None,
            shared_path,
            workspace_path,
            reason: None,
            events: Vec::new(),
        };
        Self::import_event(
            &mut import,
            "Import accepted and committed before acknowledgement",
        );
        let mut payload = request;
        payload.request_id.clear();
        let changed = connection
            .execute(
                "INSERT INTO imports(id,project,key,payload,record) VALUES(?1,?2,?3,?4,?5)",
                params![
                    import.import_id,
                    payload.project_id,
                    payload.idempotency_key,
                    serde_json::to_string(&payload).map_err(|e| e.to_string())?,
                    serde_json::to_string(&import).map_err(|e| e.to_string())?
                ],
            )
            .map_err(sql)?;
        if changed != 1 {
            return Err(
                "local journal: import acceptance not stored; acknowledgement rejected".into(),
            );
        }
        Ok(import)
    }
    pub fn get_import(&self, id: &str) -> Result<ImportRecord> {
        let record: Option<String> = self
            .connection
            .query_row("SELECT record FROM imports WHERE id=?1", [id], |r| r.get(0))
            .optional()
            .map_err(sql)?;
        serde_json::from_str(&record.ok_or("Import ID not found")?).map_err(|e| e.to_string())
    }
    pub fn imports(&self, recent: bool) -> Result<Vec<ImportRecord>> {
        let query = if recent {
            "SELECT record FROM imports ORDER BY rowid DESC LIMIT 10"
        } else {
            "SELECT record FROM imports ORDER BY rowid"
        };
        let mut statement = self.connection.prepare(query).map_err(sql)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| serde_json::from_str(&r.map_err(sql)?).map_err(|e| e.to_string()))
            .collect()
    }
    pub fn save_import(&mut self, import: &mut JournalSnapshot<ImportRecord>) -> Result<()> {
        self.guard_callback(
            Some(crate::store::OperationKind::Import),
            Some(&import.import_id),
        )?;
        let transaction = self.write_transaction(rusqlite::TransactionBehavior::Deferred)?;
        Self::write_domain(&transaction, import)?;
        if import.status.is_terminal() {
            Self::complete_attempt(
                &transaction,
                crate::store::OperationKind::Import,
                &import.import_id,
                import.status,
            )?;
        }
        transaction.commit().map_err(sql)?;
        import.committed()
    }
    pub fn import_event(import: &mut ImportRecord, summary: impl Into<String>) {
        import.events.push(JournalEvent {
            sequence: import.events.len() as u64 + 1,
            occurred_at_ms: now_ms(),
            status: import.status,
            summary: summary.into(),
        });
    }
    pub fn finish_import(
        &mut self,
        id: &str,
        status: TaskStatus,
        reason: &str,
    ) -> Result<ImportRecord> {
        self.guard_callback(Some(crate::store::OperationKind::Import), Some(id))?;
        let mut import = self.import_snapshot(id)?;
        if import.status.is_terminal() {
            return Ok(import.record);
        }
        import.status = status;
        import.finished_at_ms = Some(now_ms());
        import.reason = Some(reason.into());
        Self::import_event(&mut import, reason);
        self.save_import(&mut import)?;
        Ok(import.record)
    }
    pub fn cancel_import(&mut self, id: &str) -> Result<ImportRecord> {
        self.guard_callback(Some(crate::store::OperationKind::Import), Some(id))?;
        let mut import = self.import_snapshot(id)?;
        if import.status.is_terminal() || import.cancel_requested {
            return Ok(import.record);
        }
        if import.status == TaskStatus::Queued {
            return self.finish_import(
                id,
                TaskStatus::Cancelled,
                "Import cancelled before filesystem work",
            );
        }
        import.cancel_requested = true;
        Self::import_event(
            &mut import,
            "Cancellation requested; awaiting staging checkpoint",
        );
        self.save_import(&mut import)?;
        Ok(import.record)
    }
}
impl Worker {
    pub(crate) fn import_paths(&self, project: &str, snapshot: &str) -> Result<(PathBuf, PathBuf)> {
        sync::validate_ids(project, snapshot)?;
        let delivery = self
            .config
            .delivery
            .as_ref()
            .ok_or("Snapshot delivery is not configured in worker local JSON")?;
        let project = self
            .config
            .projects
            .iter()
            .find(|p| p.id == project)
            .ok_or("Project is not registered on worker")?;
        Ok((
            sync::package_path(delivery, &project.id, snapshot)?,
            sync::generation_path(&project.workspace_root, snapshot)?,
        ))
    }
    pub(crate) fn preview_snapshot(
        &self,
        project: &str,
        snapshot: &str,
        offset: u64,
    ) -> Result<SnapshotPreview> {
        let (shared, local) = self.import_paths(project, snapshot)?;
        let (manifest, _, ready) = sync::metadata(
            self.config.delivery.as_ref().unwrap(),
            project,
            snapshot,
            None,
        )?;
        let offset = usize::try_from(offset).map_err(|_| "Preview offset exceeds range")?;
        if offset > manifest.files.len() {
            return Err("Preview offset exceeds file count".into());
        }
        let end = (offset + 100).min(manifest.files.len());
        Ok(SnapshotPreview {
            resumable: manifest.payload_chunks.is_some(),
            delta: sync::delta_summary(&manifest),
            project_id: project.into(),
            snapshot_id: snapshot.into(),
            manifest_sha256: ready.manifest_sha256,
            source: manifest.source,
            total_files: manifest.files.len() as u64,
            total_bytes: manifest.total_bytes,
            payload_bytes: manifest.payload_bytes,
            shared_path: shared.to_string_lossy().into(),
            workspace_path: local.to_string_lossy().into(),
            files: manifest.files[offset..end].to_vec(),
            next_offset: (end < manifest.files.len()).then_some(end as u64),
            payload_verified: false,
            peer_shared_path: self.config.delivery.as_ref().and_then(|d| {
                d.peer_root.as_ref().map(|p| {
                    format!(
                        "{}/{}/deliveries/{}/{}",
                        p.trim_end_matches('/'),
                        d.namespace,
                        project,
                        snapshot
                    )
                })
            }),
        })
    }
    pub(crate) fn resolve_task(&self, request: &TaskSubmission) -> Result<ResolvedCommand> {
        let Some(snapshot) = &request.snapshot_id else {
            let command = winbridge_runner::resolve(&self.config, request)?;
            let project = self
                .config
                .projects
                .iter()
                .find(|p| p.id == request.project_id)
                .ok_or("Project not registered")?;
            let cwd = PathBuf::from(&command.working_directory);
            let root = std::fs::canonicalize(&project.workspace_root).map_err(|e| e.to_string())?;
            if winbridge_core::config::contains_path(&root.join("snapshots"), &cwd)
                || winbridge_core::config::contains_path(&root.join(".winbridge"), &cwd)
            {
                return Err("Managed snapshot commands require snapshotId; import staging cannot be a command cwd".into());
            }
            return Ok(command);
        };
        let project = self
            .config
            .projects
            .iter()
            .find(|p| p.id == request.project_id)
            .ok_or("Project is not registered")?;
        // Fast admission checks only the local marker. Full source hashes are checked under
        // the project lease immediately before process creation, without blocking IPC.
        let generation = sync::generation_path(&project.workspace_root, snapshot)?;
        let marker = sync::read_bounded(
            &generation.join(".winbridge/verified.json"),
            winbridge_core::snapshot::MAX_READY_BYTES as u64,
        ).map_err(|reason| format!(
            "Snapshot {snapshot} не готов для сборки. Сначала успешно импортируйте этот пакет в блоке «Доставка исходников». Запрос команды отклонён; процесс не запущен. Причина: {reason}"
        ))?;
        let ready = winbridge_core::snapshot::parse_ready(&marker)?;
        if ready.project_id != project.id || ready.snapshot_id != *snapshot {
            return Err("Verified generation IDs mismatch".into());
        }
        winbridge_runner::resolve_for_generation(&self.config, request, &generation)
    }
    pub(crate) fn snapshot_reference(
        &self,
        request: &TaskSubmission,
    ) -> Result<Option<VerifiedSnapshotRef>> {
        let Some(snapshot) = &request.snapshot_id else {
            return Ok(None);
        };
        let project = self
            .config
            .projects
            .iter()
            .find(|p| p.id == request.project_id)
            .ok_or("Snapshot project not registered")?;
        let (manifest, ready) =
            sync::local_metadata(&project.workspace_root, &project.id, snapshot, None)?;
        Ok(Some(VerifiedSnapshotRef {
            snapshot_id: snapshot.clone(),
            manifest_sha256: ready.manifest_sha256,
            source: manifest.source,
        }))
    }
    pub(crate) fn execute_import(
        self: &Arc<Self>,
        import: ImportRecord,
        token: crate::store::ExecutionToken,
    ) -> Result<()> {
        let id = import.import_id.clone();
        let project = self
            .config
            .projects
            .iter()
            .find(|p| p.id == import.submission.project_id)
            .ok_or("Project removed from configuration")?;
        let delivery = self
            .config
            .delivery
            .as_ref()
            .ok_or("Delivery removed from configuration")?;
        if let Ok((manifest, _, _)) = sync::metadata(
            delivery,
            &project.id,
            &import.submission.snapshot_id,
            Some(&import.submission.manifest_sha256),
        ) {
            let mut store = self.store()?;
            store.check_execution(&token)?;
            let mut record = store.import_snapshot(&id)?;
            record.delta = sync::delta_summary(&manifest);
            record.total_files = Some(manifest.files.len() as u64);
            record.total_bytes = Some(manifest.total_bytes);
            let SnapshotSource::GitCommit { commit } = manifest.source;
            record.commit = Some(commit);
            store.with_execution(&token, |store| store.save_import(&mut record))?;
        }
        let started = Instant::now();
        let mut persisted = Instant::now() - Duration::from_secs(1);
        let mut last_phase = String::new();
        let mut visited_phases = std::collections::HashSet::new();
        let mut checkpoint = |phase: &str, files: u64, bytes: u64| -> Result<()> {
            if self.stop.load(Ordering::Acquire) {
                return Err("Worker stopping; import staging remains unbuildable".into());
            }
            let mut store = self.store()?;
            store.check_execution(&token)?;
            let mut record = store.import_snapshot(&id)?;
            if record.cancel_requested {
                return Err("Import cancelled at staging checkpoint".into());
            }
            if started.elapsed() >= Duration::from_millis(import.submission.timeout_ms) {
                return Err("Snapshot import timeout; staging remains unbuildable".into());
            }
            if persisted.elapsed() >= Duration::from_millis(150) || last_phase != phase {
                if last_phase != phase {
                    if visited_phases.insert(phase.to_owned()) {
                        Store::import_event(&mut record, format!("Snapshot phase: {phase}"));
                    }
                    last_phase = phase.into();
                }
                record.phase = phase.into();
                record.completed_files = files;
                record.completed_bytes = bytes;
                store.with_execution(&token, |store| store.save_import(&mut record))?;
                persisted = Instant::now();
            }
            Ok(())
        };
        let result = sync::import_with_transfer(
            delivery,
            &project.workspace_root,
            &import.submission,
            &id,
            &mut checkpoint,
            |transfer| {
                let mut store = self.store()?;
                store.check_execution(&token)?;
                let mut record = store.import_snapshot(&id)?;
                record.transfer = Some(transfer.clone());
                store.with_execution(&token, |store| store.save_import(&mut record))
            },
        );
        let mut store = self.store()?;
        store.check_execution(&token)?;
        match result {
            Ok(manifest) => {
                let mut record = store.import_snapshot(&id)?;
                record.delta = sync::delta_summary(&manifest);
                record.phase = "verified".into();
                record.completed_files = manifest.files.len() as u64;
                record.completed_bytes = manifest.total_bytes;
                record.total_files = Some(record.completed_files);
                record.total_bytes = Some(manifest.total_bytes);
                let SnapshotSource::GitCommit { commit } = manifest.source;
                record.commit = Some(commit);
                store.with_execution(&token, |store| store.save_import(&mut record))?;
                store.with_execution(&token, |store| {
                    store.finish_import_execution(
                        &token,
                        TaskStatus::Succeeded,
                        "Local source generation verified; ready for explicit build",
                    )
                })?;
            }
            Err(reason) => {
                let cancelled =
                    store.get_import(&id)?.cancel_requested || self.stop.load(Ordering::Acquire);
                store.with_execution(&token, |store| {
                    store.finish_import_execution(
                        &token,
                        if cancelled {
                            TaskStatus::Cancelled
                        } else {
                            TaskStatus::Failed
                        },
                        &reason,
                    )
                })?;
            }
        }
        Ok(())
    }
    pub(crate) fn recover_imports(&self) -> Result<()> {
        let records = self.store()?.imports(false)?;
        for import in records
            .into_iter()
            .filter(|r| r.status == TaskStatus::Running)
        {
            let verified = self
                .config
                .projects
                .iter()
                .find(|p| p.id == import.submission.project_id)
                .and_then(|p| {
                    sync::verified_generation(
                        &p.workspace_root,
                        &p.id,
                        &import.submission.snapshot_id,
                        Some(&import.submission.manifest_sha256),
                        |_, _, _| Ok(()),
                    )
                    .ok()
                });
            let mut store = self.store()?;
            if let Some(manifest) = verified {
                let mut recovered = store.import_snapshot(&import.import_id)?;
                recovered.phase = "verified".into();
                recovered.delta = sync::delta_summary(&manifest);
                recovered.completed_files = manifest.files.len() as u64;
                recovered.completed_bytes = manifest.total_bytes;
                recovered.total_files = Some(recovered.completed_files);
                recovered.total_bytes = Some(recovered.completed_bytes);
                let SnapshotSource::GitCommit { commit } = manifest.source;
                recovered.commit = Some(commit);
                store.save_import(&mut recovered)?;
                store.finish_import(&import.import_id,TaskStatus::Succeeded,"Recovered a fully verified local generation after worker restart; no build was repeated")?;
            } else {
                store.finish_import(&import.import_id,TaskStatus::Interrupted,"Worker restarted during import; staging is not buildable. Retry requires a new explicit key")?;
            }
        }
        Ok(())
    }
}
