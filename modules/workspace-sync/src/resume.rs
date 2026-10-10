//! Manifest-pinned blocks. Incomplete data belongs to one operation, never the cache.
use super::*;

pub(super) fn describe(reader: &mut File, bytes: u64) -> Result<(String, SnapshotPayloadChunks)> {
    let mut remaining = bytes;
    let mut whole = Sha256::new();
    let mut hashes = Vec::new();
    let mut buffer = vec![0; PAYLOAD_CHUNK_BYTES as usize];
    while remaining > 0 {
        let size = remaining.min(PAYLOAD_CHUNK_BYTES) as usize;
        reader.read_exact(&mut buffer[..size]).map_err(error)?;
        whole.update(&buffer[..size]);
        hashes.push(sha256(&buffer[..size]));
        remaining -= size as u64;
    }
    if reader.read(&mut [0; 1]).map_err(error)? != 0 {
        return Err("Payload exceeds declared size".into());
    }
    Ok((
        format!("{:x}", whole.finalize()),
        SnapshotPayloadChunks {
            chunk_bytes: PAYLOAD_CHUNK_BYTES,
            sha256: hashes,
        },
    ))
}

struct HashedWriter<'a> {
    output: &'a mut File,
    hash: Sha256,
}
impl Write for HashedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.output.write(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

pub(super) fn copy(
    local: (&Path, &Path),
    shared: &Path,
    manifest: &SnapshotManifest,
    ready: &SnapshotReady,
    output: &mut File,
    progress: &mut impl FnMut(&str, u64, u64) -> Result<()>,
    on_transfer: &mut impl FnMut(&SnapshotTransfer) -> Result<()>,
) -> Result<()> {
    let (root, stage) = local;
    let chunks = manifest
        .payload_chunks
        .as_ref()
        .ok_or("Chunk metadata missing")?;
    let cache = root
        .join(".winbridge/payload-cache")
        .join(&ready.manifest_sha256);
    create_directory(&cache)?;
    let mut summary = SnapshotTransfer {
        payload_bytes: manifest.payload_bytes,
        chunk_bytes: chunks.chunk_bytes,
        reused_bytes: 0,
        shared_bytes: 0,
    };
    on_transfer(&summary)?;
    let mut input: Option<File> = None;
    let mut writer = HashedWriter {
        output,
        hash: Sha256::new(),
    };
    for (index, expected_hash) in chunks.sha256.iter().enumerate() {
        let offset = index as u64 * chunks.chunk_bytes;
        let size = (manifest.payload_bytes - offset).min(chunks.chunk_bytes);
        let cached = cache.join(format!("{index:04}.chunk"));
        safe_path(&cached)?;
        if cached.try_exists().map_err(error)? {
            let mut file = File::open(&cached).map_err(error)?;
            let mut checkpoint_failed = false;
            transfer(&mut file, &mut writer, size, Some(expected_hash), |n| {
                let result = progress("verify_cached_payload", 0, offset + n);
                checkpoint_failed = result.is_err();
                result
            }).map_err(|e| if checkpoint_failed { e } else {
                format!("Cached payload block {index}: {e}; cached data preserved. Publish/import the full package explicitly to bypass this cache")
            })?;
            summary.reused_bytes += size;
        } else {
            progress("copy_payload", 0, offset)?;
            if input.is_none() {
                safe_path(shared)?;
                input = Some(
                    File::open(shared)
                        .map_err(|e| format!("Shared payload {}: {e}", shared.display()))?,
                );
            }
            let input = input.as_mut().ok_or("Shared payload not open")?;
            input.seek(SeekFrom::Start(offset)).map_err(error)?;
            let partial = stage.join(format!("block-{index:04}.part"));
            let mut block = create_file(&partial)?;
            transfer(
                &mut input.take(size),
                &mut block,
                size,
                Some(expected_hash),
                |n| progress("copy_payload", 0, offset + n),
            )
            .map_err(|e| format!("Shared payload block {index}: {e}"))?;
            block.sync_all().map_err(error)?;
            block.rewind().map_err(error)?;
            transfer(&mut block, &mut writer, size, Some(expected_hash), |n| {
                progress("assemble_payload", 0, offset + n)
            })?;
            drop(block);
            safe_path(&cached)?;
            if cached.try_exists().map_err(error)? {
                return Err(
                    "Payload cache block appeared during transfer; existing data preserved".into(),
                );
            }
            // Closed, synced and hash-checked file; same local disk. No partial cache entry.
            fs::rename(&partial, &cached).map_err(error)?;
            summary.shared_bytes += size;
        }
        on_transfer(&summary)?;
        progress("payload_checkpoint", 0, offset + size)?;
    }
    if let Some(mut input) = input {
        progress("verify_payload", 0, manifest.payload_bytes)?;
        input
            .seek(SeekFrom::Start(manifest.payload_bytes))
            .map_err(error)?;
        if input.read(&mut [0; 1]).map_err(error)? != 0 {
            return Err("Shared payload exceeds declared size".into());
        }
    }
    if format!("{:x}", writer.hash.finalize()) != manifest.payload_sha256 {
        return Err("Assembled payload SHA-256 mismatch; generation remains unbuildable".into());
    }
    progress("verify_payload", 0, manifest.payload_bytes)
}
