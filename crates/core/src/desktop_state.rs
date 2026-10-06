//! Local desktop form persistence, independent of worker tasks and platform paths.
use crate::config::{valid_id, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use winbridge_protocol::{CommandDraft, DeliveryDraft, DesktopDraftState};

pub const DESKTOP_STATE_FILE: &str = "winbridge-desktop-state.local.json";
const MAX_BYTES: usize = 1024 * 1024;
const MAX_DRAFTS: usize = 128;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct DesktopDraftStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl DesktopDraftStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> Result<DesktopDraftState> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "desktop state lock poisoned")?;
        self.read()
    }

    pub fn save(&self, draft: CommandDraft) -> Result<()> {
        validate_draft(&draft)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "desktop state lock poisoned")?;
        let mut state = self.read()?;
        state.drafts.retain(|saved| !same_scope(saved, &draft));
        state.drafts.push(draft);
        if state.drafts.len() > MAX_DRAFTS {
            state.drafts.remove(0);
        }
        self.write_state(&state)
    }

    pub fn save_delivery(&self, draft: DeliveryDraft) -> Result<()> {
        validate_delivery_draft(&draft)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "desktop state lock poisoned")?;
        let mut state = self.read()?;
        state.delivery_drafts.retain(|saved| {
            saved.worker_instance_id != draft.worker_instance_id
                || saved.project_id != draft.project_id
                || saved.workspace_root != draft.workspace_root
        });
        state.delivery_drafts.push(draft);
        if state.delivery_drafts.len() > MAX_DRAFTS {
            state.delivery_drafts.remove(0);
        }
        self.write_state(&state)
    }

    fn write_state(&self, state: &DesktopDraftState) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("desktop form state exceeds 1 MiB".into());
        }
        self.write(&bytes)
    }

    fn read(&self) -> Result<DesktopDraftState> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DesktopDraftState {
                    schema_version: 1,
                    drafts: Vec::new(),
                    delivery_drafts: Vec::new(),
                });
            }
            Err(error) => return Err(format!("desktop state {}: {error}", self.path.display())),
        };
        if bytes.len() > MAX_BYTES {
            return Err(format!(
                "desktop state {} exceeds 1 MiB",
                self.path.display()
            ));
        }
        let state: DesktopDraftState = serde_json::from_slice(&bytes)
            .map_err(|e| format!("desktop state {}: {e}", self.path.display()))?;
        if state.schema_version != 1
            || state.drafts.len() > MAX_DRAFTS
            || state.delivery_drafts.len() > MAX_DRAFTS
        {
            return Err(format!(
                "desktop state {}: expected schemaVersion 1 and at most 128 drafts",
                self.path.display()
            ));
        }
        for draft in &state.drafts {
            validate_draft(draft)?;
        }
        for draft in &state.delivery_drafts {
            validate_delivery_draft(draft)?;
        }
        Ok(state)
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temporary = self.path.with_extension(format!(
            "{}-{timestamp}-{}.tmp",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|e| format!("desktop state {}: {e}", self.path.display()))?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|e| format!("desktop state {}: {e}", self.path.display()))
    }
}

fn same_scope(left: &CommandDraft, right: &CommandDraft) -> bool {
    left.worker_instance_id == right.worker_instance_id
        && left.project_id == right.project_id
        && left.workspace_root == right.workspace_root
}

fn validate_draft(draft: &CommandDraft) -> Result<()> {
    if !valid_id(&draft.worker_instance_id)
        || !valid_id(&draft.project_id)
        || draft.workspace_root.is_empty()
        || draft.recipe_id.as_ref().is_some_and(|id| !valid_id(id))
        || draft.snapshot_id.as_ref().is_some_and(|id| !valid_id(id))
    {
        return Err("desktop draft: invalid worker/project/recipe scope".into());
    }
    if [
        &draft.workspace_root,
        &draft.executable,
        &draft.arguments_json,
        &draft.working_directory,
    ]
    .iter()
    .any(|value| value.len() > 65536 || value.contains('\0'))
        || draft.timeout_seconds.len() > 32
    {
        return Err("desktop draft: field exceeds size limit or contains NUL".into());
    }
    Ok(())
}

