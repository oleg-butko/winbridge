//! Bounded ZIP verification and create-only staging. Never extracts into live sources.
use crate::{error, sha256, verify_source, VerifiedSource};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use winbridge_core::update::{
    parse_source_manifest, valid_hash, MAX_SOURCE_FILES, MAX_SOURCE_MANIFEST_BYTES,
    SOURCE_MANIFEST_NAME,
};
use winbridge_protocol::update::UpdateSourcePin;
use zip::{CompressionMethod, ZipArchive};

type Result<T> = std::result::Result<T, String>;
pub const MAX_UPDATE_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DIRECTORY_BYTES: u32 = 8 * 1024 * 1024;
const MANIFEST_ENTRY: &str = "winbridge/winbridge-source-manifest.json";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    pub schema_version: u16,
    pub kind: &'static str,
    pub read_only: bool,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub source: VerifiedSource,
}

fn open_archive(path: &Path, expected_sha256: &str) -> Result<(File, u64)> {
    if !valid_hash(expected_sha256, &[64]) {
        return Err("Expected ZIP SHA-256 must use 64 lowercase hex characters".into());
    }
    winbridge_platform::require_safe_path(path).map_err(error)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Hold a read-only shared handle during parsing; deny writes/deletion on Windows.
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options.open(path).map_err(error)?;
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.len() > MAX_UPDATE_ARCHIVE_BYTES {
        return Err("Update ZIP must be a regular file of at most 512 MiB".into());
    }
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(error)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > MAX_UPDATE_ARCHIVE_BYTES {
            return Err("Update ZIP exceeds byte limit".into());
        }
        hash.update(&buffer[..read]);
    }
    if count != metadata.len() || format!("{:x}", hash.finalize()) != expected_sha256 {
        return Err("Pinned ZIP size/SHA-256 mismatch".into());
    }
    file.rewind().map_err(error)?;
    Ok((file, count))
}

// Bound allocations before invoking ZipArchive. Source delivery does not use ZIP64,
// multipart archives, executable prefixes or trailing data. EOCD count is retained
// independently: some ZIP libraries collapse entries with duplicate names.
fn directory_bound(file: &mut File) -> Result<(usize, u64)> {
    let length = file.metadata().map_err(error)?.len();
    let tail_bytes = length.min(65557);
    file.seek(SeekFrom::End(-(tail_bytes as i64)))
        .map_err(error)?;
    let mut tail = Vec::new();
    (&mut *file)
        .take(65558)
        .read_to_end(&mut tail)
        .map_err(error)?;
    if tail.len() as u64 != tail_bytes {
        return Err("ZIP changed while reading its directory".into());
    }
    let index = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..i + 4] == *b"PK\x05\x06"
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or("Update ZIP end record missing")?;
    let u16_at = |n| u16::from_le_bytes([tail[index + n], tail[index + n + 1]]);
    let u32_at = |n| u32::from_le_bytes(tail[index + n..index + n + 4].try_into().unwrap());
    let count = u16_at(10) as usize;
    let directory_start = u32_at(16) as u64;
    if u16_at(4) != 0
        || u16_at(6) != 0
        || u16_at(8) != u16_at(10)
        || count == 0
        || count > MAX_SOURCE_FILES + 1
        || u32_at(12) > MAX_DIRECTORY_BYTES
        || directory_start + u32_at(12) as u64 != length - tail_bytes + index as u64
    {
        return Err("ZIP64, multipart or oversized update ZIP directory refused".into());
    }
    file.rewind().map_err(error)?;
    Ok((count, directory_start))
}

fn transfer(
    input: &mut impl Read,
    output: &mut impl Write,
    bytes: u64,
    expected_sha256: &str,
) -> Result<()> {
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        // One extra byte detects a decompressor expanding past its declared size.
        let bound = (bytes.saturating_sub(count) + 1).min(buffer.len() as u64) as usize;
        let read = input.read(&mut buffer[..bound]).map_err(error)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > bytes {
            return Err("ZIP entry exceeds declared file size".into());
        }
        hash.update(&buffer[..read]);
        output.write_all(&buffer[..read]).map_err(error)?;
    }
    if count != bytes || format!("{:x}", hash.finalize()) != expected_sha256 {
        return Err("ZIP entry size/SHA-256 mismatch".into());
    }
    Ok(())
}

