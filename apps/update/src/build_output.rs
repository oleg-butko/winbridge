//! Bounded readonly compiler output. No process control or filesystem mutation.
use crate::{journal::UpdateJournal, UpdateConfig};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use winbridge_protocol::{update::*, OutputStream};

pub const MAX_PAGE_BYTES: u32 = 64 * 1024;
type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn open(path: &Path) -> Result<File> {
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        // Capture keeps writing; replacement/deletion cannot race this handle.
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let f = options.open(path).map_err(error)?;
    winbridge_platform::require_safe_path(path).map_err(error)?;
    if !f.metadata().map_err(error)?.is_file() {
        return Err("Compiler output is not a regular file".into());
    }
    Ok(f)
}

pub fn read_compiler_output(
    cfg: &UpdateConfig,
    id: &str,
    q: &UpdateCompilerOutputRequest,
) -> Result<UpdateCompilerOutputPage> {
    if uuid::Uuid::parse_str(id).is_err()
        || uuid::Uuid::parse_str(&q.attempt_id).is_err()
        || q.limit_bytes == 0
        || q.limit_bytes > MAX_PAGE_BYTES
        || q.offset > crate::build::MAX_OUTPUT_BYTES
        || q.offset > 0 && q.file_identity.is_none()
        || q.file_identity
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 128)
    {
        return Err("Invalid bounded compiler output cursor/request".into());
    }
    for root in [&cfg.runtime_directory, &cfg.source_directory] {
        winbridge_platform::require_local_path_prefix(root).map_err(error)?;
        winbridge_platform::require_safe_path(root).map_err(error)?;
        winbridge_platform::require_local_disk(root).map_err(error)?;
    }
    let runtime = cfg.runtime_directory.canonicalize().map_err(error)?;
    winbridge_platform::require_local_disk(&runtime).map_err(error)?;
    let directory = runtime.join("update-state");
    let info = UpdateJournal::inspect(&directory).map_err(error)?;
    if info.scope_sha256 != crate::installation_scope(cfg)? {
        return Err("Compiler output belongs to another installation".into());
    }
    let view = UpdateJournal::preparation_snapshot(&directory, id, false, None).map_err(error)?;
    let r = view.build.ok_or("No admitted compiler attempt")?;
    if r.spec.update_id != id
        || r.spec.attempt_id != q.attempt_id
        || Path::new(&r.spec.runtime_directory)
            .canonicalize()
            .map_err(error)?
            != runtime
        || Path::new(&r.spec.source_directory)
            .canonicalize()
            .map_err(error)?
            != cfg.source_directory.canonicalize().map_err(error)?
    {
        return Err("Compiler output attempt/roots changed; no cursor rebind".into());
    }
    let (stream, proof) = match q.stream {
        OutputStream::Stdout => ("stdout", r.stdout.as_ref()),
        OutputStream::Stderr => ("stderr", r.stderr.as_ref()),
    };
    let path = runtime
        .join("update")
        .join(id)
        .join("compiler")
        .join(&q.attempt_id)
        .join(format!("{stream}.bin"));
    let mut page = UpdateCompilerOutputPage {
        schema_version: 1,
        kind: "update_compiler_output".into(),
        read_only: true,
        update_id: id.into(),
        attempt_id: q.attempt_id.clone(),
        build_revision: r.revision,
        observed_at_ms: view.observed_at_ms,
        phase: r.phase,
        stream: q.stream,
        offset: q.offset,
        next_offset: q.offset,
        available_bytes: 0,
        file_identity: None,
        bytes: vec![],
        has_more: false,
        capture_complete: false,
        proof_verified: false,
        observed_bytes: proof.map(|p| p.observed_bytes),
        quota_truncated: proof.map(|p| p.observed_bytes > p.retained_bytes),
        output_error: proof.and_then(|p| p.error.clone()),
    };
    winbridge_platform::require_safe_path(&path).map_err(error)?;
    if !path.try_exists().map_err(error)? {
        if q.offset != 0
            || q.file_identity.is_some()
            || proof.is_some()
            || !matches!(
                r.phase,
                UpdateBuildPhase::Intent | UpdateBuildPhase::Completed
            )
        {
            return Err("Expected compiler output file missing; cursor/proof retained".into());
        }
        page.output_error = Some(
            if r.phase == UpdateBuildPhase::Intent {
                "Capture not created yet"
            } else {
                "Compiler capture was not created; inspect build failure"
            }
            .into(),
        );
        return Ok(page);
    }
    let mut f = open(&path)?;
    let identity = winbridge_platform::file_identity(&f).map_err(error)?;
    if q.file_identity
        .as_ref()
        .is_some_and(|saved| saved != &identity)
    {
        return Err("Compiler output file replaced; cursor cannot be rebound".into());
    }
    let size = f.metadata().map_err(error)?.len();
    if size > crate::build::MAX_OUTPUT_BYTES || q.offset > size {
        return Err(
            "Compiler output truncated/oversized or cursor is ahead; explicit inspection required"
                .into(),
        );
    }
    let wanted = (size - q.offset).min(u64::from(q.limit_bytes)) as usize;
    if let Some(proof) = proof {
        if size != proof.retained_bytes {
            return Err("Compiler output size differs from retained proof".into());
        }
        let mut digest = Sha256::new();
        let mut remaining = size;
        let mut buf = [0u8; 16384];
        while remaining > 0 {
            let count = (remaining as usize).min(buf.len());
            f.read_exact(&mut buf[..count]).map_err(error)?;
            digest.update(&buf[..count]);
            // The returned bytes participate in this exact verified hash pass.
            // A second seek/read could observe a same-size rewrite after hashing.
            let start = size - remaining;
            let first = q.offset.max(start);
            let last = (q.offset + wanted as u64).min(start + count as u64);
            if first < last {
                page.bytes
                    .extend_from_slice(&buf[(first - start) as usize..(last - start) as usize]);
            }
            remaining -= count as u64;
        }
        if format!("{:x}", digest.finalize()) != proof.sha256 {
            return Err("Compiler output SHA-256 differs from retained proof".into());
        }
        page.proof_verified = true;
        page.capture_complete = r.phase == UpdateBuildPhase::Completed && proof.error.is_none();
    }
    if proof.is_none() {
        f.seek(SeekFrom::Start(q.offset)).map_err(error)?;
        page.bytes.resize(wanted, 0);
        f.read_exact(&mut page.bytes).map_err(error)?;
    }
    let after = f.metadata().map_err(error)?.len();
    if after < size
        || proof.is_some() && after != size
        || winbridge_platform::file_identity(&open(&path)?).map_err(error)? != identity
    {
        return Err("Compiler output changed during observation; retain original cursor".into());
    }
    page.available_bytes = size;
    page.file_identity = Some(identity);
    page.next_offset = q.offset + page.bytes.len() as u64;
    page.has_more = page.next_offset < size;
    Ok(page)
}
