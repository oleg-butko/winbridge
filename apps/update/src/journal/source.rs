//! Immutable source plans/intents/receipts in the updater database.
use super::*;
use winbridge_core::update::{
    validate_source_path, MAX_SOURCE_FILES, MAX_SOURCE_FILE_BYTES, SOURCE_MANIFEST_NAME,
};
use winbridge_protocol::update::*;

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE source_transactions(id TEXT PRIMARY KEY REFERENCES updates(id),binding TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>=0),phase TEXT NOT NULL,record TEXT NOT NULL CHECK(json_valid(record)));
        CREATE TABLE source_plans(update_id TEXT NOT NULL REFERENCES source_transactions(id),ordinal INTEGER NOT NULL CHECK(ordinal>=0),plan TEXT NOT NULL CHECK(json_valid(plan)),PRIMARY KEY(update_id,ordinal));
        CREATE TABLE source_file_intents(update_id TEXT NOT NULL,ordinal INTEGER NOT NULL,plan TEXT NOT NULL,PRIMARY KEY(update_id,ordinal),FOREIGN KEY(update_id,ordinal) REFERENCES source_plans(update_id,ordinal));
        CREATE TABLE source_file_receipts(update_id TEXT NOT NULL,ordinal INTEGER NOT NULL,plan TEXT NOT NULL,PRIMARY KEY(update_id,ordinal),FOREIGN KEY(update_id,ordinal) REFERENCES source_file_intents(update_id,ordinal));
        CREATE TRIGGER source_identity BEFORE UPDATE ON source_transactions WHEN NEW.id!=OLD.id OR NEW.binding!=OLD.binding OR NEW.revision!=OLD.revision+1 OR OLD.phase IN ('\"verified\"','\"blocked\"') BEGIN SELECT RAISE(ABORT,'Source identity/revision is immutable'); END;
        CREATE TRIGGER source_delete BEFORE DELETE ON source_transactions BEGIN SELECT RAISE(ABORT,'Source history is retained'); END;
        PRAGMA user_version=2;").map_err(storage)?;
    for table in [
        "source_plans",
        "source_file_intents",
        "source_file_receipts",
    ] {
        c.execute_batch(&format!("CREATE TRIGGER {table}_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'Source evidence is immutable'); END; CREATE TRIGGER {table}_delete BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT,'Source evidence is retained'); END;")).map_err(storage)?;
    }
    Ok(())
}
pub(super) fn unfinished(c: &Connection, id: &str) -> Result<bool> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM source_transactions WHERE id=?1 AND phase IN ('\"prepared\"','\"applying\"'))",[id],|r|r.get(0)).map_err(storage)
}
fn binding(r: &SourceTransactionRecord) -> Result<String> {
    encode(&(
        r.schema_version,
        &r.update_id,
        &r.source_directory,
        &r.runtime_directory,
        &r.stage_directory,
        &r.transaction_directory,
        &r.source_directory_identity,
        &r.stage_directory_identity,
        &r.transaction_directory_identity,
        &r.worker_source_spec,
        r.worker_lease_revision,
        &r.plan_sha256,
        r.total_files,
        r.accepted_at_ms,
    ))
}
fn header(c: &Connection, id: &str) -> Result<SourceTransactionRecord> {
    let row: Option<(String, String, i64, String)> = c
        .query_row(
            "SELECT record,binding,revision,phase FROM source_transactions WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(storage)?;
    let (json, bound, revision, phase) =
        row.ok_or_else(|| fail(JournalErrorCode::NotFound, "Source transaction not found"))?;
    if json.len() > 128 * 1024 {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Oversized source transaction",
        ));
    }
    let r: SourceTransactionRecord = serde_json::from_str(&json).map_err(storage)?;
    let update = record(c, id)?;
    let spec = &r.worker_source_spec;
    if r.schema_version != 1
        || r.update_id != id
        || spec.update_id != id
        || r.source_directory != spec.source_directory
        || binding(&r)? != bound
        || counter(revision)? != r.revision
        || encode(&r.phase)? != phase
        || r.revision > MAX_UPDATE_COUNTER
        || r.worker_lease_revision > MAX_UPDATE_COUNTER
        || r.accepted_at_ms > r.updated_at_ms
        || r.updated_at_ms > MAX_UPDATE_COUNTER
        || r.total_files as usize > MAX_SOURCE_FILES * 2 + 1
        || !valid_hash(&r.plan_sha256, &[64])
        || spec.baseline_manifest_sha256 != update.request.expected.baseline_manifest_sha256
        || spec.target_manifest_sha256 != update.request.source.manifest_sha256
        || r.reason.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Source transaction/index/request mismatch",
        ));
    }
    Ok(r)
}
fn validate_change(change: &SourceChange) -> Result<()> {
    if change.path != SOURCE_MANIFEST_NAME {
        validate_source_path(&change.path).map_err(|e| fail(JournalErrorCode::Corrupt, e))?;
    }
    let shape = matches!(
        (
            change.action,
            change.previous.is_some(),
            change.incoming.is_some()
        ),
        (SourceAction::Add, false, true)
            | (SourceAction::Replace, true, true)
            | (SourceAction::Remove, true, false)
    );
    if !shape
        || change
            .previous
            .iter()
            .chain(change.incoming.iter())
            .any(|f| {
                f.path != change.path
                    || !valid_hash(&f.sha256, &[64])
                    || f.bytes > MAX_SOURCE_FILE_BYTES
            })
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Invalid source file operation",
        ));
    }
    Ok(())
}
fn plans(c: &Connection, r: &SourceTransactionRecord) -> Result<Vec<SourceChange>> {
    let mut q = c
        .prepare("SELECT ordinal,plan FROM source_plans WHERE update_id=?1 ORDER BY ordinal")
        .map_err(storage)?;
    let mut rows = q.query([&r.update_id]).map_err(storage)?;
    let mut changes = Vec::new();
    let mut total_bytes = 2usize;
    let mut paths = std::collections::BTreeSet::new();
    while let Some(row) = rows.next().map_err(storage)? {
        let ordinal: i64 = row.get(0).map_err(storage)?;
        let raw = row.get_ref(1).map_err(storage)?.as_str().map_err(storage)?;
        total_bytes = total_bytes.saturating_add(raw.len() + 1);
        if raw.len() > 32 * 1024 || total_bytes > 16 * 1024 * 1024 {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Source plan exceeds bounded decoding budget",
            ));
        }
        let json: String = row.get(1).map_err(storage)?;
        if counter(ordinal)? != changes.len() as u64
            || json.len() > 32 * 1024
            || changes.len() >= r.total_files as usize
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Invalid source plan ordinal/size",
            ));
        }
        let change: SourceChange = decode(&json)?;
        validate_change(&change)?;
        if !paths.insert(change.path.to_lowercase()) {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Duplicate source operation",
            ));
        }
        if change.path == SOURCE_MANIFEST_NAME
            && (changes.len() + 1 != r.total_files as usize
                || change.action != SourceAction::Replace
                || change
                    .previous
                    .as_ref()
                    .is_none_or(|f| f.sha256 != r.worker_source_spec.baseline_manifest_sha256)
                || change
                    .incoming
                    .as_ref()
                    .is_none_or(|f| f.sha256 != r.worker_source_spec.target_manifest_sha256))
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Manifest must be the last pinned replacement",
            ));
        }
        changes.push(change);
    }
    let bytes = serde_json::to_vec(&changes).map_err(storage)?;
    if changes.len() != r.total_files as usize
        || bytes.len() > 16 * 1024 * 1024
        || crate::sha256(&bytes) != r.plan_sha256
        || (r.worker_source_spec.baseline_manifest_sha256
            != r.worker_source_spec.target_manifest_sha256
            && changes
                .last()
                .is_none_or(|c| c.path != SOURCE_MANIFEST_NAME))
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Source plan count/hash mismatch",
        ));
    }
    Ok(changes)
}
fn source_owner(
    c: &Connection,
    authority: &UpdateAuthority,
    r: &SourceTransactionRecord,
) -> Result<()> {
    check_authority(c, authority)?;
    if r.worker_source_spec.updater_owner_id != authority.owner_id
        || r.worker_source_spec.updater_epoch != authority.epoch
    {
        return Err(fail(JournalErrorCode::StaleOwner,"Source lease belongs to a previous updater; inspect both journals, do not resume file effects"));
    }
    Ok(())
}
fn set_phase(
    c: &Connection,
    current: &SourceTransactionRecord,
    phase: SourceTransactionPhase,
    reason: &str,
) -> Result<()> {
    let mut next = current.clone();
    next.phase = phase;
    next.revision = next
        .revision
        .checked_add(1)
        .filter(|n| *n <= MAX_UPDATE_COUNTER)
        .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Source revision exhausted"))?;
    next.updated_at_ms = now_ms()?.max(next.updated_at_ms);
    next.reason = Some(reason.into());
    one(c.execute("UPDATE source_transactions SET record=?1,revision=?2,phase=?3 WHERE id=?4 AND revision=?5",params![encode(&next)?,next.revision as i64,encode(&phase)?,next.update_id,current.revision as i64]).map_err(storage)?)?;
    if header(c, &next.update_id)? != next {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Source phase readback mismatch",
        ));
    }
    Ok(())
}
fn observation(
    c: &Connection,
    id: &str,
    ordinal: u32,
    change: SourceChange,
) -> Result<SourceFileObservation> {
    let intent: Option<String> = c
        .query_row(
            "SELECT plan FROM source_file_intents WHERE update_id=?1 AND ordinal=?2",
            params![id, ordinal],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let receipt: Option<String> = c
        .query_row(
            "SELECT plan FROM source_file_receipts WHERE update_id=?1 AND ordinal=?2",
            params![id, ordinal],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let encoded = encode(&change)?;
    if intent.as_ref().is_some_and(|s| s != &encoded)
        || receipt.as_ref().is_some_and(|s| s != &encoded)
        || (receipt.is_some() && intent.is_none())
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Source intent/receipt differs from pinned operation",
        ));
    }
    Ok(SourceFileObservation {
        ordinal,
        change,
        state: if receipt.is_some() {
            SourceFileState::Applied
        } else if intent.is_some() {
            SourceFileState::Intent
        } else {
            SourceFileState::Planned
        },
    })
}

