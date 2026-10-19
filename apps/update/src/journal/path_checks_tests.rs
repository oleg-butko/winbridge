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

#[test]
fn transient_sidecar_denial_revalidates_whole_path_after_disappearance() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-wal");
    fs::write(&path, b"wal").unwrap();
    let mut calls = 0;
    safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            fs::remove_file(p)?;
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(!path.exists());
}
#[test]
fn sidecar_denial_does_not_hide_hardlinked_replacement() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-shm");
    let target = root.path().join("canary");
    fs::write(&target, b"unchanged").unwrap();
    let mut calls = 0;
    let error = safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            fs::hard_link(&target, p)?;
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap_err();
    assert_eq!(calls, 2);
    assert!(error.message.contains("Hard-linked"));
    assert_eq!(fs::read(target).unwrap(), b"unchanged");
}
#[test]
fn persistent_sidecar_denial_is_bounded_and_required_files_are_not_retried() {
    let root = tempfile::tempdir().unwrap();
    for (name, expected) in [
        ("updates.sqlite-wal", 3),
        ("updates.sqlite-shm", 3),
        ("updates.sqlite", 1),
        ("owner.lock", 1),
        ("updates.sqlite-journal", 1),
    ] {
        let path = root.path().join(name);
        let mut calls = 0;
        let error = safe_with(&path, |_| {
            calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .unwrap_err();
        assert_eq!(calls, expected, "{name}");
        assert!(error.message.contains(name));
    }
}
#[cfg(windows)]
#[test]
fn actual_delete_pending_sidecar_is_revalidated_after_last_handle_closes() {
    use std::{
        fs::OpenOptions,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DeleteFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.sqlite-wal");
    fs::write(&path, b"wal").unwrap();
    let mut held = Some(
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&path)
            .unwrap(),
    );
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut calls = 0;
    safe_with(&path, |p| {
        calls += 1;
        if calls == 1 {
            // Match the real guard's metadata -> handle-open boundary. Deleting
            // before metadata lets Rust report an optional missing entry, which
            // is already safe and does not exercise an inaccessible open.
            assert!(fs::symlink_metadata(p)?.is_file());
            assert_ne!(unsafe { DeleteFileW(wide.as_ptr()) }, 0);
            let result = winbridge_platform::require_single_link(p);
            let error = result.as_ref().expect_err("Deleted entry must not open");
            // Windows/filesystem versions expose classic delete-pending or
            // POSIX-style removal here. Both require full path revalidation;
            // success, an unrelated I/O error or skipping the retry is a failure.
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
            ));
            eprintln!(
                "Native SQLite sidecar deletion boundary: {:?}, OS {:?}",
                error.kind(),
                error.raw_os_error()
            );
            drop(held.take());
            result
        } else {
            winbridge_platform::require_safe_path(p)
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(!path.exists());
}
