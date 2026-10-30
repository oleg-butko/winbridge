//! Schema24: immutable selection of the original coordinator's keeper role.
//! A row grants no process, Job, writer-close fact, lock release or bootstrap.
use super::*;
use crate::activation::finalization::{ActivationFinalizationRecord, ActivationFinalizationState};
use winbridge_protocol::update::*;

const REASON: &str = "Original coordinator selected keeper retirement; writer/authority release require live confirmation";
fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeeperSelection {
    pub schema_version: u16,
    pub before: SupervisorRecord,
    pub selected: SupervisorRecord,
    pub finalization: ActivationFinalizationRecord,
    pub query: ActivationFinalizationQuery,
    pub observation: ActivationFinalizationObservation,
}
impl KeeperSelection {
    #[cfg(any(windows, test))]
    fn prepare(
        before: SupervisorRecord,
        finalization: ActivationFinalizationRecord,
        query: ActivationFinalizationQuery,
        observation: ActivationFinalizationObservation,
        at: u64,
    ) -> Result<Self> {
        let mut selected = before.clone();
        selected.phase = SupervisorPhase::Keeper;
        selected.revision = before
            .revision
            .checked_add(1)
            .filter(|v| *v <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| bad("Keeper revision exhausted"))?;
        selected.updated_at_ms = at;
        selected.reason = REASON.into();
        let value = Self {
            schema_version: 1,
            before,
            selected,
            finalization,
            query,
            observation,
        };
        value.validate()?;
        Ok(value)
    }
    /// Historical envelope only. This intentionally never tests present-day
    /// liveness or invents the physical owner that originally selected it.
    pub fn validate(&self) -> Result<()> {
        if encode(self)?.len() > MAX_RECORD_BYTES {
            return Err(bad("Keeper selection exceeds record limit"));
        }
        self.finalization.validate().map_err(bad)?;
        crate::activation::finalization_observation::validate_query(&self.query).map_err(bad)?;
        let b = &self.finalization.command.binding;
        let original = &b.lifetime_command.binding;
        let ActivationFinalizationState::Confirmed { receipt } = &self.finalization.state else {
            return Err(bad("Keeper selection requires Confirmed Active"));
        };
        let ActivationFinalizationObservationResult::Observed {
            receipt: current,
            observed_at_ms,
        } = &self.observation.result
        else {
            return Err(bad("Keeper selection requires independent current Active"));
        };
        let old = self
            .before
            .spec
            .worker
            .as_ref()
            .ok_or_else(|| bad("Keeper selection lost its original worker binding"))?;
        if old.process == original.compatibility.identity.process {
            return Err(bad("Keeper old and new worker processes alias"));
        }
        if old.context != original.compatibility.context {
            return Err(bad("Keeper old/new worker contexts differ"));
        }
        if old.config_sha256 != original.compatibility.worker_config_sha256 {
            return Err(bad("Keeper old/new loaded config hashes differ"));
        }
        let at = self.selected.updated_at_ms;
        let mut expected = self.before.clone();
        expected.phase = SupervisorPhase::Keeper;
        expected.revision = self
            .before
            .revision
            .checked_add(1)
            .filter(|v| *v <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| bad("Keeper revision exhausted"))?;
        expected.updated_at_ms = at;
        expected.reason = REASON.into();
        if self.schema_version != 1
            || self.before.schema_version != 1
            || self.before.phase != SupervisorPhase::Active
            || self.before.spec.launch_id != original.launch_id
            || self.before.spec.update_id != original.update_id
            || self.before.process.as_ref() != Some(&original.keeper)
            || self.before.owner_epoch != Some(original.updater_epoch)
            || self.before.owner_id.as_deref() != Some(original.updater_owner_id.as_str())
            || self.selected != expected
            || self.query.command != self.finalization.command
            || self.observation.finalization_observation_version != 1
            || self.observation.request_id.as_deref() != Some(self.query.request_id.as_str())
            || current != receipt
            || at < self.before.updated_at_ms
            || at < self.finalization.updated_at_ms
            || at > MAX_UPDATE_COUNTER
            || *observed_at_ms < receipt.compatibility.observed_at_ms
            || *observed_at_ms > at
            || at - observed_at_ms > winbridge_transport::IPC_TIMEOUT.as_millis() as u64
        {
            return Err(bad(
                "Keeper selection changed original coordinator/Active/correlation/time",
            ));
        }
        Ok(())
    }
    pub fn sha256(&self) -> Result<String> {
        self.validate()?;
        let mut bytes = b"winbridge.keeper.selection.v1\n".to_vec();
        bytes.extend(
            winbridge_protocol::canonical_evidence_json(
                &serde_json::to_value(self).map_err(storage)?,
            )
            .map_err(storage)?,
        );
        Ok(crate::sha256(&bytes))
    }
    fn require_root(&self, root: &UpdateRecord) -> Result<()> {
        let b = &self.finalization.command.binding;
        if self
            .before
            .spec
            .worker
            .as_ref()
            .map(|w| w.build.build_id.as_str())
            != Some(root.request.expected.worker_build_id.as_str())
        {
            return Err(bad("Keeper root changed the original worker build"));
        }
        if root.phase != UpdatePhase::Committed
            || root.update_id != self.selected.spec.update_id
            || root.revision != b.committed_root_revision
            || root.request.expected.installation_id != b.installation_id
            || root.request.expected.scope_sha256 != b.scope_sha256
            || root.request.source.manifest_sha256 != b.source_manifest_sha256
        {
            return Err(bad("Keeper selection detached from committed root"));
        }
        Ok(())
    }
}