impl UpdateJournal {
    pub(crate) fn check_source_authority(&self, authority: &UpdateAuthority) -> Result<()> {
        self.check_directory(authority)?;
        check_authority(&self.connection, authority)
    }
    pub(crate) fn source_header(&self, id: &str) -> Result<SourceTransactionRecord> {
        header(&self.connection, id)
    }
    pub(crate) fn source_plan(&self, id: &str) -> Result<Vec<SourceChange>> {
        let r = header(&self.connection, id)?;
        plans(&self.connection, &r)
    }
    pub(crate) fn source_guard(
        &self,
        authority: &UpdateAuthority,
        id: &str,
        phase: SourceTransactionPhase,
    ) -> Result<SourceTransactionRecord> {
        self.check_directory(authority)?;
        let r = header(&self.connection, id)?;
        source_owner(&self.connection, authority, &r)?;
        let update = record(&self.connection, id)?;
        if r.phase != phase
            || update.phase
                != if phase == SourceTransactionPhase::Prepared {
                    UpdatePhase::Preparing
                } else {
                    UpdatePhase::SourceApplying
                }
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Source phase changed; no file effect permitted",
            ));
        }
        Ok(r)
    }
    pub(crate) fn store_source_plan(
        &mut self,
        authority: &UpdateAuthority,
        r: &SourceTransactionRecord,
        changes: &[SourceChange],
    ) -> Result<()> {
        self.check_directory(authority)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        source_owner(&tx, authority, r)?;
        if record(&tx, &r.update_id)?.phase != UpdatePhase::Preparing {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Source preparation requires Preparing",
            ));
        }
        one(tx
            .execute(
                "INSERT INTO source_transactions VALUES(?1,?2,?3,?4,?5)",
                params![
                    r.update_id,
                    binding(r)?,
                    r.revision as i64,
                    encode(&r.phase)?,
                    encode(r)?
                ],
            )
            .map_err(storage)?)?;
        for (i, change) in changes.iter().enumerate() {
            validate_change(change)?;
            one(tx
                .execute(
                    "INSERT INTO source_plans VALUES(?1,?2,?3)",
                    params![r.update_id, i as i64, encode(change)?],
                )
                .map_err(storage)?)?;
        }
        if header(&tx, &r.update_id)? != *r || plans(&tx, r)? != changes {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Source preparation readback failed",
            ));
        }
        tx.commit().map_err(storage)
    }
    pub(crate) fn change_source_phase(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        phase: SourceTransactionPhase,
        reason: &str,
    ) -> Result<SourceTransactionRecord> {
        let current = header(&self.connection, id)?;
        let update = record(&self.connection, id)?;
        let event = match (current.phase, phase) {
            (SourceTransactionPhase::Prepared, SourceTransactionPhase::Applying) => {
                winbridge_protocol::update::UpdateEvent::BeginSourceApply
            }
            (SourceTransactionPhase::Applying, SourceTransactionPhase::Verified) => {
                winbridge_protocol::update::UpdateEvent::SourceVerified
            }
            (
                SourceTransactionPhase::Prepared | SourceTransactionPhase::Applying,
                SourceTransactionPhase::Blocked,
            ) => winbridge_protocol::update::UpdateEvent::Fail,
            _ => {
                return Err(fail(
                    JournalErrorCode::InvalidRequest,
                    "Source transaction transition rejected",
                ))
            }
        };
        let command = UpdateCommand {
            idempotency_key: format!("source-{:?}", phase).to_lowercase(),
            expected_revision: update.revision,
            event,
            reason: reason.into(),
        };
        self.advance_with_source(authority, id, &command, true, |c, _| {
            if phase != SourceTransactionPhase::Blocked {
                source_owner(c, authority, &current)?;
            }
            if phase == SourceTransactionPhase::Verified {
                let changes = plans(c, &current)?;
                for (i, change) in changes.into_iter().enumerate() {
                    if observation(c, id, i as u32, change)?.state != SourceFileState::Applied {
                        return Err(fail(
                            JournalErrorCode::InvalidRequest,
                            "Source file receipt missing; finish forbidden",
                        ));
                    }
                }
            }
            set_phase(c, &current, phase, reason)
        })?;
        header(&self.connection, id)
    }
    pub(crate) fn source_file_intent(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        ordinal: u32,
    ) -> Result<()> {
        self.source_file_write(authority, id, ordinal, false)
    }
    pub(crate) fn source_file_receipt(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        ordinal: u32,
    ) -> Result<()> {
        self.source_file_write(authority, id, ordinal, true)
    }
    fn source_file_write(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        ordinal: u32,
        receipt: bool,
    ) -> Result<()> {
        self.check_directory(authority)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let r = header(&tx, id)?;
        source_owner(&tx, authority, &r)?;
        if r.phase != SourceTransactionPhase::Applying
            || record(&tx, id)?.phase != UpdatePhase::SourceApplying
            || ordinal >= r.total_files
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Source file intent/receipt requires applying phase and plan",
            ));
        }
        let json: String = tx
            .query_row(
                "SELECT plan FROM source_plans WHERE update_id=?1 AND ordinal=?2",
                params![id, ordinal],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let change: SourceChange = decode(&json)?;
        validate_change(&change)?;
        let before = observation(&tx, id, ordinal, change.clone())?;
        let expected = if receipt {
            SourceFileState::Intent
        } else {
            SourceFileState::Planned
        };
        if before.state != expected {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "File effect already attempted; inspect receipts, never replay it",
            ));
        }
        let table = if receipt {
            "source_file_receipts"
        } else {
            "source_file_intents"
        };
        one(tx
            .execute(
                &format!("INSERT INTO {table} VALUES(?1,?2,?3)"),
                params![id, ordinal, json],
            )
            .map_err(storage)?)?;
        let after = observation(&tx, id, ordinal, change)?;
        if after.state
            != if receipt {
                SourceFileState::Applied
            } else {
                SourceFileState::Intent
            }
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Source file write/readback ignored",
            ));
        }
        tx.commit().map_err(storage)
    }
    pub fn read_source_status(
        directory: &Path,
        id: &str,
        offset: u32,
        limit: u16,
    ) -> Result<SourceTransactionStatus> {
        if !(1..=100).contains(&limit) {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Source status page must contain 1–100 files",
            ));
        }
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? < 2 {
            return Err(fail(JournalErrorCode::UnsupportedSchema,"Source status needs updater schema 2; inspect journal, migrate only with a free owner lock"));
        }
        let tx = c.transaction().map_err(storage)?;
        let r = header(&tx, id)?;
        let plan = plans(&tx, &r)?;
        let mut applied = 0;
        let mut files = Vec::new();
        for (i, change) in plan.into_iter().enumerate() {
            let file = observation(&tx, id, i as u32, change)?;
            if file.state == SourceFileState::Applied {
                applied += 1;
            }
            if i >= offset as usize && files.len() < limit as usize {
                files.push(file);
            }
        }
        let status = SourceTransactionStatus {
            schema_version: 1,
            kind: "source_transaction_status".into(),
            read_only: true,
            has_more: (offset as u64 + files.len() as u64) < r.total_files as u64,
            transaction: r,
            applied_files: applied,
            files,
            observed_at_ms: now_ms()?,
        };
        tx.commit().map_err(storage)?;
        Ok(status)
    }
}
