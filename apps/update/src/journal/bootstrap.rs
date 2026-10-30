//! Immutable delegation reservation, atomically paired with supervisor Intent.
//! This ledger does not mint OS custody or permit replay of process creation.
use super::*;
use crate::bootstrap::BootstrapIntent;
#[cfg(any(windows, test))]
use winbridge_protocol::update::SupervisorSpec;
use winbridge_protocol::update::{SupervisorPhase, SupervisorRecord};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapReservation {
    pub schema_version: u16,
    pub intent: BootstrapIntent,
    pub supervisor: SupervisorRecord,
}
/// A fact produced by this writer's successful COMMIT. It is deliberately not
/// Clone/serde and does not itself grant process creation or physical custody.
#[cfg(any(windows, test))]
pub struct BootstrapReservationCommit {
    reservation: BootstrapReservation,
    directory: PathBuf,
}
#[cfg(any(windows, test))]
impl BootstrapReservationCommit {
    pub fn reservation(&self) -> &BootstrapReservation {
        &self.reservation
    }
}
#[cfg(any(windows, test))]
pub enum BootstrapReservationAdmission {
    Committed(Box<BootstrapReservationCommit>),
    Existing(Box<BootstrapReservation>),
}
#[cfg(any(windows, test))]
pub(super) enum ReservationWrite {
    New(BootstrapReservation),
    Existing(BootstrapReservation),
}
fn bad(message: impl Into<String>) -> JournalError {
    fail(JournalErrorCode::Corrupt, message)
}
fn normalized(path: &str) -> String {
    path.strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
impl BootstrapReservation {
    fn validate(&self, parent: &KeeperSelection) -> Result<()> {
        self.intent.validate_against(parent).map_err(bad)?;
        let i = &self.intent;
        let s = &self.supervisor;
        let spec = &s.spec;
        let worker = spec
            .worker
            .as_ref()
            .ok_or_else(|| bad("Delegation lost logical worker"))?;
        let crate::activation::finalization::ActivationFinalizationState::Confirmed { receipt } =
            &parent.finalization.state
        else {
            return Err(bad("Bootstrap parent fact missing"));
        };
        let runtime = normalized(&parent.selected.spec.runtime_directory);
        let image = format!(
            "{runtime}\\update\\{}\\supervisor\\winbridge-update.exe",
            i.update.update_id
        );
        if self.schema_version != 1
            || s.schema_version != 1
            || s.phase != SupervisorPhase::Intent
            || s.revision != 0
            || s.process.is_some()
            || s.owner_id.is_some()
            || s.owner_epoch.is_some()
            || spec.update_id != i.update.update_id
            || spec.launch_id != i.launch_id
            || spec.parent != i.previous.worker
            || spec.parent_owner_id != i.parent_owner_id
            || spec.parent_epoch != i.parent_epoch
            || spec.token_sha256 != i.token_sha256
            || spec.image_sha256 != i.image.sha256
            || spec.image_identity != i.image.file_identity
            || spec.config_sha256 != i.config.sha256
            || spec.excluded_domain.as_deref() != Some(i.excluded_domain.job_name.as_str())
            || spec.accepted_at_ms != i.prepared_at_ms
            || s.updated_at_ms != i.prepared_at_ms
            || normalized(&spec.runtime_directory) != runtime
            || normalized(&spec.config_path) != format!("{runtime}\\winbridge-update.local.json")
            || normalized(&spec.image_path) != image
            || worker.process != i.previous.worker
            || worker.context != i.previous.context
            || worker.generation != i.previous.generation
            || worker.build != receipt.compatibility.identity.descriptor.build
            || worker.config_sha256 != i.update.request.expected.worker_config_sha256
        {
            return Err(bad("Bootstrap reservation changed original scope, fixed helper, logical parent or supervisor intent"));
        }
        Ok(())
    }
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(r#"
CREATE TABLE bootstrap_reservations(
 update_id TEXT PRIMARY KEY REFERENCES updates(id),
 launch_id TEXT NOT NULL UNIQUE REFERENCES supervisor_launches(launch_id),
 previous_launch_id TEXT NOT NULL UNIQUE REFERENCES keeper_selections(launch_id),
 record TEXT NOT NULL CHECK(json_valid(record)) CHECK(length(record)<=32768),
 CHECK(json_extract(record,'$.intent.update.updateId') IS update_id),
 CHECK(json_extract(record,'$.intent.launchId') IS launch_id),
 CHECK(json_extract(record,'$.intent.previous.launchId') IS previous_launch_id)
);
CREATE TRIGGER bootstrap_reservation_immutable BEFORE UPDATE ON bootstrap_reservations BEGIN SELECT RAISE(ABORT,'Bootstrap reservation immutable');END;
CREATE TRIGGER bootstrap_reservation_retained BEFORE DELETE ON bootstrap_reservations BEGIN SELECT RAISE(ABORT,'Bootstrap reservation retained');END;
PRAGMA user_version=25;
"#).map_err(storage)
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<BootstrapReservation>> {
    if schema(c)? < 25 {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT record FROM bootstrap_reservations WHERE update_id=?1 OR launch_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    raw.map(|raw| {
        let r: BootstrapReservation = decode(&raw)?;
        let parent = keeper::read(c, &r.intent.previous.launch_id)?
            .ok_or_else(|| bad("Bootstrap parent selection missing"))?;
        r.validate(&parent)?;
        if supervisor::read(c, &parent.selected.spec.launch_id)? != parent.selected {
            return Err(bad("Bootstrap parent role changed"));
        }
        let current = supervisor::read(c, &r.intent.launch_id)?;
        let saved: String = c
            .query_row(
                "SELECT record FROM supervisor_receipts WHERE launch_id=?1 AND phase=?2",
                params![r.intent.launch_id, encode(&SupervisorPhase::Intent)?],
                |v| v.get(0),
            )
            .map_err(storage)?;
        let saved: SupervisorRecord = decode(&saved)?;
        let root = record(c, &r.intent.update.update_id)?;
        if current.spec != r.supervisor.spec
            || saved != r.supervisor
            || root.request != r.intent.update.request
            || root.accepted_at_ms != r.intent.update.accepted_at_ms
        {
            return Err(bad(
                "Bootstrap detached from immutable supervisor/root request",
            ));
        }
        Ok(r)
    })
    .transpose()
}
/// Only this writer's successful original Spawned COMMIT creates this fact.
/// Historical receipts cannot be converted back into it.
#[cfg(any(windows, test))]
pub(crate) struct BootstrapSpawnedCommit {
    record: SupervisorRecord,
    directory: PathBuf,
}
#[cfg(any(windows, test))]
impl BootstrapSpawnedCommit {
    pub(crate) fn record(&self) -> &SupervisorRecord {
        &self.record
    }
}
/// Data-only transaction primitive. The future live factory owns checkpoints
/// and COMMIT; Existing data can never become another creation permit.
#[cfg(any(windows, test))]
#[allow(dead_code)]
pub(super) fn reserve(
    c: &Connection,
    a: &UpdateAuthority,
    intent: BootstrapIntent,
    spec: SupervisorSpec,
    mut checkpoint: impl FnMut(
        &KeeperSelection,
        Option<&BootstrapReservation>,
    ) -> std::result::Result<(), String>,
) -> Result<ReservationWrite> {
    check_authority(c, a)?;
    if schema(c)? != UPDATE_SCHEMA_VERSION || schema(c)? < 25 {
        return Err(bad("Bootstrap requires current schema25 or later"));
    }
    let parent = keeper::read(c, &intent.previous.launch_id)?
        .ok_or_else(|| bad("Bootstrap keeper missing"))?;
    intent.validate_against(&parent).map_err(bad)?;
    if intent.parent_epoch != a.epoch
        || intent.parent_owner_id != a.owner_id
        || intent.update.request.expected.installation_id != a.installation_id
        || intent.update.request.expected.scope_sha256 != a.scope_sha256
    {
        return Err(fail(
            JournalErrorCode::StaleOwner,
            "Bootstrap changed current owner/scope",
        ));
    }
    let reserved: Option<String> = c
        .query_row(
            "SELECT update_id FROM bootstrap_reservations WHERE previous_launch_id=?1",
            [&intent.previous.launch_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    if let Some(old) = reserved.map(|id| read(c, &id)).transpose()?.flatten() {
        if old.intent != intent || old.supervisor.spec != spec {
            return Err(fail(
                JournalErrorCode::KeyConflict,
                "Keeper already reserved for another bootstrap payload",
            ));
        }
        return Ok(ReservationWrite::Existing(old)); // Data only; never retry creation.
    }
    if record(c, &intent.update.update_id)? != intent.update || supervisor::active(c)? {
        return Err(fail(
            JournalErrorCode::InstallationBusy,
            "Bootstrap root changed or coordinator slot occupied",
        ));
    }
    // If this keeper itself came from delegation, the live factory must prove
    // its actual previous keeper exited; a historical row is not that proof.
    let ancestor = read(c, &intent.previous.launch_id)?;
    checkpoint(&parent, ancestor.as_ref()).map_err(bad)?;
    let supervisor = supervisor::prepare(c, a, spec)?;
    let reservation = BootstrapReservation {
        schema_version: 1,
        intent,
        supervisor,
    };
    reservation.validate(&parent)?;
    one(c
        .execute(
            "INSERT INTO bootstrap_reservations VALUES(?1,?2,?3,?4)",
            params![
                reservation.intent.update.update_id,
                reservation.intent.launch_id,
                reservation.intent.previous.launch_id,
                encode(&reservation)?
            ],
        )
        .map_err(storage)?)?;
    if read(c, &reservation.intent.update.update_id)?.as_ref() != Some(&reservation) {
        return Err(bad("Bootstrap reservation readback differs"));
    }
    checkpoint(&parent, ancestor.as_ref()).map_err(bad)?;
    check_authority(c, a)?;
    Ok(ReservationWrite::New(reservation))
}
#[cfg(any(windows, test))]
fn require_original_record(
    c: &Connection,
    a: &UpdateAuthority,
    known: &BootstrapReservationCommit,
    directory: &Path,
    expected: &SupervisorRecord,
) -> Result<()> {
    check_authority(c, a)?;
    let intent = &known.reservation.intent;
    if known.directory != directory
        || intent.parent_epoch != a.epoch
        || intent.parent_owner_id != a.owner_id
        || read(c, &intent.launch_id)?.as_ref() != Some(&known.reservation)
        || supervisor::read(c, &intent.launch_id)? != *expected
        || record(c, &intent.update.update_id)? != intent.update
    {
        return Err(bad(
            "Known bootstrap COMMIT changed journal, owner, root or original supervisor record",
        ));
    }
    check_authority(c, a)
}
impl UpdateJournal {
    /// Internal writer step for the future consuming live factory. The callback
    /// must retain/check its original OS/file pins; a known SQL commit alone is
    /// never a spawn permit. Any error retains that factory's resources and must
    /// not trigger replay/adoption, including errors after COMMIT was attempted.
    #[cfg(any(windows, test))]
    #[allow(dead_code)]
    pub(crate) fn commit_bootstrap_reservation(
        &mut self,
        a: &UpdateAuthority,
        intent: BootstrapIntent,
        spec: SupervisorSpec,
        checkpoint: impl FnMut(
            &KeeperSelection,
            Option<&BootstrapReservation>,
        ) -> std::result::Result<(), String>,
    ) -> Result<BootstrapReservationAdmission> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let decision = reserve(&tx, a, intent, spec, checkpoint)?;
        tx.commit().map_err(|e| {
            storage(format!(
                "Bootstrap reservation COMMIT unconfirmed; retain original pins, no replay: {e}"
            ))
        })?;
        self.check_directory(a)?;
        check_authority(&self.connection, a)?;
        match decision {
            ReservationWrite::New(reservation) => {
                let known = BootstrapReservationCommit {
                    reservation,
                    directory: self.directory.clone(),
                };
                self.require_known_bootstrap(a, &known)?;
                Ok(BootstrapReservationAdmission::Committed(Box::new(known)))
            }
            ReservationWrite::Existing(reservation) => Ok(BootstrapReservationAdmission::Existing(
                Box::new(reservation),
            )),
        }
    }
    #[cfg(any(windows, test))]
    #[allow(dead_code)]
    pub(crate) fn require_known_bootstrap(
        &self,
        a: &UpdateAuthority,
        known: &BootstrapReservationCommit,
    ) -> Result<()> {
        self.require_bootstrap_record(a, known, &known.reservation.supervisor)
    }
    #[cfg(any(windows, test))]
    pub(crate) fn require_bootstrap_spawned(
        &self,
        a: &UpdateAuthority,
        known: &BootstrapReservationCommit,
        spawned: &BootstrapSpawnedCommit,
    ) -> Result<()> {
        if spawned.directory != self.directory {
            return Err(bad("Original Spawned COMMIT changed journal directory"));
        }
        self.require_bootstrap_record(a, known, spawned.record())
    }
    /// Single original write only. Existing receipts are data, never this fact.
    #[cfg(any(windows, test))]
    pub(crate) fn commit_bootstrap_spawned(
        &mut self,
        a: &UpdateAuthority,
        known: &BootstrapReservationCommit,
        process: &winbridge_protocol::update::SupervisorProcess,
        mut checkpoint: impl FnMut() -> std::result::Result<(), String>,
    ) -> Result<Box<BootstrapSpawnedCommit>> {
        self.check_directory(a)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        require_original_record(
            &tx,
            a,
            known,
            &self.directory,
            &known.reservation.supervisor,
        )?;
        checkpoint().map_err(bad)?;
        let old = &known.reservation.supervisor;
        let mut record = old.clone();
        record.phase = SupervisorPhase::Spawned;
        record.revision = 1;
        record.process = Some(process.clone());
        record.updated_at_ms = now_ms()?.max(record.updated_at_ms);
        record.reason="Исходный keeper подтвердил один независимый процесс; ожидается Ready, effects запрещены".into();
        supervisor::persist(&tx, old, &record)?;
        checkpoint().map_err(bad)?;
        check_authority(&tx, a)?;
        tx.commit().map_err(|e|storage(format!("Original bootstrap Spawned COMMIT unconfirmed; retain owner/process, no replay: {e}")))?;
        let fact = Box::new(BootstrapSpawnedCommit {
            record,
            directory: self.directory.clone(),
        });
        self.require_bootstrap_spawned(a, known, &fact)?;
        Ok(fact)
    }
    #[cfg(any(windows, test))]
    fn require_bootstrap_record(
        &self,
        a: &UpdateAuthority,
        known: &BootstrapReservationCommit,
        expected: &SupervisorRecord,
    ) -> Result<()> {
        self.check_directory(a)?;
        let tx = self.connection.unchecked_transaction().map_err(storage)?;
        require_original_record(&tx, a, known, &self.directory, expected)?;
        tx.commit().map_err(storage)
    }
    /// Readonly historical data. No reservation or live permission is minted.
    pub fn inspect_bootstrap(directory: &Path, id: &str) -> Result<Option<BootstrapReservation>> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        let tx = c.transaction().map_err(storage)?;
        let r = read(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(r)
    }
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn matching_copied_headers_do_not_bind_a_writer_to_the_original_runtime() {
        let root = tempfile::tempdir().unwrap();
        let scope = "a".repeat(64);
        let mut original = UpdateJournal::open(&root.path().join("original"), &scope).unwrap();
        let copy = root.path().join("copy");
        std::fs::create_dir(&copy).unwrap();
        {
            let mut destination = Connection::open(copy.join("updates.sqlite")).unwrap();
            rusqlite::backup::Backup::new(&original.connection, &mut destination)
                .unwrap()
                .run_to_completion(64, std::time::Duration::from_millis(1), None)
                .unwrap();
        }
        let mut copied = UpdateJournal::open(&copy, &scope).unwrap();
        assert_eq!(original.info().unwrap(), copied.info().unwrap());
        assert!(copied.require_journal_directory(&copy).is_ok());
        assert!(original
            .require_journal_directory(&original.directory)
            .is_ok());
        assert_eq!(
            copied
                .require_journal_directory(&original.directory)
                .unwrap_err()
                .code,
            JournalErrorCode::PreconditionsChanged
        );
    }
}
