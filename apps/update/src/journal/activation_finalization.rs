//! Schema23: root decision and release intent form one transaction. A Committed
//! root is not an Active worker. No retained row recreates an original owner.
use super::*;
use crate::activation::{
    finalization::{self, ActivationFinalizationRecord, ActivationFinalizationState},
    lifetime::ActivationLifetimeSnapshot,
};
use winbridge_protocol::update::*;
#[path = "activation_finalization_completion.rs"]
mod completion;
pub use completion::FinalizationConfirmationPermit;

fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE activation_finalization_intents(
 id TEXT NOT NULL PRIMARY KEY REFERENCES activation_lifetime_records(id),
 command_id TEXT NOT NULL UNIQUE,
 intent TEXT NOT NULL CHECK(json_valid(intent)),
 CHECK(json_extract(intent,'$.command.binding.lifetimeCommand.binding.updateId') IS id),
 CHECK(json_extract(intent,'$.command.commandId') IS command_id)
);
CREATE TABLE activation_finalization_records(
 id TEXT NOT NULL PRIMARY KEY REFERENCES activation_finalization_intents(id),
 revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),
 record TEXT NOT NULL CHECK(json_valid(record)),
 CHECK(json_extract(record,'$.revision') IS revision),
 CHECK(json_extract(record,'$.command.binding.lifetimeCommand.binding.updateId') IS id)
);
CREATE TABLE activation_finalization_history(
 id TEXT NOT NULL REFERENCES activation_finalization_records(id),
 revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 2),
 record TEXT NOT NULL CHECK(json_valid(record)),
 PRIMARY KEY(id,revision),
 CHECK(json_extract(record,'$.revision') IS revision),
 CHECK(json_extract(record,'$.command.binding.lifetimeCommand.binding.updateId') IS id)
);
CREATE TRIGGER activation_finalization_intent_immutable BEFORE UPDATE ON activation_finalization_intents BEGIN SELECT RAISE(ABORT,'Finalization intent immutable');END;
CREATE TRIGGER activation_finalization_intent_retained BEFORE DELETE ON activation_finalization_intents BEGIN SELECT RAISE(ABORT,'Finalization intent retained');END;
CREATE TRIGGER activation_finalization_record_identity BEFORE UPDATE ON activation_finalization_records
 WHEN NEW.id!=OLD.id OR NEW.revision!=OLD.revision+1
 OR json_extract(NEW.record,'$.command') IS NOT json_extract(OLD.record,'$.command')
 OR json_extract(OLD.record,'$.state.phase')='confirmed'
 BEGIN SELECT RAISE(ABORT,'Finalization command/revision/confirmation immutable');END;
CREATE TRIGGER activation_finalization_record_retained BEFORE DELETE ON activation_finalization_records BEGIN SELECT RAISE(ABORT,'Finalization record retained');END;
CREATE TRIGGER activation_finalization_history_immutable BEFORE UPDATE ON activation_finalization_history BEGIN SELECT RAISE(ABORT,'Finalization history immutable');END;
CREATE TRIGGER activation_finalization_history_retained BEFORE DELETE ON activation_finalization_history BEGIN SELECT RAISE(ABORT,'Finalization history retained');END;
PRAGMA user_version=23;
"#).map_err(storage)
}

/// Immutable evidence for the historical decision, never a live grant. The
/// ORIGINAL root and actual V7 request/response remain independently readable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationFinalizationIntent {
    pub command: WorkerFinalizationCommand,
    pub root_before: UpdateRecord,
    pub query: ActivationResidentQuery,
    pub observation: ActivationResidentObservation,
}
impl ActivationFinalizationIntent {
    fn validate(
        &self,
        graph: &ActivationLifetimeSnapshot,
        shutdown: &crate::shutdown::ShutdownRecord,
    ) -> Result<()> {
        if encode(self)?.len() > MAX_RECORD_BYTES {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Finalization intent exceeds retained record limit",
            ));
        }
        finalization::validate_command(
            &self.command,
            graph,
            &self.root_before,
            shutdown,
            &self.query,
            &self.observation,
        )
        .map_err(bad)
    }
}
/// Only the forthcoming consuming original-guard factory may construct this.
/// No public constructor, deserializer, clone or Existing->permit conversion.
/// This ledger alone intentionally does not implement that physical factory.
pub struct FinalizationDecisionPermit {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) intent: ActivationFinalizationIntent,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) observation_deadline: std::time::Instant,
}
/// One fresh delivery may be prepared only from a known successful root commit.
/// An uncertain commit or Existing result never recreates this object.
pub struct FinalizationReleasePermit {
    record: ActivationFinalizationRecord,
}
impl FinalizationReleasePermit {
    pub fn record(&self) -> &ActivationFinalizationRecord {
        &self.record
    }
}
pub enum FinalizationDecisionAdmission {
    Committed(Box<FinalizationReleasePermit>),
    Existing(Box<ActivationFinalizationRecord>),
}

