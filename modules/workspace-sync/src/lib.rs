//! Git capture and verified shared-folder delivery. No processes run from shared.
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use winbridge_core::{config::DeliverySettings, snapshot::*};
use winbridge_protocol::*;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};
pub type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Reject symlinks, Windows junctions/reparse points and special filesystem entries.
/// The local developer account remains trusted; this is not a hostile-user sandbox.
pub fn safe_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("Filesystem path must be absolute".into());
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                let mut linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    linked |= metadata.file_attributes() & 0x400 != 0;
                }
                #[cfg(not(windows))]
                {
                    let _ = &mut linked;
                }
                if linked || !(metadata.is_dir() || metadata.is_file()) {
                    return Err(format!(
                        "Linked/reparse/special path refused: {}",
                        ancestor.display()
                    ));
                }
                if metadata.is_file() {
                    winbridge_platform::require_single_link(ancestor).map_err(error)?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", ancestor.display())),
        }
    }
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    safe_path(path)?;
    if !path.is_dir() {
        return Err(format!("Directory unavailable: {}", path.display()));
    }
    Ok(())
}
fn create_directory(path: &Path) -> Result<()> {
    safe_path(path)?;
    fs::create_dir_all(path).map_err(error)?;
    directory(path)
}
fn create_file(path: &Path) -> Result<File> {
    safe_path(path)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(error)
}
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    safe_path(path)?;
    let mut result = Vec::new();
    File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(limit + 1)
        .read_to_end(&mut result)
        .map_err(error)?;
    if result.len() as u64 > limit {
        return Err(format!("{} exceeds byte limit", path.display()));
    }
    Ok(result)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create_file(path)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)
}
pub fn validate_delivery(delivery: &DeliverySettings) -> Result<()> {
    if delivery.adapter != "shared-folder" || !delivery.root.is_absolute() {
        return Err("delivery requires adapter shared-folder and absolute root".into());
    }
    validate_ids(&delivery.namespace, "delivery")
}
pub fn validate_ids(project: &str, snapshot: &str) -> Result<()> {
    for id in [project, snapshot] {
        if !winbridge_core::config::valid_id(id) {
            return Err(
                "Project/snapshot IDs require 1-80 ASCII letters, digits, '-' or '_'".into(),
            );
        }
        winbridge_core::config::validate_relative(id)?;
    }
    Ok(())
}
pub fn validate_submission(request: &ImportSubmission) -> Result<()> {
    validate_ids(&request.project_id, &request.snapshot_id)?;
    if request.protocol_version != PROTOCOL_VERSION
        || request.request_id.is_empty()
        || request.request_id.len() > 128
        || request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 128
        || request.request_id.chars().any(char::is_control)
        || request.idempotency_key.chars().any(char::is_control)
        || !(100..=1_800_000).contains(&request.timeout_ms)
    {
        return Err("Import requires protocol 1, bounded requestId/idempotencyKey and timeoutMs 100-1800000".into());
    }
    if request.manifest_sha256.len() != 64
        || !request
            .manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("manifestSha256 must be 64 lowercase hex characters".into());
    }
    Ok(())
}
pub fn package_path(delivery: &DeliverySettings, project: &str, snapshot: &str) -> Result<PathBuf> {
    validate_delivery(delivery)?;
    validate_ids(project, snapshot)?;
    Ok(delivery
        .root
        .join(&delivery.namespace)
        .join("deliveries")
        .join(project)
        .join(snapshot))
}
pub fn generation_path(root: &Path, snapshot: &str) -> Result<PathBuf> {
    validate_ids("project", snapshot)?;
    Ok(root.join("snapshots").join(snapshot))
}
pub fn metadata(
    delivery: &DeliverySettings,
    project: &str,
    snapshot: &str,
    expected: Option<&str>,
) -> Result<(SnapshotManifest, Vec<u8>, SnapshotReady)> {
    let path = package_path(delivery, project, snapshot)?;
    directory(&path)?;
    let ready = parse_ready(&read_bounded(
        &path.join("ready.json"),
        MAX_READY_BYTES as u64,
    )?)?;
    let bytes = read_bounded(&path.join("manifest.json"), MAX_MANIFEST_BYTES as u64)?;
    let digest = sha256(&bytes);
    if digest != ready.manifest_sha256 || expected.is_some_and(|e| e != digest) {
        return Err("Manifest SHA-256 mismatch; delivery changed or is incomplete".into());
    }
    let manifest = parse_manifest(&bytes)?;
    validate_ready_matches_manifest(&ready, &manifest)?;
    if manifest.project_id != project || manifest.snapshot_id != snapshot {
        return Err("Package project/snapshot IDs do not match requested directory".into());
    }
    Ok((manifest, bytes, ready))
}

