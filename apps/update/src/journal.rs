//! The updater owns this database; opening it never opens/migrates the worker journal.
//! A file lock protects physical ownership, while a persistent epoch fences DB callbacks.
//! This module records lifecycle decisions. It does not authorize filesystem effects.
use fs2::FileExt;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use winbridge_core::update::{
    advance_update, update_blocks_installation, valid_hash, validate_update_key,
    validate_update_request, MAX_UPDATE_COUNTER,
};
use winbridge_protocol::update::{
    UpdateCommand, UpdateInstallationPin, UpdatePhase, UpdateRecord, UpdateRequest,
    UpdateTransition,
};

mod abort;
mod activation;
mod activation_controller;
mod activation_finalization;
mod activation_lifetime;
pub use activation_finalization::{
    ActivationFinalizationIntent, FinalizationConfirmationPermit, FinalizationDecisionAdmission,
    FinalizationDecisionPermit, FinalizationReleasePermit, RetainedFinalizationSnapshot,
};
mod activation_local;
mod activation_service;
mod activation_writer;
mod build;
mod build_report;
mod candidate;
mod candidate_support;
mod close;
pub use close::JournalCloseFailure;
mod compatible_drain;
mod drain;
mod health;
mod runtime_baseline;
mod runtime_preimages;
mod runtime_preparation;
mod runtime_replacement;
#[cfg(windows)]
pub(crate) use runtime_replacement::AdmissionPins;
mod shutdown;
mod source;
mod state_backup;
mod supervisor;
pub const UPDATE_SCHEMA_VERSION: u32 = 23;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Driver {
    Lifecycle,
    Source,
    Abort,
    Candidate,
    Baseline,
    Preimages,
    Health,
    Drain,
    Shutdown,
}
const APPLICATION_ID: u32 = 0x5742_5550; // WBUP; never accept a worker DB as an update DB.
const MAX_RECORD_BYTES: usize = 32 * 1024;
type Result<T> = std::result::Result<T, JournalError>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalErrorCode {
    InvalidRequest,
    NotFound,
    KeyConflict,
    RevisionConflict,
    InstallationBusy,
    PreconditionsChanged,
    OwnerBusy,
    StaleOwner,
    ScopeMismatch,
    UnsupportedSchema,
    Corrupt,
    Storage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalError {
    pub code: JournalErrorCode,
    pub message: String,
}
impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for JournalError {}
fn fail(code: JournalErrorCode, message: impl Into<String>) -> JournalError {
    JournalError {
        code,
        message: message.into(),
    }
}
fn storage(e: impl fmt::Display) -> JournalError {
    fail(JournalErrorCode::Storage, format!("Update journal: {e}"))
}
fn one(changed: usize) -> Result<()> {
    if changed != 1 {
        return Err(storage(
            "Required update write was not applied; no ACK is allowed",
        ));
    }
    Ok(())
}
fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(storage)
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    if value.len() > MAX_RECORD_BYTES {
        return Err(fail(JournalErrorCode::Corrupt, "Oversized journal record"));
    }
    serde_json::from_str(value)
        .map_err(|_| fail(JournalErrorCode::Corrupt, "Invalid typed journal record"))
}
pub(crate) fn now_ms() -> Result<u64> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(storage)?
        .as_millis();
    if value > u128::from(MAX_UPDATE_COUNTER) {
        return Err(storage("Clock is outside the supported range"));
    }
    Ok(value as u64)
}
fn counter(value: i64) -> Result<u64> {
    u64::try_from(value)
        .ok()
        .filter(|n| *n <= MAX_UPDATE_COUNTER)
        .ok_or_else(|| {
            fail(
                JournalErrorCode::Corrupt,
                "Journal counter is outside the supported range",
            )
        })
}
fn safe(path: &Path) -> Result<()> {
    safe_with(path, winbridge_platform::require_safe_path)
}
fn safe_with(path: &Path, mut validate: impl FnMut(&Path) -> std::io::Result<()>) -> Result<()> {
    // Optional SQLite WAL/SHM can disappear or be delete-pending on Windows.
    // Retry only complete readonly validation, never the DB/file effect. A
    // persistent denial or an unsafe replacement still fails closed.
    let sidecar = matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("updates.sqlite-wal" | "updates.sqlite-shm")
    );
    for attempt in 0..3 {
        match validate(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempt == 0 => {}
            Err(e)
                if sidecar && e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                return result.map_err(|e| storage(format!("Update path {}: {e}", path.display())))
            }
        }
    }
    unreachable!("Final path validation always returns its result")
}
fn check_files(directory: &Path) -> Result<()> {
    safe(directory)?;
    winbridge_platform::require_local_disk(directory).map_err(storage)?;
    for name in [
        "updates.sqlite",
        "updates.sqlite-wal",
        "updates.sqlite-shm",
        "updates.sqlite-journal",
        "owner.lock",
    ] {
        safe(&directory.join(name))?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateJournalInfo {
    pub schema_version: u32,
    pub installation_id: String,
    pub scope_sha256: String,
    pub owner_epoch: u64,
    pub active_update_id: Option<String>,
    pub last_sequence: u64,
}

/// Cannot be constructed from an agent-supplied token. Dropping releases the OS lock.
/// Every mutation also checks the durable installation, epoch and owner ID.
pub struct UpdateAuthority {
    lock: File,
    directory: PathBuf,
    installation_id: String,
    scope_sha256: String,
    epoch: u64,
    owner_id: String,
}
impl UpdateAuthority {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }
}

pub struct UpdateJournal {
    connection: Connection,
    directory: PathBuf,
}

fn schema(connection: &Connection) -> Result<u32> {
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(storage)?;
    let app: u32 = connection
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(storage)?;
    if version == 0 && app == 0 {
        let tables: u32 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if tables == 0 {
            return Ok(0);
        }
    }
    if !(1..=UPDATE_SCHEMA_VERSION).contains(&version) || app != APPLICATION_ID {
        return Err(fail(JournalErrorCode::UnsupportedSchema,
            format!("Unsupported update journal (schema {version}); preserve it and use a compatible updater")));
    }
    Ok(version)
}
fn connection(path: &Path, readonly: bool) -> Result<Connection> {
    let flags = if readonly {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let connection = Connection::open_with_flags(path, flags).map_err(storage)?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(storage)?;
    Ok(connection)
}
fn installation_info(connection: &Connection) -> Result<UpdateJournalInfo> {
    let (installation_id, scope_sha256, owner_epoch): (String, String, i64) = connection
        .query_row(
            "SELECT installation,scope,epoch FROM installation WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(storage)?;
    let owner_epoch = counter(owner_epoch)?;
    if Uuid::parse_str(&installation_id).is_err()
        || !valid_hash(&scope_sha256, &[64])
        || owner_epoch > MAX_UPDATE_COUNTER
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Invalid installation identity",
        ));
    }
    let active_update_id: Option<String> = connection
        .query_row("SELECT id FROM updates WHERE active=1", [], |r| r.get(0))
        .optional()
        .map_err(storage)?;
    let last_sequence: i64 = connection
        .query_row(
            "SELECT coalesce(max(sequence),0) FROM transitions",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let last_sequence = counter(last_sequence)?;
    if last_sequence > MAX_UPDATE_COUNTER {
        return Err(fail(JournalErrorCode::Corrupt, "Sequence overflow"));
    }
    Ok(UpdateJournalInfo {
        schema_version: schema(connection)?,
        installation_id,
        scope_sha256,
        owner_epoch,
        active_update_id,
        last_sequence,
    })
}
// Historical graph readers use installation_info to avoid recursively reading
// their own finalization descendants. Public readiness/admission uses this view.
fn info(connection: &Connection) -> Result<UpdateJournalInfo> {
    let mut value = installation_info(connection)?;
    if let Some(pending) = activation_finalization::blocking(connection)? {
        if value
            .active_update_id
            .as_ref()
            .is_some_and(|id| id != &pending)
        {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Active update overlaps an unresolved finalization",
            ));
        }
        value.active_update_id = Some(pending);
    }
    Ok(value)
}
fn record(connection: &Connection, id: &str) -> Result<UpdateRecord> {
    let row: Option<(String, i64, String, bool, String, String, String)> = connection
        .query_row(
            "SELECT record,revision,phase,active,key,payload,payload_sha FROM updates WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(storage)?;
    let (json, revision, phase, active, key, payload, hash) =
        row.ok_or_else(|| fail(JournalErrorCode::NotFound, "Update ID not found"))?;
    let revision = counter(revision)?;
    let record: UpdateRecord = decode(&json)?;
    if record.update_id != id
        || Uuid::parse_str(id).is_err()
        || record.revision != revision
        || revision > MAX_UPDATE_COUNTER
        || encode(&record.phase)? != phase
        || active != update_blocks_installation(record.phase)
        || record.request.idempotency_key != key
        || encode(&record.request)? != payload
        || crate::sha256(payload.as_bytes()) != hash
        || validate_update_request(&record.request).is_err()
        || record.accepted_at_ms > MAX_UPDATE_COUNTER
        || record.updated_at_ms > MAX_UPDATE_COUNTER
        || record.updated_at_ms < record.accepted_at_ms
        || record.reason.as_ref().is_some_and(|r| r.len() > 4096)
    {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Update record/index identity mismatch",
        ));
    }
    let (count, last): (i64, Option<String>) = connection.query_row(
        "SELECT count(*),(SELECT record FROM transitions WHERE update_id=?1 ORDER BY revision DESC LIMIT 1) FROM transitions WHERE update_id=?1",
        [id], |r| Ok((r.get(0)?,r.get(1)?)),
    ).map_err(storage)?;
    if counter(count)? != revision + 1 || last.as_deref() != Some(json.as_str()) {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Update history/record mismatch",
        ));
    }
    Ok(record)
}
fn check_authority(connection: &Connection, authority: &UpdateAuthority) -> Result<()> {
    if schema(connection)? != UPDATE_SCHEMA_VERSION {
        return Err(fail(
            JournalErrorCode::UnsupportedSchema,
            "Updater schema changed; stop effects",
        ));
    }
    let current: (String, String, i64, String) = connection
        .query_row(
            "SELECT installation,scope,epoch,owner FROM installation WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(storage)?;
    if current
        != (
            authority.installation_id.clone(),
            authority.scope_sha256.clone(),
            authority.epoch as i64,
            authority.owner_id.clone(),
        )
    {
        return Err(fail(
            JournalErrorCode::StaleOwner,
            "Updater authority changed; stop effects and reconcile",
        ));
    }
    Ok(())
}

fn owner_lock(directory: &Path) -> Result<File> {
    check_files(directory)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let lock = options
        .open(directory.join("owner.lock"))
        .map_err(storage)?;
    lock.try_lock_exclusive().map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock
            || e.raw_os_error()
                .is_some_and(|n| Some(n) == fs2::lock_contended_error().raw_os_error())
        {
            fail(
                JournalErrorCode::OwnerBusy,
                "Another updater owns this installation",
            )
        } else {
            storage(e)
        }
    })?;
    Ok(lock)
}

