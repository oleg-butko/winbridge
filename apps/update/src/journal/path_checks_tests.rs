//! Real metadata/open races for optional SQLite sidecars; no DB effect retries.
use super::*;
#[test]
fn removed_sidecar_is_checked_again_without_requiring_it_to_exist() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-wal");
    fs::write(&path, b"wal").unwrap();
    let mut calls = 0;
    safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            assert!(fs::symlink_metadata(p)?.is_file());
            fs::remove_file(p)?;
            winbridge_platform::require_single_link(p)
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(!path.exists());
}
#[test]
fn sidecar_replacement_after_not_found_still_rejects_hardlinks() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-shm");
    let target = root.path().join("unrelated");
    fs::write(&path, b"old").unwrap();
    fs::write(&target, b"preserve").unwrap();
    let mut calls = 0;
    let error = safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            assert!(fs::symlink_metadata(p)?.is_file());
            fs::remove_file(p)?;
            let missing = winbridge_platform::require_single_link(p).unwrap_err();
            fs::hard_link(&target, p)?;
            Err(missing)
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap_err();
    assert_eq!(calls, 2);
    assert!(error.message.contains("Hard-linked"));
    assert!(error.message.contains("updates.sqlite-shm"));
    assert_eq!(fs::read(target).unwrap(), b"preserve");
}
#[test]
fn unsafe_existing_sidecar_is_not_retried_or_ignored() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-wal");
    let target = root.path().join("target");
    fs::write(&target, b"preserve").unwrap();
    fs::hard_link(&target, &path).unwrap();
    let mut calls = 0;
    assert!(safe_with(&path, |p| {
        calls += 1;
        winbridge_platform::require_safe_path(p)
    })
    .is_err());
    assert_eq!(calls, 1);
    assert_eq!(fs::read(target).unwrap(), b"preserve");
}
#[cfg(unix)]
#[test]
fn sidecar_replaced_by_symlink_after_not_found_still_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-wal");
    let target = root.path().join("target");
    fs::write(&path, b"old").unwrap();
    fs::write(&target, b"preserve").unwrap();
    let mut calls = 0;
    let error = safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            fs::remove_file(p)?;
            let missing = winbridge_platform::require_single_link(p).unwrap_err();
            std::os::unix::fs::symlink(&target, p)?;
            Err(missing)
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap_err();
    assert!(error.message.contains("Linked/reparse"));
    assert_eq!(calls, 2);
    assert_eq!(fs::read(target).unwrap(), b"preserve");
}
