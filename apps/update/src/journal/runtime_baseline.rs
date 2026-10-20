//! Early baseline intent/receipt, each atomic with its retained parent transition.
use super::*;
use crate::runtime_baseline::{
    RuntimeBaselineIntent, RuntimeBaselineReceipt, RuntimeBaselineSnapshot,
};
use winbridge_protocol::update::{SupervisorPhase, SupervisorRecord, UpdateEvent};
const INTENT_KEY: &str = "runtime-baseline-intent-v1";
const RECEIPT_KEY: &str = "runtime-baseline-verified-v1";
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE runtime_baseline_intents(id TEXT PRIMARY KEY REFERENCES updates(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_baselines(id TEXT PRIMARY KEY REFERENCES runtime_baseline_intents(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER runtime_baseline_intent_immutable BEFORE UPDATE ON runtime_baseline_intents BEGIN SELECT RAISE(ABORT,'Runtime baseline intent immutable'); END;
CREATE TRIGGER runtime_baseline_intent_retained BEFORE DELETE ON runtime_baseline_intents BEGIN SELECT RAISE(ABORT,'Runtime baseline intent retained'); END;
CREATE TRIGGER runtime_baseline_immutable BEFORE UPDATE ON runtime_baselines BEGIN SELECT RAISE(ABORT,'Runtime baseline immutable'); END;
CREATE TRIGGER runtime_baseline_retained BEFORE DELETE ON runtime_baselines BEGIN SELECT RAISE(ABORT,'Runtime baseline retained'); END;
PRAGMA user_version=8;").map_err(storage)
}
pub(super) fn transition(
    c: &Connection,
    id: &str,
    revision: u64,
    event: UpdateEvent,
    key: &str,
    owner: &str,
    epoch: u64,
) -> Result<u64> {
    // The same retained-transition proof is used by early baseline/preimages
    // and by candidate health. Bind both ends to the event's actual FSM edge.
    let previous = match event {
        UpdateEvent::RuntimeBaselineAdmitted
        | UpdateEvent::RuntimeBaselineVerified
        | UpdateEvent::RuntimePreimagesAdmitted
        | UpdateEvent::RuntimePreimagesVerified => UpdatePhase::Requested,
        UpdateEvent::CandidateHealthAdmitted | UpdateEvent::CandidateAccepted => {
            UpdatePhase::ValidatingCandidate
        }
        _ => {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Unsupported driver proof event",
            ))
        }
    };
    let phase = advance_update(previous, event)
        .map_err(|_| fail(JournalErrorCode::Corrupt, "Invalid driver proof FSM edge"))?;
    let (sequence, raw, snapshot, raw_transition): (i64, String, String, String) = c
        .query_row(
            "SELECT sequence,payload,record,transition FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, revision as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(storage)?;
    let command: UpdateCommand = decode(&raw)?;
    let record: UpdateRecord = decode(&snapshot)?;
    let t: UpdateTransition = decode(&raw_transition)?;
    let current = super::record(c, id)?;
    if command.event != event
        || command.idempotency_key != key
        || command.expected_revision.checked_add(1) != Some(revision)
        || record.update_id != id
        || record.revision != revision
        || record.phase != phase
        || record.request != current.request
        || record.accepted_at_ms != current.accepted_at_ms
        || t.update_id != id
        || t.sequence != counter(sequence)?
        || t.revision != revision
        || t.previous_phase != Some(previous)
        || t.phase != phase
        || t.command.as_ref() != Some(&command)
        || t.owner_id != owner
        || t.owner_epoch != epoch
        || t.occurred_at_ms != record.updated_at_ms
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Driver evidence requires its exact atomic parent transition",
        ));
    }
    Ok(record.updated_at_ms)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<RuntimeBaselineSnapshot> {
    if schema(c)? < 8 {
        return Ok(RuntimeBaselineSnapshot {
            intent: None,
            receipt: None,
        });
    }
    let intent: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_baseline_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let receipt: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_baselines WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let intent = intent
        .map(|v| decode::<RuntimeBaselineIntent>(&v))
        .transpose()?;
    let receipt = receipt
        .map(|v| decode::<RuntimeBaselineReceipt>(&v))
        .transpose()?;
    let current = record(c, id)?;
    if let Some(i) = &intent {
        let sup = supervisor::read(c, &i.launch_id)?;
        if i.schema_version != 1
            || i.update_id != id
            || i.owner_id.is_empty()
            || i.owner_epoch == 0
            || i.parent_revision == 0
            || i.parent_revision > current.revision
            || i.parent_revision > MAX_UPDATE_COUNTER
            || i.admitted_at_ms > MAX_UPDATE_COUNTER
            || i.request_sha256
                != crate::runtime_baseline::digest(&current.request).map_err(storage)?
            || sup.spec.update_id != id
            || sup.spec.worker.as_ref() != Some(&i.worker)
            || sup.spec.config_sha256 != i.config_sha256
            || sup.owner_id.as_deref() != Some(i.owner_id.as_str())
            || sup.owner_epoch != Some(i.owner_epoch)
            || i.worker.build.build_id != current.request.expected.worker_build_id
            || i.worker.config_sha256 != current.request.expected.worker_config_sha256
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Runtime baseline intent binding inconsistent",
            ));
        }
        let occurred_at = transition(
            c,
            id,
            i.parent_revision,
            UpdateEvent::RuntimeBaselineAdmitted,
            INTENT_KEY,
            &i.owner_id,
            i.owner_epoch,
        )?;
        if i.admitted_at_ms > occurred_at {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Runtime intent timestamp exceeds its transition",
            ));
        }
    }
    if let Some(r) = &receipt {
        let i = intent
            .as_ref()
            .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Runtime receipt without intent"))?;
        let supervisor = supervisor::read(c, &i.launch_id)?;
        if r.schema_version != 1
            || r.update_id != id
            || r.intent_sha256 != crate::runtime_baseline::digest(i).map_err(storage)?
            || r.parent_revision != i.parent_revision + 1
            || r.parent_revision > current.revision
            || r.observed_at_ms < i.admitted_at_ms
            || r.observed_at_ms > MAX_UPDATE_COUNTER
            || r.installed.runtime_directory != supervisor.spec.runtime_directory
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Runtime baseline receipt binding inconsistent",
            ));
        }
        crate::runtime_baseline::validate_observation(
            i,
            r,
            &current.request.expected.installation_id,
            &current.request.expected.scope_sha256,
        )
        .map_err(|e| fail(JournalErrorCode::Corrupt, e))?;
        let occurred_at = transition(
            c,
            id,
            r.parent_revision,
            UpdateEvent::RuntimeBaselineVerified,
            RECEIPT_KEY,
            &i.owner_id,
            i.owner_epoch,
        )?;
        if r.observed_at_ms > occurred_at {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Runtime receipt timestamp exceeds its transition",
            ));
        }
    }
    Ok(RuntimeBaselineSnapshot { intent, receipt })
}
pub(super) fn before_source(c: &Connection, id: &str) -> Result<()> {
    let view = read(c, id)?;
    if view.intent.is_some() && view.receipt.is_none() {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Runtime baseline pending; source effects are forbidden",
        ));
    }
    Ok(())
}
pub(super) fn early(c: &Connection, id: &str) -> Result<()> {
    let source: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_transactions WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let build: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM update_builds WHERE update_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if source || build || record(c, id)?.phase != UpdatePhase::Requested {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Runtime baseline must precede all source/compiler attempts; no late adoption",
        ));
    }
    Ok(())
}
pub(super) fn current_supervisor(
    c: &Connection,
    a: &UpdateAuthority,
    s: &SupervisorRecord,
) -> Result<()> {
    let saved = supervisor::read(c, &s.spec.launch_id)?;
    if saved != *s
        || s.phase != SupervisorPhase::Active
        || s.spec.worker.is_none()
        || s.owner_id.as_deref() != Some(a.owner_id())
        || s.owner_epoch != Some(a.epoch())
    {
        return Err(fail(
            JournalErrorCode::StaleOwner,
            "Exact active bound supervisor required for runtime baseline",
        ));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_runtime_baseline(directory: &Path, id: &str) -> Result<RuntimeBaselineSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn runtime_baseline(&self, id: &str) -> Result<RuntimeBaselineSnapshot> {
        read(&self.connection, id)
    }
    pub(crate) fn admit_runtime_baseline(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
    ) -> Result<RuntimeBaselineSnapshot> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        let id = &s.spec.update_id;
        let saved = self.runtime_baseline(id)?;
        if saved.intent.is_some() {
            return Ok(saved);
        }
        let parent = self.get(id)?;
        let intent = RuntimeBaselineIntent {
            schema_version: 1,
            update_id: id.clone(),
            owner_id: a.owner_id().into(),
            owner_epoch: a.epoch(),
            launch_id: s.spec.launch_id.clone(),
            worker: s
                .spec
                .worker
                .clone()
                .ok_or_else(|| fail(JournalErrorCode::InvalidRequest, "Bound worker required"))?,
            config_sha256: s.spec.config_sha256.clone(),
            request_sha256: crate::runtime_baseline::digest(&parent.request).map_err(storage)?,
            parent_revision: parent.revision + 1,
            admitted_at_ms: now_ms()?.max(parent.updated_at_ms),
        };
        let command = UpdateCommand {
            idempotency_key: INTENT_KEY.into(),
            expected_revision: parent.revision,
            event: UpdateEvent::RuntimeBaselineAdmitted,
            reason:
                "Проверка исходного runtime перед изменением исходников; файловые эффекты запрещены"
                    .into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Baseline, |c, _| {
            current_supervisor(c, a, s)?;
            early(c, id)?;
            one(c
                .execute(
                    "INSERT INTO runtime_baseline_intents VALUES(?1,?2)",
                    params![id, encode(&intent)?],
                )
                .map_err(storage)?)?;
            if read(c, id)?.intent.as_ref() != Some(&intent) {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Runtime intent write ignored",
                ));
            }
            Ok(())
        })?;
        read(&self.connection, id)
    }
    pub(crate) fn complete_runtime_baseline(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        receipt: &RuntimeBaselineReceipt,
    ) -> Result<RuntimeBaselineSnapshot> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        let id = &s.spec.update_id;
        let view = read(&self.connection, id)?;
        if let Some(previous) = &view.receipt {
            if previous != receipt {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Runtime receipt differs from its immutable original",
                ));
            }
            return Ok(view);
        }
        let intent = view
            .intent
            .ok_or_else(|| fail(JournalErrorCode::InvalidRequest, "Runtime intent missing"))?;
        let command = UpdateCommand {
            idempotency_key: RECEIPT_KEY.into(),
            expected_revision: intent.parent_revision,
            event: UpdateEvent::RuntimeBaselineVerified,
            reason:
                "Исходный runtime и loaded worker проверены; baseline сохранён до source effects"
                    .into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Baseline, |c, _| {
            current_supervisor(c, a, s)?;
            early(c, id)?;
            if intent.owner_id != a.owner_id() || intent.owner_epoch != a.epoch() {
                return Err(fail(
                    JournalErrorCode::StaleOwner,
                    "Runtime intent belongs to another epoch",
                ));
            }
            one(c
                .execute(
                    "INSERT INTO runtime_baselines VALUES(?1,?2)",
                    params![id, encode(receipt)?],
                )
                .map_err(storage)?)?;
            if read(c, id)?.receipt.as_ref() != Some(receipt) {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Runtime receipt write ignored",
                ));
            }
            Ok(())
        })?;
        read(&self.connection, id)
    }
}

#[cfg(test)]
#[path = "runtime_baseline_tests.rs"]
mod tests;
