//! Trusted in-process coordination only. No install/shutdown/source-write RPC yet.
use crate::{store::Store, Result, Worker};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use winbridge_protocol::update::*;
pub const UPDATE_STARTUP_GUARD: &str = "worker-update-guard.json";
const MAX_GUARD_BYTES: u64 = 32 * 1024;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartupGuard {
    schema_version: u16,
    spec: WorkerUpdateSpec,
}

fn read_guard(state: &Path) -> Result<Option<StartupGuard>> {
    let path = state.join(UPDATE_STARTUP_GUARD);
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_GUARD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_GUARD_BYTES {
        return Err(
            "Update startup guard exceeds limit; preserve it and inspect updater journal".into(),
        );
    }
    let guard: StartupGuard = serde_json::from_slice(&bytes).map_err(|_| {
        "Update startup guard is incomplete/unsupported; preserve it and inspect updater journal"
    })?;
    if guard.schema_version != 1 {
        return Err("Unsupported update startup guard; normal startup is blocked".into());
    }
    Ok(Some(guard))
}
pub(crate) fn reject_start(state: &Path) -> Result<()> {
    if let Some(guard) = read_guard(state)? {
        return Err(format!("Запуск worker заблокирован: обновление WinBridge {} ещё не завершено. Проверьте winbridge-update journal status; не удаляйте guard и не запускайте прежние binaries",guard.spec.update_id));
    }
    Store::reject_pending_update_start(&state.join("journal.sqlite"))
}
fn write_guard(state: &Path, spec: &WorkerUpdateSpec) -> Result<()> {
    let value = StartupGuard {
        schema_version: 1,
        spec: spec.clone(),
    };
    if let Some(existing) = read_guard(state)? {
        if existing == value {
            return Ok(());
        }
        return Err("Another update startup guard exists; preserve it and reconcile".into());
    }
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_GUARD_BYTES {
        return Err("Update spec exceeds startup guard limit; no marker created".into());
    }
    let path = state.join(UPDATE_STARTUP_GUARD);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    if read_guard(state)? != Some(value) {
        return Err("Update guard readback failed; no lease acknowledged".into());
    }
    Ok(())
}
fn clear_guard(state: &Path, spec: &WorkerUpdateSpec) -> Result<()> {
    let Some(guard) = read_guard(state)? else {
        return Ok(());
    };
    if guard.spec != *spec {
        return Err("Update release cannot remove another update's startup guard".into());
    }
    fs::remove_file(state.join(UPDATE_STARTUP_GUARD))
        .map_err(|e| format!("Lease released, but startup guard cleanup is unconfirmed: {e}"))
}
fn source_root(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_local_path_prefix(path).map_err(|e| e.to_string())?;
    winbridge_platform::require_safe_path(path).map_err(|e| e.to_string())?;
    winbridge_platform::require_local_disk(path).map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("Update source root must be an existing local directory".into());
    }
    path.canonicalize().map_err(|e| e.to_string())
}
impl Worker {
    /// Build/freeze this spec before sending a command; retries reuse it unchanged.
    pub fn update_source_spec(
        &self,
        update_id: &str,
        updater_owner_id: &str,
        updater_epoch: u64,
        source: &Path,
        (baseline, target): (&str, &str),
    ) -> Result<WorkerUpdateSpec> {
        let root = source_root(source)?;
        let mut projects = Vec::new();
        for project in &self.config.projects {
            let workspace = source_root(&project.workspace_root)?;
            if root.starts_with(&workspace) || workspace.starts_with(&root) {
                projects.push(project.id.clone());
            }
        }
        projects.sort();
        Ok(WorkerUpdateSpec {
            update_id: update_id.into(),
            instance_id: self.config.instance_id.clone(),
            journal_incarnation: self.store()?.journal_authority().incarnation.clone(),
            worker_generation: self.generation.clone(),
            updater_owner_id: updater_owner_id.into(),
            updater_epoch,
            source_directory: root.to_str().ok_or("Source path must be UTF-8")?.into(),
            baseline_manifest_sha256: baseline.into(),
            target_manifest_sha256: target.into(),
            projects,
        })
    }
    fn require_update_config_current(&self) -> Result<()> {
        let has_path = self
            .config_path
            .lock()
            .map_err(|_| "Config lock poisoned")?
            .is_some();
        if has_path && self.runtime_settings()?.restart_required {
            return Err("ConfigRestartPending: saved settings differ from the running worker; restart explicitly before requesting update".into());
        }
        Ok(())
    }
    pub fn begin_source_update(&self, spec: WorkerUpdateSpec) -> Result<WorkerUpdateLease> {
        let mut store = self.store()?;
        if spec.instance_id != self.config.instance_id
            || spec.journal_incarnation != store.journal_authority().incarnation
        {
            return Err("Update belongs to another worker instance/journal".into());
        }
        if let Some(existing) = store.existing_worker_update(&spec)? {
            return Ok(existing);
        }
        Store::validate_worker_update_spec(&spec)?;
        if self.inspection
            || self.stop.load(Ordering::Acquire)
            || self.journal_recovery_required()?.is_some()
        {
            return Err("Worker update coordination unavailable during recovery/stop".into());
        }
        if spec.worker_generation != self.generation {
            return Err("Worker generation changed; no source lease accepted".into());
        }
        // Construct while already holding Store; update_source_spec takes Store too.
        let root = source_root(Path::new(&spec.source_directory))?;
        if root.to_str() != Some(spec.source_directory.as_str()) {
            return Err("Source spec must use the canonical path".into());
        }
        let mut projects = Vec::new();
        for project in &self.config.projects {
            let workspace = source_root(&project.workspace_root)?;
            if root.starts_with(&workspace) || workspace.starts_with(&root) {
                projects.push(project.id.clone());
            }
        }
        projects.sort();
        if projects != spec.projects {
            return Err(
                "Source lease projects differ from actual registered workspace overlap".into(),
            );
        }
        self.require_update_config_current()?;
        if store.active_worker_update()?.is_some() {
            return Err("Another worker update is active".into());
        }
        // Persist the startup barrier before acceptance. A crash/orphan blocks startup,
        // never authorizes source writes or a silent takeover of either journal.
        write_guard(&self.config.state_directory, &spec)?;
        store.begin_worker_update(spec)
    }
    pub fn advance_source_update(
        &self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
    ) -> Result<WorkerUpdateLease> {
        self.advance_source_update_checked(spec, command, &|| Ok(()))
    }
    pub(crate) fn advance_source_update_checked(
        &self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
        authorize: &dyn Fn() -> Result<()>,
    ) -> Result<WorkerUpdateLease> {
        self.advance_source_update_inner(spec, command, authorize, false, &|root, hash| {
            winbridge_workspace_sync::source_tree::verify_source(root, hash).map(|_| ())
        })
    }
    #[cfg(test)]
    pub(crate) fn advance_source_update_test_verifier(
        &self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
        authorize: &dyn Fn() -> Result<()>,
        verify: &dyn Fn(&Path, &str) -> Result<()>,
    ) -> Result<WorkerUpdateLease> {
        self.advance_source_update_inner(spec, command, authorize, false, verify)
    }
    fn advance_source_update_inner(
        &self,
        spec: &WorkerUpdateSpec,
        command: &WorkerUpdateCommand,
        authorize: &dyn Fn() -> Result<()>,
        verified: bool,
        verify: &dyn Fn(&Path, &str) -> Result<()>,
    ) -> Result<WorkerUpdateLease> {
        // Scheduler takes active_projects -> Store; keep this order for close/grant.
        let active = self
            .active_projects
            .lock()
            .map_err(|_| "Process registry lock poisoned")?;
        let mut store = self.store()?;
        if spec.instance_id != self.config.instance_id
            || spec.journal_incarnation != store.journal_authority().incarnation
        {
            return Err("Worker update scope changed".into());
        }
        if let Some(receipt) = store.worker_update_receipt(spec, command)? {
            if !self.inspection
                && receipt.phase == WorkerUpdatePhase::Released
                && store.active_worker_update()?.is_none()
            {
                clear_guard(&self.config.state_directory, spec)?;
            }
            return Ok(receipt);
        }
        if self.inspection
            || self.stop.load(Ordering::Acquire)
            || spec.worker_generation != self.generation
            || self.journal_recovery_required()?.is_some()
        {
            return Err(
                "Worker update owner/generation unavailable; preserve both journals".into(),
            );
        }
        if read_guard(&self.config.state_directory)?.is_none_or(|g| g.spec != *spec) {
            return Err("Worker update startup guard missing/changed; no effect permitted".into());
        }
        authorize()?;
        if command.action != WorkerUpdateAction::Release {
            self.require_update_config_current()?;
        }
        let current = store.worker_update_lease(&spec.update_id)?;
        if current.spec != *spec {
            return Err("Worker update owner/scope mismatch".into());
        }
        let source_only = current.phase == WorkerUpdatePhase::SourcePending;
        let in_flight = if source_only {
            active.keys().filter(|p| spec.projects.contains(p)).count() as u64
        } else {
            self.active_jobs.load(Ordering::Acquire) as u64
                + active.len() as u64
                + self
                    .checks
                    .lock()
                    .map_err(|_| "Readiness check lock poisoned")?
                    .len() as u64
        };
        let mut blockers = WorkerUpdateBlockers {
            accepted_operations: 0,
            logical_owners: 0,
            in_flight_effects: in_flight
                + u64::from(self.maintenance_running.load(Ordering::Acquire)),
            physical_owners: 0,
            uncertain_owners: 0,
        };
        if command.action == WorkerUpdateAction::AcquireSource
            && cfg!(windows)
            && self
                .coverage
                .lock()
                .map_err(|_| "Coverage lock poisoned")?
                .as_ref()
                .is_none_or(|c| !c.complete)
        {
            blockers.uncertain_owners += 1;
        }
        if command.action == WorkerUpdateAction::ConfirmQuiesced {
            if current.phase != WorkerUpdatePhase::Quiescing {
                return Err("Physical scan requires committed dispatch closure first".into());
            }
            // No source scan/OS inventory can race a new DB claim after Quiescing.
            #[cfg(windows)]
            {
                use winbridge_platform::process_identity::ProcessObservation as O;
                let domain = self
                    .execution_domain
                    .lock()
                    .map_err(|_| "Execution domain lock poisoned")?;
                match domain
                    .as_ref()
                    .map(|d| winbridge_platform::execution_domain::observe_domain(d.identity()))
                {
                    Some(O::Empty(_)) => {}
                    Some(O::Occupied(_)) => blockers.physical_owners += 1,
                    Some(O::Unknown(_)) | None => blockers.uncertain_owners += 1,
                }
            }
        }
        let manifest = if command.action == WorkerUpdateAction::AcquireSource {
            Some(spec.baseline_manifest_sha256.as_str())
        } else if command.action == WorkerUpdateAction::Release
            && current.phase != WorkerUpdatePhase::SourcePending
        {
            let hash = command
                .source_manifest_sha256
                .as_deref()
                .ok_or("Source release requires a verified full manifest")?;
            if hash != spec.baseline_manifest_sha256 && hash != spec.target_manifest_sha256 {
                return Err("Release source manifest is not pinned by the lease".into());
            }
            Some(hash)
        } else {
            None
        };
        if let Some(hash) = manifest.filter(|_| !verified) {
            store.require_source_verification(spec, command, blockers)?;
            // Never hold the global process registry or journal while scanning
            // source bytes. The durable admission barrier remains in place.
            drop(store);
            drop(active);
            verify(Path::new(&spec.source_directory), hash)?;
            return self.advance_source_update_inner(spec, command, authorize, true, verify);
        }
        let lease = store.advance_worker_update(spec, command, blockers)?;
        if lease.phase == WorkerUpdatePhase::Released {
            clear_guard(&self.config.state_directory, spec)?;
        }
        Ok(lease)
    }
}