/// Minted by the consuming original physical owner after a fresh normal read.
/// No Clone, serde, constructor or conversion from saved rows.
pub struct KeeperSelectionPermit {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) capture: FinalizationConfirmationPermit,
}
/// A known role-selection commit, not a claim that connections/locks closed.
#[must_use = "Retain original custody until writer retirement and authority release"]
pub struct KeeperSelectionCommit {
    selection: KeeperSelection,
}
impl KeeperSelectionCommit {
    pub fn selection(&self) -> &KeeperSelection {
        &self.selection
    }
}
pub enum KeeperSelectionAdmission {
    Committed(Box<KeeperSelectionCommit>),
    Existing(Box<KeeperSelection>),
}

pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE keeper_selections(
 launch_id TEXT PRIMARY KEY REFERENCES supervisor_launches(launch_id),
 update_id TEXT NOT NULL UNIQUE REFERENCES activation_finalization_records(id),
 selection TEXT NOT NULL CHECK(json_valid(selection)) CHECK(length(selection)<=32768),
 CHECK(json_extract(selection,'$.selected.spec.launchId') IS launch_id),
 CHECK(json_extract(selection,'$.selected.spec.updateId') IS update_id),
 CHECK(json_extract(selection,'$.selected.phase')='keeper')
);
CREATE TRIGGER keeper_selection_immutable BEFORE UPDATE ON keeper_selections BEGIN SELECT RAISE(ABORT,'Keeper selection immutable');END;
CREATE TRIGGER keeper_selection_retained BEFORE DELETE ON keeper_selections BEGIN SELECT RAISE(ABORT,'Keeper selection retained');END;
PRAGMA user_version=24;
"#).map_err(storage)
}
fn read(c: &Connection, id: &str) -> Result<Option<KeeperSelection>> {
    let raw: Option<String> = c
        .query_row(
            "SELECT selection FROM keeper_selections WHERE launch_id=?1 OR update_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    raw.map(|s| {
        let v: KeeperSelection = decode(&s)?;
        v.validate()?;
        Ok(v)
    })
    .transpose()
}
/// Avoid recursively loading the original graph through supervisor::read.
/// This is an immutable historical-role check, never physical admission.
pub(super) fn require_selected_role(c: &Connection, r: &SupervisorRecord) -> Result<()> {
    if schema(c)? < 24 {
        return Err(bad("Keeper role requires schema24"));
    }
    let selected =
        read(c, &r.spec.launch_id)?.ok_or_else(|| bad("Keeper role has no immutable selection"))?;
    for (phase, expected) in [
        (SupervisorPhase::Active, &selected.before),
        (SupervisorPhase::Keeper, &selected.selected),
    ] {
        let raw: String = c
            .query_row(
                "SELECT record FROM supervisor_receipts WHERE launch_id=?1 AND phase=?2",
                params![r.spec.launch_id, encode(&phase)?],
                |v| v.get(0),
            )
            .map_err(storage)?;
        let retained: SupervisorRecord = decode(&raw)?;
        if retained != *expected {
            return Err(bad(
                "Keeper selection detached from immutable supervisor receipts",
            ));
        }
    }
    if selected.selected != *r {
        return Err(bad("Keeper role/selection mismatch"));
    }
    selected.require_root(&record(c, &r.spec.update_id)?)?;
    let raw: String = c
        .query_row(
            "SELECT record FROM activation_finalization_records WHERE id=?1",
            [&r.spec.update_id],
            |v| v.get(0),
        )
        .map_err(storage)?;
    let current: ActivationFinalizationRecord = decode(&raw)?;
    if current != selected.finalization {
        return Err(bad("Keeper finalization fact changed"));
    }
    Ok(())
}

