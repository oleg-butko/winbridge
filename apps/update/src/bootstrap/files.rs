//! Fixed helper preparation. No coordinator is created, and saved rows never
//! reconstruct these file handles or the private startup token.
use super::*;
use crate::{
    artifact,
    journal::{UpdateAuthority, UpdateJournal},
    load_config,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use winbridge_platform::execution_domain::ExecutionDomain;
use winbridge_protocol::{ArtifactDescriptor, ArtifactRole};
use winbridge_transport::supervisor as ipc;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn local(path: &Path) -> Result<PathBuf> {
    winbridge_platform::require_local_path_prefix(path).map_err(error)?;
    winbridge_platform::require_safe_path(path).map_err(error)?;
    winbridge_platform::require_local_disk(path).map_err(error)?;
    path.canonicalize().map_err(error)
}
fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Bootstrap paths must be UTF-8".into())
}
fn file_evidence(pin: &artifact::Pinned) -> BootstrapFile {
    BootstrapFile {
        sha256: pin.evidence.sha256.clone(),
        bytes: pin.evidence.bytes,
        file_identity: pin.evidence.file_identity.clone(),
    }
}
fn current(pin: &artifact::Pinned) -> Result<()> {
    let path = Path::new(&pin.evidence.path);
    winbridge_platform::require_single_link_handle(&pin.file).map_err(error)?;
    let now = artifact::pin(path)?;
    if now.evidence != pin.evidence
        || winbridge_platform::file_identity(&pin.file).map_err(error)?
            != pin.evidence.file_identity
    {
        return Err("Bootstrap file changed after it was pinned".into());
    }
    winbridge_platform::require_single_link_handle(&now.file).map_err(error)
}
fn verify_build(descriptor: &ArtifactDescriptor, source: &UpdateSourcePin) -> Result<()> {
    let b = &descriptor.build;
    if descriptor.schema_version != 1
        || descriptor.query_contract != 1
        || descriptor.kind != "winbridge_artifact"
        || descriptor.role != ArtifactRole::Updater
        || descriptor.target_os != std::env::consts::OS
        || descriptor.target_arch != std::env::consts::ARCH
        || b.source_dirty
        || b.source_commit.as_ref() != Some(&source.source_commit)
        || b.source_manifest_sha256.as_ref() != Some(&source.manifest_sha256)
        || !text(&b.build_id, 128)
        || !text(&b.version, 128)
    {
        return Err("Fresh updater is not from the requested clean source/target".into());
    }
    Ok(())
}
/// A caller-owned result of a trusted managed build. The descriptor query runs
/// in a bounded owned Job; it is not candidate acceptance or a launch permission.
pub struct FreshUpdater {
    image: artifact::Pinned,
    descriptor: ArtifactDescriptor,
    source: UpdateSourcePin,
}
impl FreshUpdater {
    pub fn from_managed_build(
        project_root: &Path,
        image: &Path,
        source: &UpdateSourcePin,
    ) -> Result<Self> {
        let root = local(project_root)?;
        let image = local(image)?;
        if !root.is_dir() || !image.starts_with(&root) || image == root {
            return Err("Fresh updater must come from the registered project build scope".into());
        }
        let image = artifact::pin(&image)?;
        let report = artifact::query_image(Path::new(&image.evidence.path), ArtifactRole::Updater)?;
        verify_build(&report.descriptor, source)?;
        if report.image != image.evidence || !report.cleanup_verified {
            return Err("Fresh updater query changed image identity or left a process tree".into());
        }
        Ok(Self {
            image,
            descriptor: report.descriptor,
            source: source.clone(),
        })
    }
}
/// Output pins deny replacement but intentionally allow the keeper's writer.
/// No writable config/image handle survives preparation.
struct OutputPin {
    file: File,
    path: PathBuf,
    identity: String,
}
impl OutputPin {
    fn create(path: &Path) -> Result<Self> {
        winbridge_platform::require_safe_path(path).map_err(error)?;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(error)?;
        f.flush().map_err(error)?;
        f.sync_all().map_err(error)?;
        drop(f);
        winbridge_platform::source_files::sync_parent(path).map_err(error)?;
        let file = Self::open(path)?;
        winbridge_platform::require_single_link_handle(&file).map_err(error)?;
        let identity = winbridge_platform::file_identity(&file).map_err(error)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
            identity,
        })
    }
    fn open(path: &Path) -> Result<File> {
        winbridge_platform::require_safe_path(path).map_err(error)?;
        let mut o = OpenOptions::new();
        o.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
            o.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        o.open(path).map_err(error)
    }
    fn current(&self) -> Result<()> {
        let now = Self::open(&self.path)?;
        if winbridge_platform::file_identity(&now).map_err(error)? != self.identity
            || winbridge_platform::file_identity(&self.file).map_err(error)? != self.identity
        {
            return Err("Bootstrap output identity changed".into());
        }
        winbridge_platform::require_single_link_handle(&now).map_err(error)?;
        winbridge_platform::require_single_link_handle(&self.file).map_err(error)
    }
}
#[must_use = "Retain helper/config/output pins until admission and creation are resolved"]
pub struct PreparedBootstrapFiles {
    update: UpdateRecord,
    launch_id: String,
    parent_owner_id: String,
    parent_epoch: u64,
    worker: SupervisorWorkerBinding,
    domain: winbridge_platform::execution_domain::DomainIdentity,
    runtime: PathBuf,
    image: artifact::Pinned,
    config: artifact::Pinned,
    worker_config: artifact::Pinned,
    descriptor: ArtifactDescriptor,
    stdout: OutputPin,
    stderr: OutputPin,
    token: String,
}
impl PreparedBootstrapFiles {
    /// Trusted worker entry only, never command JSON. This staging step neither
    /// reads keeper rows as custody nor writes the supervisor/reservation ledger.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        j: &mut UpdateJournal,
        a: &UpdateAuthority,
        id: &str,
        config_path: &Path,
        worker: SupervisorWorkerBinding,
        domain: &ExecutionDomain,
        fresh: FreshUpdater,
    ) -> Result<Self> {
        j.check_source_authority(a).map_err(error)?;
        let update = j.get(id).map_err(error)?;
        if update.phase != UpdatePhase::Requested
            || update.revision != 0
            || update.reason.is_some()
            || update.updated_at_ms != update.accepted_at_ms
            || fresh.source != update.request.source
        {
            return Err(
                "Bootstrap files require the original Requested root and fresh requested updater"
                    .into(),
            );
        }
        match j.supervisor(id) {
            Ok(_) => {
                return Err("Existing supervisor data are not a fresh staging permission".into())
            }
            Err(e) if e.code == crate::journal::JournalErrorCode::NotFound => {}
            Err(e) => return Err(error(e)),
        }
        let actual =
            winbridge_platform::infrastructure::stamp(std::process::id()).map_err(error)?;
        #[cfg(windows)]
        if worker.endpoint != ipc::endpoint(Path::new("."), &worker.generation).map_err(error)? {
            return Err("Bootstrap worker-control endpoint changed its loaded generation".into());
        }
        if actual != worker.process
            || worker.build.build_id != update.request.expected.worker_build_id
            || worker.config_sha256 != update.request.expected.worker_config_sha256
            || domain.identity().host_id != actual.host_id
            || domain.identity().session_id != actual.session_id
        {
            return Err("Bootstrap staging changed the actual worker/loaded config/domain".into());
        }
        verify_build(&fresh.descriptor, &update.request.source)?;
        let config_path = local(config_path)?;
        let config = artifact::pin(&config_path)?;
        if config.evidence.bytes > 65536 {
            return Err("Updater config exceeds bound".into());
        }
        let cfg = load_config(&config_path)?;
        let runtime = local(&cfg.runtime_directory)?;
        j.require_journal_directory(&runtime.join("update-state"))
            .map_err(error)?;
        if config_path != local(&runtime.join("winbridge-update.local.json"))?
            || crate::installation_scope(&cfg)? != j.info().map_err(error)?.scope_sha256
        {
            return Err("Bootstrap config/runtime changed the accepted installation scope".into());
        }
        let worker_config = artifact::pin(&local(&cfg.worker_config_path)?)?;
        if worker_config.evidence.bytes > 65536
            || worker_config.evidence.sha256 != worker.config_sha256
        {
            return Err("Loaded worker configuration no longer matches disk".into());
        }
        current(&config)?;
        current(&worker_config)?;
        crate::verify_source(
            &cfg.source_directory,
            &update.request.expected.baseline_manifest_sha256,
        )?;
        let updates = runtime.join("update");
        winbridge_platform::require_safe_path(&updates).map_err(error)?;
        if !updates.exists() {
            fs::create_dir(&updates).map_err(error)?;
        }
        let run = updates.join(id);
        winbridge_platform::require_safe_path(&run).map_err(error)?;
        if !run.exists() {
            fs::create_dir(&run).map_err(error)?;
        }
        let helper = run.join("supervisor");
        winbridge_platform::require_safe_path(&helper).map_err(error)?;
        fs::create_dir(&helper).map_err(|e| {
            format!(
                "Helper staging already exists or cannot be created; preserve it, no replay: {e}"
            )
        })?;
        winbridge_platform::source_files::sync_parent(&helper).map_err(error)?;
        let image = fresh
            .image
            .copy_new_checked(&helper.join("winbridge-update.exe"), || {
                j.check_source_authority(a).map_err(error)
            })?;
        let stdout = OutputPin::create(&helper.join("stdout.log"))?;
        let stderr = OutputPin::create(&helper.join("stderr.log"))?;
        let value = Self {
            update,
            launch_id: uuid::Uuid::new_v4().to_string(),
            parent_owner_id: a.owner_id().into(),
            parent_epoch: a.epoch(),
            worker,
            domain: domain.identity().clone(),
            runtime,
            image,
            config,
            worker_config,
            descriptor: fresh.descriptor,
            stdout,
            stderr,
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
        };
        value.require_current()?;
        j.check_source_authority(a).map_err(error)?;
        if j.get(id).map_err(error)? != value.update {
            return Err("Bootstrap root changed during staging; retain files".into());
        }
        Ok(value)
    }
    pub fn require_current(&self) -> Result<()> {
        current(&self.image)?;
        current(&self.config)?;
        current(&self.worker_config)?;
        self.stdout.current()?;
        self.stderr.current()?;
        let ids = [
            &self.image.evidence.file_identity,
            &self.config.evidence.file_identity,
            &self.worker_config.evidence.file_identity,
            &self.stdout.identity,
            &self.stderr.identity,
        ];
        if !(0..ids.len()).all(|i| ((i + 1)..ids.len()).all(|k| ids[i] != ids[k])) {
            return Err("Bootstrap files alias".into());
        }
        Ok(())
    }
    pub(super) fn intent(
        &self,
        previous: &KeeperBinding,
        at: u64,
    ) -> Result<(BootstrapIntent, SupervisorSpec)> {
        let intent = BootstrapIntent {
            schema_version: 1,
            previous: previous.clone(),
            update: self.update.clone(),
            launch_id: self.launch_id.clone(),
            parent_owner_id: self.parent_owner_id.clone(),
            parent_epoch: self.parent_epoch,
            updater_build: self.descriptor.build.clone(),
            image: file_evidence(&self.image),
            config: file_evidence(&self.config),
            stdout_identity: self.stdout.identity.clone(),
            stderr_identity: self.stderr.identity.clone(),
            token_sha256: crate::sha256(self.token.as_bytes()),
            excluded_domain: self.domain.clone(),
            prepared_at_ms: at,
        };
        intent.validate()?;
        let spec = SupervisorSpec {
            launch_id: self.launch_id.clone(),
            update_id: self.update.update_id.clone(),
            parent: self.worker.process.clone(),
            parent_owner_id: self.parent_owner_id.clone(),
            parent_epoch: self.parent_epoch,
            token_sha256: intent.token_sha256.clone(),
            config_path: self.config.evidence.path.clone(),
            config_sha256: self.config.evidence.sha256.clone(),
            runtime_directory: path_text(&self.runtime)?,
            image_path: self.image.evidence.path.clone(),
            image_sha256: self.image.evidence.sha256.clone(),
            image_identity: self.image.evidence.file_identity.clone(),
            excluded_domain: Some(self.domain.job_name.clone()),
            endpoint: ipc::endpoint(&self.runtime.join("update-state"), &self.launch_id)
                .map_err(error)?,
            accepted_at_ms: at,
            worker: Some(self.worker.clone()),
        };
        Ok((intent, spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepared(root: &Path) -> PreparedBootstrapFiles {
        let model = crate::bootstrap::tests::fixture();
        let runtime = root.join("runtime");
        fs::create_dir(&runtime).unwrap();
        let helper = runtime
            .join("update")
            .join(&model.update.update_id)
            .join("supervisor");
        fs::create_dir_all(&helper).unwrap();
        let image = helper.join("winbridge-update.exe");
        fs::write(&image, b"fresh helper bytes").unwrap();
        let config = runtime.join("winbridge-update.local.json");
        fs::write(&config, b"{}").unwrap();
        let worker = runtime.join("worker.local.json");
        fs::write(&worker, b"{\"worker\":true}").unwrap();
        let descriptor = ArtifactDescriptor {
            schema_version: 1,
            kind: "winbridge_artifact".into(),
            query_contract: 1,
            role: ArtifactRole::Updater,
            target_os: std::env::consts::OS.into(),
            target_arch: std::env::consts::ARCH.into(),
            build: model.updater_build.clone(),
        };
        let mut update = model.update;
        update.request.expected.worker_config_sha256 = crate::sha256(b"{\"worker\":true}");
        let worker_binding = SupervisorWorkerBinding {
            endpoint: "fixture-only-endpoint".into(),
            process: model.previous.worker,
            context: model.previous.context,
            generation: model.previous.generation,
            build: winbridge_protocol::BuildIdentity {
                build_id: update.request.expected.worker_build_id.clone(),
                ..model.updater_build
            },
            config_sha256: update.request.expected.worker_config_sha256.clone(),
        };
        PreparedBootstrapFiles {
            update,
            launch_id: model.launch_id,
            parent_owner_id: model.parent_owner_id,
            parent_epoch: model.parent_epoch,
            worker: worker_binding,
            domain: model.excluded_domain,
            runtime: runtime.canonicalize().unwrap(),
            image: artifact::pin(&image).unwrap(),
            config: artifact::pin(&config).unwrap(),
            worker_config: artifact::pin(&worker).unwrap(),
            descriptor,
            stdout: OutputPin::create(&helper.join("stdout.log")).unwrap(),
            stderr: OutputPin::create(&helper.join("stderr.log")).unwrap(),
            token: "6".repeat(64),
        }
    }
    #[test]
    fn helper_files_bind_new_build_fixed_paths_and_separate_config_hashes() {
        // Synthetic descriptor/worker metadata, real local file handles; no CLI
        // query, OS worker identity or supervisor admission is proved here.
        let root = tempfile::tempdir().unwrap();
        let p = prepared(root.path());
        p.require_current().unwrap();
        let previous = crate::bootstrap::tests::fixture().previous;
        let (intent, spec) = p.intent(&previous, 20).unwrap();
        assert_eq!(intent.image.file_identity, p.image.evidence.file_identity);
        assert_eq!(intent.config.file_identity, p.config.evidence.file_identity);
        assert_eq!(spec.parent, p.worker.process);
        assert_eq!(spec.config_path, p.config.evidence.path);
        assert_eq!(spec.accepted_at_ms, intent.prepared_at_ms);
        assert_eq!(spec.token_sha256, crate::sha256(p.token.as_bytes()));
        assert_ne!(intent.config.sha256, p.worker.config_sha256);
        assert!(!serde_json::to_string(&intent)
            .unwrap()
            .contains(&format!("\"{}\"", p.token)));
        verify_build(&p.descriptor, &p.update.request.source).unwrap();
        for fault in 0..5 {
            let mut bad = p.descriptor.clone();
            match fault {
                0 => bad.build.source_commit = Some("0".repeat(40)),
                1 => bad.build.source_manifest_sha256 = Some("0".repeat(64)),
                2 => bad.build.source_dirty = true,
                3 => bad.role = ArtifactRole::Worker,
                _ => bad.target_os = "foreign".into(),
            }
            assert!(
                verify_build(&bad, &p.update.request.source).is_err(),
                "fault {fault}"
            );
        }
    }
    #[test]
    fn outputs_allow_writes_but_preparation_detects_path_rebinding() {
        let root = tempfile::tempdir().unwrap();
        let p = prepared(root.path());
        let mut writer = OpenOptions::new()
            .append(true)
            .open(&p.stdout.path)
            .unwrap();
        writer.write_all(b"diagnostic\n").unwrap();
        drop(writer);
        p.require_current().unwrap();
        let renamed = p.stdout.path.with_extension("renamed");
        #[cfg(not(windows))]
        {
            fs::rename(&p.stdout.path, &renamed).unwrap();
            fs::write(&p.stdout.path, b"different").unwrap();
            assert!(p.require_current().is_err());
        }
        #[cfg(windows)]
        {
            assert!(fs::rename(&p.stdout.path, &renamed).is_err());
            p.require_current().unwrap();
        }
    }
    #[test]
    fn linked_config_or_staged_image_cannot_pass_as_fresh_files() {
        for name in ["config", "image"] {
            let root = tempfile::tempdir().unwrap();
            let p = prepared(root.path());
            let path = if name == "config" {
                &p.config.evidence.path
            } else {
                &p.image.evidence.path
            };
            let alias = root.path().join("alias");
            match fs::hard_link(path, &alias) {
                Ok(()) => assert!(p.require_current().is_err()),
                Err(_error) => {
                    #[cfg(not(windows))]
                    panic!("Local Unix fixture must support hard links: {_error}");
                    #[cfg(windows)]
                    p.require_current().unwrap();
                }
            }
        }
    }
}

#[cfg(all(test, windows))]
mod native {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Input {
        source: UpdateSourcePin,
        candidate: PathBuf,
        proof: PathBuf,
    }
    #[test]
    #[ignore = "Fresh real updater query plus Windows file-sharing; synthetic worker metadata, no coordinator"]
    fn fresh_helper_and_config_pins_allow_only_the_intended_file_access() {
        let input: Input = serde_json::from_slice(
            &fs::read(
                std::env::var_os("WINBRIDGE_BOOTSTRAP_FILES_INPUT")
                    .expect("Explicit native input required"),
            )
            .unwrap(),
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let runtime = root.path().join("runtime");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&runtime).unwrap();
        fs::write(source.join("Cargo.toml"), b"fixture").unwrap();
        let manifest = winbridge_protocol::update::SourceManifest {
            schema_version: 1,
            source_commit: "a".repeat(40),
            dirty: false,
            created_at: "2026-10-30T00:00:00Z".into(),
            includes_built_ui: false,
            files: vec![winbridge_protocol::update::SourceFile {
                path: "Cargo.toml".into(),
                bytes: 7,
                sha256: crate::sha256(b"fixture"),
            }],
        };
        let baseline = serde_json::to_vec(&manifest).unwrap();
        fs::write(source.join("winbridge-source-manifest.json"), &baseline).unwrap();
        let worker_config = runtime.join("worker.local.json");
        fs::write(&worker_config, b"{}").unwrap();
        let config = runtime.join("winbridge-update.local.json");
        let config_bytes=serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sourceDirectory":source,"runtimeDirectory":runtime,"workerConfigPath":worker_config})).unwrap();
        fs::write(&config, &config_bytes).unwrap();
        let old = b"old installed updater sentinel, must not be copied";
        fs::write(runtime.join("winbridge-update.exe"), old).unwrap();
        let candidate = root.path().join("candidate.exe");
        fs::copy(&input.candidate, &candidate).unwrap();
        let fresh =
            FreshUpdater::from_managed_build(root.path(), &candidate, &input.source).unwrap();
        let queried_build = fresh.descriptor.build.clone();
        let fresh_sha = fresh.image.evidence.sha256.clone();
        let cfg = load_config(&config).unwrap();
        let state = runtime.join("update-state");
        let mut j = UpdateJournal::open(&state, &crate::installation_scope(&cfg).unwrap()).unwrap();
        let a = j.acquire_owner().unwrap();
        let info = j.info().unwrap();
        let expected = UpdateInstallationPin {
            installation_id: info.installation_id,
            scope_sha256: info.scope_sha256,
            worker_build_id: winbridge_protocol::build_identity().build_id,
            worker_config_sha256: crate::sha256(b"{}"),
            baseline_manifest_sha256: crate::sha256(&baseline),
        };
        let request = UpdateRequest {
            contract_version: 1,
            idempotency_key: "fresh-files-native".into(),
            source: input.source.clone(),
            expected: expected.clone(),
        };
        let accepted = j.accept(&a, &request, &expected).unwrap();
        let generation = uuid::Uuid::new_v4().to_string();
        let worker = SupervisorWorkerBinding {
            process: winbridge_platform::infrastructure::stamp(std::process::id()).unwrap(),
            context: winbridge_protocol::RequestContext {
                instance_id: "bootstrap-files-fixture".into(),
                journal_incarnation: uuid::Uuid::new_v4().to_string(),
            },
            generation: generation.clone(),
            build: winbridge_protocol::build_identity(),
            config_sha256: expected.worker_config_sha256,
            endpoint: ipc::endpoint(root.path(), &generation).unwrap(),
        };
        let domain_id = ExecutionDomain::identity_for(
            "bootstrap-files-fixture",
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap()
        .unwrap();
        let domain = ExecutionDomain::open(&domain_id).unwrap();
        let prepared = PreparedBootstrapFiles::prepare(
            &mut j,
            &a,
            &accepted.update_id,
            &config,
            worker,
            &domain,
            fresh,
        )
        .unwrap();
        prepared.require_current().unwrap();
        assert_eq!(prepared.image.evidence.sha256, fresh_sha);
        assert_eq!(fs::read(runtime.join("winbridge-update.exe")).unwrap(), old);
        let staged = PathBuf::from(&prepared.image.evidence.path);
        assert!(OpenOptions::new().write(true).open(&staged).is_err());
        assert!(OpenOptions::new().write(true).open(&config).is_err());
        assert!(OpenOptions::new().write(true).open(&worker_config).is_err());
        let replacement = runtime.join("replacement.json");
        fs::write(&replacement, &config_bytes).unwrap();
        assert!(fs::rename(&replacement, &config).is_err());
        let stdout = prepared.stdout.path.clone();
        let mut writer = OpenOptions::new().append(true).open(&stdout).unwrap();
        writer.write_all(b"expected writer\n").unwrap();
        drop(writer);
        assert!(fs::rename(&stdout, runtime.join("relocated.log")).is_err());
        prepared.require_current().unwrap();
        assert_eq!(j.get(&accepted.update_id).unwrap(), accepted);
        assert_eq!(
            j.supervisor(&accepted.update_id).unwrap_err().code,
            crate::journal::JournalErrorCode::NotFound
        );
        assert!(
            UpdateJournal::inspect_bootstrap(&state, &accepted.update_id)
                .unwrap()
                .is_none()
        );
        drop(prepared);
        fs::write(&config, b"write after retiring pins").unwrap();
        fs::rename(&replacement, &config).unwrap();
        assert_eq!(fs::read(&config).unwrap(), config_bytes);
        let report = serde_json::json!({"schemaVersion":1,"kind":"bootstrap_files_native_proof","platform":"windows","passed":true,"build":winbridge_protocol::build_identity(),"queriedBuild":queried_build,"source":input.source,"scope":"fresh helper files and Windows sharing; synthetic worker state, no coordinator","syntheticWorkerState":true,"freshImageSha256":fresh_sha,"stagedImageSha256":crate::sha256(&fs::read(staged).unwrap()),"oldInstalledImageUnchanged":true,"imageWriteBlocked":true,"configWriteBlocked":true,"workerConfigWriteBlocked":true,"configReplaceBlocked":true,"outputWriteAllowed":true,"outputReplaceBlocked":true,"pinsRetiredWriteAndReplace":true,"rootUnchanged":true,"noSupervisorIntent":true,"noBootstrapReservation":true,"coordinatorCreated":false,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false,"networkUsed":false});
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(input.proof)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&report).unwrap())
            .unwrap();
    }
}