fn inspect_or_extract(
    path: &Path,
    archive_sha256: &str,
    manifest_sha256: &str,
    destination: Option<&Path>,
) -> Result<ArchiveInspection> {
    if !valid_hash(manifest_sha256, &[64]) {
        return Err("Expected manifest SHA-256 must use 64 lowercase hex characters".into());
    }
    let (mut file, archive_bytes) = open_archive(path, archive_sha256)?;
    let (count, directory_start) = directory_bound(&mut file)?;
    let mut archive = ZipArchive::new(file).map_err(error)?;
    if archive.len() != count {
        return Err("Update ZIP has duplicate or missing directory entries".into());
    }
    let mut seen = BTreeSet::new();
    let mut ranges = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(error)?;
        let name = std::str::from_utf8(entry.name_raw()).map_err(|_| "ZIP names must use UTF-8")?;
        if !seen.insert(name.to_string())
            || !entry.is_file()
            || entry.is_symlink()
            || entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
            || entry
                .unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
        {
            return Err(
                "Update ZIP duplicate, directory, link, encryption or compression refused".into(),
            );
        }
        let start = entry.header_start();
        let data = entry.data_start().ok_or("ZIP entry data offset missing")?;
        let end = data
            .checked_add(entry.compressed_size())
            .ok_or("ZIP offset overflow")?;
        if start >= data || end > directory_start {
            return Err("Update ZIP data overlaps its directory".into());
        }
        ranges.push((start, end));
    }
    ranges.sort_unstable();
    if ranges.first().is_none_or(|r| r.0 != 0) || ranges.windows(2).any(|r| r[0].1 > r[1].0) {
        return Err("Update ZIP executable prefix or overlapping entries refused".into());
    }
    let manifest_bytes = {
        let mut entry = archive
            .by_name(MANIFEST_ENTRY)
            .map_err(|_| "Source manifest missing from update ZIP")?;
        if entry.size() > MAX_SOURCE_MANIFEST_BYTES {
            return Err("Source manifest exceeds byte limit".into());
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_SOURCE_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() as u64 > MAX_SOURCE_MANIFEST_BYTES
            || bytes.len() as u64 != entry.size()
            || sha256(&bytes) != manifest_sha256
        {
            return Err("Pinned ZIP source manifest size/SHA-256 mismatch".into());
        }
        bytes
    };
    let manifest = parse_source_manifest(&manifest_bytes)?;
    let expected: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|f| (format!("winbridge/{}", f.path), f))
        .collect();
    if archive.len() != expected.len() + 1
        || seen
            .iter()
            .any(|n| n != MANIFEST_ENTRY && !expected.contains_key(n))
    {
        return Err("Update ZIP contains extra paths or differs from manifest file count".into());
    }
    // No filesystem output until all metadata/manifest/path preconditions passed.
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(error)?;
        let name = std::str::from_utf8(entry.name_raw())
            .map_err(error)?
            .to_string();
        let (relative, bytes, hash) = if name == MANIFEST_ENTRY {
            (
                SOURCE_MANIFEST_NAME,
                manifest_bytes.len() as u64,
                manifest_sha256,
            )
        } else {
            let f = expected
                .get(&name)
                .ok_or("ZIP entry missing from manifest")?;
            (f.path.as_str(), f.bytes, f.sha256.as_str())
        };
        if entry.size() != bytes {
            return Err("ZIP entry declared size differs from source manifest".into());
        }
        if let Some(root) = destination {
            let output = root.join(relative);
            winbridge_platform::require_safe_path(&output).map_err(error)?;
            fs::create_dir_all(output.parent().ok_or("Missing source parent")?).map_err(error)?;
            winbridge_platform::require_safe_path(&output).map_err(error)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(error)?;
            transfer(&mut entry, &mut output, bytes, hash)?;
            output.sync_all().map_err(error)?;
        } else {
            transfer(&mut entry, &mut std::io::sink(), bytes, hash)?;
        }
    }
    // Pin the bytes again: shared input may have changed during decompression.
    open_archive(path, archive_sha256)?;
    Ok(ArchiveInspection {
        schema_version: 1,
        kind: "source_archive_verified",
        read_only: destination.is_none(),
        archive_sha256: archive_sha256.into(),
        archive_bytes,
        source: VerifiedSource {
            schema_version: 1,
            kind: "source_verified",
            read_only: true,
            source_commit: manifest.source_commit,
            manifest_sha256: manifest_sha256.into(),
            includes_built_ui: manifest.includes_built_ui,
            file_count: manifest.files.len(),
            bytes: manifest.files.iter().map(|f| f.bytes).sum(),
        },
    })
}

pub fn verify_archive(
    path: &Path,
    archive_sha256: &str,
    manifest_sha256: &str,
) -> Result<ArchiveInspection> {
    inspect_or_extract(path, archive_sha256, manifest_sha256, None)
}

/// New local staging only. A partial stage is retained and never reused or auto-deleted.
/// The caller must durably own its stage; this does not grant a lease on live `src`.
pub fn stage_archive(
    path: &Path,
    pin: &UpdateSourcePin,
    destination: &Path,
) -> Result<ArchiveInspection> {
    let inspected = verify_archive(path, &pin.archive_sha256, &pin.manifest_sha256)?;
    if inspected.source.source_commit != pin.source_commit {
        return Err("Pinned update source commit mismatch".into());
    }
    winbridge_platform::require_safe_path(destination).map_err(error)?;
    winbridge_platform::require_local_disk(destination).map_err(error)?;
    // create_dir, not create_dir_all: parent belongs to the caller; never adopt existing stage.
    fs::create_dir(destination).map_err(error)?;
    let copied = destination.join("source.zip");
    let mut input = File::open(path).map_err(error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&copied)
        .map_err(error)?;
    transfer(
        &mut input,
        &mut output,
        inspected.archive_bytes,
        &pin.archive_sha256,
    )?;
    output.sync_all().map_err(error)?;
    drop(output);
    let source = destination.join("source");
    fs::create_dir(&source).map_err(error)?;
    let result = inspect_or_extract(
        &copied,
        &pin.archive_sha256,
        &pin.manifest_sha256,
        Some(&source),
    )?;
    verify_source(&source, &pin.manifest_sha256)?;
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.join("source-ready.json"))
        .map_err(error)?;
    marker
        .write_all(&serde_json::to_vec_pretty(&result).map_err(error)?)
        .map_err(error)?;
    marker.sync_all().map_err(error)?;
    Ok(result)
}