#[cfg(any(windows, test))]
#[allow(clippy::too_many_arguments)]
pub(super) fn select(
    c: &Connection,
    a: &UpdateAuthority,
    before: &SupervisorRecord,
    graph: &crate::activation::lifetime::ActivationLifetimeSnapshot,
    shutdown: &crate::shutdown::ShutdownRecord,
    permit: &KeeperSelectionPermit,
    mut clock: impl FnMut() -> Result<u64>,
    mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
) -> Result<KeeperSelection> {
    check_authority(c, a)?;
    if schema(c)? != UPDATE_SCHEMA_VERSION
        || supervisor::read(c, &before.spec.launch_id)? != *before
        || before.phase != SupervisorPhase::Active
        || before.owner_epoch != Some(a.epoch)
        || before.owner_id.as_deref() != Some(a.owner_id.as_str())
    {
        return Err(bad("Keeper selection requires original active coordinator"));
    }
    let (_, finalization) =
        activation_finalization::ledger(c, &before.spec.update_id, graph, shutdown)?;
    if !matches!(
        finalization.state,
        ActivationFinalizationState::Confirmed { .. }
    ) {
        return Err(bad("Keeper selection requires confirmed finalization"));
    }
    let now = clock()?;
    if activation_finalization::completion::confirmation(
        &finalization,
        shutdown,
        &permit.capture,
        now,
    )?
    .is_some()
    {
        return Err(bad("Keeper selection cannot confirm finalization itself"));
    }
    let selection = KeeperSelection::prepare(
        before.clone(),
        finalization,
        permit.capture.query.clone(),
        permit.capture.observation.clone(),
        now,
    )?;
    selection.require_root(&record(c, &before.spec.update_id)?)?;
    let b = &selection.finalization.command.binding;
    if a.installation_id != b.installation_id || a.scope_sha256 != b.scope_sha256 {
        return Err(bad("Keeper installation authority changed"));
    }
    checkpoint().map_err(bad)?;
    check_authority(c, a)?;
    one(c
        .execute(
            "INSERT INTO keeper_selections VALUES(?1,?2,?3)",
            params![
                before.spec.launch_id,
                before.spec.update_id,
                encode(&selection)?
            ],
        )
        .map_err(storage)?)?;
    supervisor::persist(c, before, &selection.selected)?;
    if read(c, &before.spec.launch_id)?.as_ref() != Some(&selection) {
        return Err(bad("Keeper selection readback differs"));
    }
    checkpoint().map_err(bad)?;
    check_authority(c, a)?;
    activation_finalization::completion::confirmation(
        &selection.finalization,
        shutdown,
        &permit.capture,
        clock()?,
    )?;
    Ok(selection)
}

#[cfg(windows)]
impl UpdateJournal {
    pub fn require_known_keeper_selection(
        &self,
        a: &UpdateAuthority,
        known: &KeeperSelectionCommit,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        check_authority(&tx, a)?;
        if read(&tx, &known.selection.selected.spec.launch_id)?.as_ref() != Some(&known.selection)
            || supervisor::read(&tx, &known.selection.selected.spec.launch_id)?
                != known.selection.selected
        {
            return Err(bad(
                "Known keeper selection changed before writer retirement",
            ));
        }
        let b = &known
            .selection
            .finalization
            .command
            .binding
            .lifetime_command
            .binding;
        if a.owner_id() != b.updater_owner_id || a.epoch() != b.updater_epoch {
            return Err(bad("Writer retirement changed original owner"));
        }
        check_authority(&tx, a)?;
        tx.commit().map_err(storage)
    }
    pub fn select_keeper_role(
        &mut self,
        a: &UpdateAuthority,
        before: &SupervisorRecord,
        permit: KeeperSelectionPermit,
        checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<KeeperSelectionAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, a)?;
        if let Some(old) = read(&tx, &before.spec.launch_id)? {
            if old.before != *before || old.query.command != permit.capture.query.command {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Keeper selection already has another original scope",
                ));
            }
            return Ok(KeeperSelectionAdmission::Existing(Box::new(old)));
        }
        if before
            .process
            .as_ref()
            .is_none_or(|p| p.process_id != std::process::id())
        {
            return Err(bad("Keeper selection is original-process only"));
        }
        let actual = winbridge_platform::infrastructure::PinnedProcess::live(
            before.process.as_ref().unwrap(),
        )
        .map_err(storage)?;
        let saved = activation_finalization::snapshot(&tx, &before.spec.update_id)?
            .ok_or_else(|| bad("Keeper finalization missing"))?;
        let selection = select(
            &tx,
            a,
            before,
            &saved.lifetime,
            &saved.shutdown,
            &permit,
            now_ms,
            checkpoint,
        )?;
        actual.require_live().map_err(storage)?;
        tx.commit().map_err(|e|storage(format!("Keeper role commit unconfirmed; retain original custody and authority, no replay: {e}")))?;
        Ok(KeeperSelectionAdmission::Committed(Box::new(
            KeeperSelectionCommit { selection },
        )))
    }
}