fn root_command(intent: &ActivationFinalizationIntent) -> UpdateCommand {
    UpdateCommand {
        idempotency_key: format!("finalization-root-{}", intent.command.command_id),
        expected_revision: intent.root_before.revision,
        event: UpdateEvent::ControllerConfirmed,
        reason: "Установка подтверждена; освобождение maintenance и рабочий режим worker ещё требуют отдельного подтверждения".into(),
    }
}
fn committed_root(intent: &ActivationFinalizationIntent) -> Result<UpdateRecord> {
    finalization::validate_command_envelope(&intent.command).map_err(bad)?;
    let old = &intent.root_before;
    let command = root_command(intent);
    let mut root = old.clone();
    root.phase = advance_update(root.phase, command.event).map_err(bad)?;
    root.revision = intent.command.binding.committed_root_revision;
    if old.revision.checked_add(1) != Some(root.revision)
        || root.updated_at_ms > intent.command.issued_at_ms
    {
        return Err(bad("Finalization root revision/time differs"));
    }
    root.updated_at_ms = intent.command.issued_at_ms;
    root.reason = Some(command.reason);
    Ok(root)
}
fn presence(c: &Connection, id: &str) -> Result<bool> {
    let (intent, current, history): (bool, bool, bool) = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM activation_finalization_intents WHERE id=?1),EXISTS(SELECT 1 FROM activation_finalization_records WHERE id=?1),EXISTS(SELECT 1 FROM activation_finalization_history WHERE id=?1)",
        [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(storage)?;
    if intent != current || intent != history {
        return Err(bad("Finalization partial ledger is not absence"));
    }
    Ok(intent)
}
fn ledger(
    c: &Connection,
    id: &str,
    graph: &ActivationLifetimeSnapshot,
    shutdown: &crate::shutdown::ShutdownRecord,
) -> Result<(ActivationFinalizationIntent, ActivationFinalizationRecord)> {
    let (key, raw): (String, String) = c
        .query_row(
            "SELECT command_id,intent FROM activation_finalization_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let intent: ActivationFinalizationIntent = decode(&raw)?;
    intent.validate(graph, shutdown)?;
    if intent.command.command_id != key || intent.root_before.update_id != id {
        return Err(bad("Finalization intent/index scope differs"));
    }
    let root = record(c, id)?;
    let (old_raw, edge_raw): (String, String) = c
        .query_row(
            "SELECT record,transition FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, intent.root_before.revision as i64],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let old: UpdateRecord = decode(&old_raw)?;
    let old_edge: UpdateTransition = decode(&edge_raw)?;
    activation_controller::validate_local_root_edge(
        &graph.controller.local,
        &graph.controller.service,
        &old,
        &old_edge,
    )?;
    if old != intent.root_before
        || old_edge.update_id != id
        || old_edge.revision != old.revision
        || old_edge.phase != old.phase
        || old_edge.occurred_at_ms != old.updated_at_ms
        || root != committed_root(&intent)?
    {
        return Err(bad(
            "Finalization original root/committed decision detached from history",
        ));
    }
    let raw: String = c
        .query_row(
            "SELECT transition FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, root.revision as i64],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let edge: UpdateTransition = decode(&raw)?;
    let binding = &intent.command.binding.lifetime_command.binding;
    if edge.command.as_ref() != Some(&root_command(&intent))
        || edge.update_id != id
        || edge.revision != root.revision
        || edge.previous_phase != Some(old.phase)
        || edge.phase != UpdatePhase::Committed
        || edge.occurred_at_ms != root.updated_at_ms
        || edge.owner_id != binding.updater_owner_id
        || edge.owner_epoch != binding.updater_epoch
    {
        return Err(bad("Finalization root edge/key/owner differs"));
    }
    let (revision, raw): (i64, String) = c
        .query_row(
            "SELECT revision,record FROM activation_finalization_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    let current: ActivationFinalizationRecord = decode(&raw)?;
    current.validate().map_err(bad)?;
    if current.revision != counter(revision)? || current.command != intent.command {
        return Err(bad("Finalization current/intent/revision differs"));
    }
    let mut q = c.prepare("SELECT revision,record FROM activation_finalization_history WHERE id=?1 ORDER BY revision LIMIT 4").map_err(storage)?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage)?;
    if rows.len() as u64 != current.revision + 1 {
        return Err(bad("Finalization history missing/oversized"));
    }
    let mut previous =
        ActivationFinalizationRecord::pending(intent.command.clone()).map_err(bad)?;
    for (index, (revision, raw)) in rows.into_iter().enumerate() {
        let next: ActivationFinalizationRecord = decode(&raw)?;
        if revision != index as i64 || next.revision != index as u64 {
            return Err(bad("Finalization history sequence gap"));
        }
        if index == 0 {
            if next != previous {
                return Err(bad("Finalization history must start ReleasePending"));
            }
        } else {
            previous.advance(&next, shutdown).map_err(bad)?;
        }
        previous = next;
    }
    if previous != current {
        return Err(bad("Finalization current/history differs"));
    }
    current.blocks_next_update(shutdown).map_err(bad)?;
    Ok((intent, current))
}
/// Data only. Reading this structure never authenticates a process, reopens a
/// Store or creates a finalization delivery/Active permit.
#[derive(Clone, Debug)]
pub struct RetainedFinalizationSnapshot {
    pub intent: ActivationFinalizationIntent,
    pub record: ActivationFinalizationRecord,
    pub lifetime: ActivationLifetimeSnapshot,
    pub shutdown: crate::shutdown::ShutdownRecord,
}
fn snapshot(c: &Connection, id: &str) -> Result<Option<RetainedFinalizationSnapshot>> {
    if schema(c)? < 23 || !presence(c, id)? {
        return Ok(None);
    }
    let controller = activation_controller::snapshot(c, id)?;
    let lifetime = activation_lifetime::read(c, id)?
        .ok_or_else(|| bad("Finalization lifetime prefix missing"))?;
    let shutdown =
        shutdown::read(c, id)?.ok_or_else(|| bad("Finalization old-worker exit prefix missing"))?;
    let lifetime = ActivationLifetimeSnapshot {
        controller,
        lifetime,
    };
    let (intent, record) = ledger(c, id, &lifetime, &shutdown)?;
    Ok(Some(RetainedFinalizationSnapshot {
        intent,
        record,
        lifetime,
        shutdown,
    }))
}
fn read(
    c: &Connection,
    id: &str,
) -> Result<Option<(ActivationFinalizationIntent, ActivationFinalizationRecord)>> {
    Ok(snapshot(c, id)?.map(|s| (s.intent, s.record)))
}

fn require_resident_prefix(
    saved: &RetainedFinalizationSnapshot,
    actual: &SupervisorProcess,
    original: &WorkerLifetimeActivationCommand,
    entry: &WorkerLifetimeActivationReceipt,
    requested: &WorkerFinalizationCommand,
) -> Result<()> {
    let b = &original.binding;
    let local = &saved.lifetime.controller.local;
    let crate::activation::lifetime::ActivationLifetimeState::Confirmed { receipt } =
        &saved.lifetime.lifetime.state
    else {
        return Err(bad(
            "Pending finalization requires confirmed original lifetime",
        ));
    };
    finalization::validate_command_envelope(requested).map_err(bad)?;
    crate::activation::lifetime::validate_receipt(entry, original).map_err(bad)?;
    if saved.intent.command != *requested
        || saved.record.command != *requested
        || saved.lifetime.lifetime.command != *original
        || requested.binding.lifetime_command != *original
        || receipt.as_ref() != entry
        || actual != &b.compatibility.identity.process
        || local.created.as_ref().map(|v| &v.process) != Some(actual)
        || local.ready.as_ref().map(|v| &v.identity.process) != Some(actual)
        || local.state_owned.as_ref().map(|v| &v.process) != Some(actual)
        || local.intent.context.supervisor != b.keeper
        || matches!(
            saved.record.state,
            ActivationFinalizationState::Confirmed { .. }
        )
    {
        return Err(bad(
            "Pending receiver changed original resident/entry/decision or is already confirmed",
        ));
    }
    Ok(())
}

impl UpdateJournal {
    /// Readonly scope for a FUTURE authenticated V8 receiver that ALREADY owns
    /// the same Supervised/ReleasePending service, actual OS keeper pin and Job.
    /// The arguments must come from that sealed memory, never from caller JSON.
    /// Old V5/V6 receiver admission remains frozen at AwaitingController.
    pub fn inspect_pending_finalization_receiver(
        directory: &Path,
        actual: &SupervisorProcess,
        original: &WorkerLifetimeActivationCommand,
        entry: &WorkerLifetimeActivationReceipt,
        requested: &WorkerFinalizationCommand,
    ) -> Result<RetainedFinalizationSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        if schema(&tx)? != UPDATE_SCHEMA_VERSION {
            return Err(bad(
                "Pending finalization requires exact current updater schema",
            ));
        }
        let b = &original.binding;
        let id = &b.update_id;
        let saved = snapshot(&tx, id)?
            .ok_or_else(|| bad("No committed finalization intent; a query cannot create it"))?;
        require_resident_prefix(&saved, actual, original, entry, requested)?;
        let owner: (i64, String) = tx
            .query_row(
                "SELECT epoch,owner FROM installation WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(storage)?;
        let installation = installation_info(&tx)?;
        let coordinator = supervisor::read(&tx, &b.launch_id)?;
        if saved
            .lifetime
            .controller
            .local
            .intent
            .context
            .updater_journal_schema
            != UPDATE_SCHEMA_VERSION
            || counter(owner.0)? != b.updater_epoch
            || owner.1 != b.updater_owner_id
            || installation.installation_id != requested.binding.installation_id
            || installation.scope_sha256 != requested.binding.scope_sha256
            || coordinator.phase != SupervisorPhase::Active
            || coordinator.spec.update_id != *id
            || coordinator.spec.launch_id != b.launch_id
            || coordinator.owner_id.as_ref() != Some(&b.updater_owner_id)
            || coordinator.owner_epoch != Some(b.updater_epoch)
            || coordinator.process.as_ref() != Some(&b.keeper)
        {
            return Err(bad(
                "Pending finalization has stale updater owner/coordinator/schema; no adoption",
            ));
        }
        tx.commit().map_err(storage)?;
        Ok(saved)
    }
}

/// Validate every retained decision before allowing another admission. This
/// also detects orphan current/history rows and a Committed lifetime root that
/// has lost its mandatory release intent. Reads never recreate physical owners.
pub(super) fn blocking(c: &Connection) -> Result<Option<String>> {
    if schema(c)? < 23 {
        return Ok(None);
    }
    let missing: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM updates u JOIN activation_lifetime_records l ON l.id=u.id LEFT JOIN activation_finalization_intents f ON f.id=u.id WHERE u.phase=?1 AND f.id IS NULL)",[encode(&UpdatePhase::Committed)?],|r|r.get(0)).map_err(storage)?;
    if missing {
        return Err(bad(
            "Committed lifetime root has no finalization intent; preserve and reconcile",
        ));
    }
    let mut q = c.prepare("SELECT id FROM activation_finalization_intents UNION SELECT id FROM activation_finalization_records UNION SELECT id FROM activation_finalization_history").map_err(storage)?;
    let mut rows = q.query([]).map_err(storage)?;
    let mut blocked = None;
    while let Some(row) = rows.next().map_err(storage)? {
        let id: String = row.get(0).map_err(storage)?;
        let (_, record) =
            read(c, &id)?.ok_or_else(|| bad("Finalization partial state disappeared"))?;
        if !matches!(record.state, ActivationFinalizationState::Confirmed { .. }) {
            if blocked.is_some() {
                return Err(bad("Multiple unresolved finalizations"));
            }
            blocked = Some(id);
        }
    }
    Ok(blocked)
}

#[cfg(any(windows, test))]
fn seed_and_root(
    c: &Connection,
    a: &UpdateAuthority,
    intent: &ActivationFinalizationIntent,
    graph: &ActivationLifetimeSnapshot,
    shutdown: &crate::shutdown::ShutdownRecord,
) -> Result<ActivationFinalizationRecord> {
    check_authority(c, a)?;
    intent.validate(graph, shutdown)?;
    let old = &intent.root_before;
    let id = &old.update_id;
    let b = &intent.command.binding.lifetime_command.binding;
    if a.owner_id() != b.updater_owner_id
        || a.epoch() != b.updater_epoch
        || a.installation_id != intent.command.binding.installation_id
        || a.scope_sha256 != intent.command.binding.scope_sha256
        || record(c, id)? != *old
        || presence(c, id)?
    {
        return Err(bad(
            "Finalization requires exact current root/authority and no previous intent",
        ));
    }
    let pending = ActivationFinalizationRecord::pending(intent.command.clone()).map_err(bad)?;
    let raw = encode(&pending)?;
    one(c
        .execute(
            "INSERT INTO activation_finalization_intents VALUES(?1,?2,?3)",
            params![id, intent.command.command_id, encode(intent)?],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_finalization_records VALUES(?1,0,?2)",
            params![id, raw],
        )
        .map_err(storage)?)?;
    one(c
        .execute(
            "INSERT INTO activation_finalization_history VALUES(?1,0,?2)",
            params![id, raw],
        )
        .map_err(storage)?)?;
    let root = committed_root(intent)?;
    let raw = encode(&root)?;
    one(c.execute("UPDATE updates SET record=?1,revision=?2,phase=?3,active=0 WHERE id=?4 AND revision=?5",params![raw,root.revision as i64,encode(&root.phase)?,id,old.revision as i64]).map_err(storage)?)?;
    let command = root_command(intent);
    append_transition(
        c,
        &UpdateTransition {
            sequence: 0,
            update_id: id.clone(),
            revision: root.revision,
            previous_phase: Some(old.phase),
            phase: root.phase,
            occurred_at_ms: root.updated_at_ms,
            owner_epoch: a.epoch(),
            owner_id: a.owner_id().into(),
            command: Some(command.clone()),
        },
        Some(&encode(&command)?),
        &raw,
    )?;
    let (_, actual) = ledger(c, id, graph, shutdown)?;
    if actual != pending {
        return Err(bad("Finalization/root append readback differs"));
    }
    Ok(actual)
}

impl UpdateJournal {
    /// Library-only trusted decision. The physical factory is intentionally
    /// still absent; callers cannot create its non-Serde permit from JSON.
    /// No worker command is delivered and no lease is released by this method.
    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub fn commit_activation_finalization(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        preparation: &crate::activation::ActivationPreparation,
        local: &crate::activation::ActivationRecord,
        permit: FinalizationDecisionPermit,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<FinalizationDecisionAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        let deadline = permit.observation_deadline;
        let intent = permit.intent;
        let id = &s.spec.update_id;
        if let Some((previous, current)) = read(&tx, id)? {
            if encode(&previous)? != encode(&intent)? {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Finalization has another key/payload; no new release",
                ));
            }
            return Ok(FinalizationDecisionAdmission::Existing(Box::new(current)));
        }
        let controller = activation_controller::owned_snapshot(&tx, a, s, preparation, local)?;
        let lifetime = activation_lifetime::read(&tx, id)?
            .ok_or_else(|| bad("Finalization lifetime missing"))?;
        let shutdown =
            shutdown::read(&tx, id)?.ok_or_else(|| bad("Finalization shutdown missing"))?;
        intent.validate(
            &ActivationLifetimeSnapshot {
                controller: controller.clone(),
                lifetime: lifetime.clone(),
            },
            &shutdown,
        )?;
        let fresh = || -> Result<()> {
            if std::time::Instant::now() >= deadline {
                return Err(bad("Finalization original monotonic capture expired"));
            }
            finalization::validate_admission_time(&intent.command, &intent.observation, now_ms()?)
                .map_err(bad)
        };
        fresh()?;
        supervisor::require_driver(&tx, a, id, UpdateEvent::ControllerConfirmed)?;
        checkpoint().map_err(bad)?;
        fresh()?;
        let value = seed_and_root(
            &tx,
            a,
            &intent,
            &ActivationLifetimeSnapshot {
                controller,
                lifetime,
            },
            &shutdown,
        )?;
        checkpoint().map_err(bad)?;
        fresh()?;
        tx.commit().map_err(|e|storage(format!("Finalization/root commit unconfirmed; inspect same key, never redeliver automatically: {e}")))?;
        Ok(FinalizationDecisionAdmission::Committed(Box::new(
            FinalizationReleasePermit { record: value },
        )))
    }
    pub fn activation_finalization(
        &mut self,
        id: &str,
    ) -> Result<Option<ActivationFinalizationRecord>> {
        let tx = self.connection.transaction().map_err(storage)?;
        let value = read(&tx, id)?.map(|(_, record)| record);
        tx.commit().map_err(storage)?;
        Ok(value)
    }
    pub fn inspect_activation_finalization(
        directory: &Path,
        id: &str,
    ) -> Result<Option<ActivationFinalizationRecord>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let value = read(&tx, id)?.map(|(_, record)| record);
        tx.commit().map_err(storage)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