/// Copy with a strict length bound, hashing actual bytes; progress may cancel at every 64 KiB.
fn transfer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected_bytes: u64,
    expected_hash: Option<&str>,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<String> {
    let mut bytes = [0u8; 65536];
    let mut count = 0u64;
    let mut hash = Sha256::new();
    loop {
        progress(count)?;
        let n = reader.read(&mut bytes).map_err(error)?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n as u64).ok_or("Byte count overflow")?;
        if count > expected_bytes {
            return Err("Payload/file exceeds declared size".into());
        }
        hash.update(&bytes[..n]);
        writer.write_all(&bytes[..n]).map_err(error)?;
    }
    let digest = format!("{:x}", hash.finalize());
    if count != expected_bytes || expected_hash.is_some_and(|h| h != digest) {
        return Err("Actual file/payload size or SHA-256 mismatch".into());
    }
    progress(count)?;
    Ok(digest)
}

// Bound central-directory allocation before calling the ZIP parser. ZIP64 is unnecessary
// with our 512 MiB / 10,000-entry limits and is deliberately excluded from schema 1.
fn zip_directory_bound(file: &mut File) -> Result<()> {
    let length = file.metadata().map_err(error)?.len();
    if length > MAX_PAYLOAD_BYTES {
        return Err("ZIP exceeds payload limit".into());
    }
    let tail_bytes = length.min(65557);
    file.seek(SeekFrom::End(-(tail_bytes as i64)))
        .map_err(error)?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).map_err(error)?;
    let index = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..i + 4] == *b"PK\x05\x06"
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or("Missing ZIP end record")?;
    let u16_at = |n| u16::from_le_bytes([tail[index + n], tail[index + n + 1]]);
    let u32_at = |n| u32::from_le_bytes(tail[index + n..index + n + 4].try_into().unwrap());
    if u16_at(4) != 0
        || u16_at(6) != 0
        || u16_at(8) != u16_at(10)
        || u16_at(10) as usize > MAX_FILES
        || u32_at(12) > 4 * 1024 * 1024
        || u32_at(16) as u64 + u32_at(12) as u64 != length - tail_bytes + index as u64
    {
        return Err("ZIP64, multipart or oversized ZIP directory refused".into());
    }
    file.rewind().map_err(error)
}
fn extract(
    payload: &Path,
    manifest: &SnapshotManifest,
    root: &Path,
    progress: &mut impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<()> {
    let mut file = File::open(payload).map_err(error)?;
    zip_directory_bound(&mut file)?;
    let mut archive = ZipArchive::new(file).map_err(error)?;
    if archive.len() != manifest.files.len() {
        return Err(
            "ZIP entry count differs from manifest; extra entries/directories are refused".into(),
        );
    }
    let expected: HashMap<_, _> = manifest
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let mut seen = HashSet::new();
    let mut completed = 0;
    let mut bytes = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(error)?;
        let name =
            std::str::from_utf8(entry.name_raw()).map_err(|_| "ZIP entry names must be UTF-8")?;
        let record = expected
            .get(name)
            .ok_or("ZIP contains a path absent from the manifest")?;
        if !seen.insert(name.to_string())
            || entry.is_dir()
            || entry.size() != record.bytes
            || entry.compression() != zip::CompressionMethod::Stored
            || entry
                .unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
        {
            return Err("ZIP duplicate, link, special file, compression or size refused".into());
        }
        let path = root.join(&record.path);
        create_directory(path.parent().ok_or("Missing file parent")?)?;
        let mut output = create_file(&path)?;
        transfer(
            &mut entry,
            &mut output,
            record.bytes,
            Some(&record.sha256),
            |n| progress("extract", completed, bytes + n),
        )?;
        output.sync_all().map_err(error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if record.executable { 0o755 } else { 0o644 }),
            )
            .map_err(error)?;
        }
        completed += 1;
        bytes += record.bytes;
        progress("extract", completed, bytes)?;
    }
    Ok(())
}
fn verify_tree(
    root: &Path,
    manifest: &SnapshotManifest,
    progress: &mut impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<()> {
    directory(root)?;
    let mut allowed_files: HashSet<String> =
        manifest.files.iter().map(|f| f.path.clone()).collect();
    allowed_files.extend([
        ".winbridge/manifest.json".into(),
        ".winbridge/verified.json".into(),
    ]);
    let mut allowed_dirs = HashSet::from([".winbridge".to_string()]);
    for record in &manifest.files {
        let mut parent = Path::new(&record.path).parent();
        while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
            allowed_dirs.insert(path.to_string_lossy().replace('\\', "/"));
            parent = path.parent();
        }
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        directory(&dir)?;
        for entry in fs::read_dir(&dir).map_err(error)? {
            let entry = entry.map_err(error)?;
            let path = entry.path();
            safe_path(&path)?;
            let relative = path
                .strip_prefix(root)
                .map_err(error)?
                .to_str()
                .ok_or("Local filename is not UTF-8")?
                .replace('\\', "/");
            if path.is_dir() {
                if !allowed_dirs.contains(&relative) {
                    return Err(
                        "Local generation contains an unexpected directory; sources are immutable"
                            .into(),
                    );
                }
                pending.push(path);
            } else if !allowed_files.contains(&relative) {
                return Err("Local generation contains an unexpected file; write build outputs outside snapshots".into());
            }
        }
    }
    let mut bytes = 0;
    for (i, record) in manifest.files.iter().enumerate() {
        let path = root.join(&record.path);
        safe_path(&path)?;
        let mut input = File::open(&path).map_err(error)?;
        transfer(
            &mut input,
            &mut std::io::sink(),
            record.bytes,
            Some(&record.sha256),
            |n| progress("verify_local", i as u64, bytes + n),
        )?;
        bytes += record.bytes;
    }
    progress("verify_local", manifest.files.len() as u64, bytes)
}
pub fn local_metadata(
    root: &Path,
    project: &str,
    snapshot: &str,
    expected: Option<&str>,
) -> Result<(SnapshotManifest, SnapshotReady)> {
    let generation = generation_path(root, snapshot)?;
    directory(&generation)?;
    let bytes = read_bounded(
        &generation.join(".winbridge/manifest.json"),
        MAX_MANIFEST_BYTES as u64,
    )?;
    let ready = parse_ready(&read_bounded(
        &generation.join(".winbridge/verified.json"),
        MAX_READY_BYTES as u64,
    )?)?;
    if ready.manifest_sha256 != sha256(&bytes)
        || expected.is_some_and(|h| h != ready.manifest_sha256)
    {
        return Err("Local verified manifest hash mismatch".into());
    }
    let manifest = parse_manifest(&bytes)?;
    validate_ready_matches_manifest(&ready, &manifest)?;
    if manifest.project_id != project || manifest.snapshot_id != snapshot {
        return Err("Local generation IDs mismatch".into());
    }
    Ok((manifest, ready))
}
pub fn verified_generation(
    root: &Path,
    project: &str,
    snapshot: &str,
    expected: Option<&str>,
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<SnapshotManifest> {
    let (manifest, _) = local_metadata(root, project, snapshot, expected)?;
    verify_tree(&generation_path(root, snapshot)?, &manifest, &mut progress)?;
    Ok(manifest)
}

/// Stage on the local disk, publish an immutable generation only after byte verification.
/// Interrupted staging remains as owned evidence; it is never a buildable generation.
pub fn import(
    delivery: &DeliverySettings,
    root: &Path,
    request: &ImportSubmission,
    operation_id: &str,
    mut progress: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<SnapshotManifest> {
    validate_submission(request)?;
    validate_ids("operation", operation_id)?;
    directory(root)?;
    winbridge_platform::require_local_disk(root).map_err(error)?;
    let final_path = generation_path(root, &request.snapshot_id)?;
    if final_path.exists() {
        return verified_generation(
            root,
            &request.project_id,
            &request.snapshot_id,
            Some(&request.manifest_sha256),
            progress,
        );
    }
    let (manifest, manifest_bytes, ready) = metadata(
        delivery,
        &request.project_id,
        &request.snapshot_id,
        Some(&request.manifest_sha256),
    )?;
    progress("copy_payload", 0, 0)?;
    let stage = root.join(".winbridge/imports").join(operation_id);
    safe_path(&stage)?;
    if stage.exists() {
        return Err("Import staging already exists; submit a new key after inspecting the interrupted operation".into());
    }
    create_directory(&stage)?;
    let payload_path = stage.join("payload.zip");
    let shared =
        package_path(delivery, &request.project_id, &request.snapshot_id)?.join("payload.zip");
    safe_path(&shared)?;
    let mut input =
        File::open(&shared).map_err(|e| format!("Shared payload {}: {e}", shared.display()))?;
    let mut payload = create_file(&payload_path)?;
    transfer(
        &mut input,
        &mut payload,
        manifest.payload_bytes,
        Some(&manifest.payload_sha256),
        |n| progress("copy_payload", 0, n),
    )?;
    payload.sync_all().map_err(error)?;
    drop(payload);
    let source = stage.join("source");
    create_directory(&source)?;
    extract(&payload_path, &manifest, &source, &mut progress)?;
    verify_tree(&source, &manifest, &mut progress)?;
    create_directory(&source.join(".winbridge"))?;
    write_new(&source.join(".winbridge/manifest.json"), &manifest_bytes)?;
    write_new(
        &source.join(".winbridge/verified.json"),
        &serde_json::to_vec_pretty(&ready).map_err(error)?,
    )?;
    create_directory(final_path.parent().ok_or("Missing generation parent")?)?;
    progress(
        "publish_local",
        manifest.files.len() as u64,
        manifest.total_bytes,
    )?;
    safe_path(&final_path)?;
    safe_path(&source)?;
    if final_path.exists() {
        return Err("Generation appeared during import; existing directory preserved".into());
    }
    fs::rename(&source, &final_path).map_err(error)?;
    Ok(manifest)
}

fn git_output(root: &Path, args: &[&str], limit: u64) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Git unavailable: {e}"))?;
    let mut bytes = Vec::new();
    let result = child
        .stdout
        .take()
        .ok_or("Git stdout missing")?
        .take(limit + 1)
        .read_to_end(&mut bytes);
    if result.is_err() || bytes.len() as u64 > limit {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Git output exceeds snapshot limit or cannot be read".into());
    }
    if !child.wait().map_err(error)?.success() {
        return Err(
            "Git capture failed; check the registered repository and requested commit".into(),
        );
    }
    result.map_err(error)?;
    Ok(bytes)
}

/// Read only committed objects, never mutable working-tree file contents.
pub fn publish(
    delivery: &DeliverySettings,
    source_root: &Path,
    local_state: &Path,
    project: &str,
    revision: &str,
) -> Result<SnapshotReady> {
    validate_delivery(delivery)?;
    validate_ids(project, "snapshot")?;
    directory(source_root)?;
    directory(&delivery.root)?;
    let top = String::from_utf8(git_output(
        source_root,
        &["rev-parse", "--show-toplevel"],
        8192,
    )?)
    .map_err(error)?;
    if fs::canonicalize(top.trim()).map_err(error)?
        != fs::canonicalize(source_root).map_err(error)?
    {
        return Err("Registered sourceRoot must be the Git repository root".into());
    }
    winbridge_platform::require_local_disk(local_state).map_err(error)?;
    if revision.is_empty()
        || revision.len() > 200
        || revision.starts_with('-')
        || revision.chars().any(|c| c.is_control())
    {
        return Err("Invalid Git revision".into());
    }
    let commit = String::from_utf8(git_output(
        source_root,
        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        128,
    )?)
    .map_err(error)?
    .trim()
    .to_string();
    let snapshot = format!("s-{commit}");
    let mut manifest = SnapshotManifest {
        schema_version: 1,
        project_id: project.into(),
        snapshot_id: snapshot.clone(),
        source: SnapshotSource::GitCommit {
            commit: commit.clone(),
        },
        payload_bytes: 1,
        payload_sha256: "0".repeat(64),
        total_bytes: 0,
        files: Vec::new(),
    };
    let tree = git_output(
        source_root,
        &["ls-tree", "-rz", "--full-tree", &commit],
        4 * 1024 * 1024,
    )?;
    let mut objects = Vec::new();
    for item in tree.split(|&c| c == 0).filter(|e| !e.is_empty()) {
        let item = std::str::from_utf8(item).map_err(|_| "Git paths must be UTF-8")?;
        let (head, path) = item.split_once('\t').ok_or("Invalid Git tree entry")?;
        let fields: Vec<_> = head.split(' ').collect();
        if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
            return Err(
                "Git symlinks, submodules and special entries are excluded from commit snapshots"
                    .into(),
            );
        }
        manifest.files.push(SnapshotFile {
            path: path.into(),
            bytes: 0,
            sha256: "0".repeat(64),
            executable: fields[0] == "100755",
        });
        objects.push(fields[2].to_string());
    }
    validate_manifest(&manifest)?; // Check paths before any content is packed.
    create_directory(&local_state.join("capture"))?;
    let stage = local_state
        .join("capture")
        .join(uuid::Uuid::new_v4().to_string());
    create_directory(&stage)?;
    let path = stage.join("payload.zip");
    let mut archive = ZipWriter::new(create_file(&path)?);
    for (record, object) in manifest.files.iter_mut().zip(objects) {
        let size = String::from_utf8(git_output(source_root, &["cat-file", "-s", &object], 32)?)
            .map_err(error)?
            .trim()
            .parse::<u64>()
            .map_err(error)?;
        manifest.total_bytes = manifest
            .total_bytes
            .checked_add(size)
            .ok_or("Git tree size overflow")?;
        if manifest.total_bytes > MAX_UNPACKED_BYTES {
            return Err("Git source exceeds unpacked limit".into());
        }
        record.bytes = size;
        archive
            .start_file(
                &record.path,
                SimpleFileOptions::default()
                    .last_modified_time(zip::DateTime::default())
                    .compression_method(zip::CompressionMethod::Stored)
                    .unix_permissions(if record.executable { 0o755 } else { 0o644 }),
            )
            .map_err(error)?;
        let mut child = Command::new("git")
            .arg("--no-replace-objects")
            .arg("-C")
            .arg(source_root)
            .args(["cat-file", "blob", &object])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(error)?;
        let result = transfer(
            &mut child.stdout.take().ok_or("Missing Git stdout")?,
            &mut archive,
            size,
            None,
            |_| Ok(()),
        );
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait().map_err(error)?;
        record.sha256 = result?;
        if !status.success() {
            return Err("Git blob capture failed".into());
        }
        if archive
            .get_ref()
            .ok_or("ZIP writer closed")?
            .metadata()
            .map_err(error)?
            .len()
            > MAX_PAYLOAD_BYTES
        {
            return Err("ZIP payload exceeds limit".into());
        }
    }
    let mut payload = archive.finish().map_err(error)?;
    payload.sync_all().map_err(error)?;
    manifest.payload_bytes = payload.metadata().map_err(error)?.len();
    payload.rewind().map_err(error)?;
    manifest.payload_sha256 = transfer(
        &mut payload,
        &mut std::io::sink(),
        manifest.payload_bytes,
        None,
        |_| Ok(()),
    )?;
    validate_manifest(&manifest)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(error)?;
    parse_manifest(&manifest_bytes)?;
    let ready = SnapshotReady {
        schema_version: 1,
        project_id: project.into(),
        snapshot_id: snapshot.clone(),
        manifest_sha256: sha256(&manifest_bytes),
        payload_sha256: manifest.payload_sha256.clone(),
        payload_bytes: manifest.payload_bytes,
    };
    let final_path = package_path(delivery, project, &snapshot)?;
    if final_path.exists() {
        let (_, _, existing) =
            metadata(delivery, project, &snapshot, Some(&ready.manifest_sha256))?;
        safe_path(&final_path.join("payload.zip"))?;
        let mut input = File::open(final_path.join("payload.zip")).map_err(error)?;
        transfer(
            &mut input,
            &mut std::io::sink(),
            manifest.payload_bytes,
            Some(&manifest.payload_sha256),
            |_| Ok(()),
        )?;
        return Ok(existing);
    }
    create_directory(final_path.parent().ok_or("Missing shared package parent")?)?;
    safe_path(&final_path)?;
    // HGFS can reject rename of a nonempty directory. Claim a new package directory,
    // then rename verified individual .part files. ready.json remains the visibility barrier.
    fs::create_dir(&final_path).map_err(|e| {
        format!(
            "Cannot claim new delivery {}: {e}; existing package preserved",
            final_path.display()
        )
    })?;
    payload.rewind().map_err(error)?;
    let partial = final_path.join("payload.zip.part");
    let mut shared_payload = create_file(&partial)?;
    transfer(
        &mut payload,
        &mut shared_payload,
        manifest.payload_bytes,
        Some(&manifest.payload_sha256),
        |_| Ok(()),
    )?;
    shared_payload.sync_all().map_err(error)?;
    drop(shared_payload);
    safe_path(&partial)?;
    let mut copied = File::open(&partial).map_err(error)?;
    transfer(
        &mut copied,
        &mut std::io::sink(),
        manifest.payload_bytes,
        Some(&manifest.payload_sha256),
        |_| Ok(()),
    )?;
    drop(copied);
    fs::rename(&partial, final_path.join("payload.zip"))
        .map_err(|e| format!("Shared payload file rename: {e}"))?;
    let partial_manifest = final_path.join("manifest.json.part");
    write_new(&partial_manifest, &manifest_bytes)?;
    if read_bounded(&partial_manifest, MAX_MANIFEST_BYTES as u64)? != manifest_bytes {
        return Err("Shared manifest readback mismatch".into());
    }
    fs::rename(&partial_manifest, final_path.join("manifest.json"))
        .map_err(|e| format!("Shared manifest file rename: {e}"))?;
    write_new(
        &final_path.join("ready.json"),
        &serde_json::to_vec_pretty(&ready).map_err(error)?,
    )?;
    metadata(delivery, project, &snapshot, Some(&ready.manifest_sha256))?;
    Ok(ready)
}
