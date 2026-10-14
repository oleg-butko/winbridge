//! Immutable local evidence. A truncated write can never become a valid receipt.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
};
use winbridge_protocol::{evidence_sha256, is_sha256, PairVerification};
const MAX_RECEIPT_BYTES: u64 = 256 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u16,
    sha256: String,
    receipt: PairVerification,
}

pub struct ReceiptStore {
    root: PathBuf,
}
impl ReceiptStore {
    pub fn open(state_directory: &Path) -> Result<Self, String> {
        if !state_directory.is_absolute()
            || state_directory
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("Evidence store requires an absolute local state directory".into());
        }
        winbridge_platform::require_local_disk(state_directory).map_err(|e| e.to_string())?;
        for parent in state_directory.ancestors() {
            if fs::symlink_metadata(parent)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Evidence directory ancestors cannot be symbolic links".into());
            }
        }
        super::activity::private_directory(state_directory)?;
        let root = state_directory.join("contract-receipts-v1");
        super::activity::private_directory(&root)?;
        Ok(Self { root })
    }
    fn check_root(&self) -> Result<(), String> {
        for parent in self.root.ancestors() {
            let metadata = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("Evidence directory changed; no read/write allowed".into());
            }
        }
        super::activity::private_directory(&self.root)?;
        Ok(())
    }
    pub fn put(&self, receipt: &PairVerification) -> Result<String, String> {
        self.check_root()?;
        winbridge_core::readiness::validate_verification(receipt)?;
        let payload = serde_json::to_vec(receipt).map_err(|e| e.to_string())?;
        let id = evidence_sha256(&payload);
        let bytes = serde_json::to_vec(&Envelope {
            schema_version: 1,
            sha256: id.clone(),
            receipt: receipt.clone(),
        })
        .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("Receipt exceeds 256 KiB".into());
        }
        let staging = self.root.join(format!(".pending-{}", uuid::Uuid::new_v4()));
        let committed = self.root.join(format!("{id}.json"));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&staging)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            // Atomic create-only publication. Rename would overwrite an existing receipt.
            match fs::hard_link(&staging, &committed) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
            fs::File::open(&self.root)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| e.to_string())?;
            let observed = self.get(&id)?.ok_or("Published receipt is missing")?;
            if observed != *receipt {
                return Err("Receipt readback differs; no verification granted".into());
            }
            Ok(id)
        })();
        // Only our generated pending file is removed. Existing receipts stay immutable.
        let _ = fs::remove_file(staging);
        result
    }
    pub fn get(&self, id: &str) -> Result<Option<PairVerification>, String> {
        self.check_root()?;
        if !is_sha256(id) {
            return Err("Invalid receipt ID".into());
        }
        let path = self.root.join(format!("{id}.json"));
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Receipt must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_RECEIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("Receipt exceeds 256 KiB".into());
        }
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid receipt: {e}"))?;
        if envelope.schema_version != 1
            || envelope.sha256 != id
            || evidence_sha256(&serde_json::to_vec(&envelope.receipt).map_err(|e| e.to_string())?)
                != id
        {
            return Err("Receipt schema or checksum mismatch; no verification granted".into());
        }
        winbridge_core::readiness::validate_verification(&envelope.receipt)?;
        Ok(Some(envelope.receipt))
    }
}
