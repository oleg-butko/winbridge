#![cfg(unix)]
use std::{fs, os::unix::fs::symlink, sync::Arc};
use winbridge_controller::readiness_store::ReceiptStore;
use winbridge_protocol::PairVerification;
fn private_temp() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn receipt() -> PairVerification {
    serde_json::from_str(include_str!("../../../tests/fixtures/readiness/pair.json")).unwrap()
}
#[test]
fn concurrent_create_only_dedup_survives_reopen() {
    let temp = private_temp();
    let store = Arc::new(ReceiptStore::open(temp.path()).unwrap());
    let r = receipt();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let s = Arc::clone(&store);
            let r = r.clone();
            std::thread::spawn(move || s.put(&r).unwrap())
        })
        .collect();
    let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(ids.iter().all(|id| id == &ids[0]));
    drop(store);
    let reopened = ReceiptStore::open(temp.path()).unwrap();
    assert_eq!(reopened.get(&ids[0]).unwrap(), Some(r));
    assert_eq!(
        fs::read_dir(temp.path().join("contract-receipts-v1"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn corrupted_or_partial_receipts_cannot_be_overwritten_or_interpreted_as_proof() {
    let temp = private_temp();
    let store = ReceiptStore::open(temp.path()).unwrap();
    let r = receipt();
    let id = store.put(&r).unwrap();
    let path = temp
        .path()
        .join("contract-receipts-v1")
        .join(format!("{id}.json"));
    let original = fs::read(&path).unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
    changed["receipt"]["checks"] = serde_json::json!(["other"]);
    fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(store.get(&id).is_err());
    let corrupted = fs::read(&path).unwrap();
    assert!(store.put(&r).is_err());
    assert_eq!(fs::read(&path).unwrap(), corrupted);
    fs::write(&path, b"{\"schemaVersion\":1").unwrap();
    assert!(store.get(&id).is_err());
    assert!(store.get("../../outside").is_err());
    assert_eq!(store.get(&"0".repeat(64)).unwrap(), None);
}
#[test]
fn symlink_targets_and_unknown_envelopes_are_preserved() {
    let temp = private_temp();
    let store = ReceiptStore::open(temp.path()).unwrap();
    let r = receipt();
    let id = winbridge_protocol::evidence_sha256(&serde_json::to_vec(&r).unwrap());
    let path = temp
        .path()
        .join("contract-receipts-v1")
        .join(format!("{id}.json"));
    let untouched = temp.path().join("untouched");
    fs::write(&untouched, b"untouched").unwrap();
    symlink(&untouched, &path).unwrap();
    assert!(store.get(&id).is_err());
    assert!(store.put(&r).is_err());
    assert_eq!(fs::read(&untouched).unwrap(), b"untouched");
    fs::remove_file(path).unwrap();
    let saved = store.put(&r).unwrap();
    let path = temp
        .path()
        .join("contract-receipts-v1")
        .join(format!("{saved}.json"));
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope["schemaVersion"] = 2.into();
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(store.get(&saved).is_err());
}
#[test]
fn linked_store_is_rejected_before_writes() {
    let temp = private_temp();
    let target = tempfile::tempdir().unwrap();
    symlink(target.path(), temp.path().join("contract-receipts-v1")).unwrap();
    assert!(ReceiptStore::open(temp.path()).is_err());
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn nonregular_receipt_is_rejected_without_waiting_for_a_writer() {
    use std::os::unix::ffi::OsStrExt;
    let temp = private_temp();
    let store = ReceiptStore::open(temp.path()).unwrap();
    let id = "0".repeat(64);
    let path = temp
        .path()
        .join("contract-receipts-v1")
        .join(format!("{id}.json"));
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(store.get(&id).is_err());
}
