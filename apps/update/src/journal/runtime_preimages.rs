//! One immutable, create-only previous-runtime generation before source effects.
use super::*;
use crate::runtime_baseline::digest;
use crate::runtime_preimages::{
    RuntimePreimagesIntent, RuntimePreimagesReceipt, RuntimePreimagesSnapshot,
};
use winbridge_protocol::update::{SupervisorRecord, UpdateEvent};
const INTENT_KEY: &str = "runtime-preimages-intent-v1";
const RECEIPT_KEY: &str = "runtime-preimages-verified-v1";
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE runtime_preimage_intents(id TEXT PRIMARY KEY REFERENCES runtime_baselines(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TABLE runtime_preimages(id TEXT PRIMARY KEY REFERENCES runtime_preimage_intents(id),record TEXT NOT NULL CHECK(json_valid(record)));
CREATE TRIGGER runtime_preimage_intent_immutable BEFORE UPDATE ON runtime_preimage_intents BEGIN SELECT RAISE(ABORT,'Runtime preimage intent immutable'); END;
CREATE TRIGGER runtime_preimage_intent_retained BEFORE DELETE ON runtime_preimage_intents BEGIN SELECT RAISE(ABORT,'Runtime preimage intent retained'); END;
CREATE TRIGGER runtime_preimages_immutable BEFORE UPDATE ON runtime_preimages BEGIN SELECT RAISE(ABORT,'Runtime preimages immutable'); END;
CREATE TRIGGER runtime_preimages_retained BEFORE DELETE ON runtime_preimages BEGIN SELECT RAISE(ABORT,'Runtime preimages retained'); END;
PRAGMA user_version=9;").map_err(storage)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<RuntimePreimagesSnapshot> {
    let empty = RuntimePreimagesSnapshot {
        intent: None,
        receipt: None,
    };
    if schema(c)? < 9 {
        return Ok(empty);
    }
    let intent: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_preimage_intents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let receipt: Option<String> = c
        .query_row(
            "SELECT record FROM runtime_preimages WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let intent = intent
        .map(|s| decode::<RuntimePreimagesIntent>(&s))
        .transpose()?;
    let receipt = receipt
        .map(|s| decode::<RuntimePreimagesReceipt>(&s))
        .transpose()?;
    if intent.is_none() && receipt.is_none() {
        return Ok(empty);
    }
    let i = intent.as_ref().ok_or_else(|| {
        fail(
            JournalErrorCode::Corrupt,
            "Preimages receipt without intent",
        )
    })?;
    let baseline = runtime_baseline::read(c, id)?;
    let b = baseline.receipt.ok_or_else(|| {
        fail(
            JournalErrorCode::Corrupt,
            "Preimages without verified baseline",
        )
    })?;
    let bi = baseline
        .intent
        .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Baseline intent missing"))?;
    let sup = supervisor::read(c, &i.launch_id)?;
    let parent = crate::runtime_preimages::parent_path(&sup.spec.runtime_directory, id);
    let current = record(c, id)?;
    if i.schema_version != 1
        || i.update_id != id
        || i.owner_id != bi.owner_id
        || i.owner_epoch != bi.owner_epoch
        || i.launch_id != bi.launch_id
        || i.baseline_sha256 != digest(&b).map_err(storage)?
        || i.parent_revision != b.parent_revision + 1
        || i.parent_revision > current.revision
        || i.parent_revision > MAX_UPDATE_COUNTER
        || i.admitted_at_ms > MAX_UPDATE_COUNTER
        || i.available_bytes_before < 6 * 1024 * 1024 * 1024
        || i.parent_directory
            != parent
                .to_str()
                .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Parent path not UTF-8"))?
        || i.directory
            != parent
                .join(crate::runtime_preimages::DIRECTORY)
                .to_str()
                .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Preimages path not UTF-8"))?
        || i.parent_identity.is_empty()
        || i.parent_identity.len() > 128
        || i.parent_identity == b.installed.runtime_identity
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Preimages intent/baseline/scope binding inconsistent",
        ));
    }
    let timestamp = runtime_baseline::transition(
        c,
        id,
        i.parent_revision,
        UpdateEvent::RuntimePreimagesAdmitted,
        INTENT_KEY,
        &i.owner_id,
        i.owner_epoch,
    )?;
    if i.admitted_at_ms > timestamp {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Preimages intent timestamp exceeds transition",
        ));
    }
    if let Some(r) = &receipt {
        crate::runtime_preimages::validate_receipt(i, r, &b)
            .map_err(|e| fail(JournalErrorCode::Corrupt, e))?;
        if r.parent_revision > current.revision
            || r.parent_revision > MAX_UPDATE_COUNTER
            || r.completed_at_ms > MAX_UPDATE_COUNTER
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Preimages receipt counters exceed parent",
            ));
        }
        let timestamp = runtime_baseline::transition(
            c,
            id,
            r.parent_revision,
            UpdateEvent::RuntimePreimagesVerified,
            RECEIPT_KEY,
            &i.owner_id,
            i.owner_epoch,
        )?;
        if r.completed_at_ms > timestamp {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Preimages receipt timestamp exceeds transition",
            ));
        }
    }
    Ok(RuntimePreimagesSnapshot { intent, receipt })
}
pub(super) fn before_source(c: &Connection, id: &str) -> Result<()> {
    let view = read(c, id)?;
    if view.intent.is_some() && view.receipt.is_none() {
        return Err(fail(
            JournalErrorCode::PreconditionsChanged,
            "Runtime preimages pending; source effects are forbidden",
        ));
    }
    Ok(())
}
impl UpdateJournal {
    pub fn inspect_runtime_preimages(
        directory: &Path,
        id: &str,
    ) -> Result<RuntimePreimagesSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        read(&tx, id)
    }
    pub(crate) fn runtime_preimages(&self, id: &str) -> Result<RuntimePreimagesSnapshot> {
        read(&self.connection, id)
    }
    pub(crate) fn require_runtime_preimages_current(
        &self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        i: &RuntimePreimagesIntent,
    ) -> Result<()> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        runtime_baseline::current_supervisor(&self.connection, a, s)?;
        runtime_baseline::early(&self.connection, &i.update_id)?;
        let snapshot = read(&self.connection, &i.update_id)?;
        let parent = record(&self.connection, &i.update_id)?;
        if snapshot.intent.as_ref() != Some(i)
            || snapshot.receipt.is_some()
            || i.owner_id != a.owner_id()
            || i.owner_epoch != a.epoch()
            || i.launch_id != s.spec.launch_id
            || parent.revision != i.parent_revision
        {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Fresh pending runtime preimages permit required",
            ));
        }
        Ok(())
    }
    pub(crate) fn admit_runtime_preimages(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        i: &RuntimePreimagesIntent,
    ) -> Result<RuntimePreimagesSnapshot> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        let id = &s.spec.update_id;
        if read(&self.connection, id)?.intent.is_some() {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Previous preimages attempt is observation only; no replay",
            ));
        }
        let p = self.get(id)?;
        let command = UpdateCommand {
            idempotency_key: INTENT_KEY.into(),
            expected_revision: p.revision,
            event: UpdateEvent::RuntimePreimagesAdmitted,
            reason: "Сохранение предыдущего runtime принято; рабочие exe не заменяются".into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Preimages, |c, _| {
            runtime_baseline::current_supervisor(c, a, s)?;
            runtime_baseline::early(c, id)?;
            if i.owner_id != a.owner_id() || i.owner_epoch != a.epoch() {
                return Err(fail(
                    JournalErrorCode::StaleOwner,
                    "Preimages intent belongs to another owner",
                ));
            }
            one(c
                .execute(
                    "INSERT INTO runtime_preimage_intents VALUES(?1,?2)",
                    params![id, encode(i)?],
                )
                .map_err(storage)?)?;
            if read(c, id)?.intent.as_ref() != Some(i) {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Preimages intent write ignored",
                ));
            }
            Ok(())
        })?;
        read(&self.connection, id)
    }
    pub(crate) fn complete_runtime_preimages(
        &mut self,
        a: &UpdateAuthority,
        s: &SupervisorRecord,
        r: &RuntimePreimagesReceipt,
    ) -> Result<RuntimePreimagesSnapshot> {
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        let id = &s.spec.update_id;
        let saved = read(&self.connection, id)?;
        if let Some(previous) = &saved.receipt {
            if previous != r {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Preimages receipt differs from original",
                ));
            }
            return Ok(saved);
        }
        let i = saved
            .intent
            .ok_or_else(|| fail(JournalErrorCode::NotFound, "Preimages intent missing"))?;
        self.require_runtime_preimages_current(a, s, &i)?;
        let command = UpdateCommand {
            idempotency_key: RECEIPT_KEY.into(),
            expected_revision: i.parent_revision,
            event: UpdateEvent::RuntimePreimagesVerified,
            reason: "Пять независимых копий предыдущего runtime проверены и сохранены".into(),
        };
        self.advance_with_driver(a, id, &command, Driver::Preimages, |c, _| {
            runtime_baseline::current_supervisor(c, a, s)?;
            runtime_baseline::early(c, id)?;
            one(c
                .execute(
                    "INSERT INTO runtime_preimages VALUES(?1,?2)",
                    params![id, encode(r)?],
                )
                .map_err(storage)?)?;
            if read(c, id)?.receipt.as_ref() != Some(r) {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Preimages receipt write ignored",
                ));
            }
            Ok(())
        })?;
        read(&self.connection, id)
    }
}