impl UpdateJournal {
    /// Initializes only a blank, local, updater-owned DB, never an existing unknown DB.
    pub fn open(directory: &Path, scope_sha256: &str) -> Result<Self> {
        if !valid_hash(scope_sha256, &[64]) {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Invalid installation scope SHA-256",
            ));
        }
        safe(directory)?;
        winbridge_platform::require_local_disk(directory).map_err(storage)?;
        fs::create_dir_all(directory).map_err(storage)?;
        check_files(directory)?;
        let directory = directory.canonicalize().map_err(storage)?;
        let path = directory.join("updates.sqlite");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file.sync_all().map_err(storage)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(storage(e)),
        }
        // Refuse newer/foreign databases before WAL/synchronous/migration writes.
        let inspector = connection(&path, true)?;
        let initial_schema = schema(&inspector)?;
        drop(inspector);
        let migration_lock = if (1..UPDATE_SCHEMA_VERSION).contains(&initial_schema) {
            Some(owner_lock(&directory)?)
        } else {
            None
        };
        let mut connection = connection(&path, false)?;
        if (1..UPDATE_SCHEMA_VERSION).contains(&initial_schema)
            && schema(&connection)? == initial_schema
        {
            let before = info(&connection)?;
            if before.scope_sha256 != scope_sha256 {
                return Err(fail(
                    JournalErrorCode::ScopeMismatch,
                    "Migration belongs to another installation",
                ));
            }
            let backup = directory.join(format!(
                "updates-v{}-{}-{}.sqlite",
                initial_schema,
                now_ms()?,
                Uuid::new_v4()
            ));
            connection
                .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
                .map_err(storage)?;
            let c = super::journal::connection(&backup, true)?;
            let integrity: String = c
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .map_err(storage)?;
            if schema(&c)? != initial_schema || info(&c)? != before || integrity != "ok" {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Migration backup verification failed",
                ));
            }
            drop(c);
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&backup)
                .map_err(storage)?
                .sync_all()
                .map_err(storage)?;
            winbridge_platform::source_files::sync_parent(&backup).map_err(storage)?;
        }
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(storage)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if schema(&tx)? == 0 {
            tx.execute_batch("CREATE TABLE installation(singleton INTEGER PRIMARY KEY CHECK(singleton=1),installation TEXT NOT NULL,scope TEXT NOT NULL,epoch INTEGER NOT NULL CHECK(epoch>=0),owner TEXT NOT NULL);
                CREATE TABLE updates(id TEXT PRIMARY KEY,key TEXT NOT NULL UNIQUE,payload TEXT NOT NULL,payload_sha TEXT NOT NULL,phase TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>=0),active INTEGER NOT NULL CHECK(active IN(0,1)),record TEXT NOT NULL);
                CREATE UNIQUE INDEX single_active_update ON updates(active) WHERE active=1;
                CREATE TABLE transitions(sequence INTEGER PRIMARY KEY AUTOINCREMENT,update_id TEXT NOT NULL REFERENCES updates(id),revision INTEGER NOT NULL,command_key TEXT,payload TEXT,transition TEXT NOT NULL,record TEXT NOT NULL,UNIQUE(update_id,revision),UNIQUE(update_id,command_key));
                PRAGMA user_version=1; PRAGMA application_id=1463965008;").map_err(storage)?;
            tx.execute(
                "INSERT INTO installation VALUES(1,?1,?2,0,'')",
                params![Uuid::new_v4().to_string(), scope_sha256],
            )
            .map_err(storage)?;
            source::migrate(&tx)?;
            supervisor::migrate(&tx)?;
            build::migrate(&tx)?;
            abort::migrate(&tx)?;
            build_report::migrate(&tx)?;
            candidate::migrate(&tx)?;
            runtime_baseline::migrate(&tx)?;
            runtime_preimages::migrate(&tx)?;
            health::migrate(&tx)?;
            drain::migrate(&tx)?;
            shutdown::migrate(&tx)?;
            state_backup::migrate(&tx)?;
            runtime_preparation::migrate(&tx)?;
            runtime_replacement::migrate(&tx)?;
            candidate_support::migrate(&tx)?;
            compatible_drain::migrate(&tx)?;
            activation::migrate(&tx)?;
            activation_writer::migrate(&tx)?;
            activation_service::migrate(&tx)?;
            activation_controller::migrate(&tx)?;
            activation_lifetime::migrate(&tx)?;
            activation_finalization::migrate(&tx)?;
        } else if (1..UPDATE_SCHEMA_VERSION).contains(&schema(&tx)?) {
            if migration_lock.is_none() {
                return Err(fail(
                    JournalErrorCode::OwnerBusy,
                    "Journal initialized concurrently; reopen before migration",
                ));
            }
            let previous = info(&tx)?;
            let epoch = previous
                .owner_epoch
                .checked_add(1)
                .filter(|n| *n <= MAX_UPDATE_COUNTER)
                .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Migration epoch exhausted"))?;
            let owner = Uuid::new_v4().to_string();
            one(tx
                .execute(
                    "UPDATE installation SET epoch=?1,owner=?2 WHERE singleton=1",
                    params![epoch as i64, owner],
                )
                .map_err(storage)?)?;
            if schema(&tx)? == 1 {
                source::migrate(&tx)?;
            }
            if schema(&tx)? == 2 {
                supervisor::migrate(&tx)?;
            }
            if schema(&tx)? == 3 {
                build::migrate(&tx)?;
            }
            if schema(&tx)? == 4 {
                abort::migrate(&tx)?;
            }
            if schema(&tx)? == 5 {
                build_report::migrate(&tx)?;
            }
            if schema(&tx)? == 6 {
                candidate::migrate(&tx)?;
            }
            if schema(&tx)? == 7 {
                runtime_baseline::migrate(&tx)?;
            }
            if schema(&tx)? == 8 {
                runtime_preimages::migrate(&tx)?;
            }
            if schema(&tx)? == 9 {
                health::migrate(&tx)?;
            }
            if schema(&tx)? == 10 {
                drain::migrate(&tx)?;
            }
            if schema(&tx)? == 11 {
                shutdown::migrate(&tx)?;
            }
            if schema(&tx)? == 12 {
                state_backup::migrate(&tx)?;
            }
            if schema(&tx)? == 13 {
                runtime_preparation::migrate(&tx)?;
            }
            if schema(&tx)? == 14 {
                runtime_replacement::migrate(&tx)?;
            }
            if schema(&tx)? == 15 {
                candidate_support::migrate(&tx)?;
            }
            if schema(&tx)? == 16 {
                compatible_drain::migrate(&tx)?;
            }
            if schema(&tx)? == 17 {
                activation::migrate(&tx)?;
            }
            if schema(&tx)? == 18 {
                activation_writer::migrate(&tx)?;
            }
            if schema(&tx)? == 19 {
                activation_service::migrate(&tx)?;
            }
            if schema(&tx)? == 20 {
                activation_controller::migrate(&tx)?;
            }
            if schema(&tx)? == 21 {
                activation_lifetime::migrate(&tx)?;
            }
            if schema(&tx)? == 22 {
                activation_finalization::migrate(&tx)?;
            }
            if info(&tx)?.owner_epoch != epoch {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Migration epoch readback failed",
                ));
            }
        }
        if info(&tx)?.scope_sha256 != scope_sha256 {
            return Err(fail(
                JournalErrorCode::ScopeMismatch,
                "Journal belongs to different installation paths; preserve it",
            ));
        }
        tx.commit().map_err(storage)?;
        connection
            .execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(storage)?;
        Ok(Self {
            connection,
            directory,
        })
    }

    /// No create/migration/epoch changes; useful while another updater owns the installation.
    pub fn inspect(directory: &Path) -> Result<UpdateJournalInfo> {
        Self::read_snapshot(directory, None).map(|(info, _)| info)
    }
    pub fn read_snapshot(
        directory: &Path,
        id: Option<&str>,
    ) -> Result<(UpdateJournalInfo, Option<UpdateRecord>)> {
        check_files(directory)?;
        let mut connection = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&connection)? == 0 {
            return Err(fail(
                JournalErrorCode::UnsupportedSchema,
                "Update journal is not initialized",
            ));
        }
        let tx = connection.transaction().map_err(storage)?;
        let info = info(&tx)?;
        let target = id.or(info.active_update_id.as_deref());
        let record = target.map(|id| record(&tx, id)).transpose()?;
        tx.commit().map_err(storage)?;
        Ok((info, record))
    }
    pub fn info(&mut self) -> Result<UpdateJournalInfo> {
        let tx = self.connection.transaction().map_err(storage)?;
        let result = info(&tx)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    pub fn get(&mut self, id: &str) -> Result<UpdateRecord> {
        let tx = self.connection.transaction().map_err(storage)?;
        let result = record(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    /// Duplicate-first observation remains available while the supervisor owns the lock.
    pub fn find_request(directory: &Path, request: &UpdateRequest) -> Result<Option<UpdateRecord>> {
        validate_update_key(&request.idempotency_key)
            .map_err(|e| fail(JournalErrorCode::InvalidRequest, e))?;
        let payload = encode(request)?;
        if payload.len() > MAX_RECORD_BYTES / 2 {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Oversized update request",
            ));
        }
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        schema(&c)?;
        let tx = c.transaction().map_err(storage)?;
        let found: Option<(String, String)> = tx
            .query_row(
                "SELECT id,payload FROM updates WHERE key=?1",
                [&request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        let result = match found {
            None => None,
            Some((id, previous)) => {
                if payload != previous {
                    return Err(fail(
                        JournalErrorCode::KeyConflict,
                        "Update key was already used with a different payload",
                    ));
                }
                Some(record(&tx, &id)?)
            }
        };
        tx.commit().map_err(storage)?;
        Ok(result)
    }

    fn check_directory(&self, authority: &UpdateAuthority) -> Result<()> {
        if authority.directory != self.directory {
            return Err(fail(
                JournalErrorCode::StaleOwner,
                "Authority belongs to another journal directory",
            ));
        }
        let path = self.directory.join("owner.lock");
        safe(&path)?;
        let current = File::open(path).map_err(storage)?;
        if winbridge_platform::file_identity(&current).map_err(storage)?
            != winbridge_platform::file_identity(&authority.lock).map_err(storage)?
        {
            return Err(fail(
                JournalErrorCode::StaleOwner,
                "Owner lock file was replaced; stop and reconcile",
            ));
        }
        Ok(())
    }

    pub fn acquire_owner(&mut self) -> Result<UpdateAuthority> {
        check_files(&self.directory)?;
        let lock = owner_lock(&self.directory)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let info = info(&tx)?;
        let epoch = info
            .owner_epoch
            .checked_add(1)
            .filter(|n| *n <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Owner epoch exhausted"))?;
        let owner_id = Uuid::new_v4().to_string();
        one(tx
            .execute(
                "UPDATE installation SET epoch=?1,owner=?2 WHERE singleton=1",
                params![epoch as i64, owner_id],
            )
            .map_err(storage)?)?;
        let authority = UpdateAuthority {
            lock,
            directory: self.directory.clone(),
            installation_id: info.installation_id,
            scope_sha256: info.scope_sha256,
            epoch,
            owner_id,
        };
        check_authority(&tx, &authority)?;
        tx.commit().map_err(storage)?;
        Ok(authority)
    }

    /// Duplicate lookup is before current preconditions/admission, but after fencing.
    /// `observed` must come from the owning worker adapter; it is not caller evidence.
    pub fn accept(
        &mut self,
        authority: &UpdateAuthority,
        request: &UpdateRequest,
        observed: &UpdateInstallationPin,
    ) -> Result<UpdateRecord> {
        self.check_directory(authority)?;
        let payload = encode(request)?;
        if payload.len() > MAX_RECORD_BYTES / 2 {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Oversized update request",
            ));
        }
        validate_update_key(&request.idempotency_key)
            .map_err(|e| fail(JournalErrorCode::InvalidRequest, e))?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, authority)?;
        let duplicate: Option<(String, String)> = tx
            .query_row(
                "SELECT id,payload FROM updates WHERE key=?1",
                [&request.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        if let Some((id, previous)) = duplicate {
            if payload != previous {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Update key was already used with a different payload",
                ));
            }
            return record(&tx, &id);
        }
        if supervisor::active(&tx)? {
            return Err(fail(JournalErrorCode::InstallationBusy,"A supervisor launch still owns this installation; inspect it, never spawn a duplicate"));
        }
        validate_update_request(request).map_err(|e| fail(JournalErrorCode::InvalidRequest, e))?;
        let installed = info(&tx)?;
        if request.expected.installation_id != installed.installation_id
            || request.expected.scope_sha256 != installed.scope_sha256
        {
            return Err(fail(
                JournalErrorCode::ScopeMismatch,
                "Request is for a different installation",
            ));
        }
        if &request.expected != observed {
            return Err(fail(
                JournalErrorCode::PreconditionsChanged,
                "Worker build/config/source baseline changed; inspect before making a new request",
            ));
        }
        if installed.active_update_id.is_some() {
            return Err(fail(
                JournalErrorCode::InstallationBusy,
                "Another update or manual recovery blocks this installation",
            ));
        }
        let now = now_ms()?;
        let record = UpdateRecord {
            update_id: Uuid::new_v4().to_string(),
            revision: 0,
            phase: UpdatePhase::Requested,
            request: request.clone(),
            accepted_at_ms: now,
            updated_at_ms: now,
            reason: None,
        };
        let json = encode(&record)?;
        one(tx
            .execute(
                "INSERT INTO updates VALUES(?1,?2,?3,?4,?5,0,1,?6)",
                params![
                    record.update_id,
                    request.idempotency_key,
                    payload,
                    crate::sha256(payload.as_bytes()),
                    encode(&record.phase)?,
                    json
                ],
            )
            .map_err(storage)?)?;
        let transition = UpdateTransition {
            sequence: 0,
            update_id: record.update_id.clone(),
            revision: 0,
            previous_phase: None,
            phase: record.phase,
            occurred_at_ms: now,
            owner_epoch: authority.epoch,
            owner_id: authority.owner_id.clone(),
            command: None,
        };
        append_transition(&tx, &transition, None, &json)?;
        if crate::journal::record(&tx, &record.update_id)? != record {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Accepted update readback mismatch",
            ));
        }
        tx.commit().map_err(storage)?; // ACK is only allowed after this commit.
        Ok(record)
    }

    /// Idempotent receipt returns its original snapshot, even after subsequent transitions.
    /// This records a trusted driver decision; it is deliberately absent from CLI/RPC.
    pub fn advance(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        command: &UpdateCommand,
    ) -> Result<UpdateRecord> {
        self.advance_with_source(authority, id, command, false, |_, _| Ok(()))
    }
    fn advance_with_source(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        command: &UpdateCommand,
        source_driver: bool,
        write_source: impl FnOnce(&Connection, &UpdateRecord) -> Result<()>,
    ) -> Result<UpdateRecord> {
        self.advance_with_driver(
            authority,
            id,
            command,
            if source_driver {
                Driver::Source
            } else {
                Driver::Lifecycle
            },
            write_source,
        )
    }
    fn advance_with_driver(
        &mut self,
        authority: &UpdateAuthority,
        id: &str,
        command: &UpdateCommand,
        driver: Driver,
        write_source: impl FnOnce(&Connection, &UpdateRecord) -> Result<()>,
    ) -> Result<UpdateRecord> {
        self.check_directory(authority)?;
        validate_update_key(&command.idempotency_key)
            .map_err(|e| fail(JournalErrorCode::InvalidRequest, e))?;
        let payload = encode(command)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        check_authority(&tx, authority)?;
        let duplicate: Option<(String, String)> = tx
            .query_row(
                "SELECT payload,record FROM transitions WHERE update_id=?1 AND command_key=?2",
                params![id, command.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        if let Some((previous, json)) = duplicate {
            if payload != previous {
                return Err(fail(
                    JournalErrorCode::KeyConflict,
                    "Lifecycle command key was used with a different payload",
                ));
            }
            let snapshot: UpdateRecord = decode(&json)?;
            if snapshot.update_id != id
                || snapshot.revision != command.expected_revision.saturating_add(1)
            {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Lifecycle receipt identity mismatch",
                ));
            }
            // Also ensure the current aggregate/history is still readable.
            let current = record(&tx, id)?;
            if snapshot.request != current.request
                || snapshot.accepted_at_ms != current.accepted_at_ms
                || snapshot.revision > current.revision
            {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Lifecycle receipt request mismatch",
                ));
            }
            return Ok(snapshot);
        }
        if command.reason.is_empty()
            || command.reason.len() > 4096
            || command
                .reason
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Lifecycle reason must contain 1–4096 bytes of readable text",
            ));
        }
        supervisor::require_driver(&tx, authority, id, command.event)?;
        if command.event == winbridge_protocol::update::UpdateEvent::BeginShutdown
            && driver != Driver::Shutdown
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Shutdown requires its trusted driver and durable intent",
            ));
        }
        if command.event == winbridge_protocol::update::UpdateEvent::ShutdownVerified
            && driver != Driver::Shutdown
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Exact process exit requires the shutdown driver and its held OS guard",
            ));
        }
        if matches!(
            command.event,
            winbridge_protocol::update::UpdateEvent::FilesVerified
                | winbridge_protocol::update::UpdateEvent::LocalHealthVerified
                | winbridge_protocol::update::UpdateEvent::ControllerConfirmed
        ) {
            return Err(fail(JournalErrorCode::InvalidRequest,
                "Raw lifecycle cannot verify runtime files or activation; trusted drivers and physical receipts are required"));
        }
        if matches!(
            command.event,
            winbridge_protocol::update::UpdateEvent::BeginDrain
                | winbridge_protocol::update::UpdateEvent::Quiesce { .. }
        ) && driver != Driver::Drain
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Drain/quiescence require their trusted driver and durable worker receipts",
            ));
        }
        if matches!(
            command.event,
            winbridge_protocol::update::UpdateEvent::RuntimeBaselineAdmitted
                | winbridge_protocol::update::UpdateEvent::RuntimeBaselineVerified
        ) && driver != Driver::Baseline
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Runtime baseline requires its trusted driver",
            ));
        }
        if command.event == winbridge_protocol::update::UpdateEvent::BeginPreparation {
            runtime_baseline::before_source(&tx, id)?;
            runtime_preimages::before_source(&tx, id)?;
        }
        if matches!(
            command.event,
            winbridge_protocol::update::UpdateEvent::RuntimePreimagesAdmitted
                | winbridge_protocol::update::UpdateEvent::RuntimePreimagesVerified
        ) && driver != Driver::Preimages
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Runtime preimages require their trusted driver",
            ));
        }
        if matches!(
            command.event,
            winbridge_protocol::update::UpdateEvent::CandidateHealthAdmitted
                | winbridge_protocol::update::UpdateEvent::CandidateAccepted
        ) && driver != Driver::Health
        {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Candidate health requires its trusted driver and fresh receipts",
            ));
        }
        if driver != Driver::Abort
            && !(driver == Driver::Candidate
                && command.event == winbridge_protocol::update::UpdateEvent::BuildSucceeded)
            && !(driver == Driver::Health
                && command.event == winbridge_protocol::update::UpdateEvent::CandidateAccepted)
        {
            build::require_driver(&tx, id, command.event)?;
        }
        abort::require_driver(&tx, id, command.event, driver)?;
        let mut current = record(&tx, id)?;
        if driver == Driver::Lifecycle
            && matches!(
                command.event,
                winbridge_protocol::update::UpdateEvent::BeginSourceApply
                    | winbridge_protocol::update::UpdateEvent::SourceVerified
                    | winbridge_protocol::update::UpdateEvent::Fail
            )
            && source::unfinished(&tx, id)?
        {
            return Err(fail(JournalErrorCode::InvalidRequest,"Source transaction requires coordinated verification/abort; raw lifecycle event cannot bypass file receipts"));
        }
        if current.revision != command.expected_revision {
            return Err(fail(
                JournalErrorCode::RevisionConflict,
                "Update revision changed; reload the current state",
            ));
        }
        let next = advance_update(current.phase, command.event)
            .map_err(|e| fail(JournalErrorCode::InvalidRequest, e))?;
        let previous_phase = current.phase;
        current.revision = current
            .revision
            .checked_add(1)
            .filter(|n| *n <= MAX_UPDATE_COUNTER)
            .ok_or_else(|| fail(JournalErrorCode::Corrupt, "Update revision exhausted"))?;
        current.phase = next;
        current.updated_at_ms = now_ms()?.max(current.updated_at_ms);
        current.reason = Some(command.reason.clone());
        let json = encode(&current)?;
        one(tx.execute("UPDATE updates SET record=?1,revision=?2,phase=?3,active=?4 WHERE id=?5 AND revision=?6",params![json,current.revision as i64,encode(&next)?,update_blocks_installation(next),id,command.expected_revision as i64]).map_err(storage)?)?;
        let transition = UpdateTransition {
            sequence: 0,
            update_id: id.into(),
            revision: current.revision,
            previous_phase: Some(previous_phase),
            phase: next,
            occurred_at_ms: current.updated_at_ms,
            owner_epoch: authority.epoch,
            owner_id: authority.owner_id.clone(),
            command: Some(command.clone()),
        };
        append_transition(&tx, &transition, Some(&payload), &json)?;
        if record(&tx, id)? != current {
            return Err(fail(
                JournalErrorCode::Corrupt,
                "Lifecycle transition readback mismatch",
            ));
        }
        write_source(&tx, &current)?;
        tx.commit().map_err(storage)?;
        Ok(current)
    }

    /// Bounded installation-wide cursor; records and sequence come from one read transaction.
    pub fn transitions(&mut self, after: u64, limit: u16) -> Result<Vec<UpdateTransition>> {
        if after > MAX_UPDATE_COUNTER || !(1..=100).contains(&limit) {
            return Err(fail(
                JournalErrorCode::InvalidRequest,
                "Invalid transition cursor/limit",
            ));
        }
        let tx = self.connection.transaction().map_err(storage)?;
        let mut statement = tx.prepare("SELECT sequence,transition FROM transitions WHERE sequence>?1 ORDER BY sequence LIMIT ?2").map_err(storage)?;
        let rows = statement
            .query_map(params![after as i64, limit], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(storage)?;
        let mut values = Vec::new();
        for row in rows {
            let (sequence, json) = row.map_err(storage)?;
            let sequence = counter(sequence)?;
            let transition: UpdateTransition = decode(&json)?;
            if transition.sequence != sequence || sequence > MAX_UPDATE_COUNTER {
                return Err(fail(
                    JournalErrorCode::Corrupt,
                    "Transition sequence mismatch",
                ));
            }
            values.push(transition);
        }
        drop(statement);
        tx.commit().map_err(storage)?;
        Ok(values)
    }
}

fn append_transition(
    connection: &Connection,
    transition: &UpdateTransition,
    payload: Option<&str>,
    record: &str,
) -> Result<()> {
    one(connection.execute("INSERT INTO transitions(update_id,revision,command_key,payload,transition,record) VALUES(?1,?2,?3,?4,'',?5)",
        params![transition.update_id,transition.revision as i64,transition.command.as_ref().map(|c| &c.idempotency_key),payload,record]).map_err(storage)?)?;
    let sequence = connection.last_insert_rowid();
    if sequence < 1 || sequence as u64 > MAX_UPDATE_COUNTER {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Transition sequence exhausted",
        ));
    }
    let mut stored = transition.clone();
    stored.sequence = sequence as u64;
    let json = encode(&stored)?;
    one(connection
        .execute(
            "UPDATE transitions SET transition=?1 WHERE sequence=?2",
            params![json, sequence],
        )
        .map_err(storage)?)?;
    let actual: String = connection
        .query_row(
            "SELECT transition FROM transitions WHERE sequence=?1",
            [sequence],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if actual != json {
        return Err(fail(
            JournalErrorCode::Corrupt,
            "Transition event readback mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "journal/path_checks_tests.rs"]
mod path_checks_tests;