fn validate_delivery_draft(draft: &DeliveryDraft) -> Result<()> {
    if !valid_id(&draft.worker_instance_id)
        || !valid_id(&draft.project_id)
        || draft.workspace_root.is_empty()
        || draft.workspace_root.len() > 65536
        || draft.workspace_root.contains('\0')
        || draft.snapshot_id.as_ref().is_some_and(|id| !valid_id(id))
    {
        return Err("desktop delivery draft: invalid worker/project/root/snapshot scope".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(project: &str) -> CommandDraft {
        CommandDraft {
            worker_instance_id: "worker-1".into(),
            project_id: project.into(),
            workspace_root: r"K:\project with spaces".into(),
            recipe_id: None,
            snapshot_id: None,
            executable: r"K:\project with spaces\fixture.exe".into(),
            arguments_json: "[\"long\", \"Привет ✓\"]".into(),
            working_directory: ".".into(),
            timeout_seconds: "120".into(),
        }
    }

    #[test]
    fn draft_survives_new_store_with_exact_text_and_no_task_submission() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(DESKTOP_STATE_FILE);
        let store = DesktopDraftStore::new(path.clone());
        assert!(store.load().unwrap().drafts.is_empty());
        let mut saved = draft("project");
        saved.arguments_json = "[\n  \"Привет ✓\", ".into();
        saved.timeout_seconds = String::new();
        store.save(saved.clone()).unwrap();
        drop(store);
        assert_eq!(
            DesktopDraftStore::new(path).load().unwrap().drafts,
            vec![saved]
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn scopes_are_independent_and_updates_replace_only_matching_draft() {
        let root = tempfile::tempdir().unwrap();
        let store = DesktopDraftStore::new(root.path().join(DESKTOP_STATE_FILE));
        let first = draft("first");
        let second = draft("second");
        let mut other_worker = first.clone();
        other_worker.worker_instance_id = "worker-2".into();
        let mut other_root = first.clone();
        other_root.workspace_root = r"K:\other project".into();
        for saved in [&first, &second, &other_worker, &other_root] {
            store.save(saved.clone()).unwrap();
        }
        let mut updated = first.clone();
        updated.executable = "changed.exe".into();
        store.save(updated.clone()).unwrap();
        assert_eq!(
            store.load().unwrap().drafts,
            vec![second, other_worker, other_root, updated]
        );
    }

    #[test]
    fn delivery_and_command_state_survive_updates_and_legacy_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(DESKTOP_STATE_FILE);
        fs::write(&path, br#"{"schemaVersion":1,"drafts":[]}"#).unwrap();
        let store = DesktopDraftStore::new(path.clone());
        assert!(store.load().unwrap().delivery_drafts.is_empty());
        let mut first = DeliveryDraft {
            worker_instance_id: "worker-1".into(),
            project_id: "first".into(),
            workspace_root: r"K:\first".into(),
            snapshot_id: Some("s-first".into()),
        };
        let mut second = first.clone();
        second.project_id = "second".into();
        second.workspace_root = r"K:\second".into();
        store.save_delivery(first.clone()).unwrap();
        store.save(draft("first")).unwrap();
        store.save_delivery(second.clone()).unwrap();
        first.snapshot_id = Some("s-updated".into());
        store.save_delivery(first.clone()).unwrap();
        let reopened = DesktopDraftStore::new(path).load().unwrap();
        assert_eq!(reopened.drafts, vec![draft("first")]);
        assert_eq!(reopened.delivery_drafts, vec![second, first]);
    }

    #[test]
    fn invalid_existing_state_is_preserved_and_oversized_draft_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(DESKTOP_STATE_FILE);
        let store = DesktopDraftStore::new(path.clone());
        for original in [
            b"{broken".as_slice(),
            br#"{"schemaVersion":2,"drafts":[]}"#,
            br#"{"schemaVersion":1,"drafts":[],"unknown":true}"#,
        ] {
            fs::write(&path, original).unwrap();
            assert!(store.load().is_err());
            assert!(store.save(draft("project")).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
        fs::remove_file(&path).unwrap();
        let mut huge = draft("project");
        huge.arguments_json = "x".repeat(65537);
        assert!(store.save(huge).is_err());
        assert!(!path.exists());
    }
}
