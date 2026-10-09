use super::*;

/// Git object IDs select candidate reuse, but capture still hashes actual target bytes.
pub(super) fn base_objects(
    root: &Path,
    base: &SnapshotManifest,
) -> Result<HashMap<String, SnapshotFile>> {
    let SnapshotSource::GitCommit { commit } = &base.source;
    let tree = git_output(
        root,
        &["ls-tree", "-rz", "--full-tree", commit],
        4 * 1024 * 1024,
    )?;
    let files: HashMap<_, _> = base.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut objects = HashMap::new();
    for entry in tree.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry).map_err(error)?;
        let (header, path) = entry.split_once('\t').ok_or("Invalid base Git tree")?;
        let fields: Vec<_> = header.split(' ').collect();
        if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
            return Err("Invalid base Git tree entry; publish a full snapshot".into());
        }
        let file = files
            .get(path)
            .ok_or("Base manifest omits Git files; publish a full snapshot")?;
        objects
            .entry(fields[2].into())
            .or_insert_with(|| (*file).clone());
    }
    Ok(objects)
}

pub(super) fn copy_base(
    workspace: &Path,
    manifest: &SnapshotManifest,
    destination: &Path,
    progress: &mut impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<()> {
    let Some(delta) = &manifest.delta else {
        return Ok(());
    };
    progress("reuse_base", 0, 0)?;
    let (base, _) = local_metadata(
        workspace,
        &manifest.project_id,
        &delta.snapshot_id,
        Some(&delta.manifest_sha256),
    )
    .map_err(|e| {
        format!(
            "Delta base {} unavailable or invalid: {e}; import the full snapshot explicitly",
            delta.snapshot_id
        )
    })?;
    let base_root = generation_path(workspace, &delta.snapshot_id)?;
    let base_files: HashMap<_, _> = base.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let targets: HashMap<_, _> = manifest
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let mut bytes = 0;
    for (index, reused) in delta.reused_files.iter().enumerate() {
        let target = targets
            .get(reused.path.as_str())
            .ok_or("Delta target missing")?;
        let source = base_files.get(reused.base_path.as_str()).ok_or(
            "Delta base file absent from pinned manifest; import the full snapshot explicitly",
        )?;
        if target.bytes != source.bytes || target.sha256 != source.sha256 {
            return Err("Delta base file does not match target SHA-256/size; import the full snapshot explicitly".into());
        }
        let source_path = base_root.join(&source.path);
        safe_path(&source_path)?;
        let destination_path = destination.join(&target.path);
        create_directory(
            destination_path
                .parent()
                .ok_or("Delta file parent missing")?,
        )?;
        let mut input = File::open(&source_path).map_err(error)?;
        let mut output = create_file(&destination_path)?;
        let mut checkpoint_failed = false;
        transfer(
            &mut input,
            &mut output,
            target.bytes,
            Some(&target.sha256),
            |n| {
                let result = progress("reuse_base", index as u64, bytes + n);
                checkpoint_failed = result.is_err();
                result
            },
        )
        .map_err(|e| {
            if checkpoint_failed {
                return e;
            }
            format!(
                "Delta base file {} failed verification: {e}; import the full snapshot explicitly",
                source.path
            )
        })?;
        output.sync_all().map_err(error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &destination_path,
                fs::Permissions::from_mode(if target.executable { 0o755 } else { 0o644 }),
            )
            .map_err(error)?;
        }
        bytes += target.bytes;
        progress("reuse_base", index as u64 + 1, bytes)?;
    }
    Ok(())
}
